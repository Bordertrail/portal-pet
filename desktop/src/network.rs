use std::{collections::VecDeque, net::IpAddr, sync::Mutex, time::Duration};

use futures_util::{SinkExt, StreamExt};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use keyring_core::api::CredentialStoreApi;
use portal_protocol::{
    Activate, Activated, ApiError, Avatar, AvatarChange, ConversationEnd, DEFAULT_SERVER_ADDRESS,
    Directory, Event, FriendAnswer, IMAGE_MAX_BYTES, IssuedAccount, Link, Member, Message,
    NicknameChange, SendMessage,
};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tauri::{AppHandle, Emitter, Manager, State, async_runtime::JoinHandle};
use tokio_tungstenite::tungstenite::{
    Error as SocketError, Message as Frame, client::IntoClientRequest, http::HeaderValue,
};
use url::Url;

use crate::{CHAT, PET, handle_remote_event};

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    server: Url,
    member: Member,
    token: String,
}

#[derive(Default)]
struct Connection {
    session: Option<Session>,
    task: Option<JoinHandle<()>>,
    link: Link,
    photos: ImageCache,
}

const IMAGE_CACHE_BYTES: usize = 32 * 1024 * 1024;
const IMAGE_CACHE_COUNT: usize = 96;

#[derive(Default)]
struct ImageCache {
    entries: VecDeque<(i64, Vec<u8>)>,
    bytes: usize,
}

impl ImageCache {
    fn get(&mut self, id: i64, expected: Option<usize>) -> Option<Vec<u8>> {
        let index = self.entries.iter().position(|(key, bytes)| {
            *key == id && expected.is_none_or(|size| size == bytes.len())
        })?;
        let entry = self.entries.remove(index)?;
        let bytes = entry.1.clone();
        self.entries.push_back(entry);
        Some(bytes)
    }

    fn insert(&mut self, id: i64, bytes: Vec<u8>) {
        if let Some(index) = self.entries.iter().position(|(key, _)| *key == id)
            && let Some((_, old)) = self.entries.remove(index)
        {
            self.bytes -= old.len();
        }
        self.bytes += bytes.len();
        self.entries.push_back((id, bytes));
        while self.bytes > IMAGE_CACHE_BYTES || self.entries.len() > IMAGE_CACHE_COUNT {
            let (_, expired) = self.entries.pop_front().expect("cache is over its limit");
            self.bytes -= expired.len();
        }
    }
}

pub struct Network {
    client: Client,
    state: Mutex<Connection>,
    lifecycle: tokio::sync::Mutex<()>,
}

