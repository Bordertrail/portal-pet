use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    time::{Duration, Instant},
};

use portal_protocol::{Arrival, ConversationEnd, Member, Message, VisitAction, VisitAnimation};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, Monitor, PhysicalPosition,
    menu::{Menu, MenuItem},
};

use crate::{
    BUBBLE, BUBBLE_SIZE, CHAT, Desktop, DesktopResult, PET, PORTAL, PORTAL_SIZE, create_webview,
    hide, lookup, network::Network, run_desktop, select_chat, show, unread_indicator,
};

const MOTION_TIME: Duration = Duration::from_millis(1400);
const BUBBLE_DELAY: Duration = Duration::from_millis(740);
const BUBBLE_OFFSET_X: f64 = 139.0;
const GUEST_X: f64 = 157.0;
const GUEST_Y: f64 = 49.0;
const GUEST_SIZE: f64 = 72.0;
const KICKER_OFFSET: (f64, f64) = (193.0, 43.0);

#[derive(Default, Serialize, Deserialize)]
struct Visibility {
    own_hidden: bool,
    hidden_guests: HashMap<String, HashSet<i64>>,
}

#[derive(Clone, Copy)]
struct Spot {
    x: i32,
    y: i32,
    scale: f64,
}

impl Spot {
    fn offset(self, x: f64, y: f64) -> Self {
        Self {
            x: self.x + (x * self.scale).round() as i32,
            y: self.y + (y * self.scale).round() as i32,
            ..self
        }
    }

    fn put(self, window: &tauri::WebviewWindow) -> DesktopResult {
        // A hidden window can still use its old monitor's scale on macOS.
        #[cfg(target_os = "macos")]
        let position = tauri::LogicalPosition::new(
            f64::from(self.x) / self.scale,
            f64::from(self.y) / self.scale,
        );
        #[cfg(not(target_os = "macos"))]
        let position = PhysicalPosition::new(self.x, self.y);
        window
            .set_position(position)
            .map_err(|error| error.to_string())
    }

    fn between(self, target: Self, fraction: f64) -> Self {
        #[cfg(target_os = "macos")]
        if self.scale != target.scale {
            return if fraction < 0.5 { self } else { target };
        }
        let lerp = |start: i32, end: i32| {
            (f64::from(start) + (f64::from(end) - f64::from(start)) * fraction).round() as i32
        };
        Self {
            x: lerp(self.x, target.x),
            y: lerp(self.y, target.y),
            ..self
        }
    }
}

#[derive(Clone)]
struct Guest {
    member: Member,
    label: String,
    spot: Spot,
    last_message_id: i64,
    appeared: bool,
    ready: bool,
    pending_bubble: Option<Arrival>,
}

struct Motion {
    scene: VisitAnimation,
    spot: Spot,
    guest_label: Option<String>,
    own_home: Option<Spot>,
}

#[derive(Default)]
pub(super) struct Visits {
    serial: u64,
    guests: BTreeMap<i64, Guest>,
    endings: HashMap<i64, i64>,
    announced_endings: HashMap<i64, i64>,
    queue: VecDeque<Motion>,
    active: Option<Motion>,
    bubble: Option<Arrival>,
    visibility: Visibility,
}

impl Visits {
    pub(super) fn own_is_kicking(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|motion| motion.own_home.is_some())
    }

    fn next_id(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }

    fn guest_hidden(&self, scope: Option<&str>, peer: i64) -> bool {
        scope
            .and_then(|scope| self.visibility.hidden_guests.get(scope))
            .is_some_and(|hidden| hidden.contains(&peer))
    }
}

fn scope(app: &AppHandle) -> Option<String> {
    let link = app.state::<Network>().link();
    Some(format!("{}#{}", link.server?, link.member?.id))
}

fn visibility_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("robot-visibility.json"))
}

