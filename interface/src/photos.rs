use std::{
    cell::OnceCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use leptos::{html, prelude::*};
use portal_protocol::{IMAGE_MAX_BYTES, ImageInfo, Message, SavedSticker};
use send_wrapper::SendWrapper;
use serde::Serialize;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    Blob, BlobPropertyBag, CanvasRenderingContext2d, File, FilePropertyBag, HtmlCanvasElement,
    HtmlImageElement, IntersectionObserver, IntersectionObserverEntry, IntersectionObserverInit,
    Url,
};

use super::{
    super::{invoke, invoke_with_options, request},
    Model,
};

pub(super) const IMAGE_ACCEPT: &str = "image/jpeg,image/png,image/webp,.jpg,.jpeg,.png,.webp";
pub(super) const MAX_ATTACHMENTS: usize = 9;

#[derive(Clone)]
pub(super) struct ImageDraft {
    pub file: File,
    pub client_id: String,
    prepared: SendWrapper<Rc<OnceCell<Blob>>>,
}

impl ImageDraft {
    pub fn new(file: File) -> Result<Self, &'static str> {
        if file.size() == 0.0 || file.size() > IMAGE_MAX_BYTES as f64 {
            return Err("每张图片必须小于或等于 8 MiB");
        }
        let name = file.name().to_ascii_lowercase();
        if !matches!(
            file.type_().as_str(),
            "image/jpeg" | "image/png" | "image/webp"
        ) && ![".jpg", ".jpeg", ".png", ".webp"]
            .iter()
            .any(|ext| name.ends_with(ext))
        {
            return Err("请选择 JPEG、PNG 或 WebP 图片");
        }
        Ok(Self {
            file,
            client_id: uuid::Uuid::new_v4().to_string(),
            prepared: SendWrapper::new(Rc::new(OnceCell::new())),
        })
    }

    pub fn can_reduce(&self) -> bool {
        let mime = self.file.type_();
        (mime == "image/jpeg"
            || (mime.is_empty() && {
                let name = self.file.name().to_ascii_lowercase();
                name.ends_with(".jpg") || name.ends_with(".jpeg")
            }))
            && self.file.size() > 1024.0 * 1024.0
    }

    async fn upload_blob(&self, original: bool) -> Blob {
        if let Some(prepared) = self.prepared.get() {
            return prepared.clone();
        }
        let blob: Blob = self.file.clone().into();
        let prepared = if !original && self.can_reduce() {
            reduce_jpeg(&blob).await.unwrap_or(blob)
        } else {
            blob
        };
        // Keep the chosen bytes for retries using the same message ID.
        let _ = self.prepared.set(prepared.clone());
        prepared
    }
}

async fn reduce_jpeg(original: &Blob) -> Option<Blob> {
    let image = HtmlImageElement::new().ok()?;
    let url = Url::create_object_url_with_blob(original).ok()?;
    image.set_src(&url);
    let decoded = JsFuture::from(image.decode()).await;
    let _ = Url::revoke_object_url(&url);
    decoded.ok()?;
    let (width, height) = (image.natural_width(), image.natural_height());
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 32_000_000
    {
        return None;
    }
    let scale = (1920.0 / f64::from(width.max(height))).min(1.0);
    let width = (f64::from(width) * scale).round().max(1.0) as u32;
    let height = (f64::from(height) * scale).round().max(1.0) as u32;
    let canvas = web_sys::window()?
        .document()?
        .create_element("canvas")
        .ok()?
        .dyn_into::<HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(width);
    canvas.set_height(height);
    let context = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<CanvasRenderingContext2d>()
        .ok()?;
    context
        .draw_image_with_html_image_element_and_dw_and_dh(
            &image,
            0.0,
            0.0,
            f64::from(width),
            f64::from(height),
        )
        .ok()?;
    let encoded = js_sys::Promise::new(&mut |resolve, reject| {
        if let Err(error) = canvas.to_blob_with_type_and_encoder_options(
            &resolve,
            "image/jpeg",
            &JsValue::from_f64(0.84),
        ) {
            let _ = reject.call1(&JsValue::UNDEFINED, &error);
        }
    });
    let smaller = JsFuture::from(encoded)
        .await
        .ok()?
        .dyn_into::<Blob>()
        .ok()?;
    (smaller.size() > 0.0
        && smaller.size() < original.size() * 0.9
        && smaller.type_() == "image/jpeg")
        .then_some(smaller)
}

#[component]
pub(super) fn DraftThumbnail(file: File) -> impl IntoView {
    let url = Url::create_object_url_with_blob(&file).ok();
    if let Some(url) = url.clone() {
        on_cleanup(move || {
            let _ = Url::revoke_object_url(&url);
        });
    }
    view! { <img src=url alt="" draggable="false"/> }
}

#[derive(Serialize)]
struct UploadOptions<'a> {
    headers: UploadHeaders<'a>,
}

