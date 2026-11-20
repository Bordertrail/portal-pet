use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{
        ConnectInfo, DefaultBodyLimit, FromRequestParts, Path, State, WebSocketUpgrade,
        ws::{Message as Frame, WebSocket},
    },
    http::{HeaderMap, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use portal_protocol::{
    Activate, Activated, ApiError, Avatar, AvatarChange, ConversationEnd, Directory, Event,
    FriendAnswer, IssuedAccount, Member, Message, NicknameChange, Person, Relationship,
    SECURITY_CODE_LENGTH, SendMessage,
};
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::store;

type Shared = Arc<App>;
type ApiResult<T> = Result<T, Fault>;

struct App {
    pool: SqlitePool,
    trust_proxy: bool,
    online: Mutex<HashMap<i64, usize>>,
    activation_attempts: Mutex<ActivationAttempts>,
    events: broadcast::Sender<Notice>,
}

struct ActivationAttempts {
    by_ip: HashMap<IpAddr, (Instant, u32)>,
    short_codes: (Instant, u32),
}

#[derive(Clone)]
struct Notice {
    audience: Audience,
    event: Event,
}

#[derive(Clone, Copy)]
enum Audience {
    Everyone,
    Member(i64),
    Pair(i64, i64),
}

impl Audience {
    fn includes(self, user: i64) -> bool {
        match self {
            Self::Everyone => true,
            Self::Member(id) => id == user,
            Self::Pair(first, second) => first == user || second == user,
        }
    }
}

impl App {
    fn publish(&self, audience: Audience, event: Event) {
        let _ = self.events.send(Notice { audience, event });
    }
}

pub fn router(pool: SqlitePool, trust_proxy: bool) -> Router {
    let (events, _) = broadcast::channel(128);
    let app = Arc::new(App {
        pool,
        trust_proxy,
        online: Mutex::new(HashMap::new()),
        activation_attempts: Mutex::new(ActivationAttempts {
            by_ip: HashMap::new(),
            short_codes: (Instant::now(), 0),
        }),
        events,
    });
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/v1/activate", post(activate))
        .route("/api/v1/session/logout", post(logout))
        .route("/api/v1/directory", get(directory))
        .route("/api/v1/me/nickname", post(change_nickname))
        .route("/api/v1/me/avatar", post(change_avatar))
        .route("/api/v1/admin/accounts", post(create_account))
        .route("/api/v1/friends/{peer}/request", post(request_friend))
        .route("/api/v1/friends/{peer}/answer", post(answer_friend))
        .route("/api/v1/messages/{peer}", get(history).post(send_message))
        .route("/api/v1/messages/{peer}/end", post(end_conversation))
        .route("/api/v1/events", get(websocket))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(app)
}

#[derive(Debug)]
struct Fault(StatusCode, &'static str);

impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(ApiError {
                message: self.1.to_owned(),
            }),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for Fault {
    fn from(error: sqlx::Error) -> Self {
        eprintln!("SQLite request failed: {error}");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "服务暂时无法读取或保存数据",
        )
    }
}

fn unauthorized() -> Fault {
    Fault(StatusCode::UNAUTHORIZED, "登录凭据已失效，请重新激活")
}

fn forbidden() -> Fault {
    Fault(StatusCode::FORBIDDEN, "双方成为好友后才能收发消息")
}

struct Auth {
    member: Member,
    is_admin: bool,
    token_hash: String,
}

impl FromRequestParts<Shared> for Auth {
    type Rejection = Fault;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> ApiResult<Self> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|token| token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(unauthorized)?;
        let token_hash = store::digest(token);
        let (member, is_admin) = store::member_by_token(&app.pool, &token_hash)
            .await?
            .ok_or_else(unauthorized)?;
        Ok(Self {
            member,
            is_admin,
            token_hash,
        })
    }
}