impl Network {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .build()
                .expect("create HTTP client"),
            state: Mutex::new(Connection::default()),
            lifecycle: tokio::sync::Mutex::new(()),
        }
    }

    pub(crate) fn link(&self) -> Link {
        self.state.lock().unwrap().link.clone()
    }

    pub(crate) fn sticker_account(&self, revision: u64) -> Result<(String, i64), String> {
        let (session, current) = self.session()?;
        if revision != current {
            return Err("会话已切换".into());
        }
        Ok((session.server.to_string(), session.member.id))
    }

    fn session(&self) -> Result<(Session, u64), String> {
        let state = self.state.lock().unwrap();
        let session = state.session.clone().ok_or("请先用安全码连接服务器")?;
        Ok((session, state.link.revision))
    }

    pub fn current(&self, revision: u64) -> bool {
        let state = self.state.lock().unwrap();
        state.link.revision == revision && state.session.is_some()
    }

    fn remember_image(&self, revision: u64, id: i64, bytes: Vec<u8>) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.link.revision != revision || state.session.is_none() {
            return false;
        }
        state.photos.insert(id, bytes);
        true
    }

    fn announce(&self, app: &AppHandle) {
        let link = self.link();
        let _ = app.emit_to(
            PET,
            "portal-avatar",
            link.member
                .as_ref()
                .map_or(Avatar::Default, |member| member.avatar),
        );
        let _ = app.emit_to(CHAT, "portal-link", link);
    }

    fn apply_member(&self, app: &AppHandle, revision: u64, member: &Member) {
        let mut state = self.state.lock().unwrap();
        if state.link.revision != revision || state.link.member.as_ref() == Some(member) {
            return;
        }
        let Some(session) = state
            .session
            .as_mut()
            .filter(|session| session.member.id == member.id)
        else {
            return;
        };
        session.member = member.clone();
        state.link.member = Some(member.clone());
        drop(state);
        self.announce(app);
    }

    fn install(&self, app: &AppHandle, session: Session, notice: Option<String>) {
        let mut state = self.state.lock().unwrap();
        if let Some(task) = state.task.take() {
            task.abort();
        }
        state.link = Link {
            member: Some(session.member.clone()),
            server: Some(session.server.to_string()),
            connected: false,
            revision: state.link.revision + 1,
            notice,
        };
        state.session = Some(session.clone());
        state.photos = ImageCache::default();
        state.task = Some(tauri::async_runtime::spawn(connect_loop(
            app.clone(),
            session,
            state.link.revision,
        )));
        drop(state);
        self.announce(app);
    }

    fn finish(&self, app: &AppHandle, notice: Option<String>, abort: bool) {
        {
            let mut state = self.state.lock().unwrap();
            if let Some(task) = state.task.take()
                && abort
            {
                task.abort();
            }
            state.session = None;
            state.photos = ImageCache::default();
            state.link = Link {
                revision: state.link.revision + 1,
                notice,
                ..Link::default()
            };
        }
        self.announce(app);
        crate::reset_remote_arrival(app);
    }

    fn socket_status(&self, app: &AppHandle, revision: u64, connected: bool) {
        let mut state = self.state.lock().unwrap();
        if state.link.revision == revision && state.link.connected != connected {
            state.link.connected = connected;
            drop(state);
            self.announce(app);
        }
    }

    async fn ended(&self, app: &AppHandle, revision: u64, from_socket: bool) {
        let _guard = self.lifecycle.lock().await;
        if self.current(revision) {
            let notice = if erase().is_err() {
                "登录凭据已失效，请重新激活。系统凭据清除失败。"
            } else {
                "登录凭据已失效，请重新激活"
            };
            self.finish(app, Some(notice.into()), !from_socket);
        }
    }

    async fn check(
        &self,
        app: &AppHandle,
        revision: u64,
        response: Result<reqwest::Response, reqwest::Error>,
    ) -> Result<reqwest::Response, String> {
        match response {
            Ok(response) if response.status() == StatusCode::UNAUTHORIZED => {
                self.ended(app, revision, false).await;
                Err("登录凭据已失效，请重新激活".into())
            }
            response => check(response).await,
        }
    }
}

fn configured_server() -> Result<Url, String> {
    let address = std::env::var("PORTAL_PET_SERVER_ADDRESS");
    let address = address.as_deref().unwrap_or(DEFAULT_SERVER_ADDRESS);
    let server = Url::parse(address.trim()).map_err(|_| "服务器配置无效")?;
    let host = server.host_str().ok_or("服务器配置无效")?;
    let loopback = host == "localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    match server.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => return Err("服务器配置无效".into()),
    }
    if !server.username().is_empty()
        || server.password().is_some()
        || server.query().is_some()
        || server.fragment().is_some()
        || server.path() != "/"
    {
        return Err("服务器配置无效".into());
    }
    Ok(server)
}

fn endpoint(server: &Url, path: &str) -> Url {
    let mut url = server.clone();
    url.set_path(&format!("/api/v1/{path}"));
    url
}

async fn check(
    result: Result<reqwest::Response, reqwest::Error>,
) -> Result<reqwest::Response, String> {
    let response = result.map_err(|error| {
        if error.is_timeout() {
            "服务器响应超时，请稍后重试"
        } else {
            "无法连接服务器"
        }
        .to_owned()
    })?;
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(response
            .json::<ApiError>()
            .await
            .map(|value| value.message)
            .unwrap_or_else(|_| "服务器请求失败".into()))
    }
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    response
        .json()
        .await
        .map_err(|_| "无法读取服务器响应".into())
}

fn credentials() -> keyring_core::Result<Option<keyring_core::Entry>> {
    if std::env::var_os("PORTAL_PET_MEMORY_SESSION").is_some() {
        return Ok(None);
    }
    #[cfg(target_os = "macos")]
    let entry = apple_native_keyring_store::keychain::Store::new()?.build(
        "org.portalpet.chat.desktop",
        "session",
        None,
    )?;
    #[cfg(target_os = "windows")]
    let entry = windows_native_keyring_store::Store::new()?.build(
        "org.portalpet.chat.desktop",
        "session",
        None,
    )?;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    return Ok(Some(entry));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Ok(None)
}

fn persist(session: &Session) -> Result<(), String> {
    if let Some(entry) = credentials().map_err(|_| "无法打开系统凭据存储")? {
        let value = serde_json::to_string(session).map_err(|_| "无法保存登录信息")?;
        entry.set_password(&value).map_err(|_| "无法保存系统凭据")?;
    }
    Ok(())
}