#[derive(Serialize)]
struct UploadHeaders<'a> {
    #[serde(rename = "X-Portal-Peer")]
    peer: String,
    #[serde(rename = "X-Portal-Client-Id")]
    client_id: &'a str,
}

pub(super) async fn send(peer: i64, draft: &ImageDraft, original: bool) -> Result<Message, String> {
    let bytes = JsFuture::from(draft.upload_blob(original).await.array_buffer())
        .await
        .map_err(|_| "无法读取所选图片")?;
    let options = UploadOptions {
        headers: UploadHeaders {
            peer: peer.to_string(),
            client_id: &draft.client_id,
        },
    };
    let options = serde_wasm_bindgen::to_value(&options).map_err(|_| "无法发送图片")?;
    let result = JsFuture::from(invoke_with_options("send_image", bytes, options))
        .await
        .map_err(|error| error.as_string().unwrap_or_else(|| "图片发送失败".into()))?;
    serde_wasm_bindgen::from_value(result).map_err(|_| "无法读取发送结果".into())
}

#[derive(Clone)]
struct LoadedImage {
    url: String,
    blob: Blob,
}

async fn fetch(id: i64, info: &ImageInfo) -> Result<LoadedImage, String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        id: i64,
        expected_bytes: u32,
    }
    let args = serde_wasm_bindgen::to_value(&Args {
        id,
        expected_bytes: info.bytes,
    })
    .map_err(|_| "无法查询图片")?;
    let result = JsFuture::from(invoke("fetch_image", args))
        .await
        .map_err(|error| error.as_string().unwrap_or_else(|| "图片加载失败".into()))?;
    let bytes = js_sys::Uint8Array::new(&result);
    if bytes.length() != info.bytes || bytes.length() as usize > IMAGE_MAX_BYTES {
        return Err("图片数据不完整".into());
    }
    let options = BlobPropertyBag::new();
    options.set_type(&info.content_type);
    let blob = Blob::new_with_u8_array_sequence_and_options(&js_sys::Array::of1(&bytes), &options)
        .map_err(|_| "图片加载失败")?;
    let url = Url::create_object_url_with_blob(&blob).map_err(|_| "图片加载失败")?;
    Ok(LoadedImage { url, blob })
}

pub(super) async fn save_sticker(
    revision: u64,
    name: &str,
    image: &Blob,
) -> Result<SavedSticker, String> {
    #[derive(Serialize)]
    struct Options {
        headers: Headers,
    }
    #[derive(Serialize)]
    struct Headers {
        #[serde(rename = "X-Portal-Revision")]
        revision: String,
        #[serde(rename = "X-Portal-Sticker-Name")]
        name: String,
    }
    let name: String = name.chars().take(60).collect();
    let name = js_sys::encode_uri_component(&name)
        .as_string()
        .unwrap_or_default();
    let options = serde_wasm_bindgen::to_value(&Options {
        headers: Headers {
            revision: revision.to_string(),
            name,
        },
    })
    .map_err(|_| "无法保存表情")?;
    let bytes = JsFuture::from(image.array_buffer())
        .await
        .map_err(|_| "无法读取表情图片")?;
    let result = JsFuture::from(invoke_with_options("save_sticker", bytes, options))
        .await
        .map_err(|error| error.as_string().unwrap_or_else(|| "无法保存表情".into()))?;
    serde_wasm_bindgen::from_value(result).map_err(|_| "无法读取表情收藏".into())
}

#[derive(Serialize)]
pub(super) struct StickerArgs<'a> {
    pub revision: u64,
    pub id: &'a str,
}

pub(super) async fn sticker_file(revision: u64, entry: &SavedSticker) -> Result<File, String> {
    let args = serde_wasm_bindgen::to_value(&StickerArgs {
        revision,
        id: &entry.id,
    })
    .map_err(|_| "无法读取表情图片")?;
    let result = JsFuture::from(invoke("load_sticker", args))
        .await
        .map_err(|error| {
            error
                .as_string()
                .unwrap_or_else(|| "无法读取表情图片".into())
        })?;
    let bytes = js_sys::Uint8Array::new(&result);
    if bytes.length() != entry.bytes || bytes.length() as usize > IMAGE_MAX_BYTES {
        return Err("表情图片数据不完整".into());
    }
    let options = FilePropertyBag::new();
    options.set_type(&entry.content_type);
    File::new_with_u8_array_sequence_and_options(&js_sys::Array::of1(&bytes), &entry.name, &options)
        .map_err(|_| "无法读取表情图片".into())
}

pub(super) async fn remove_sticker(revision: u64, id: &str) -> Result<(), String> {
    request("remove_sticker", StickerArgs { revision, id }).await
}

struct VisibilityObserver {
    observer: IntersectionObserver,
    _callback: Closure<dyn FnMut(js_sys::Array, IntersectionObserver)>,
}

