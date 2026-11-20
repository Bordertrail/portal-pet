use leptos::{ev, ev::SubmitEvent, html, prelude::*};
use portal_protocol::{
    Avatar, AvatarChange, ConversationEnd, Directory, Event, IssuedAccount, Link, Member, Message,
    NicknameChange, Person, Relationship, SECURITY_CODE_LENGTH, SendMessage,
};
use serde::Serialize;
use wasm_bindgen_futures::spawn_local;

use super::{Sprite, command, friend_palette, listen_payload, request};

#[derive(Serialize)]
struct Activation {
    code: String,
}

#[derive(Serialize)]
struct Peer {
    peer: i64,
}

#[derive(Serialize)]
struct EndingPeer {
    peer: Member,
}

#[derive(Serialize)]
struct Selection {
    peer: Option<i64>,
}

#[derive(Serialize)]
struct Answer {
    peer: i64,
    accept: bool,
}

#[derive(Serialize)]
struct Outgoing {
    peer: i64,
    message: SendMessage,
}

#[derive(Clone, Copy)]
struct Model {
    link: RwSignal<Link>,
    directory: RwSignal<Option<Directory>>,
    selected: RwSignal<Option<Member>>,
    history: RwSignal<Vec<Message>>,
    directory_request: RwSignal<u64>,
    draft: RwSignal<String>,
    retry: RwSignal<Option<(i64, String, String)>>,
    error: RwSignal<String>,
    ending: RwSignal<bool>,
    sending: RwSignal<bool>,
    members: RwSignal<bool>,
    settings: RwSignal<bool>,
}

impl Model {
    fn new() -> Self {
        Self {
            link: RwSignal::new(Link::default()),
            directory: RwSignal::new(None),
            selected: RwSignal::new(None),
            history: RwSignal::new(Vec::new()),
            directory_request: RwSignal::new(0),
            draft: RwSignal::new(String::new()),
            retry: RwSignal::new(None),
            error: RwSignal::new(String::new()),
            ending: RwSignal::new(false),
            sending: RwSignal::new(false),
            members: RwSignal::new(true),
            settings: RwSignal::new(false),
        }
    }

    fn apply_link(self, link: Link) {
        let previous = self.link.get_untracked();
        if link.revision < previous.revision {
            return;
        }
        if previous.revision != link.revision {
            self.directory.set(None);
            self.selected.set(None);
            self.history.set(Vec::new());
            self.retry.set(None);
            self.draft.set(String::new());
            self.error.set(String::new());
            self.ending.set(false);
            self.members.set(true);
            self.settings.set(false);
        }
        self.link.set(link);
        if previous.revision != self.revision() {
            self.refresh_directory();
        }
    }

    fn revision(self) -> u64 {
        self.link.with_untracked(|link| link.revision)
    }

    fn peer(self) -> Option<i64> {
        self.selected
            .with_untracked(|peer| peer.as_ref().map(|member| member.id))
    }

    fn refresh_directory(self) {
        if self.link.with_untracked(|link| link.member.is_none()) {
            return;
        }
        let revision = self.revision();
        let serial = self.directory_request.get_untracked() + 1;
        self.directory_request.set(serial);
        spawn_local(async move {
            let response = request::<_, Directory>("fetch_directory", ()).await;
            if self.revision() != revision || self.directory_request.get_untracked() != serial {
                return;
            }
            match response {
                Ok(data) => {
                    if let Some(peer) = self.peer()
                        && let Some(person) =
                            data.people.iter().find(|person| person.member.id == peer)
                    {
                        self.selected.set(Some(person.member.clone()));
                    }
                    self.directory.set(Some(data));
                }
                Err(error) => self.error.set(error),
            }
        });
    }

    fn nickname(self) -> String {
        self.directory
            .with(|directory| directory.as_ref().map(|data| data.me.nickname.clone()))
            .unwrap_or_else(|| {
                self.link.with(|link| {
                    link.member
                        .as_ref()
                        .map(|member| member.nickname.clone())
                        .unwrap_or_default()
                })
            })
    }

    fn avatar(self) -> Avatar {
        self.directory
            .with(|directory| directory.as_ref().map(|data| data.me.avatar))
            .or_else(|| {
                self.link
                    .with(|link| link.member.as_ref().map(|member| member.avatar))
            })
            .unwrap_or_default()
    }

    fn apply_profile(self, member: Member) {
        self.directory.update(|directory| {
            if let Some(directory) = directory {
                directory.me = member;
            }
        });
        self.refresh_directory();
    }