pub(super) fn restore_settings(app: &AppHandle) {
    let result = (|| -> Result<Visibility, String> {
        let path = visibility_path(app)?;
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Visibility::default()),
            Err(error) => Err(error.to_string()),
        }
    })();
    match result {
        Ok(settings) => {
            app.state::<Desktop>()
                .state
                .lock()
                .unwrap()
                .visits
                .visibility = settings
        }
        Err(error) => eprintln!("无法读取机器人显示设置：{error}"),
    }
}

fn save_settings(app: &AppHandle) -> DesktopResult {
    let path = visibility_path(app)?;
    let data = serde_json::to_vec(
        &app.state::<Desktop>()
            .state
            .lock()
            .unwrap()
            .visits
            .visibility,
    )
    .map_err(|e| e.to_string())?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, data).map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}

fn overlaps(a: Spot, b: Spot, width: f64, height: f64) -> bool {
    a.x < b.offset(width, 0.0).x
        && b.x < a.offset(width, 0.0).x
        && a.y < b.offset(0.0, height).y
        && b.y < a.offset(0.0, height).y
}

fn visit_location(app: &AppHandle) -> Result<Spot, String> {
    let pet = lookup(app, PET)?;
    let monitor: Monitor = pet
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or(app.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or("没有可用显示器")?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let px = |points: f64| (points * scale).round() as i32;
    let PhysicalPosition { x, y } = pet.outer_position().map_err(|e| e.to_string())?;
    let width = pet.outer_size().map_err(|e| e.to_string())?.width as i32;
    let left = area.position.x;
    let top = area.position.y + px(1.0);
    let right = left + area.size.width as i32;
    let bottom = area.position.y + area.size.height as i32;
    let max_x = (right - px(BUBBLE_OFFSET_X + BUBBLE_SIZE.0)).max(left);
    let max_y = (bottom - px(PORTAL_SIZE.1)).max(top);
    let left_room = x - left;
    let right_room = right - (x + width);
    let on_left = left_room >= px(PORTAL_SIZE.0 + 11.0) || left_room >= right_room;
    let preferred_x = if on_left {
        x - px(PORTAL_SIZE.0 + 11.0)
    } else {
        x + width + px(11.0)
    };
    let base = Spot {
        x: preferred_x.clamp(left, max_x),
        y: (y - px(37.0)).clamp(top, max_y),
        scale,
    };
    let occupied: Vec<_> = app
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .visits
        .guests
        .values()
        .map(|guest| guest.spot.offset(GUEST_X, GUEST_Y))
        .collect();
    let own = Spot { x, y, scale };
    let free = |spot: Spot| {
        let guest = spot.offset(GUEST_X, GUEST_Y);
        !overlaps(guest, own, 97.0, 84.0)
            && !occupied
                .iter()
                .any(|used| overlaps(guest, *used, GUEST_SIZE + 10.0, GUEST_SIZE + 10.0))
    };
    if free(base) {
        return Ok(base);
    }
    let step = px(GUEST_SIZE + 16.0).max(1) as usize;
    (top..=max_y)
        .step_by(step)
        .flat_map(|y| {
            (left..=max_x)
                .step_by(step)
                .map(move |x| Spot { x, y, scale })
        })
        .filter(|spot| free(*spot))
        .min_by_key(|spot| (spot.y - base.y).unsigned_abs() + (spot.x - base.x).unsigned_abs())
        .ok_or_else(|| "屏幕空间不足，无法放置更多好友机器人".into())
}

fn destroy(app: &AppHandle, label: &str) -> DesktopResult {
    if let Some(window) = app.get_webview_window(label) {
        window.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn build_guest(app: &AppHandle, guest: &Guest) -> DesktopResult {
    if app.get_webview_window(&guest.label).is_none() {
        let data = serde_json::to_string(&guest.member).map_err(|e| e.to_string())?;
        let window = create_webview(
            app,
            &guest.label,
            "visitor",
            (GUEST_SIZE, GUEST_SIZE),
            Some(&data),
        )
        .map_err(|e| e.to_string())?;
        guest.spot.offset(GUEST_X, GUEST_Y).put(&window)?;
    }
    Ok(())
}

fn reveal_guest(app: &AppHandle, peer: i64) -> DesktopResult {
    let account = scope(app);
    let guest = {
        let desktop = app.state::<Desktop>();
        let state = desktop.state.lock().unwrap();
        state
            .visits
            .guests
            .get(&peer)
            .filter(|guest| {
                guest.appeared
                    && !state.hidden
                    && !state.visits.guest_hidden(account.as_deref(), peer)
            })
            .cloned()
    };
    if let Some(guest) = guest {
        build_guest(app, &guest)?;
        if guest.ready {
            show(app, &guest.label)?;
        }
    }
    Ok(())
}

pub(super) fn guest_ready(app: &AppHandle, label: &str) -> DesktopResult {
    let peer = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        state
            .visits
            .guests
            .values_mut()
            .find(|guest| guest.label == label)
            .map(|guest| {
                guest.ready = true;
                guest.member.id
            })
    };
    if let Some(peer) = peer {
        reveal_guest(app, peer)
    } else {
        destroy(app, label)
    }
}

pub(super) fn sync_guest_profiles(app: &AppHandle, people: &[Member]) -> DesktopResult {
    let updates = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        people
            .iter()
            .filter_map(|member| {
                let guest = state.visits.guests.get_mut(&member.id)?;
                if guest.member == *member {
                    return None;
                }
                guest.member = member.clone();
                Some((guest.label.clone(), member))
            })
            .collect::<Vec<_>>()
    };
    for (label, member) in updates {
        app.emit_to(label, "portal-visitor-profile", member)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn dismiss_bubble(app: &AppHandle, peer: Option<i64>) -> DesktopResult {
    let dismiss = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        let found = state
            .visits
            .bubble
            .as_ref()
            .is_some_and(|arrival| peer.is_none_or(|peer| peer == arrival.peer.id));
        if found {
            state.visits.bubble = None;
        }
        found
    };
    if dismiss {
        hide(app, BUBBLE)?;
    }
    Ok(())
}

fn later(
    app: &AppHandle,
    delay: Duration,
    action: impl FnOnce(&AppHandle) -> DesktopResult + Send + 'static,
) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if let Err(error) = run_desktop(&app, action) {
            eprintln!("{error}");
        }
    });
}