fn erase() -> keyring_core::Result<()> {
    if let Some(entry) = credentials()? {
        match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub async fn restore(app: AppHandle) {
    let network = app.state::<Network>();
    let _guard = network.lifecycle.lock().await;
    if network.link().revision != 0 {
        return;
    }
    let Ok(Some(entry)) = credentials() else {
        return;
    };
    let Ok(value) = entry.get_password() else {
        return;
    };
    let Ok(session) = serde_json::from_str::<Session>(&value) else {
        return;
    };
    if configured_server().is_ok_and(|server| server == session.server) {
        network.install(&app, session, None);
    }
}

#[tauri::command]
pub fn server_link(network: State<'_, Network>) -> Link {
    network.link()
}

#[tauri::command]
pub async fn activate_server(app: AppHandle, code: String) -> Result<Link, String> {
    let network = app.state::<Network>();
    let _guard = network.lifecycle.lock().await;
    let server = configured_server()?;
    let activation: Activated = decode(
        check(
            network
                .client
                .post(endpoint(&server, "activate"))
                .json(&Activate { code })
                .send()
                .await,
        )
        .await?,
    )
    .await?;
    let session = Session {
        server,
        member: activation.member,
        token: activation.token,
    };
    let notice = persist(&session).err().map(|message| {
        let _ = erase();
        format!("{message}，当前登录仅保留到退出应用")
    });
    crate::reset_remote_arrival(&app);
    network.install(&app, session, notice);
    Ok(network.link())
}

#[tauri::command]
pub async fn disconnect_server(app: AppHandle) -> Result<(), String> {
    let network = app.state::<Network>();
    let _guard = network.lifecycle.lock().await;
    let revoked = if let Ok((session, _)) = network.session() {
        network
            .client
            .post(endpoint(&session.server, "session/logout"))
            .bearer_auth(session.token)
            .send()
            .await
            .is_ok_and(|response| {
                response.status().is_success() || response.status() == StatusCode::UNAUTHORIZED
            })
    } else {
        true
    };
    let erased = erase().is_ok();
    network.finish(&app, None, true);
    match (erased, revoked) {
        (false, _) => Err("本次会话已结束，系统凭据清除失败".into()),
        (true, false) => Err("已退出本机，服务器暂时无法确认会话已注销".into()),
        (true, true) => Ok(()),
    }
}

#[tauri::command]
pub async fn fetch_directory(app: AppHandle) -> Result<Directory, String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let result = network
        .client
        .get(endpoint(&session.server, "directory"))
        .bearer_auth(session.token)
        .send()
        .await;
    let directory: Directory = decode(network.check(&app, revision, result).await?).await?;
    synchronize_profiles(&app, revision, &directory);
    Ok(directory)
}

fn synchronize_profiles(app: &AppHandle, revision: u64, directory: &Directory) {
    app.state::<Network>()
        .apply_member(app, revision, &directory.me);
    let people = directory
        .people
        .iter()
        .map(|person| person.member.clone())
        .collect::<Vec<_>>();
    if let Err(error) = crate::run_desktop(app, move |app| {
        if app.state::<Network>().current(revision) {
            crate::visits::sync_guest_profiles(app, &people)?;
        }
        Ok(())
    }) {
        eprintln!("{error}");
    }
}

#[tauri::command]
pub async fn change_nickname(app: AppHandle, nickname: String) -> Result<Member, String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let response = network
        .client
        .post(endpoint(&session.server, "me/nickname"))
        .bearer_auth(session.token)
        .json(&NicknameChange { nickname })
        .send()
        .await;
    let member: Member = decode(network.check(&app, revision, response).await?).await?;
    network.apply_member(&app, revision, &member);
    Ok(member)
}

#[tauri::command]
pub async fn change_avatar(app: AppHandle, avatar: Avatar) -> Result<Member, String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let response = network
        .client
        .post(endpoint(&session.server, "me/avatar"))
        .bearer_auth(session.token)
        .json(&AvatarChange { avatar })
        .send()
        .await;
    let member: Member = decode(network.check(&app, revision, response).await?).await?;
    network.apply_member(&app, revision, &member);
    Ok(member)
}

#[tauri::command]
pub async fn create_account(app: AppHandle, nickname: String) -> Result<IssuedAccount, String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let response = network
        .client
        .post(endpoint(&session.server, "admin/accounts"))
        .bearer_auth(session.token)
        .json(&NicknameChange { nickname })
        .send()
        .await;
    decode(network.check(&app, revision, response).await?).await
}

