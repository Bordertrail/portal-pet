use js_sys::{Function, Promise, Reflect};
use leptos::prelude::*;
use portal_protocol::{Arrival, Avatar, Link, Member, VisitAction, VisitAnimation};
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::{JsCast, JsValue, closure::Closure, prelude::wasm_bindgen};
use wasm_bindgen_futures::{JsFuture, spawn_local};

#[path = "chat.rs"]
mod chat;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    fn invoke(command: &str, args: JsValue) -> Promise;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = listen)]
    fn listen(event: &str, callback: &Function) -> Promise;
}

fn command(name: &'static str) {
    spawn_local(async move {
        if let Err(error) = request::<_, ()>(name, ()).await {
            leptos::logging::error!("{name}: {error:?}");
        }
    });
}

async fn request<P: Serialize, R: DeserializeOwned>(name: &str, args: P) -> Result<R, String> {
    let args = serde_wasm_bindgen::to_value(&args).map_err(|_| "无法发送操作参数")?;
    let result = JsFuture::from(invoke(name, args)).await.map_err(|error| {
        error
            .as_string()
            .unwrap_or_else(|| "操作失败，请重试".into())
    })?;
    serde_wasm_bindgen::from_value(result).map_err(|_| "无法读取操作结果".into())
}

async fn listen_payload<T: DeserializeOwned + 'static>(
    event: &str,
    mut handler: impl FnMut(T) + 'static,
) -> Result<(), JsValue> {
    subscribe(event, move |event| {
        if let Ok(value) = Reflect::get(&event, &JsValue::from_str("payload"))
            && let Ok(payload) = serde_wasm_bindgen::from_value::<T>(value)
        {
            handler(payload);
        }
    })
    .await
}

async fn subscribe(event: &str, handler: impl FnMut(JsValue) + 'static) -> Result<(), JsValue> {
    let callback = Closure::<dyn FnMut(JsValue)>::new(handler);
    JsFuture::from(listen(event, callback.as_ref().unchecked_ref())).await?;
    // This listener is owned by its webview until the window is destroyed.
    callback.forget();
    Ok(())
}

fn window_role() -> String {
    web_sys::window()
        .and_then(|window| Reflect::get(&window, &JsValue::from_str("portalView")).ok())
        .and_then(|value| value.as_string())
        .unwrap_or_default()
}

pub fn app() -> impl IntoView {
    match window_role().as_str() {
        "pet" => view! { <Pet/> }.into_any(),
        "portal" => view! { <Portal/> }.into_any(),
        "visitor" => view! { <Visitor/> }.into_any(),
        "bubble" => view! { <Bubble/> }.into_any(),
        "chat" => view! { <chat::Chat/> }.into_any(),
        _ => ().into_any(),
    }
}

const ROBOT: [&str; 16] = [
    "..XX............",
    ".XaaX...........",
    "..XX...XXXXX....",
    "...XXXXhhhXX....",
    "...XhhhhhhhXX...",
    "..XXggggggggXX..",
    "..XbbbbbbbbbbX..",
    "..XbsssssbsbbX..",
    "..XbbbbbbbbbXX..",
    "...XXhhhhhXXX...",
    "...XgaaaaagX....",
    "...XgafafagX....",
    "...XgaafaagX....",
    "....XXXXXXXX....",
    "...XggX..XggX...",
    "...XXXX..XXXX...",
];

fn sprite_shadow(palette: u8) -> String {
    ROBOT
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.chars().enumerate().filter_map(move |(x, pixel)| {
                let color = match (palette, pixel) {
                    (_, 'X') => "#241d33",
                    (_, 'b') => "#3a2c53",
                    (_, 's') => "#a6f3d6",
                    (_, 'f') => "#705014",
                    (_, 'a') => "#f9d16f",
                    (0, 'g') => "#be5d43",
                    (0, 'h') => "#f2a879",
                    (1, 'g') => "#bd526d",
                    (1, 'h') => "#f596a8",
                    (2, 'g') => "#4389a7",
                    (2, 'h') => "#89cada",
                    (3, 'g') => "#8559b7",
                    (3, 'h') => "#c399e6",
                    _ => return None,
                };
                Some(format!("{}px {}px {color}", x * 4, y * 4))
            })
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Clone, Copy, Default)]
enum AvatarMotion {
    #[default]
    Still,
    Idle,
    Walk,
    Kick,
}