fn play_own_kick(app: &AppHandle, generation: u64, home: Spot, beside_guest: Spot) {
    let app = app.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        const STEPS: u32 = 10;
        for (from, to, start_ms, travel_ms) in
            [(home, beside_guest, 0, 300), (beside_guest, home, 740, 420)]
        {
            for step in 1..=STEPS {
                let elapsed = Duration::from_millis(
                    start_ms + travel_ms * u64::from(step) / u64::from(STEPS),
                );
                std::thread::sleep((start + elapsed).saturating_duration_since(Instant::now()));
                let fraction = f64::from(step) / f64::from(STEPS);
                let eased = fraction * fraction * (3.0 - 2.0 * fraction);
                let position = from.between(to, eased);
                if let Err(error) = run_desktop(&app, move |app| {
                    let playing = app
                        .state::<Desktop>()
                        .state
                        .lock()
                        .unwrap()
                        .visits
                        .active
                        .as_ref()
                        .is_some_and(|motion| motion.scene.generation == generation);
                    if playing {
                        position.put(&lookup(app, PET)?)?;
                    }
                    Ok(())
                }) {
                    eprintln!("{error}");
                    return;
                }
            }
        }
    });
}

fn finish_own_kick(app: &AppHandle, home: Spot) -> DesktopResult {
    let position = home.put(&lookup(app, PET)?);
    let clear = app
        .emit_to(PET, "portal-own-kick", None::<u64>)
        .map_err(|e| e.to_string());
    position.and(clear)
}