#[tauri::command]
pub async fn add_friend(app: AppHandle, peer: i64) -> Result<(), String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let result = network
        .client
        .post(endpoint(
            &session.server,
            &format!("friends/{peer}/request"),
        ))
        .bearer_auth(session.token)
        .send()
        .await;
    network.check(&app, revision, result).await.map(|_| ())
}

#[tauri::command]
pub async fn answer_friend(app: AppHandle, peer: i64, accept: bool) -> Result<(), String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let result = network
        .client
        .post(endpoint(&session.server, &format!("friends/{peer}/answer")))
        .bearer_auth(session.token)
        .json(&FriendAnswer { accept })
        .send()
        .await;
    network.check(&app, revision, result).await.map(|_| ())
}

#[tauri::command]
pub async fn fetch_messages(app: AppHandle, peer: i64) -> Result<Vec<Message>, String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let result = network
        .client
        .get(endpoint(&session.server, &format!("messages/{peer}")))
        .bearer_auth(session.token)
        .send()
        .await;
    decode(network.check(&app, revision, result).await?).await
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    peer: i64,
    message: SendMessage,
) -> Result<Message, String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let result = network
        .client
        .post(endpoint(&session.server, &format!("messages/{peer}")))
        .bearer_auth(session.token)
        .json(&message)
        .send()
        .await;
    decode(network.check(&app, revision, result).await?).await
}

#[tauri::command]
pub async fn send_image(
    app: AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<Message, String> {
    let header = |name| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
    };
    let peer = header("x-portal-peer")
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or("无效的会话")?;
    let client_id = header("x-portal-client-id")
        .filter(|value| {
            value.len() == 36
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        })
        .ok_or("无效的消息编号")?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("无法读取图片".into());
    };
    if bytes.is_empty() || bytes.len() > IMAGE_MAX_BYTES {
        return Err("图片不能超过 8 MiB".into());
    }
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let result = network
        .client
        .post(endpoint(
            &session.server,
            &format!("message-images/{peer}/{client_id}"),
        ))
        .timeout(Duration::from_secs(60))
        .bearer_auth(session.token)
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(bytes.clone())
        .send()
        .await;
    let message: Message = decode(network.check(&app, revision, result).await?).await?;
    if message
        .image
        .as_ref()
        .is_some_and(|info| info.bytes as usize == bytes.len())
        && !network.remember_image(revision, message.id, bytes.clone())
    {
        return Err("会话已切换".into());
    }
    Ok(message)
}