    fn conversation(self, member: Member) {
        self.settings.set(false);
        self.members.set(false);
        if self.peer() != Some(member.id) {
            self.history.set(Vec::new());
            self.draft.set(String::new());
            self.retry.set(None);
        }
        let peer = member.id;
        self.selected.set(Some(member));
        self.error.set(String::new());
        spawn_local(async move {
            let _ = request::<_, ()>("choose_conversation", Selection { peer: Some(peer) }).await;
        });
        self.refresh_history();
    }

    fn refresh_history(self) {
        let Some(peer) = self.peer() else { return };
        let revision = self.revision();
        spawn_local(async move {
            let response = request::<_, Vec<Message>>("fetch_messages", Peer { peer }).await;
            if self.revision() != revision || self.peer() != Some(peer) {
                return;
            }
            match response {
                Ok(history) => self.merge(history),
                Err(error) => self.error.set(error),
            }
        });
    }

    fn merge(self, messages: impl IntoIterator<Item = Message>) {
        self.history.update(|all| {
            all.extend(messages);
            all.sort_unstable_by_key(|message| message.id);
            all.dedup_by_key(|message| message.id);
        });
    }

    fn event(self, event: Event) {
        match event {
            Event::Ready | Event::Resync => {
                self.refresh_directory();
                self.refresh_history();
            }
            Event::DirectoryChanged => self.refresh_directory(),
            Event::Message { message } if self.peer() == Some(message.sender.id) => {
                self.merge([message])
            }
            Event::ConversationEnded { ending } => self.conversation_ended(&ending),
            _ => {}
        }
    }

    fn conversation_ended(self, end: &ConversationEnd) {
        let me = self
            .link
            .with_untracked(|link| link.member.as_ref().map(|member| member.id));
        if me.is_some_and(|me| self.peer() == Some(end.other_id(me)))
            && self.history.with_untracked(|messages| {
                messages
                    .last()
                    .is_none_or(|message| message.id <= end.through_message_id)
            })
        {
            self.leave();
        }
    }

    fn leave(self) {
        self.members.set(true);
        self.selected.set(None);
        self.history.set(Vec::new());
        self.draft.set(String::new());
        self.retry.set(None);
        spawn_local(async {
            let _ = request::<_, ()>("choose_conversation", Selection { peer: None }).await;
        });
    }

    fn end(self) {
        let Some(peer) = self.selected.get_untracked() else {
            return;
        };
        if self.ending.get_untracked() || self.sending.get_untracked() {
            return;
        }
        self.ending.set(true);
        self.error.set(String::new());
        let revision = self.revision();
        spawn_local(async move {
            let response =
                request::<_, ConversationEnd>("end_conversation", EndingPeer { peer }).await;
            if self.revision() != revision {
                return;
            }
            self.ending.set(false);
            match response {
                Ok(end) => self.conversation_ended(&end),
                Err(error) => self.error.set(error),
            }
        });
    }

    fn friend_action(self, peer: i64, accept: Option<bool>) {
        let revision = self.revision();
        self.error.set(String::new());
        spawn_local(async move {
            let result = match accept {
                Some(accept) => request::<_, ()>("answer_friend", Answer { peer, accept }).await,
                None => request::<_, ()>("add_friend", Peer { peer }).await,
            };
            if self.revision() != revision {
                return;
            }
            match result {
                Ok(()) => self.refresh_directory(),
                Err(error) => self.error.set(error),
            }
        });
    }

    fn send(self, form: SubmitEvent) {
        form.prevent_default();
        let body = self.draft.get_untracked().trim().to_owned();
        if self.sending.get_untracked() || self.ending.get_untracked() || body.is_empty() {
            return;
        }
        let Some(peer) = self.peer() else { return };
        let id = self
            .retry
            .get_untracked()
            .filter(|(old_peer, old_body, _)| *old_peer == peer && old_body == &body)
            .map_or_else(|| uuid::Uuid::new_v4().to_string(), |(_, _, id)| id);
        self.retry.set(Some((peer, body.clone(), id.clone())));
        self.sending.set(true);
        self.error.set(String::new());
        let revision = self.revision();
        spawn_local(async move {
            let response = request::<_, Message>(
                "send_message",
                Outgoing {
                    peer,
                    message: SendMessage {
                        client_id: id,
                        body: body.clone(),
                    },
                },
            )
            .await;
            self.sending.set(false);
            if self.revision() != revision || self.peer() != Some(peer) {
                return;
            }
            match response {
                Ok(message) => {
                    self.merge([message]);
                    self.retry.set(None);
                    if self.draft.with_untracked(|draft| draft.trim() == body) {
                        self.draft.set(String::new());
                    }
                }
                Err(error) => self.error.set(error),
            }
        });
    }
}