fn show_bubble(app: &AppHandle, arrival: Arrival, spot: Spot) -> DesktopResult {
    spot.offset(BUBBLE_OFFSET_X, -1.0)
        .put(&lookup(app, BUBBLE)?)?;
    let generation = arrival.generation;
    app.emit_to(BUBBLE, "portal-bubble", &arrival)
        .map_err(|e| e.to_string())?;
    app.state::<Desktop>().state.lock().unwrap().visits.bubble = Some(arrival);
    show(app, BUBBLE)?;
    later(app, app.state::<Desktop>().hold, move |app| {
        let current = app
            .state::<Desktop>()
            .state
            .lock()
            .unwrap()
            .visits
            .bubble
            .as_ref()
            .is_some_and(|arrival| arrival.generation == generation);
        if current {
            dismiss_bubble(app, None)?;
        }
        Ok(())
    });
    Ok(())
}

fn begin_next(app: &AppHandle) -> DesktopResult {
    let account = scope(app);
    loop {
        let motion = {
            let desktop = app.state::<Desktop>();
            let mut state = desktop.state.lock().unwrap();
            if state.hidden || state.visits.active.is_some() {
                return Ok(());
            }
            state.visits.queue.pop_front()
        };
        let Some(mut motion) = motion else {
            return Ok(());
        };
        let peer = motion.scene.peer.id;
        let guest = app
            .state::<Desktop>()
            .state
            .lock()
            .unwrap()
            .visits
            .guests
            .get(&peer)
            .cloned();
        let hidden = app
            .state::<Desktop>()
            .state
            .lock()
            .unwrap()
            .visits
            .guest_hidden(account.as_deref(), peer);
        if motion.scene.action == VisitAction::Arrive {
            let Some(guest) =
                guest.filter(|guest| Some(&guest.label) == motion.guest_label.as_ref())
            else {
                continue;
            };
            if hidden {
                if let Some(live) = app
                    .state::<Desktop>()
                    .state
                    .lock()
                    .unwrap()
                    .visits
                    .guests
                    .get_mut(&peer)
                {
                    live.appeared = true;
                    live.pending_bubble = None;
                }
                continue;
            }
            build_guest(app, &guest)?;
        } else if hidden {
            continue;
        }
        let own_move = if motion.scene.action == VisitAction::KickAway {
            let pet = lookup(app, PET)?;
            if pet.is_visible().unwrap_or(false) {
                let PhysicalPosition { x, y } = pet.outer_position().map_err(|e| e.to_string())?;
                let scale = pet.scale_factor().map_err(|e| e.to_string())?;
                let home = Spot { x, y, scale };
                motion.own_home = Some(home);
                Some((home, motion.spot.offset(KICKER_OFFSET.0, KICKER_OFFSET.1)))
            } else {
                None
            }
        } else {
            None
        };
        motion.spot.put(&lookup(app, PORTAL)?)?;
        app.emit_to(PORTAL, "portal-motion", &motion.scene)
            .map_err(|e| e.to_string())?;
        show(app, PORTAL)?;
        let generation = motion.scene.generation;
        let incoming = motion.scene.action == VisitAction::Arrive;
        let guest_label = motion.guest_label.clone();
        app.state::<Desktop>().state.lock().unwrap().visits.active = Some(motion);
        later(app, MOTION_TIME, move |app| finish_motion(app, generation));
        if let Some((home, beside_guest)) = own_move {
            play_own_kick(app, generation, home, beside_guest);
            app.emit_to(PET, "portal-own-kick", generation)
                .map_err(|e| e.to_string())?;
        }
        if incoming {
            later(app, BUBBLE_DELAY, move |app| {
                let notice = {
                    let desktop = app.state::<Desktop>();
                    let mut state = desktop.state.lock().unwrap();
                    if state
                        .visits
                        .active
                        .as_ref()
                        .is_none_or(|motion| motion.scene.generation != generation)
                    {
                        return Ok(());
                    }
                    state
                        .visits
                        .guests
                        .get_mut(&peer)
                        .filter(|guest| Some(&guest.label) == guest_label.as_ref())
                        .and_then(|guest| {
                            guest
                                .pending_bubble
                                .take()
                                .map(|arrival| (arrival, guest.spot))
                        })
                };
                if let Some((arrival, spot)) = notice {
                    show_bubble(app, arrival, spot)?;
                }
                Ok(())
            });
        }
        return Ok(());
    }
}

