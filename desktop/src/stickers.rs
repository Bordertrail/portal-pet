use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use portal_protocol::{IMAGE_MAX_BYTES, SavedSticker};
use sha2::{Digest, Sha256};
use tauri::{Manager, WebviewWindow, ipc};

use crate::{CHAT, network::Network};

const MAX_STICKERS: usize = 80;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_HEADER_BYTES: u64 = 2048;

#[derive(Default)]
pub struct Shelf(Mutex<()>);

const STORAGE_ERROR: &str = "无法读写表情收藏";

fn collection(window: &WebviewWindow, revision: u64) -> Result<PathBuf, String> {
    if window.label() != CHAT {
        return Err("请在聊天窗口操作".into());
    }
    let (server, member) = window.state::<Network>().sticker_account(revision)?;
    let scope = format!("{:x}-{member}", Sha256::digest(server.as_bytes()));
    Ok(window
        .path()
        .app_data_dir()
        .map_err(|_| STORAGE_ERROR)?
        .join("stickers")
        .join(scope))
}

fn valid_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_entry(path: &Path) -> Option<(SavedSticker, BufReader<File>)> {
    let id = path.file_name()?.to_str()?;
    let file = File::open(path).ok()?;
    let total = file.metadata().ok()?.len();
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    (&mut reader)
        .take(MAX_HEADER_BYTES)
        .read_until(b'\n', &mut line)
        .ok()?;
    if line.last() != Some(&b'\n') {
        return None;
    }
    let entry: SavedSticker = serde_json::from_slice(&line).ok()?;
    (entry.id == id
        && total == line.len() as u64 + u64::from(entry.bytes)
        && (1..=IMAGE_MAX_BYTES as u32).contains(&entry.bytes))
    .then_some((entry, reader))
}

fn entries(dir: &Path) -> Result<Vec<SavedSticker>, String> {
    let files = match fs::read_dir(dir) {
        Ok(files) => files,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(STORAGE_ERROR.into()),
    };
    let mut records = Vec::new();
    for file in files {
        let file = file.map_err(|_| STORAGE_ERROR)?;
        if file.file_name().to_str().is_some_and(valid_id)
            && file.file_type().is_ok_and(|kind| kind.is_file())
            && let Some((entry, _)) = read_entry(&file.path())
        {
            records.push((file.metadata().and_then(|data| data.modified()).ok(), entry));
        }
    }
    records.sort_unstable_by(|(left_time, left), (right_time, right)| {
        right_time
            .cmp(left_time)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(records.into_iter().map(|(_, entry)| entry).collect())
}

#[tauri::command]
pub async fn list_stickers(
    window: WebviewWindow,
    revision: u64,
) -> Result<Vec<SavedSticker>, String> {
    let dir = collection(&window, revision)?;
    let shelf = window.state::<Shelf>();
    let _guard = shelf.0.lock().map_err(|_| STORAGE_ERROR)?;
    entries(&dir)
}

#[tauri::command]
pub async fn save_sticker(
    window: WebviewWindow,
    request: ipc::Request<'_>,
) -> Result<SavedSticker, String> {
    let revision = request
        .headers()
        .get("x-portal-revision")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .ok_or("无效的会话")?;
    let dir = collection(&window, revision)?;
    let ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("无法读取表情图片".into());
    };
    let name = request
        .headers()
        .get("x-portal-sticker-name")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            url::form_urlencoded::parse(format!("name={value}").as_bytes())
                .next()
                .map(|(_, name)| {
                    name.trim()
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(60)
                        .collect()
                })
                .unwrap_or_default()
        })
        .filter(|name: &String| !name.is_empty())
        .unwrap_or_else(|| "聊天图片".into());
    let shelf = window.state::<Shelf>();
    let _guard = shelf.0.lock().map_err(|_| STORAGE_ERROR)?;
    store_image(&dir, name, bytes)
}