#[component]
pub fn Chat() -> impl IntoView {
    let model = Model::new();
    let input = NodeRef::<html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = input.get() {
            let _ = input.focus();
        }
    });
    spawn_local(async move {
        let listeners = [
            listen_payload("portal-link", move |link| model.apply_link(link)).await,
            listen_payload("portal-wire", move |event| model.event(event)).await,
            listen_payload("portal-open-visitor", move |member| {
                model.conversation(member)
            })
            .await,
            listen_payload("portal-chat-focus", move |_: ()| {
                if let Some(input) = input.get_untracked() {
                    let _ = input.focus();
                }
            })
            .await,
        ];
        for error in listeners.into_iter().filter_map(Result::err) {
            leptos::logging::error!("desktop events: {error:?}");
        }
        if let Ok(link) = request::<_, Link>("server_link", ()).await {
            model.apply_link(link);
        }
        command("view_ready");
    });

    view! {
        <Show when=move || model.link.with(|link| link.member.is_some())
            fallback=move || view! { <Login model=model/> }>
            <main class="chat">
                <header class="chat-header">
                    <div class="chat-avatar">{move || view! { <Sprite palette=1 avatar=model.avatar()/> }}</div>
                    <div class="identity">
                        <h1>{move || model.nickname()}</h1>
                        <p>{move || if model.link.with(|link| link.connected) { "已连接服务器" } else { "正在重连" }}</p>
                    </div>
                    <button class="plain" aria-pressed=move || model.settings.get().to_string()
                        on:click=move |_| {
                            model.error.set(String::new());
                            model.settings.update(|open| *open = !*open);
                        }>"设置"</button>
                    <button class="plain" on:click=move |_| {
                        spawn_local(async move {
                            if let Err(error) = request::<_, ()>("disconnect_server", ()).await { model.error.set(error); }
                        });
                    }>"退出登录"</button>
                </header>
                <Show when=move || !model.settings.get() fallback=move || view! { <ProfileSettings model=model/> }>
                    <Show when=move || model.members.get() fallback=move || view! {
                        <Conversation model=model input=input/>
                    }>
                        <DirectoryView model=model/>
                    </Show>
                </Show>
                <Show when=move || model.error.with(|error| !error.is_empty())
                    || model.link.with(|link| link.notice.is_some())>
                    <p class="notice" role="status">{move || {
                        let error = model.error.get();
                        if error.is_empty() { model.link.with(|link| link.notice.clone().unwrap_or_default()) }
                        else { error }
                    }}</p>
                </Show>
            </main>
        </Show>
    }
}

#[component]
fn ProfileSettings(model: Model) -> impl IntoView {
    let nickname = RwSignal::new(model.nickname());
    let busy = RwSignal::new(false);
    let success = RwSignal::new(false);
    let save = move |event: SubmitEvent| {
        event.prevent_default();
        let data = NicknameChange {
            nickname: nickname.get_untracked(),
        };
        if busy.get_untracked() || data.nickname.trim().is_empty() {
            return;
        }
        busy.set(true);
        success.set(false);
        model.error.set(String::new());
        let revision = model.revision();
        spawn_local(async move {
            let result = request::<_, Member>("change_nickname", data).await;
            busy.try_set(false);
            if model.revision() != revision {
                return;
            }
            match result {
                Ok(member) => {
                    nickname.try_set(member.nickname.clone());
                    model.apply_profile(member);
                    success.try_set(true);
                }
                Err(error) => model.error.set(error),
            }
        });
    };
    view! {
        <section class="profile-settings">
            <form on:submit=save>
                <label for="my-nickname">"我的昵称"</label>
                <div class="settings-fields">
                    <input id="my-nickname" type="text" maxlength="64" prop:value=move || nickname.get()
                        on:input=move |event| {
                            nickname.set(event_target_value(&event));
                            success.set(false);
                        }/>
                    <button class="primary" type="submit"
                        disabled=move || busy.get() || nickname.with(|name| name.trim().is_empty())>"保存"</button>
                </div>
                <Show when=move || success.get()><p role="status">"昵称已保存"</p></Show>
            </form>
            <AvatarPicker model=model/>
            <Show when=move || model.directory.with(|value| value.as_ref().is_some_and(|value| value.is_admin))>
                <AdminAccounts model=model/>
            </Show>
        </section>
    }
}