fn finish_motion(app: &AppHandle, generation: u64) -> DesktopResult {
    let (own_home, incoming) = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        let Some(active) = state
            .visits
            .active
            .take_if(|motion| motion.scene.generation == generation)
        else {
            return Ok(());
        };
        let incoming = if active.scene.action == VisitAction::Arrive {
            let peer = active.scene.peer.id;
            state
                .visits
                .guests
                .get_mut(&peer)
                .filter(|guest| Some(&guest.label) == active.guest_label.as_ref())
                .map(|guest| {
                    guest.appeared = true;
                    (peer, guest.pending_bubble.take(), guest.spot)
                })
        } else {
            None
        };
        (active.own_home, incoming)
    };
    if let Some(home) = own_home
        && let Err(error) = finish_own_kick(app, home)
    {
        eprintln!("{error}");
    }
    if let Some((peer, arrival, spot)) = incoming {
        reveal_guest(app, peer)?;
        if let Some(arrival) = arrival {
            show_bubble(app, arrival, spot)?;
        }
    }
    hide(app, PORTAL)?;
    app.emit_to(PORTAL, "portal-motion", None::<VisitAnimation>)
        .map_err(|e| e.to_string())?;
    begin_next(app)
}

pub(super) fn receive(app: &AppHandle, message: Message) -> DesktopResult {
    let account = scope(app);
    let peer = message.sender.id;
    let existing = {
        let desktop = app.state::<Desktop>();
        let state = desktop.state.lock().unwrap();
        if state
            .visits
            .endings
            .get(&peer)
            .is_some_and(|ending| message.id <= *ending)
        {
            return Ok(());
        }
        state.visits.guests.get(&peer).map(|guest| guest.spot)
    };
    let spot = match existing {
        Some(spot) => spot,
        None => visit_location(app)?,
    };
    let chat_focused = lookup(app, CHAT)?.is_focused().unwrap_or(false);
    let bubble = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        if state
            .visits
            .guests
            .get(&peer)
            .is_some_and(|guest| message.id <= guest.last_message_id)
        {
            return Ok(());
        }
        let reading = chat_focused && state.selected == Some(peer);
        if !reading {
            state.unread.insert(peer);
        }
        let hidden = state.hidden || state.visits.guest_hidden(account.as_deref(), peer);
        let generation = state.visits.next_id();
        let notice = (!reading && !hidden).then(|| Arrival {
            generation,
            peer: message.sender.clone(),
            body: message.body,
        });
        if let Some(guest) = state.visits.guests.get_mut(&peer) {
            guest.last_message_id = message.id;
            if guest.appeared {
                notice.map(|arrival| (arrival, guest.spot))
            } else {
                guest.pending_bubble = notice;
                None
            }
        } else {
            let label = format!("visitor-{peer}-{generation}");
            state.visits.guests.insert(
                peer,
                Guest {
                    member: message.sender.clone(),
                    label: label.clone(),
                    spot,
                    last_message_id: message.id,
                    appeared: hidden,
                    ready: false,
                    pending_bubble: notice,
                },
            );
            if !hidden {
                state.visits.queue.push_back(Motion {
                    scene: VisitAnimation {
                        generation,
                        peer: message.sender,
                        action: VisitAction::Arrive,
                    },
                    spot,
                    guest_label: Some(label),
                    own_home: None,
                });
            }
            None
        }
    };
    let unread = !app
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .unread
        .is_empty();
    unread_indicator(app, unread)?;
    if let Some((arrival, spot)) = bubble {
        show_bubble(app, arrival, spot)?;
    }
    begin_next(app)
}