async fn activate(
    State(app): State<Shared>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(request): Json<Activate>,
) -> ApiResult<Json<Activated>> {
    let code = request.code.trim();
    let is_short =
        code.len() == SECURITY_CODE_LENGTH && code.bytes().all(|b| b.is_ascii_alphanumeric());
    let client_ip = if app.trust_proxy {
        headers
            .get("x-portal-client-ip")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(addr.ip())
    } else {
        addr.ip()
    };
    {
        const WINDOW: Duration = Duration::from_secs(60);
        let now = Instant::now();
        let mut limits = app.activation_attempts.lock().unwrap();
        limits
            .by_ip
            .retain(|_, (since, _)| now.duration_since(*since) < WINDOW);
        let limited = Fault(StatusCode::TOO_MANY_REQUESTS, "登录尝试过多，请稍后再试");
        let ip_attempts = &mut limits.by_ip.entry(client_ip).or_insert((now, 0)).1;
        if *ip_attempts >= 30 {
            return Err(limited);
        }
        *ip_attempts += 1;
        if is_short {
            // Bound distributed guesses against the smaller reusable code space.
            if now.duration_since(limits.short_codes.0) >= WINDOW {
                limits.short_codes = (now, 0);
            }
            if limits.short_codes.1 >= 120 {
                return Err(limited);
            }
            limits.short_codes.1 += 1;
        }
    }
    let invalid = Fault(StatusCode::UNAUTHORIZED, "安全码无效或已失效");
    let code = if is_short {
        code.to_ascii_uppercase()
    } else if code.len() == 32 && code.bytes().all(|b| b.is_ascii_hexdigit()) {
        code.to_ascii_lowercase()
    } else {
        return Err(invalid);
    };
    let activated = store::activate(&app.pool, &code).await?.ok_or(invalid)?;
    Ok(Json(activated))
}

async fn logout(State(app): State<Shared>, auth: Auth) -> ApiResult<StatusCode> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(auth.token_hash)
        .execute(&app.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(sqlx::FromRow)]
struct DirectoryRow {
    id: i64,
    nickname: String,
    avatar: String,
    enabled: bool,
    requester_id: Option<i64>,
    accepted: Option<bool>,
}

async fn directory(State(app): State<Shared>, auth: Auth) -> ApiResult<Json<Directory>> {
    let rows = sqlx::query_as::<_, DirectoryRow>(
        "SELECT m.id, m.nickname, m.avatar, m.enabled, r.requester_id, r.accepted
         FROM members m
         LEFT JOIN relationships r ON r.member_low = min(m.id, ?) AND r.member_high = max(m.id, ?)
         WHERE m.id != ? AND (m.enabled = 1 OR r.accepted = 1)
         ORDER BY m.nickname, m.id",
    )
    .bind(auth.member.id)
    .bind(auth.member.id)
    .bind(auth.member.id)
    .fetch_all(&app.pool)
    .await?;
    let endings = sqlx::query_as::<_, EndRow>(
        "SELECT e.*, m.nickname, m.avatar FROM conversation_ends e JOIN members m ON m.id = e.ended_by
         WHERE e.member_low = ? OR e.member_high = ?",
    )
    .bind(auth.member.id)
    .bind(auth.member.id)
    .fetch_all(&app.pool)
    .await?
    .into_iter()
    .map(EndRow::ending)
    .collect();
    let online = app.online.lock().unwrap();
    let people = rows
        .into_iter()
        .filter_map(
            |DirectoryRow {
                 id,
                 nickname,
                 avatar,
                 enabled,
                 requester_id,
                 accepted,
             }| {
                let online = enabled && online.contains_key(&id);
                let relationship = match (requester_id, accepted) {
                    (_, Some(true)) => Relationship::Friend,
                    (Some(author), _) if author == auth.member.id => Relationship::Outgoing,
                    (Some(_), _) => Relationship::Incoming,
                    _ if online => Relationship::None,
                    _ => return None,
                };
                Some(Person {
                    member: Member {
                        id,
                        nickname,
                        avatar: Avatar::from_storage(&avatar),
                    },
                    online,
                    relationship,
                })
            },
        )
        .collect();
    Ok(Json(Directory {
        me: auth.member,
        is_admin: auth.is_admin,
        people,
        endings,
    }))
}

fn nickname_input(input: &str) -> ApiResult<&str> {
    store::valid_nickname(input).ok_or(Fault(
        StatusCode::BAD_REQUEST,
        "昵称须为 1 至 32 个字符，且不能包含控制字符",
    ))
}

async fn change_nickname(
    State(app): State<Shared>,
    auth: Auth,
    Json(request): Json<NicknameChange>,
) -> ApiResult<Json<Member>> {
    update_profile(&app, auth, Some(nickname_input(&request.nickname)?), None).await
}

async fn change_avatar(
    State(app): State<Shared>,
    auth: Auth,
    Json(request): Json<AvatarChange>,
) -> ApiResult<Json<Member>> {
    update_profile(&app, auth, None, Some(request.avatar)).await
}