#[component]
fn AvatarPicker(model: Model) -> impl IntoView {
    let busy = RwSignal::new(false);
    let draft = RwSignal::new(None::<Avatar>);
    let selection = move || draft.get().unwrap_or_else(|| model.avatar());
    let save = move |event: SubmitEvent| {
        event.prevent_default();
        if busy.get_untracked() {
            return;
        }
        let Some(avatar) = draft.get_untracked() else {
            return;
        };
        let revision = model.revision();
        busy.set(true);
        model.error.set(String::new());
        spawn_local(async move {
            let result = request::<_, Member>("change_avatar", AvatarChange { avatar }).await;
            busy.try_set(false);
            if model.revision() != revision {
                return;
            }
            match result {
                Ok(member) => {
                    model.apply_profile(member);
                    draft.try_set(None);
                }
                Err(error) => model.error.set(error),
            }
        });
    };
    view! {
        <form on:submit=save>
            <label for="my-avatar">"我的形象"</label>
            <div class="settings-fields">
                <select id="my-avatar" prop:value=move || selection().as_str() disabled=move || busy.get()
                    on:change=move |event| draft.set(Some(Avatar::from_storage(&event_target_value(&event))))>
                    <option value="default">"默认机器人"</option>
                    <option value="blue">"蓝色机器人"</option>
                    <option value="purple">"紫色机器人"</option>
                </select>
                <button class="primary" disabled=move || busy.get() || selection() == model.avatar() type="submit">"保存形象"</button>
            </div>
        </form>
    }
}

#[component]
fn AdminAccounts(model: Model) -> impl IntoView {
    let nickname = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let issued = RwSignal::new(None::<IssuedAccount>);
    let create = move |event: SubmitEvent| {
        event.prevent_default();
        let data = NicknameChange {
            nickname: nickname.get_untracked(),
        };
        if busy.get_untracked() || data.nickname.trim().is_empty() {
            return;
        }
        busy.set(true);
        model.error.set(String::new());
        let revision = model.revision();
        spawn_local(async move {
            let result = request::<_, IssuedAccount>("create_account", data).await;
            busy.try_set(false);
            if model.revision() != revision {
                return;
            }
            match result {
                Ok(account) => {
                    nickname.try_set(String::new());
                    issued.try_set(Some(account));
                }
                Err(error) => model.error.set(error),
            }
        });
    };
    view! {
        <form on:submit=create>
            <label for="new-account-nickname">"新账号昵称"</label>
            <div class="settings-fields">
                <input id="new-account-nickname" type="text" maxlength="64" prop:value=move || nickname.get()
                    on:input=move |event| nickname.set(event_target_value(&event))/>
                <button class="primary" type="submit"
                    disabled=move || busy.get() || nickname.with(|name| name.trim().is_empty())>"创建账号"</button>
            </div>
            {move || issued.with(|value| value.as_ref().map(|account| view! {
                <div class="issued-account" role="status">
                    <span>{format!("{} 的安全码", account.member.nickname)}</span>
                    <code>{account.code.clone()}</code>
                </div>
            }))}
        </form>
    }
}

#[component]
fn Login(model: Model) -> impl IntoView {
    let code = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let input = NodeRef::<html::Input>::new();

    let enter_code = move |event: ev::Event| {
        if busy.get_untracked() {
            return;
        }
        let value = event_target_value(&event);
        let normalized: String = value
            .bytes()
            .filter(u8::is_ascii_alphanumeric)
            .map(|byte| char::from(byte.to_ascii_uppercase()))
            .collect();
        if normalized.len() > SECURITY_CODE_LENGTH {
            if let Some(input) = input.get_untracked() {
                input.set_value(&code.get_untracked());
            }
            model.error.set("请输入 6 位安全码".into());
            return;
        }
        if value != normalized
            && let Some(input) = input.get_untracked()
        {
            input.set_value(&normalized);
        }
        model.error.set(String::new());
        code.set(normalized.clone());
        if normalized.len() != SECURITY_CODE_LENGTH {
            return;
        }
        let activation = Activation { code: normalized };
        busy.set(true);
        spawn_local(async move {
            let result = request::<_, Link>("activate_server", activation).await;
            code.try_set(String::new());
            busy.try_set(false);
            match result {
                Ok(link) => model.apply_link(link),
                Err(error) => model.error.set(error),
            }
        });
    };
    let slots = (0..SECURITY_CODE_LENGTH)
        .map(|index| view! {
            <span class="security-slot" class:next=move || code.with(|value| value.len() == index)>
                {move || if code.with(|value| value.len() > index) { "•" } else { "" }}
            </span>
        })
        .collect_view();
    view! {
        <main class="login">
            <div class="login-card" aria-busy=move || busy.get().to_string()>
                <div class="login-robot"><Sprite/></div>
                <h1>"连接桌宠聊天"</h1>
                <label for="security-code">"6 位安全码"</label>
                <div class="security-slots">
                    <input id="security-code" class="security-input" type="password" autocomplete="off"
                        node_ref=input readonly=move || busy.get() aria-describedby="login-result"
                        prop:value=move || code.get() on:input=enter_code/>
                    <div class="security-cells" aria-hidden="true">{slots}</div>
                </div>
                <p id="login-result" class="login-result" role="status">{move || {
                    if busy.get() { return "正在验证…".to_owned(); }
                    let error = model.error.get();
                    if error.is_empty() { model.link.with(|link| link.notice.clone().unwrap_or_default()) }
                    else { error }
                }}</p>
            </div>
        </main>
    }
}