pub(super) fn end(
    app: &AppHandle,
    me: i64,
    end: ConversationEnd,
    member: Option<Member>,
    animate: bool,
) -> DesktopResult {
    let peer = end.other_id(me);
    let account = scope(app);
    let (guest, should_animate) = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        let previous = state.visits.endings.get(&peer).copied().unwrap_or(-1);
        let announced = state
            .visits
            .announced_endings
            .get(&peer)
            .copied()
            .unwrap_or(-1);
        if previous > end.through_message_id
            || (!animate && previous == end.through_message_id)
            || (animate && announced >= end.through_message_id)
        {
            return Ok(());
        }
        state.visits.endings.insert(peer, end.through_message_id);
        if state
            .visits
            .guests
            .get(&peer)
            .is_some_and(|guest| guest.last_message_id > end.through_message_id)
        {
            return Ok(());
        }
        if state.selected == Some(peer) {
            state.selected = None;
        }
        if animate {
            state
                .visits
                .announced_endings
                .insert(peer, end.through_message_id);
        }
        state
            .visits
            .queue
            .retain(|motion| motion.scene.peer.id != peer);
        let guest = state.visits.guests.remove(&peer);
        let allowed =
            animate && !state.hidden && !state.visits.guest_hidden(account.as_deref(), peer);
        (guest, allowed)
    };
    dismiss_bubble(app, Some(peer))?;
    if let Some(guest) = &guest {
        destroy(app, &guest.label)?;
    }
    if should_animate {
        let friend = guest
            .as_ref()
            .map(|guest| guest.member.clone())
            .or(member)
            .unwrap_or_else(|| end.ended_by.clone());
        let spot = match guest {
            Some(guest) => guest.spot,
            None => visit_location(app)?,
        };
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        let generation = state.visits.next_id();
        let action = if end.ended_by.id == me {
            VisitAction::KickAway
        } else {
            VisitAction::WalkAway
        };
        state.visits.queue.push_back(Motion {
            scene: VisitAnimation {
                generation,
                peer: friend,
                action,
            },
            spot,
            guest_label: None,
            own_home: None,
        });
    }
    begin_next(app)
}

pub(super) fn open_guest(app: &AppHandle, label: &str) -> DesktopResult {
    let member = app
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .visits
        .guests
        .values()
        .find(|guest| guest.label == label)
        .map(|guest| guest.member.clone());
    if let Some(member) = member {
        open_peer(app, member)?;
    }
    Ok(())
}

pub(super) fn open_bubble(app: &AppHandle, generation: u64) -> DesktopResult {
    let member = app
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .visits
        .bubble
        .as_ref()
        .filter(|arrival| arrival.generation == generation)
        .map(|arrival| arrival.peer.clone());
    if let Some(member) = member {
        open_peer(app, member)?;
    }
    Ok(())
}

fn open_peer(app: &AppHandle, member: Member) -> DesktopResult {
    select_chat(app, Some(member.id))?;
    crate::open_chat(app)?;
    app.emit_to(CHAT, "portal-open-visitor", member)
        .map_err(|e| e.to_string())
}

pub(super) fn read_peer(app: &AppHandle, peer: i64) -> DesktopResult {
    if let Some(guest) = app
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .visits
        .guests
        .get_mut(&peer)
    {
        guest.pending_bubble = None;
    }
    dismiss_bubble(app, Some(peer))
}

pub(super) fn show_own(app: &AppHandle) -> DesktopResult {
    let desktop = app.state::<Desktop>();
    let visible = {
        let state = desktop.state.lock().unwrap();
        !state.hidden && !state.visits.visibility.own_hidden
    };
    if visible {
        show(app, PET)
    } else {
        hide(app, PET)
    }
}

