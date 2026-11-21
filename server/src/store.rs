use std::{path::Path, time::Duration};

use anyhow::{Result, ensure};
use portal_protocol::{Activated, Avatar, ImageInfo, Member, Message, SECURITY_CODE_LENGTH};
use sha2::{Digest, Sha256};
use sqlx::{
    Row, SqliteConnection, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use uuid::Uuid;

pub async fn open(path: &Path) -> Result<SqlitePool> {
    #[cfg(unix)]
    {
        use std::{fs::OpenOptions, io::ErrorKind, os::unix::fs::OpenOptionsExt};
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let mut probe = sqlx::SqliteConnection::connect_with(&options).await?;
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut probe)
        .await?;
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    let supported_wal = [
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    ] >= [3, 51, 3];
    let journal = if supported_wal {
        SqliteJournalMode::Wal
    } else {
        let current: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut probe)
            .await?;
        ensure!(
            current != "wal",
            "SQLite {version} 太旧，打开 WAL 数据库需要 SQLite 3.51.3 或更新版本"
        );
        SqliteJournalMode::Delete
    };
    probe.close().await?;
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options.journal_mode(journal))
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

use sqlx::Connection;

pub fn digest(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

pub fn valid_nickname(input: &str) -> Option<&str> {
    let nickname = input.trim();
    ((1..=32).contains(&nickname.chars().count()) && !nickname.chars().any(char::is_control))
        .then_some(nickname)
}

pub async fn create_member(db: &mut SqliteConnection, nickname: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("INSERT INTO members(nickname) VALUES (?) RETURNING id")
        .bind(nickname)
        .fetch_one(db)
        .await
}

fn new_security_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let cutoff = 256 / ALPHABET.len() * ALPHABET.len();
    loop {
        // The first six bytes of a v4 UUID have no fixed version or variant bits.
        let random = Uuid::new_v4();
        let code: String = random.as_bytes()[..SECURITY_CODE_LENGTH]
            .iter()
            .map(|byte| usize::from(*byte))
            .filter(|byte| *byte < cutoff)
            .map(|byte| char::from(ALPHABET[byte % ALPHABET.len()]))
            .collect();
        if code.len() == SECURITY_CODE_LENGTH
            && code.bytes().any(|b| b.is_ascii_uppercase())
            && code.bytes().any(|b| b.is_ascii_digit())
        {
            return code;
        }
    }
}

pub async fn issue(
    pool: &SqlitePool,
    nickname: Option<&str>,
    member_id: Option<i64>,
) -> Result<(i64, String)> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let id = if let Some(nickname) = nickname {
        let nickname = valid_nickname(nickname)
            .ok_or_else(|| anyhow::anyhow!("昵称须为 1 至 32 个字符，且不能包含控制字符"))?;
        create_member(&mut tx, nickname).await?
    } else {
        let id = member_id.expect("CLI validates reissue arguments");
        let enabled: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM members WHERE id = ? AND enabled = 1)",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(enabled, "成员不存在或已停用");
        id
    };
    let code = issue_code(&mut tx, id).await?;
    tx.commit().await?;
    Ok((id, code))
}

pub async fn issue_code(db: &mut SqliteConnection, id: i64) -> sqlx::Result<String> {
    loop {
        let code = new_security_code();
        let hash = digest(&code);
        let inserted = sqlx::query(
            "INSERT INTO invitations(member_id, code_hash)
             SELECT ?, ? WHERE NOT EXISTS (SELECT 1 FROM invitations WHERE code_hash = ?)
             ON CONFLICT(member_id) DO UPDATE SET code_hash = excluded.code_hash",
        )
        .bind(id)
        .bind(&hash)
        .bind(&hash)
        .execute(&mut *db)
        .await?;
        if inserted.rows_affected() == 1 {
            return Ok(code);
        }
    }
}

pub async fn grant_admin(pool: &SqlitePool, id: i64) -> Result<()> {
    let updated = sqlx::query("UPDATE members SET is_admin = 1 WHERE id = ? AND enabled = 1")
        .bind(id)
        .execute(pool)
        .await?;
    ensure!(updated.rows_affected() == 1, "成员不存在或已停用");
    Ok(())
}

pub async fn disable(pool: &SqlitePool, member_id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    let updated = sqlx::query("UPDATE members SET enabled = 0 WHERE id = ?")
        .bind(member_id)
        .execute(&mut *tx)
        .await?;
    ensure!(updated.rows_affected() == 1, "成员不存在");
    sqlx::query("DELETE FROM invitations WHERE member_id = ?")
        .bind(member_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE member_id = ?")
        .bind(member_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn activate(pool: &SqlitePool, code: &str) -> sqlx::Result<Option<Activated>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let member: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT m.id, m.nickname, m.avatar FROM members m JOIN invitations i ON i.member_id = m.id
         WHERE i.code_hash = ? AND m.enabled = 1",
    )
    .bind(digest(code))
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, nickname, avatar)) = member else {
        return Ok(None);
    };
    if code.len() != SECURITY_CODE_LENGTH {
        // Previously issued 32-character codes retain their one-time behavior.
        sqlx::query("DELETE FROM invitations WHERE member_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO sessions(member_id, token_hash) VALUES (?, ?)
         ON CONFLICT(member_id) DO UPDATE SET token_hash = excluded.token_hash",
    )
    .bind(id)
    .bind(digest(&token))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(Activated {
        member: Member {
            id,
            nickname,
            avatar: Avatar::from_storage(&avatar),
        },
        token,
    }))
}

pub async fn member_by_token<'db>(
    db: impl sqlx::Executor<'db, Database = sqlx::Sqlite>,
    token_hash: &str,
) -> sqlx::Result<Option<(Member, bool)>> {
    let row: Option<(i64, String, String, bool)> = sqlx::query_as(
        "SELECT m.id, m.nickname, m.avatar, m.is_admin FROM members m JOIN sessions s ON s.member_id = m.id
         WHERE s.token_hash = ? AND m.enabled = 1",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(id, nickname, avatar, is_admin)| {
        (
            Member {
                id,
                nickname,
                avatar: Avatar::from_storage(&avatar),
            },
            is_admin,
        )
    }))
}

pub fn pair(first: i64, second: i64) -> (i64, i64) {
    (first.min(second), first.max(second))
}

pub fn row_message(row: &sqlx::sqlite::SqliteRow) -> sqlx::Result<Message> {
    let image = if let Some(content_type) = row.try_get::<Option<String>, _>("image_type")? {
        Some(ImageInfo {
            content_type,
            width: row.try_get("image_width")?,
            height: row.try_get("image_height")?,
            bytes: row.try_get("image_bytes")?,
        })
    } else {
        None
    };
    Ok(Message {
        id: row.try_get("id")?,
        sender: Member {
            id: row.try_get("sender_id")?,
            nickname: row.try_get("nickname")?,
            avatar: Avatar::from_storage(row.try_get("avatar")?),
        },
        recipient_id: row.try_get("recipient_id")?,
        body: row.try_get("body")?,
        sent_at: row.try_get("sent_at")?,
        image,
    })
}
