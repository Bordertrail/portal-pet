#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod network;
mod passive;
mod visits;

use std::{
    collections::HashSet,
    sync::Mutex,
    time::{Duration, Instant},
};

use portal_protocol::{ConversationEnd, Event};
#[cfg(not(target_os = "windows"))]
use tauri::LogicalPosition;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const PET: &str = "pet";
const PORTAL: &str = "portal";
const BUBBLE: &str = "bubble";
const CHAT: &str = "chat";
const WINDOW_NAMES: [&str; 4] = [PET, PORTAL, BUBBLE, CHAT];
const PORTAL_SIZE: (f64, f64) = (290.0, 140.0);
const BUBBLE_SIZE: (f64, f64) = (183.0, 55.0);
const DEFAULT_HOLD: Duration = Duration::from_secs(8);

type DesktopResult = Result<(), String>;

#[derive(Default)]
struct DesktopState {
    ready: HashSet<String>,
    hidden: bool,
    unread: HashSet<i64>,
    selected: Option<i64>,
    visits: visits::Visits,
}

struct Desktop {
    state: Mutex<DesktopState>,
    started: Instant,
    hold: Duration,
}

impl Desktop {
    fn new(hold: Duration) -> Self {
        Self {
            state: Mutex::new(DesktopState::default()),
            started: Instant::now(),
            hold,
        }
    }

    fn trace(&self, event: &str) {
        eprintln!("[{:>5} ms] {event}", self.started.elapsed().as_millis());
    }
}

fn run_desktop(
    app: &AppHandle,
    action: impl FnOnce(&AppHandle) -> DesktopResult + Send + 'static,
) -> DesktopResult {
    let app_for_action = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = action(&app_for_action) {
            eprintln!("{error}");
        }
    })
    .map_err(|error| error.to_string())
}

fn lookup(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    app.get_webview_window(label)
        .ok_or_else(|| format!("窗口尚未创建 {label}"))
}

fn show(app: &AppHandle, label: &str) -> DesktopResult {
    passive::show(&lookup(app, label)?)
}

fn show_pet(app: &AppHandle) -> DesktopResult {
    visits::show_own(app)
}

fn hide(app: &AppHandle, label: &str) -> DesktopResult {
    lookup(app, label)?
        .hide()
        .map_err(|error| error.to_string())
}

fn unread_indicator(app: &AppHandle, unread: bool) -> DesktopResult {
    app.emit_to(PET, "portal-unread", unread)
        .map_err(|error| error.to_string())
}

fn hide_all(app: &AppHandle) -> DesktopResult {
    visits::hide_windows(app)?;
    hide(app, PET)?;
    hide(app, CHAT)
}

fn toggle_hidden(app: &AppHandle) -> DesktopResult {
    let desktop = app.state::<Desktop>();
    let (hidden, ready, unread) = {
        let mut state = desktop.state.lock().unwrap();
        state.hidden = !state.hidden;
        (
            state.hidden,
            state.ready.len() == WINDOW_NAMES.len(),
            !state.unread.is_empty(),
        )
    };
    if hidden {
        hide_all(app)?;
        desktop.trace("hide all windows");
    } else if ready {
        unread_indicator(app, unread)?;
        show_pet(app)?;
        visits::show_guests(app)?;
        desktop.trace("restore robots");
    }
    Ok(())
}

fn open_chat(app: &AppHandle) -> DesktopResult {
    let desktop = app.state::<Desktop>();
    {
        let mut state = desktop.state.lock().unwrap();
        if state.ready.len() != WINDOW_NAMES.len() {
            return Ok(());
        }
        state.hidden = false;
    }
    show_pet(app)?;
    visits::show_guests(app)?;
    let chat = lookup(app, CHAT)?;
    chat.show().map_err(|error| error.to_string())?;
    chat.unminimize().map_err(|error| error.to_string())?;
    chat.set_focus().map_err(|error| error.to_string())?;
    app.emit_to(CHAT, "portal-chat-focus", ())
        .map_err(|error| error.to_string())?;
    desktop.trace("open chat from explicit click or menu");
    Ok(())
}

fn select_chat(app: &AppHandle, peer: Option<i64>) -> DesktopResult {
    let desktop = app.state::<Desktop>();
    let unread = {
        let mut state = desktop.state.lock().unwrap();
        state.selected = peer;
        if let Some(peer) = peer {
            state.unread.remove(&peer);
        }
        !state.unread.is_empty()
    };
    if let Some(peer) = peer {
        visits::read_peer(app, peer)?;
    }
    unread_indicator(app, unread)
}

#[tauri::command]
async fn choose_conversation(app: AppHandle, peer: Option<i64>) -> DesktopResult {
    run_desktop(&app, move |app| select_chat(app, peer))
}