fn store_image(dir: &Path, name: String, bytes: &[u8]) -> Result<SavedSticker, String> {
    if bytes.is_empty() || bytes.len() > IMAGE_MAX_BYTES {
        return Err("表情图片不能超过 8 MiB".into());
    }
    let content_type = match imagesize::image_type(bytes) {
        Ok(imagesize::ImageType::Jpeg) => "image/jpeg",
        Ok(imagesize::ImageType::Png) => "image/png",
        Ok(imagesize::ImageType::Webp) => "image/webp",
        _ => return Err("请选择 JPEG、PNG 或 WebP 图片".into()),
    };
    let size = imagesize::blob_size(bytes).map_err(|_| "表情图片无法读取")?;
    if size.width == 0
        || size.height == 0
        || size.width > 8192
        || size.height > 8192
        || size.width * size.height > 32_000_000
    {
        return Err("图片尺寸过大或无效".into());
    }
    let id = format!("{:x}", Sha256::digest(bytes));
    let destination = dir.join(&id);
    if let Some((entry, _)) = read_entry(&destination) {
        return Ok(entry);
    }
    let saved = entries(dir)?;
    let total: usize = saved.iter().map(|entry| entry.bytes as usize).sum();
    if saved.len() >= MAX_STICKERS || total.saturating_add(bytes.len()) > MAX_TOTAL_BYTES {
        return Err("表情收藏已满，请移除一些表情后再试".into());
    }
    let entry = SavedSticker {
        id,
        name,
        content_type: content_type.into(),
        bytes: bytes.len() as u32,
    };
    fs::create_dir_all(dir).map_err(|_| STORAGE_ERROR)?;
    let mut temporary = tempfile::NamedTempFile::new_in(dir).map_err(|_| STORAGE_ERROR)?;
    serde_json::to_writer(&mut temporary, &entry).map_err(|_| STORAGE_ERROR)?;
    temporary
        .write_all(b"\n")
        .and_then(|()| temporary.write_all(bytes))
        .map_err(|_| STORAGE_ERROR)?;
    temporary.persist(destination).map_err(|_| STORAGE_ERROR)?;
    Ok(entry)
}

#[tauri::command]
pub async fn load_sticker(
    window: WebviewWindow,
    revision: u64,
    id: String,
) -> Result<ipc::Response, String> {
    if !valid_id(&id) {
        return Err("无效的表情".into());
    }
    let path = collection(&window, revision)?.join(id);
    let shelf = window.state::<Shelf>();
    let _guard = shelf.0.lock().map_err(|_| STORAGE_ERROR)?;
    let (entry, mut reader) = read_entry(&path).ok_or("找不到这个收藏表情")?;
    let mut bytes = vec![0; entry.bytes as usize];
    reader.read_exact(&mut bytes).map_err(|_| STORAGE_ERROR)?;
    Ok(ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn remove_sticker(
    window: WebviewWindow,
    revision: u64,
    id: String,
) -> Result<(), String> {
    if !valid_id(&id) {
        return Err("无效的表情".into());
    }
    let path = collection(&window, revision)?.join(id);
    let shelf = window.state::<Shelf>();
    let _guard = shelf.0.lock().map_err(|_| STORAGE_ERROR)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(STORAGE_ERROR.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_persist_and_duplicate_data_keeps_one_entry() {
        const PNG: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 4, 0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 252, 255, 31,
            0, 3, 3, 2, 0, 239, 162, 167, 91, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        let scratch = tempfile::tempdir().unwrap();
        let directory = scratch.path().join("first-account");
        let entry = store_image(&directory, "星星".into(), PNG).unwrap();
        assert_eq!(
            store_image(&directory, "复制品".into(), PNG).unwrap(),
            entry
        );
        assert!(store_image(&directory, "无效图片".into(), b"<svg/>").is_err());
        assert_eq!(entries(&directory).unwrap(), [entry.clone()]);
        let path = directory.join(entry.id);
        let (_, mut reader) = read_entry(&path).unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, PNG);
        assert!(
            entries(&scratch.path().join("second-account"))
                .unwrap()
                .is_empty()
        );
        drop(reader);
        fs::remove_file(path).unwrap();
        assert!(entries(&directory).unwrap().is_empty());
    }
}