async fn update_profile(
    app: &App,
    auth: Auth,
    nickname: Option<&str>,
    avatar: Option<Avatar>,
) -> ApiResult<Json<Member>> {
    let (nickname, avatar): (String, String) = sqlx::query_as(
        "UPDATE members SET nickname = coalesce(?, nickname), avatar = coalesce(?, avatar)
         WHERE id = ? AND enabled = 1
         AND EXISTS (SELECT 1 FROM sessions WHERE member_id = members.id AND token_hash = ?)
         RETURNING nickname, avatar",
    )
    .bind(nickname)
    .bind(avatar.map(Avatar::as_str))
    .bind(auth.member.id)
    .bind(auth.token_hash)
    .fetch_optional(&app.pool)
    .await?
    .ok_or_else(unauthorized)?;
    app.publish(Audience::Everyone, Event::DirectoryChanged);
    Ok(Json(Member {
        id: auth.member.id,
        nickname,
        avatar: Avatar::from_storage(&avatar),
    }))
}

async fn create_account(
    State(app): State<Shared>,
    auth: Auth,
    Json(request): Json<NicknameChange>,
) -> ApiResult<impl IntoResponse> {
    let nickname = nickname_input(&request.nickname)?;
    let mut tx = app.pool.begin_with("BEGIN IMMEDIATE").await?;
    let (_, is_admin) = store::member_by_token(&mut *tx, &auth.token_hash)
        .await?
        .ok_or_else(unauthorized)?;
    if !is_admin {
        return Err(Fault(StatusCode::FORBIDDEN, "只有管理员可以创建账号"));
    }
    let id = store::create_member(&mut tx, nickname).await?;
    let code = store::issue_code(&mut tx, id).await?;
    tx.commit().await?;
    let issued = IssuedAccount {
        member: Member {
            id,
            nickname: nickname.to_owned(),
            avatar: Avatar::Default,
        },
        code,
    };
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(issued),
    ))
}

fn other(member: i64, peer: i64) -> ApiResult<(i64, i64)> {
    if peer <= 0 || member == peer {
        return Err(Fault(StatusCode::BAD_REQUEST, "无效的好友编号"));
    }
    Ok(store::pair(member, peer))
}

async fn request_friend(
    State(app): State<Shared>,
    auth: Auth,
    Path(peer): Path<i64>,
) -> ApiResult<StatusCode> {
    let (low, high) = other(auth.member.id, peer)?;
    if !app.online.lock().unwrap().contains_key(&peer) {
        return Err(Fault(StatusCode::NOT_FOUND, "对方当前不在线"));
    }
    let updated = sqlx::query(
        "INSERT INTO relationships(member_low, member_high, requester_id)
         SELECT ?, ?, ? WHERE EXISTS (SELECT 1 FROM members WHERE id = ? AND enabled = 1)
         ON CONFLICT(member_low, member_high) DO NOTHING",
    )
    .bind(low)
    .bind(high)
    .bind(auth.member.id)
    .bind(peer)
    .execute(&app.pool)
    .await?;
    if updated.rows_affected() != 0 {
        app.publish(Audience::Pair(low, high), Event::DirectoryChanged);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn answer_friend(
    State(app): State<Shared>,
    auth: Auth,
    Path(peer): Path<i64>,
    Json(answer): Json<FriendAnswer>,
) -> ApiResult<StatusCode> {
    let (low, high) = other(auth.member.id, peer)?;
    let statement = if answer.accept {
        "UPDATE relationships SET accepted = 1 WHERE member_low = ? AND member_high = ?
         AND requester_id = ? AND accepted = 0"
    } else {
        "DELETE FROM relationships WHERE member_low = ? AND member_high = ?
         AND requester_id = ? AND accepted = 0"
    };
    let update = sqlx::query(statement)
        .bind(low)
        .bind(high)
        .bind(peer)
        .execute(&app.pool)
        .await?;
    if update.rows_affected() == 0 {
        return Err(Fault(StatusCode::CONFLICT, "没有待处理的好友申请"));
    }
    app.publish(Audience::Pair(low, high), Event::DirectoryChanged);
    Ok(StatusCode::NO_CONTENT)
}

async fn history(
    State(app): State<Shared>,
    auth: Auth,
    Path(peer): Path<i64>,
) -> ApiResult<Json<Vec<Message>>> {
    let (low, high) = other(auth.member.id, peer)?;
    let accepted: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM relationships WHERE member_low = ? AND member_high = ? AND accepted = 1)",
    ).bind(low).bind(high).fetch_one(&app.pool).await?;
    if !accepted {
        return Err(forbidden());
    }
    let rows = sqlx::query(
        "SELECT msg.*, m.nickname, m.avatar FROM messages msg JOIN members m ON m.id = msg.sender_id
         WHERE (sender_id = ? AND recipient_id = ?) OR (sender_id = ? AND recipient_id = ?)
         ORDER BY msg.id DESC LIMIT 100",
    )
    .bind(auth.member.id)
    .bind(peer)
    .bind(peer)
    .bind(auth.member.id)
    .fetch_all(&app.pool)
    .await?;
    let messages = rows
        .iter()
        .rev()
        .map(store::row_message)
        .collect::<sqlx::Result<Vec<_>>>()?;
    Ok(Json(messages))
}