#[component]
fn Sprite(
    #[prop(default = 0)] palette: u8,
    #[prop(default = Avatar::Default)] avatar: Avatar,
    #[prop(default = AvatarMotion::Still)] motion: AvatarMotion,
) -> impl IntoView {
    if avatar != Avatar::Default {
        let source = format!("avatars/{}.png", avatar.as_str());
        let walking = matches!(motion, AvatarMotion::Walk);
        let kicking = matches!(motion, AvatarMotion::Kick);
        if walking || kicking {
            return view! {
                <span class="sprite avatar-layers"
                    class:avatar-walking=walking class:avatar-kicking=kicking aria-hidden="true">
                    <img class="avatar-sprite avatar-layer avatar-upper" src=source.clone() alt="" draggable="false"/>
                    <img class="avatar-sprite avatar-layer avatar-leg-left" src=source.clone() alt="" draggable="false"/>
                    <img class="avatar-sprite avatar-layer avatar-leg-right" src=source alt="" draggable="false"/>
                </span>
            }.into_any();
        }
        if matches!(motion, AvatarMotion::Idle) {
            return view! {
                <span class="sprite avatar-idle" aria-hidden="true">
                    <img class="avatar-sprite avatar-layer" src=source alt="" draggable="false"/>
                </span>
            }
            .into_any();
        }
        return view! {
            <img class="sprite avatar-sprite" src=source alt="" aria-hidden="true" draggable="false"/>
        }.into_any();
    }
    view! {
        <span class="sprite" aria-hidden="true">
            <span class="pixel-layer" style:box-shadow=sprite_shadow(palette)></span>
        </span>
    }
    .into_any()
}

#[component]
fn Pet() -> impl IntoView {
    #[derive(Serialize)]
    struct Drag {
        screen: (i32, i32),
        grab_offset: (i32, i32),
    }

    let unread = RwSignal::new(false);
    let avatar = RwSignal::new(Avatar::Default);
    let kick = RwSignal::new(None::<u64>);
    let button = NodeRef::<leptos::html::Button>::new();
    let press = StoredValue::new(None::<(i32, i32, i32)>);
    let dragged = StoredValue::new(false);
    let pointer_down = move |event: web_sys::PointerEvent| {
        if !event.is_primary() || event.button() != 0 {
            return;
        }
        dragged.set_value(false);
        let id = event.pointer_id();
        press.set_value(Some((id, event.client_x(), event.client_y())));
        if let Some(button) = button.get_untracked() {
            let _ = button.set_pointer_capture(id);
        }
    };
    let pointer_move = move |event: web_sys::PointerEvent| {
        const DRAG_THRESHOLD: i32 = 5;
        if let Some((id, x, y)) = press.get_value()
            && event.pointer_id() == id
            && event.buttons() & 1 != 0
            && (dragged.get_value()
                || (event.client_x() - x)
                    .abs()
                    .max((event.client_y() - y).abs())
                    >= DRAG_THRESHOLD)
        {
            dragged.set_value(true);
            let drag = Drag {
                screen: (event.screen_x(), event.screen_y()),
                grab_offset: (x, y),
            };
            spawn_local(async move {
                #[derive(Serialize)]
                struct Args {
                    drag: Drag,
                }
                if let Err(error) = request::<_, ()>("drag_pet", Args { drag }).await {
                    leptos::logging::error!("drag_pet: {error}");
                }
            });
        }
    };
    spawn_local(async move {
        let result = subscribe("portal-unread", move |event| {
            let payload = Reflect::get(&event, &JsValue::from_str("payload")).ok();
            unread.set(payload.and_then(|value| value.as_bool()).unwrap_or(false));
        })
        .await;
        if let Err(error) = result {
            leptos::logging::error!("portal-unread: {error:?}");
        }
        if let Err(error) =
            listen_payload("portal-own-kick", move |generation| kick.set(generation)).await
        {
            leptos::logging::error!("portal-own-kick: {error:?}");
        }
        if let Err(error) = listen_payload("portal-avatar", move |value| avatar.set(value)).await {
            leptos::logging::error!("portal-avatar: {error:?}");
        }
        if let Ok(link) = request::<_, Link>("server_link", ()).await {
            avatar.set(link.member.map_or(Avatar::Default, |member| member.avatar));
        }
        command("view_ready");
    });
    view! {
        <main class="pet">
            <button class="pet-character" aria-label="打开聊天" node_ref=button
                on:contextmenu=move |event| {
                    event.prevent_default();
                    command("robot_context");
                }
                on:pointerdown=pointer_down on:pointermove=pointer_move
                on:pointerup=move |_| press.set_value(None)
                on:pointercancel=move |_| press.set_value(None)
                on:click=move |event| {
                    if event.detail() == 0 || !dragged.get_value() {
                        command("show_chat");
                    }
                }>
                <For each=move || [kick.get()]
                    key=|generation| *generation
                    children=move |generation| {
                        let kicking = generation.is_some();
                        let motion = if kicking { AvatarMotion::Kick } else { AvatarMotion::Idle };
                        view! {
                            <span class="pet-figure" class:pet-kicking=kicking>
                                {move || view! { <Sprite avatar=avatar.get() motion=motion/> }}
                                <Show when=move || kicking && avatar.get() == Avatar::Default>
                                    <span class="kick-foot" aria-hidden="true"></span>
                                </Show>
                            </span>
                        }
                    }
                />
                <span class="unread" class:visible=move || unread.get() aria-hidden="true"></span>
            </button>
        </main>
    }
}