#[component]
pub(super) fn MessageImage(
    model: Model,
    id: i64,
    info: ImageInfo,
    history: NodeRef<html::Div>,
) -> impl IntoView {
    let target = NodeRef::<html::Div>::new();
    let dialog = NodeRef::<html::Dialog>::new();
    let attempts = RwSignal::new(0u32);
    let result = RwSignal::new_local(None::<Result<LoadedImage, String>>);
    let favorite = RwSignal::new(None::<bool>);
    let object_url = Arc::new(Mutex::new(None::<String>));
    let observer = Arc::new(Mutex::new(None::<SendWrapper<VisibilityObserver>>));
    let width = info.width.clamp(72, 250);
    let height = ((width as f64 * info.height as f64 / info.width.max(1) as f64).round() as u32)
        .clamp(48, 220);

    let visibility = observer.clone();
    target.on_load(move |element| {
        let callback = Closure::<dyn FnMut(js_sys::Array, IntersectionObserver)>::new(
            move |entries: js_sys::Array, observer: IntersectionObserver| {
                if entries.iter().any(|entry: JsValue| {
                    entry
                        .unchecked_into::<IntersectionObserverEntry>()
                        .is_intersecting()
                }) {
                    observer.disconnect();
                    attempts.try_set(1);
                }
            },
        );
        let options = IntersectionObserverInit::new();
        if let Some(list) = history.get_untracked() {
            options.set_root(Some(&list));
            options.set_root_margin("280px 0px");
        }
        match IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options) {
            Ok(observer) => {
                observer.observe(&element);
                *visibility.lock().unwrap() = Some(SendWrapper::new(VisibilityObserver {
                    observer,
                    _callback: callback,
                }));
            }
            Err(_) => attempts.set(1),
        }
    });

    let saved = object_url.clone();
    on_cleanup(move || {
        if let Some(observer) = observer.lock().unwrap().take() {
            observer.observer.disconnect();
        }
        if let Some(url) = saved.lock().unwrap().take() {
            let _ = Url::revoke_object_url(&url);
        }
    });

    Effect::new(move |_| {
        if attempts.get() == 0 {
            return;
        }
        result.set(None);
        let info = info.clone();
        let object_url = object_url.clone();
        spawn_local(async move {
            let loaded = fetch(id, &info).await;
            if let Ok(image) = &loaded {
                if result.is_disposed() {
                    let _ = Url::revoke_object_url(&image.url);
                    return;
                }
                *object_url.lock().unwrap() = Some(image.url.clone());
            }
            result.try_set(Some(loaded));
        });
    });

    view! {
        <div class="message-image" node_ref=target style:width=format!("{width}px") style:height=format!("{height}px")>
            {move || match result.get() {
                Some(Ok(image)) => view! {
                    <button type="button" class="photo-thumbnail" aria-label="查看大图"
                        on:click=move |_| if let Some(dialog) = dialog.get_untracked() { let _ = dialog.show_modal(); }>
                        <img src=image.url.clone() alt="聊天图片" draggable="false"/>
                    </button>
                    <button type="button" class="favorite-photo"
                        aria-label=move || if favorite.get() == Some(true) { "已收藏为表情" } else { "收藏为表情" }
                        title=move || if favorite.get() == Some(true) { "已收藏为表情" } else { "收藏为表情" }
                        disabled=move || favorite.get() == Some(false)
                        on:click={
                            let blob = image.blob;
                            move |_| {
                                favorite.set(Some(false));
                                let blob = blob.clone();
                                let revision = model.revision();
                                spawn_local(async move {
                                    let saved = save_sticker(revision, "聊天图片", &blob).await;
                                    if model.revision() != revision { return; }
                                    match saved {
                                        Ok(_) => {
                                            favorite.try_set(Some(true));
                                            model.stickers_changed.update(|value| *value += 1);
                                            set_timeout(move || { favorite.try_set(None); }, std::time::Duration::from_secs(2));
                                        }
                                        Err(error) => {
                                            favorite.try_set(None);
                                            model.error.set(error);
                                        }
                                    }
                                });
                            }
                        }>{move || match favorite.get() { Some(true) => "★", Some(false) => "…", None => "☆" }}</button>
                    <dialog class="photo-dialog" node_ref=dialog aria-label="聊天图片预览">
                        <button type="button" class="plain" aria-label="关闭图片预览"
                            on:click=move |_| if let Some(dialog) = dialog.get_untracked() { dialog.close(); }>"关闭"</button>
                        <img src=image.url alt="聊天图片" draggable="false"/>
                    </dialog>
                }.into_any(),
                Some(Err(error)) => view! {
                    <button type="button" class="photo-retry" title=error
                        on:click=move |_| attempts.update(|count| *count += 1)>"图片未加载，点此重试"</button>
                }.into_any(),
                None => view! { <span class="photo-loading">"图片加载中…"</span> }.into_any(),
            }}
        </div>
    }
}