#[derive(sqlx::FromRow)]
struct EndRow {
    member_low: i64,
    member_high: i64,
    ended_by: i64,
    nickname: String,
    avatar: String,
    through_message_id: i64,
}

impl EndRow {
    fn ending(self) -> ConversationEnd {
        ConversationEnd {
            ended_by: Member {
                id: self.ended_by,
                nickname: self.nickname,
                avatar: Avatar::from_storage(&self.avatar),
            },
            peer_id: if self.ended_by == self.member_low {
                self.member_high
            } else {
                self.member_low
            },
            through_message_id: self.through_message_id,
        }
    }
}

async fn end_conversation(
    State(app): State<Shared>,
    auth: Auth,
    Path(peer): Path<i64>,
) -> ApiResult<Json<ConversationEnd>> {
    let (low, high) = other(auth.member.id, peer)?;
    let mut tx = app.pool.begin_with("BEGIN IMMEDIATE").await?;
    let accepted: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM relationships WHERE member_low = ? AND member_high = ? AND accepted = 1)",
    ).bind(low).bind(high).fetch_one(&mut *tx).await?;
    if !accepted {
        return Err(forbidden());
    }
    let current: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sessions WHERE member_id = ? AND token_hash = ?)",
    )
    .bind(auth.member.id)
    .bind(&auth.token_hash)
    .fetch_one(&mut *tx)
    .await?;
    if !current {
        return Err(unauthorized());
    }
    let through: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(id), 0) FROM messages WHERE (sender_id = ? AND recipient_id = ?)
         OR (sender_id = ? AND recipient_id = ?)",
    )
    .bind(low)
    .bind(high)
    .bind(high)
    .bind(low)
    .fetch_one(&mut *tx)
    .await?;
    let changed = sqlx::query(
        "INSERT INTO conversation_ends(member_low, member_high, ended_by, through_message_id)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(member_low, member_high) DO UPDATE SET ended_by = excluded.ended_by,
         through_message_id = excluded.through_message_id
         WHERE excluded.through_message_id > conversation_ends.through_message_id",
    )
    .bind(low)
    .bind(high)
    .bind(auth.member.id)
    .bind(through)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    let ending = sqlx::query_as::<_, EndRow>(
        "SELECT e.*, m.nickname, m.avatar FROM conversation_ends e JOIN members m ON m.id = e.ended_by
         WHERE e.member_low = ? AND e.member_high = ?",
    )
    .bind(low)
    .bind(high)
    .fetch_one(&mut *tx)
    .await?
    .ending();
    tx.commit().await?;
    if changed {
        app.publish(
            Audience::Member(peer),
            Event::ConversationEnded {
                ending: ending.clone(),
            },
        );
    }
    Ok(Json(ending))
}