#[tauri::command]
pub async fn fetch_image(
    app: AppHandle,
    id: i64,
    expected_bytes: Option<u32>,
) -> Result<tauri::ipc::Response, String> {
    let expected = expected_bytes.map(|bytes| bytes as usize);
    if expected.is_some_and(|length| length == 0 || length > IMAGE_MAX_BYTES) {
        return Err("图片大小无效".into());
    }
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    {
        let mut state = network.state.lock().unwrap();
        if state.link.revision != revision {
            return Err("会话已切换".into());
        }
        if let Some(bytes) = state.photos.get(id, expected) {
            return Ok(tauri::ipc::Response::new(bytes));
        }
    }
    let result = network
        .client
        .get(endpoint(&session.server, &format!("message-images/{id}")))
        .timeout(Duration::from_secs(45))
        .bearer_auth(session.token)
        .send()
        .await;
    let mut response = network.check(&app, revision, result).await?;
    let declared = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok());
    if declared.is_some_and(|length| length > IMAGE_MAX_BYTES) {
        return Err("图片不能超过 8 MiB".into());
    }
    if expected
        .zip(declared)
        .is_some_and(|(expected, declared)| expected != declared)
    {
        return Err("图片数据不完整".into());
    }
    let expected = expected.or(declared);
    let mut bytes = Vec::with_capacity(expected.unwrap_or_default());
    while let Some(chunk) = response.chunk().await.map_err(|_| "下载图片失败")? {
        if chunk.len() > IMAGE_MAX_BYTES - bytes.len() {
            return Err("图片不能超过 8 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() || expected.is_some_and(|length| length != bytes.len()) {
        return Err("图片数据不完整".into());
    }
    if !network.remember_image(revision, id, bytes.clone()) {
        return Err("会话已切换".into());
    }
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(test)]
mod image_cache_tests {
    use super::*;

    #[test]
    fn cache_checks_size_and_drops_least_recently_used_images() {
        let mut cache = ImageCache::default();
        for id in 0..4 {
            cache.insert(id, vec![id as u8; IMAGE_MAX_BYTES]);
        }
        assert!(cache.get(0, Some(16)).is_none());
        assert_eq!(cache.get(0, Some(IMAGE_MAX_BYTES)).unwrap()[0], 0);
        cache.insert(4, vec![4; IMAGE_MAX_BYTES]);
        assert_eq!(cache.bytes, IMAGE_CACHE_BYTES);
        assert!(cache.get(1, None).is_none());
        assert_eq!(cache.get(0, None).unwrap().len(), IMAGE_MAX_BYTES);
        cache.insert(0, vec![9]);
        assert_eq!(cache.get(0, Some(1)), Some(vec![9]));
        assert_eq!(cache.bytes, 3 * IMAGE_MAX_BYTES + 1);
    }
}

#[tauri::command]
pub async fn end_conversation(app: AppHandle, peer: Member) -> Result<ConversationEnd, String> {
    let network = app.state::<Network>();
    let (session, revision) = network.session()?;
    let response = network
        .client
        .post(endpoint(
            &session.server,
            &format!("messages/{}/end", peer.id),
        ))
        .bearer_auth(&session.token)
        .send()
        .await;
    let ending: ConversationEnd = decode(network.check(&app, revision, response).await?).await?;
    let displayed = ending.clone();
    crate::run_desktop(&app, move |app| {
        if app.state::<Network>().current(revision) {
            crate::visits::end(app, session.member.id, displayed, Some(peer), true)?;
        }
        Ok(())
    })?;
    Ok(ending)
}

fn sync_endings(app: &AppHandle, session: &Session, revision: u64) {
    let app = app.clone();
    let session = session.clone();
    tauri::async_runtime::spawn(async move {
        let network = app.state::<Network>();
        let response = network
            .client
            .get(endpoint(&session.server, "directory"))
            .bearer_auth(&session.token)
            .send()
            .await;
        if let Ok(response) = response
            && response.status().is_success()
            && let Ok(directory) = decode::<Directory>(response).await
        {
            synchronize_profiles(&app, revision, &directory);
            if let Err(error) =
                crate::reconcile_endings(&app, revision, session.member.id, directory.endings)
            {
                eprintln!("{error}");
            }
        }
    });
}

async fn connect_loop(app: AppHandle, session: Session, revision: u64) {
    let network = app.state::<Network>();
    let mut url = endpoint(&session.server, "events");
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    if url.set_scheme(scheme).is_err() {
        return;
    }
    let mut retry = Duration::from_secs(1);
    while network.current(revision) {
        let Ok(mut request) = url.as_str().into_client_request() else {
            return;
        };
        let Ok(mut header) = HeaderValue::from_str(&format!("Bearer {}", session.token)) else {
            return;
        };
        header.set_sensitive(true);
        request.headers_mut().insert("Authorization", header);
        let result = tokio::time::timeout(
            Duration::from_secs(12),
            tokio_tungstenite::connect_async(request),
        )
        .await;
        match result {
            Ok(Ok((mut socket, _))) => {
                retry = Duration::from_secs(1);
                while let Ok(Some(Ok(frame))) =
                    tokio::time::timeout(Duration::from_secs(35), socket.next()).await
                {
                    let event = match frame {
                        Frame::Text(text) => serde_json::from_str::<Event>(&text).ok(),
                        Frame::Ping(_) => {
                            if socket.flush().await.is_err() {
                                break;
                            }
                            None
                        }
                        Frame::Close(_) => break,
                        _ => None,
                    };
                    match event {
                        Some(Event::Ready) => {
                            network.socket_status(&app, revision, true);
                            handle_remote_event(&app, revision, Event::Ready);
                            sync_endings(&app, &session, revision);
                        }
                        Some(Event::Resync) => {
                            handle_remote_event(&app, revision, Event::Resync);
                            sync_endings(&app, &session, revision);
                        }
                        Some(Event::SessionEnded) => {
                            network.ended(&app, revision, true).await;
                            return;
                        }
                        Some(event) => handle_remote_event(&app, revision, event),
                        None => {}
                    }
                }
                network.socket_status(&app, revision, false);
            }
            Ok(Err(SocketError::Http(response))) if response.status().as_u16() == 401 => {
                network.ended(&app, revision, true).await;
                return;
            }
            _ => {}
        }
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(Duration::from_secs(8));
    }
}
