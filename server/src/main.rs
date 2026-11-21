mod api;
mod oss;
mod store;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Result, bail};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "serve".into());
    let path = std::env::var_os("PORTAL_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| "portal.sqlite".into());
    match (command.as_str(), args.next(), args.next()) {
        ("serve", address, None) => {
            let address: SocketAddr = address.as_deref().unwrap_or("127.0.0.1:8765").parse()?;
            let storage = oss::Storage::from_env()?;
            let pool = store::open(&path).await?;
            let listener = tokio::net::TcpListener::bind(address).await?;
            eprintln!("Portal server listening on {}", listener.local_addr()?);
            // Compose enables this only on the private network behind Caddy,
            // which overwrites the client address header.
            let trust_proxy = std::env::var("PORTAL_TRUST_PROXY").as_deref() == Ok("1");
            axum::serve(
                listener,
                api::router(pool, trust_proxy, storage)
                    .into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }
        ("invite", Some(nickname), None) => {
            let pool = store::open(&path).await?;
            let (member, code) = store::issue(&pool, Some(&nickname), None).await?;
            println!("{member}\t{code}");
        }
        ("reissue", Some(member), None) => {
            let pool = store::open(&path).await?;
            let (member, code) = store::issue(&pool, None, Some(member.parse()?)).await?;
            println!("{member}\t{code}");
        }
        ("disable", Some(member), None) => {
            store::disable(&store::open(&path).await?, member.parse()?).await?;
        }
        ("grant-admin", Some(member), None) => {
            store::grant_admin(&store::open(&path).await?, member.parse()?).await?;
        }
        _ => bail!(
            "用法：portal-server serve [地址:端口] | invite <昵称> | reissue <成员编号> | disable <成员编号> | grant-admin <成员编号>"
        ),
    }
    Ok(())
}