pub(super) fn hide_windows(app: &AppHandle) -> DesktopResult {
    let (labels, own_home) = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        state.visits.queue.clear();
        let own_home = state
            .visits
            .active
            .take()
            .and_then(|motion| motion.own_home);
        let labels = state
            .visits
            .guests
            .values_mut()
            .map(|guest| {
                guest.appeared = true;
                guest.pending_bubble = None;
                guest.label.clone()
            })
            .collect::<Vec<_>>();
        (labels, own_home)
    };
    if let Some(home) = own_home
        && let Err(error) = finish_own_kick(app, home)
    {
        eprintln!("{error}");
    }
    dismiss_bubble(app, None)?;
    hide(app, PORTAL)?;
    app.emit_to(PORTAL, "portal-motion", None::<VisitAnimation>)
        .map_err(|e| e.to_string())?;
    for label in labels {
        if let Some(window) = app.get_webview_window(&label) {
            window.hide().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn show_guests(app: &AppHandle) -> DesktopResult {
    let peers: Vec<_> = app
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .visits
        .guests
        .keys()
        .copied()
        .collect();
    for peer in peers {
        reveal_guest(app, peer)?;
    }
    Ok(())
}

pub(super) fn reset(app: &AppHandle) -> DesktopResult {
    hide_windows(app)?;
    let removed = {
        let desktop = app.state::<Desktop>();
        let mut state = desktop.state.lock().unwrap();
        state.visits.endings.clear();
        state.visits.announced_endings.clear();
        state.unread.clear();
        state.selected = None;
        std::mem::take(&mut state.visits.guests)
    };
    for guest in removed.values() {
        destroy(app, &guest.label)?;
    }
    unread_indicator(app, false)
}

pub(super) fn context_menu(app: &AppHandle, label: &str) -> DesktopResult {
    let window = lookup(app, label)?;
    let peer = app
        .state::<Desktop>()
        .state
        .lock()
        .unwrap()
        .visits
        .guests
        .values()
        .find(|guest| guest.label == label)
        .map(|guest| guest.member.id);
    let own = MenuItem::with_id(app, "hide-own", "隐藏我的机器人", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = if let Some(peer) = peer {
        let friend = MenuItem::with_id(
            app,
            format!("hide-guest-{peer}"),
            "隐藏此好友的机器人",
            true,
            None::<&str>,
        )
        .map_err(|e| e.to_string())?;
        Menu::with_items(app, &[&friend, &own])
    } else if label == PET {
        Menu::with_items(app, &[&own])
    } else {
        return Ok(());
    }
    .map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

pub(super) fn visibility_action(app: &AppHandle, action: &str) -> DesktopResult {
    match action {
        "hide-own" | "show-own" => {
            app.state::<Desktop>()
                .state
                .lock()
                .unwrap()
                .visits
                .visibility
                .own_hidden = action == "hide-own";
            show_own(app)?;
        }
        "show-guests" => {
            if let Some(account) = scope(app) {
                app.state::<Desktop>()
                    .state
                    .lock()
                    .unwrap()
                    .visits
                    .visibility
                    .hidden_guests
                    .remove(&account);
            }
            show_guests(app)?;
        }
        id => {
            let Some(peer) = id
                .strip_prefix("hide-guest-")
                .and_then(|id| id.parse::<i64>().ok())
            else {
                return Ok(());
            };
            let Some(account) = scope(app) else {
                return Ok(());
            };
            let label = {
                let desktop = app.state::<Desktop>();
                let mut state = desktop.state.lock().unwrap();
                state
                    .visits
                    .visibility
                    .hidden_guests
                    .entry(account)
                    .or_default()
                    .insert(peer);
                state.visits.guests.get_mut(&peer).map(|guest| {
                    guest.pending_bubble = None;
                    guest.label.clone()
                })
            };
            if let Some(label) = label.filter(|label| app.get_webview_window(label).is_some()) {
                hide(app, &label)?;
            }
            dismiss_bubble(app, Some(peer))?;
        }
    }
    save_settings(app)
}