async fn send_message(
    State(app): State<Shared>,
    auth: Auth,
    Path(peer): Path<i64>,
    Json(input): Json<SendMessage>,
) -> ApiResult<Json<Message>> {
    let (low, high) = other(auth.member.id, peer)?;
    let body = input.body.trim();
    if !(1..=2000).contains(&body.chars().count()) {
        return Err(Fault(
            StatusCode::BAD_REQUEST,
            "消息必须有 1 至 2000 个字符",
        ));
    }
    let client_id = Uuid::parse_str(&input.client_id)
        .map_err(|_| Fault(StatusCode::BAD_REQUEST, "无效的消息编号"))?
        .to_string();
    let mut tx = app.pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO messages(sender_id, recipient_id, client_id, body)
         SELECT ?, ?, ?, ?
         WHERE EXISTS (SELECT 1 FROM relationships WHERE member_low = ? AND member_high = ? AND accepted = 1)
           AND EXISTS (SELECT 1 FROM members WHERE id = ? AND enabled = 1)
           AND EXISTS (SELECT 1 FROM sessions WHERE token_hash = ? AND member_id = ?)
         ON CONFLICT(sender_id, client_id) DO NOTHING",
    ).bind(auth.member.id).bind(peer).bind(&client_id).bind(body)
      .bind(low).bind(high).bind(peer).bind(&auth.token_hash).bind(auth.member.id)
      .execute(&mut *tx).await?.rows_affected() == 1;
    let row = sqlx::query(
        "SELECT msg.*, m.nickname, m.avatar FROM messages msg JOIN members m ON m.id = msg.sender_id
         WHERE msg.sender_id = ? AND msg.client_id = ?",
    )
    .bind(auth.member.id)
    .bind(&client_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(forbidden)?;
    let message = store::row_message(&row)?;
    if message.recipient_id != peer || message.body != body {
        return Err(Fault(StatusCode::CONFLICT, "消息编号已用于其他内容"));
    }
    tx.commit().await?;
    if inserted {
        app.publish(
            Audience::Member(peer),
            Event::Message {
                message: message.clone(),
            },
        );
    }
    Ok(Json(message))
}

struct Presence {
    app: Shared,
    member: i64,
}

impl Presence {
    fn join(app: &Shared, member: i64) -> Self {
        let first = {
            let mut online = app.online.lock().unwrap();
            let count = online.entry(member).or_default();
            *count += 1;
            *count == 1
        };
        if first {
            app.publish(Audience::Everyone, Event::DirectoryChanged);
        }
        Self {
            app: app.clone(),
            member,
        }
    }
}

impl Drop for Presence {
    fn drop(&mut self) {
        let offline = {
            let mut online = self.app.online.lock().unwrap();
            let Some(count) = online.get_mut(&self.member) else {
                return;
            };
            *count -= 1;
            if *count == 0 {
                online.remove(&self.member);
                true
            } else {
                false
            }
        };
        if offline {
            self.app
                .publish(Audience::Everyone, Event::DirectoryChanged);
        }
    }
}

async fn websocket(State(app): State<Shared>, auth: Auth, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(16 * 1024)
        .max_frame_size(16 * 1024)
        .on_upgrade(move |socket| handle_socket(app, auth, socket))
}

async fn push(socket: &mut WebSocket, event: Event) -> bool {
    let Ok(serialized) = serde_json::to_string(&event) else {
        return false;
    };
    matches!(
        tokio::time::timeout(
            Duration::from_secs(5),
            socket.send(Frame::Text(serialized.into()))
        )
        .await,
        Ok(Ok(()))
    )
}

async fn handle_socket(app: Shared, auth: Auth, mut socket: WebSocket) {
    let mut events = app.events.subscribe();
    let _presence = Presence::join(&app, auth.member.id);
    if !push(&mut socket, Event::Ready).await {
        return;
    }
    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(10),
        Duration::from_secs(10),
    );
    let mut last_ping_reply = Instant::now();
    loop {
        let event = tokio::select! {
            frame = socket.recv() => match frame {
                None | Some(Err(_)) | Some(Ok(Frame::Close(_))) => break,
                Some(Ok(Frame::Pong(_))) => { last_ping_reply = Instant::now(); continue; }
                _ => continue,
            },
            notice = events.recv() => match notice {
                Ok(notice) if notice.audience.includes(auth.member.id) => notice.event,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => Event::Resync,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = heartbeat.tick() => {
                if last_ping_reply.elapsed() >= Duration::from_secs(35) { break; }
                match store::member_by_token(&app.pool, &auth.token_hash).await {
                    Ok(Some(_)) => {
                        if !matches!(tokio::time::timeout(Duration::from_secs(5), socket.send(Frame::Ping(Vec::new().into()))).await, Ok(Ok(()))) { break; }
                        continue;
                    }
                    Ok(None) => {
                        push(&mut socket, Event::SessionEnded).await;
                        break;
                    }
                    Err(_) => break,
                }
            }
        };
        if matches!(event, Event::Message { .. }) {
            match store::member_by_token(&app.pool, &auth.token_hash).await {
                Ok(Some(_)) => {}
                Ok(None) => {
                    push(&mut socket, Event::SessionEnded).await;
                    break;
                }
                Err(_) => break,
            }
        }
        if !push(&mut socket, event).await {
            break;
        }
    }
}
