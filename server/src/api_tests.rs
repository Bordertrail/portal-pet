use std::{collections::HashMap, path::PathBuf};

use axum::{extract::Path, http::Method, routing::any};
use reqwest::Client;

use super::*;

const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4, 0,
    0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 252, 255, 31, 0, 3, 3, 2, 0,
    239, 162, 167, 91, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

type StoredObjects = Arc<Mutex<HashMap<String, Bytes>>>;

async fn mock_oss(
    State(objects): State<StoredObjects>,
    Path(key): Path<String>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    assert!(key.starts_with("portal-pet/images/"));
    assert!(
        headers[header::AUTHORIZATION]
            .to_str()
            .unwrap()
            .starts_with("OSS4-HMAC-SHA256 Credential=test-key/")
    );
    match method {
        Method::PUT => {
            assert_eq!(headers[header::CONTENT_TYPE], "image/png");
            objects.lock().unwrap().insert(key, body);
            StatusCode::OK.into_response()
        }
        Method::GET => objects
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .map(IntoResponse::into_response)
            .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response()),
        Method::DELETE => {
            objects.lock().unwrap().remove(&key);
            StatusCode::NO_CONTENT.into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

async fn launch(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (origin, task)
}

struct DatabaseDir(PathBuf);

impl Drop for DatabaseDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn images_are_private_and_retries_keep_one_message() {
    let directory =
        DatabaseDir(std::env::temp_dir().join(format!("portal-image-test-{}", Uuid::new_v4())));
    std::fs::create_dir(&directory.0).unwrap();
    let pool = store::open(&directory.0.join("chat.sqlite")).await.unwrap();
    async fn member(pool: &SqlitePool, name: &str) -> Activated {
        let (_, code) = store::issue(pool, Some(name), None).await.unwrap();
        store::activate(pool, &code).await.unwrap().unwrap()
    }
    let sender = member(&pool, "发送方").await;
    let recipient = member(&pool, "接收方").await;
    let outsider = member(&pool, "其他成员").await;
    sqlx::query("INSERT INTO relationships(member_low, member_high, requester_id, accepted) VALUES (?, ?, ?, 1)")
        .bind(sender.member.id).bind(recipient.member.id).bind(sender.member.id).execute(&pool).await.unwrap();

    let objects = StoredObjects::default();
    let (oss_origin, oss_task) = launch(
        Router::new()
            .route("/{*key}", any(mock_oss))
            .with_state(objects.clone()),
    )
    .await;
    let (origin, api_task) = launch(router(
        pool.clone(),
        false,
        Some(oss::Storage::for_test(oss_origin.parse().unwrap())),
    ))
    .await;
    let client = Client::builder().no_proxy().build().unwrap();
    let api = format!("{origin}/api/v1");
    let client_id = Uuid::new_v4();
    let upload = format!("{api}/message-images/{}/{client_id}", recipient.member.id);
    let send = |data: Vec<u8>, token: &str| client.post(&upload).bearer_auth(token).body(data);
    assert_eq!(
        send(PNG.to_vec(), &outsider.token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(vec![0; IMAGE_MAX_BYTES + 1], &sender.token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        send(b"<svg/>".to_vec(), &sender.token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let (first, second) = tokio::join!(
        send(PNG.to_vec(), &sender.token).send(),
        send(PNG.to_vec(), &sender.token).send()
    );
    let first = first.unwrap().error_for_status().unwrap();
    let second = second.unwrap().error_for_status().unwrap();
    let message: Message = serde_json::from_slice(&first.bytes().await.unwrap()).unwrap();
    assert_eq!(
        message,
        serde_json::from_slice(&second.bytes().await.unwrap()).unwrap()
    );
    assert_eq!(message.body, "[图片]");
    assert_eq!(message.image.as_ref().unwrap().width, 1);
    assert_eq!(objects.lock().unwrap().len(), 1);

    let mut different = PNG.to_vec();
    different.push(0);
    assert_eq!(
        send(different, &sender.token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let read = format!("{api}/message-images/{}", message.id);
    assert_eq!(
        client.get(&read).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(&read)
            .bearer_auth(&outsider.token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    for token in [&sender.token, &recipient.token] {
        let response = client
            .get(&read)
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(response.bytes().await.unwrap().as_ref(), PNG);
    }
    let history = client
        .get(format!("{api}/messages/{}", sender.member.id))
        .bearer_auth(&recipient.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Vec<Message>>(&history).unwrap(),
        [message]
    );
    let text = serde_json::json!({"client_id": Uuid::new_v4().to_string(), "body": "普通消息"});
    let response = client
        .post(format!("{api}/messages/{}", recipient.member.id))
        .bearer_auth(&sender.token)
        .header(header::CONTENT_TYPE, "application/json")
        .body(text.to_string())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let text_message: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(text_message["body"], "普通消息");
    assert!(text_message.get("image").is_none());
    api_task.abort();
    oss_task.abort();
    pool.close().await;
}