#[component]
fn DirectoryView(model: Model) -> impl IntoView {
    view! {
        <section class="directory">
            <div class="section-heading"><h2>"好友与在线成员"</h2>
                <button class="plain" on:click=move |_| model.refresh_directory()>"刷新"</button>
            </div>
            <Show when=move || model.directory.with(|directory| directory.as_ref().is_some_and(|d| d.people.is_empty()))>
                <p class="empty-chat">"其他受邀成员上线后会显示在这里。"</p>
            </Show>
            <For each=move || model.directory.with(|directory| directory.as_ref().map_or_else(Vec::new, |d| d.people.clone()))
                key=|person| (person.member.id, person.member.nickname.clone(), person.member.avatar, person.relationship, person.online)
                children=move |person| view! { <DirectoryRow model=model person=person/> }/>
        </section>
    }
}

#[component]
fn DirectoryRow(model: Model, person: Person) -> impl IntoView {
    let peer = person.member.id;
    let name = person.member.nickname.clone();
    let color = friend_palette(Some(peer));
    let avatar = person.member.avatar;
    let button = match person.relationship {
        Relationship::Friend => view! {
            <button class="primary small" data-action="chat" on:click=move |_| model.conversation(person.member.clone())>"聊天"</button>
        }.into_any(),
        Relationship::Incoming => view! {
            <button class="plain" data-action="reject" on:click=move |_| model.friend_action(peer,Some(false))>"拒绝"</button>
            <button class="primary small" data-action="accept" on:click=move |_| model.friend_action(peer,Some(true))>"同意"</button>
        }.into_any(),
        Relationship::Outgoing => view! { <span class="member-state">"等待同意"</span> }.into_any(),
        Relationship::None => view! {
            <button class="primary small" data-action="add" on:click=move |_| model.friend_action(peer,None)>"加好友"</button>
        }.into_any(),
    };
    view! {
        <div class="member-row" data-member-id=peer>
            <div class="member-avatar"><Sprite palette=color avatar=avatar/></div>
            <div class="member-info">
                <strong>{name}</strong>
                <span class="member-state">{format!("{} · #{peer}", if person.online { "在线" } else { "离线" })}</span>
            </div>
            <div class="member-actions">{button}</div>
        </div>
    }
}

#[component]
fn Conversation(model: Model, input: NodeRef<html::Input>) -> impl IntoView {
    view! {
        <section class="conversation">
            <div class="conversation-heading">
                <button class="plain" on:click=move |_| model.leave()>"返回成员"</button>
                <strong>{move || model.selected.with(|peer| peer.as_ref().map(|m| m.nickname.clone()).unwrap_or_default())}</strong>
                <button class="plain end-conversation" data-action="end" on:click=move |_| model.end()
                    disabled=move || model.ending.get() || model.sending.get()>
                    {move || if model.ending.get() { "正在结束…" } else { "结束聊天" }}
                </button>
            </div>
            <div class="chat-history" role="log" aria-label="聊天记录">
                <For each=move || model.history.get() key=|message| message.id
                    children=move |message| {
                        let mine = model.link.with_untracked(|link| link.member.as_ref().is_some_and(|me| me.id == message.sender.id));
                        view! { <div class="chat-row" class:mine=mine><p>{message.body}</p></div> }
                    }/>
            </div>
            <form class="compose" on:submit=move |event| model.send(event)>
                <input node_ref=input aria-label="回复消息" placeholder="输入消息…"
                    maxlength=2000 prop:value=move || model.draft.get()
                    on:input=move |event| model.draft.set(event_target_value(&event))/>
                <button type="submit" disabled=move || model.sending.get() || model.ending.get()
                    || model.draft.with(|text| text.trim().is_empty())>"发送"</button>
            </form>
        </section>
    }
}