#[component]
fn Visitor() -> impl IntoView {
    let peer = web_sys::window()
        .and_then(|window| Reflect::get(&window, &JsValue::from_str("portalVisitor")).ok())
        .and_then(|value| serde_wasm_bindgen::from_value::<Member>(value).ok());
    let profile = RwSignal::new(peer);
    let label = move || {
        profile.with(|peer| {
            peer.as_ref().map_or_else(
                || "打开聊天".to_owned(),
                |member| format!("与{}聊天", member.nickname),
            )
        })
    };
    spawn_local(async move {
        if let Err(error) = listen_payload("portal-visitor-profile", move |member| {
            profile.set(Some(member))
        })
        .await
        {
            leptos::logging::error!("portal-visitor-profile: {error:?}");
        }
        command("view_ready");
    });
    view! {
        <button class="visitor-pet" title=label aria-label=label
            on:click=move |_| command("open_visitor")
            on:contextmenu=move |event| {
                event.prevent_default();
                command("robot_context");
            }>
            {move || {
                let peer = profile.get();
                let palette = friend_palette(peer.as_ref().map(|member| member.id));
                let avatar = peer.map_or(Avatar::Default, |member| member.avatar);
                view! { <Sprite palette=palette avatar=avatar motion=AvatarMotion::Idle/> }
            }}
        </button>
    }
}

#[component]
fn Portal() -> impl IntoView {
    let replay = RwSignal::new(None::<VisitAnimation>);
    spawn_local(async move {
        if let Err(error) = listen_payload("portal-motion", move |motion| replay.set(motion)).await
        {
            leptos::logging::error!("portal-motion: {error:?}");
        }
        command("view_ready");
    });
    view! {
        <main class="stage" aria-hidden="true">
            <For each=move || replay.get().into_iter()
                key=|motion| motion.generation
                children=move |motion| {
                    let arrive = motion.action == VisitAction::Arrive;
                    let kick = motion.action == VisitAction::KickAway;
                    let movement = if kick { AvatarMotion::Still } else { AvatarMotion::Walk };
                    view! {
                        <div class="visit-motion" class:arrival=arrive class:walk-away=!arrive && !kick class:kick-away=kick>
                            <div class="portal-ground"></div>
                            <div class="gate">
                                <div class="gate-light"></div>
                                <div class="gate-frame">
                                    <div class="gate-depth"><div class="gate-door"></div></div>
                                </div>
                                <span class="spark spark-a"></span>
                                <span class="spark spark-b"></span>
                                <span class="spark spark-c"></span>
                            </div>
                            <div class="visitor">
                                <Sprite palette=friend_palette(Some(motion.peer.id)) avatar=motion.peer.avatar motion=movement/>
                                {(!arrive).then(|| view! { <span class="farewell">"byebye"</span> })}
                            </div>
                        </div>
                    }
                }
            />
        </main>
    }
}

fn friend_palette(peer: Option<i64>) -> u8 {
    peer.map_or(1, |id| (id.rem_euclid(3) + 1) as u8)
}

#[derive(Serialize)]
struct OpenArrival {
    generation: u64,
}

#[component]
fn Bubble() -> impl IntoView {
    let arrival = RwSignal::new(None::<Arrival>);
    spawn_local(async move {
        if let Err(error) =
            listen_payload("portal-bubble", move |message| arrival.set(Some(message))).await
        {
            leptos::logging::error!("portal-bubble: {error:?}");
        }
        command("view_ready");
    });
    let open = move |_| {
        if let Some(message) = arrival.get_untracked() {
            spawn_local(async move {
                let _ = request::<_, ()>(
                    "open_arrival",
                    OpenArrival {
                        generation: message.generation,
                    },
                )
                .await;
            });
        }
    };
    view! {
        <button class="bubble" on:click=open aria-label="打开来信会话">
            <span>{move || arrival.get().map(|message| message.body).unwrap_or_default()}</span>
        </button>
    }
}
