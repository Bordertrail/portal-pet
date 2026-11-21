use std::{collections::BTreeMap, fmt::Write, time::Duration};

use anyhow::{Context, Result, ensure};
use axum::body::Bytes;
use hmac::{Hmac, Mac};
use reqwest::{Client, Method, RequestBuilder, Url, header};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

const ALGORITHM: &str = "OSS4-HMAC-SHA256";
const PAYLOAD: &str = "UNSIGNED-PAYLOAD";

pub struct Storage {
    client: Client,
    endpoint: Url,
    bucket: String,
    region: String,
    access_key: String,
    secret: String,
    security_token: Option<String>,
}

fn dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Storage {
    pub fn from_env() -> Result<Option<Self>> {
        let bucket = std::env::var("PORTAL_OSS_BUCKET").unwrap_or_default();
        if bucket.is_empty() {
            return Ok(None);
        }
        let required = |name: &str| -> Result<String> {
            let value = std::env::var(name).with_context(|| format!("必须配置 {name}"))?;
            ensure!(
                !value.is_empty() && !value.chars().any(char::is_whitespace),
                "{name} 格式无效"
            );
            Ok(value)
        };
        let region = required("PORTAL_OSS_REGION")?;
        ensure!(
            dns_label(&bucket) && dns_label(&region) && bucket.len() >= 3,
            "OSS Bucket 或地域格式无效"
        );
        let internal = if std::env::var("PORTAL_OSS_INTERNAL").as_deref() == Ok("1") {
            "-internal"
        } else {
            ""
        };
        let endpoint = Url::parse(&format!(
            "https://{bucket}.oss-{region}{internal}.aliyuncs.com"
        ))?;
        let client = Client::builder()
            .timeout(Duration::from_secs(25))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()?;
        Ok(Some(Self {
            client,
            endpoint,
            bucket,
            region,
            access_key: required("OSS_ACCESS_KEY_ID")?,
            secret: required("OSS_ACCESS_KEY_SECRET")?,
            security_token: std::env::var("OSS_SECURITY_TOKEN")
                .ok()
                .filter(|value| !value.is_empty()),
        }))
    }

    fn request(&self, method: Method, key: &str, content_type: Option<&str>) -> RequestBuilder {
        let mut url = self.endpoint.clone();
        url.set_path(key);
        let now = OffsetDateTime::now_utc();
        let timestamp = format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            now.year(),
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second(),
        );
        let date = &timestamp[..8];
        let scope = format!("{date}/{}/oss/aliyun_v4_request", self.region);
        let host = url.host_str().expect("validated OSS endpoint");
        let mut headers = BTreeMap::from([
            ("host", host),
            ("x-oss-content-sha256", PAYLOAD),
            ("x-oss-date", timestamp.as_str()),
        ]);
        if let Some(content_type) = content_type {
            headers.insert("content-type", content_type);
        }
        if let Some(token) = &self.security_token {
            headers.insert("x-oss-security-token", token);
        }
        let mut canonical = format!("{method}\n/{}{path}\n\n", self.bucket, path = url.path());
        let mut request = self.client.request(method, url.clone());
        for (name, value) in headers {
            writeln!(canonical, "{name}:{}", value.trim()).unwrap();
            request = request.header(name, value);
        }
        write!(canonical, "\nhost\n{PAYLOAD}").unwrap();
        let signature = sign(&self.secret, &self.region, &timestamp, &scope, &canonical);
        request.header(
            header::AUTHORIZATION,
            format!(
                "{ALGORITHM} Credential={}/{scope},AdditionalHeaders=host,Signature={signature}",
                self.access_key
            ),
        )
    }

    pub async fn put(&self, key: &str, content_type: &str, body: Bytes) -> Result<()> {
        send(
            self.request(Method::PUT, key, Some(content_type))
                .body(body),
        )
        .await?;
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        send(self.request(Method::DELETE, key, None)).await?;
        Ok(())
    }

    pub async fn get(&self, key: &str, expected_size: usize) -> Result<Vec<u8>> {
        let mut response = send(self.request(Method::GET, key, None)).await?;
        let mut body = Vec::with_capacity(expected_size);
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                chunk.len() <= expected_size - body.len(),
                "OSS 返回的图片大小超出记录"
            );
            body.extend_from_slice(&chunk);
        }
        ensure!(body.len() == expected_size, "OSS 返回的图片大小与记录不符");
        Ok(body)
    }

    #[cfg(test)]
    pub(crate) fn for_test(endpoint: Url) -> Self {
        Self {
            client: Client::builder().no_proxy().build().unwrap(),
            endpoint,
            bucket: "test-bucket".into(),
            region: "cn-hangzhou".into(),
            access_key: "test-key".into(),
            secret: "test-secret".into(),
            security_token: None,
        }
    }
}

async fn send(request: RequestBuilder) -> Result<reqwest::Response> {
    let response = request.send().await?;
    ensure!(
        response.status().is_success(),
        "OSS HTTP {}",
        response.status()
    );
    Ok(response)
}

fn sign(secret: &str, region: &str, timestamp: &str, scope: &str, canonical: &str) -> String {
    fn hmac(key: &[u8], input: &str) -> [u8; 32] {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
        mac.update(input.as_bytes());
        mac.finalize().into_bytes().into()
    }

    let date_key = hmac(format!("aliyun_v4{secret}").as_bytes(), &timestamp[..8]);
    let region_key = hmac(&date_key, region);
    let service_key = hmac(&region_key, "oss");
    let signing_key = hmac(&service_key, "aliyun_v4_request");
    let hash = Sha256::digest(canonical.as_bytes());
    let signature = hmac(
        &signing_key,
        &format!("{ALGORITHM}\n{timestamp}\n{scope}\n{hash:x}"),
    );
    signature.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::sign;

    #[test]
    fn oss_v4_signature_with_example_values() {
        let canonical = concat!(
            "PUT\n/examplebucket/exampleobject\n\n",
            "content-disposition:attachment\ncontent-length:3\n",
            "content-md5:ICy5YqxZB1uWSwcVLSNLcA==\ncontent-type:text/plain\n",
            "x-oss-content-sha256:UNSIGNED-PAYLOAD\nx-oss-date:20250411T064124Z\n\n",
            "content-disposition;content-length\nUNSIGNED-PAYLOAD",
        );
        assert_eq!(
            sign(
                "yourAccessKeySecret",
                "cn-hangzhou",
                "20250411T064124Z",
                "20250411/cn-hangzhou/oss/aliyun_v4_request",
                canonical
            ),
            "d3694c2dfc5371ee6acd35e88c4871ac95a7ba01d3a2f476768fe61218590097",
        );
    }
}