#[tauri::command]
async fn open_arrival(app: AppHandle, generation: u64) -> DesktopResult {
    run_desktop(&app, move |app| visits::open_bubble(app, generation))
}

#[tauri::command]
async fn open_visitor(window: WebviewWindow) -> DesktopResult {
    let label = window.label().to_owned();
    run_desktop(window.app_handle(), move |app| {
        visits::open_guest(app, &label)
    })
}

#[tauri::command]
async fn robot_context(window: WebviewWindow) -> DesktopResult {
    let label = window.label().to_owned();
    run_desktop(window.app_handle(), move |app| {
        visits::context_menu(app, &label)
    })
}

fn handle_remote_event(app: &AppHandle, revision: u64, event: Event) {
    if let Err(error) = run_desktop(app, move |app| {
        let network = app.state::<network::Network>();
        if !network.current(revision) {
            return Ok(());
        }
        app.emit_to(CHAT, "portal-wire", &event)
            .map_err(|error| error.to_string())?;
        match event {
            Event::Message { message } => visits::receive(app, message),
            Event::ConversationEnded { ending } => {
                if let Some(me) = network.link().member {
                    visits::end(app, me.id, ending, None, true)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }) {
        eprintln!("{error}");
    }
}

fn reconcile_endings(
    app: &AppHandle,
    revision: u64,
    me: i64,
    endings: Vec<ConversationEnd>,
) -> DesktopResult {
    run_desktop(app, move |app| {
        if app.state::<network::Network>().current(revision) {
            for ending in endings {
                visits::end(app, me, ending, None, false)?;
            }
        }
        Ok(())
    })
}

fn reset_remote_arrival(app: &AppHandle) {
    let _ = run_desktop(app, visits::reset);
}

#[tauri::command]
async fn view_ready(window: WebviewWindow, desktop: State<'_, Desktop>) -> DesktopResult {
    if window.label().starts_with("visitor-") {
        let label = window.label().to_owned();
        return run_desktop(window.app_handle(), move |app| {
            visits::guest_ready(app, &label)
        });
    }
    if !WINDOW_NAMES.contains(&window.label()) {
        return Ok(());
    }
    let mut state = desktop.state.lock().unwrap();
    let added = state.ready.insert(window.label().to_owned());
    if added {
        desktop.trace(&format!("view ready: {}", window.label()));
    }
    if added && state.ready.len() == WINDOW_NAMES.len() {
        let should_show = !state.hidden;
        drop(state);
        if should_show {
            run_desktop(window.app_handle(), show_pet)?;
        }
        tauri::async_runtime::spawn(network::restore(window.app_handle().clone()));
        desktop.trace("ready");
    }
    Ok(())
}

#[tauri::command]
fn webview_warning(window: WebviewWindow, warning: String) {
    eprintln!("[{}] WebView: {warning}", window.label());
}

#[tauri::command]
async fn show_chat(app: AppHandle) -> DesktopResult {
    run_desktop(&app, open_chat)
}

#[tauri::command]
fn drag_pet(window: WebviewWindow, drag: PetDrag) -> DesktopResult {
    if window.label() != PET {
        return Err("只能拖动桌宠窗口".into());
    }
    if window
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .visits
        .own_is_kicking()
    {
        return Ok(());
    }
    let (offset_x, offset_y) = drag.grab_offset;
    #[cfg(not(target_os = "windows"))]
    let position = LogicalPosition::new(
        f64::from(drag.screen.0) - f64::from(offset_x),
        f64::from(drag.screen.1) - f64::from(offset_y),
    );
    #[cfg(target_os = "windows")]
    let position = {
        // The Windows cursor and window positions share physical desktop coordinates.
        let cursor = window
            .cursor_position()
            .map_err(|error| error.to_string())?;
        let monitor = window
            .monitor_from_point(cursor.x, cursor.y)
            .map_err(|error| error.to_string())?;
        let scale = monitor.map_or_else(
            || window.scale_factor().map_err(|error| error.to_string()),
            |monitor| Ok(monitor.scale_factor()),
        )?;
        PhysicalPosition::new(
            cursor.x - f64::from(offset_x) * scale,
            cursor.y - f64::from(offset_y) * scale,
        )
    };
    window
        .set_position(position)
        .map_err(|error| error.to_string())
}

#[derive(serde::Deserialize)]
struct PetDrag {
    #[cfg(not(target_os = "windows"))]
    screen: (i32, i32),
    grab_offset: (i32, i32),
}

fn create_webview(
    app: &AppHandle,
    label: &str,
    role: &str,
    (width, height): (f64, f64),
    visitor: Option<&str>,
) -> tauri::Result<WebviewWindow> {
    let chat = label == CHAT;
    let mut script = format!(
        "globalThis.portalView = '{role}';
         const portalWarning = warning => globalThis.__TAURI__?.core.invoke('webview_warning', {{ warning }});
         addEventListener('error', e => portalWarning(e.message || 'Asset failed to load'), true);
         addEventListener('unhandledrejection', e => portalWarning(String(e.reason)));"
    );
    if let Some(member) = visitor {
        script.push_str(&format!("globalThis.portalVisitor = {member};"));
    }
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .initialization_script(script)
        .on_page_load(|window, payload| {
            window.state::<Desktop>().trace(&format!(
                "view {} {:?}: {}",
                window.label(),
                payload.event(),
                payload.url()
            ));
        })
        .inner_size(width, height)
        .visible(false)
        .focused(false);
    if chat {
        builder = builder
            .title("桌宠聊天")
            .min_inner_size(530.0, 440.0)
            .center();
    } else {
        builder = builder
            .title("Portal Pet")
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .accept_first_mouse(true)
            .focusable(label != PORTAL);
    }
    builder.build()
}

fn setup_windows(app: &AppHandle) -> tauri::Result<()> {
    let monitor = app.primary_monitor()?.expect("需要一个桌面显示器");
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let px = |points: f64| (points * scale).round() as i32;
    let origin_x = (area.position.x + area.size.width as i32 - px(480.0)).max(area.position.x);
    let origin_y = (area.position.y + area.size.height as i32 - px(255.0)).max(area.position.y);
    for (label, (width, height)) in [
        (PORTAL, PORTAL_SIZE),
        (BUBBLE, BUBBLE_SIZE),
        (PET, (124.0, 90.0)),
        (CHAT, (660.0, 560.0)),
    ] {
        let window = create_webview(app, label, label, (width, height), None)?;
        if label == PET {
            window.set_position(PhysicalPosition::new(
                origin_x + px(313.0),
                origin_y + px(97.0),
            ))?;
        }
        if label == PORTAL {
            window.set_ignore_cursor_events(true)?;
        }
    }
    Ok(())
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let chat = MenuItem::with_id(app, "chat", "打开聊天", true, None::<&str>)?;
    let visibility = MenuItem::with_id(app, "visibility", "一键隐藏或恢复", true, None::<&str>)?;
    let own = MenuItem::with_id(app, "show-own", "显示我的机器人", true, None::<&str>)?;
    let guests = MenuItem::with_id(app, "show-guests", "显示好友机器人", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出桌宠", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&chat, &visibility, &own, &guests, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("Portal Pet")
        .menu(&menu)
        .show_menu_on_left_click(true);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn duration() -> Duration {
    std::env::var("PORTAL_PET_HOLD_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(|milliseconds: u64| Duration::from_millis(milliseconds.clamp(1000, 60_000)))
        .unwrap_or(DEFAULT_HOLD)
}

fn main() {
    let hide_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyH);
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.activate_ignoring_other_apps(false);
    builder
        .manage(Desktop::new(duration()))
        .manage(network::Network::new())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if *shortcut == hide_shortcut
                        && event.state() == ShortcutState::Pressed
                        && let Err(error) = run_desktop(app, toggle_hidden)
                    {
                        eprintln!("{error}");
                    }
                })
                .build(),
        )
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            visits::restore_settings(app.handle());
            setup_windows(app.handle())?;
            setup_tray(app.handle())?;
            match app.global_shortcut().register(hide_shortcut) {
                Ok(()) => app
                    .state::<Desktop>()
                    .trace(&format!("registered {hide_shortcut}")),
                Err(error) => {
                    eprintln!("快捷键 {hide_shortcut} 注册失败：{error}。可使用托盘菜单。")
                }
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref().to_owned();
            if let Err(error) = run_desktop(app, move |app| match id.as_str() {
                "chat" => open_chat(app),
                "visibility" => toggle_hidden(app),
                "quit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => visits::visibility_action(app, &id),
            }) {
                eprintln!("{error}");
            }
        })
        .on_window_event(|window, event| {
            if window.label() == CHAT
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                if let Err(error) = window.hide() {
                    eprintln!("{error}");
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            view_ready,
            show_chat,
            drag_pet,
            open_arrival,
            open_visitor,
            robot_context,
            choose_conversation,
            network::server_link,
            network::activate_server,
            network::disconnect_server,
            network::fetch_directory,
            network::change_nickname,
            network::change_avatar,
            network::create_account,
            network::add_friend,
            network::answer_friend,
            network::fetch_messages,
            network::send_message,
            network::end_conversation,
            webview_warning
        ])
        .run(tauri::generate_context!())
        .expect("无法启动桌宠原型");
}
