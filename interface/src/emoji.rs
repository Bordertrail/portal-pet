use leptos::{html, prelude::*};
use portal_protocol::SavedSticker;
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

use super::{Model, photos, request};

type Emoji = (&'static str, &'static str);

const EMOJI: &[(&str, &[Emoji])] = &[
    (
        "笑脸",
        &[
            ("😀", "开心 笑脸 smile"),
            ("😃", "大笑"),
            ("😄", "笑眯眯"),
            ("😁", "咧嘴"),
            ("😆", "笑出声"),
            ("😂", "笑哭 lol"),
            ("🤣", "笑翻 rofl"),
            ("😊", "微笑 害羞"),
            ("🙂", "微笑"),
            ("🙃", "倒脸"),
            ("😉", "眨眼 wink"),
            ("😌", "松口气"),
            ("🥰", "喜欢 爱心"),
            ("😍", "心动 花痴 love"),
            ("😘", "飞吻 kiss"),
            ("😋", "好吃"),
            ("😜", "调皮 吐舌"),
            ("🤪", "鬼脸"),
            ("🤭", "捂嘴偷笑"),
            ("🤫", "嘘 保密"),
            ("🤔", "思考 疑问 think"),
            ("🙄", "白眼"),
            ("😏", "坏笑"),
            ("😒", "嫌弃"),
            ("🥺", "可怜 撒娇"),
            ("🥹", "感动"),
            ("😢", "伤心 流泪 sad"),
            ("😭", "大哭 cry"),
            ("😤", "生气"),
            ("😡", "愤怒 angry"),
            ("😱", "惊吓"),
            ("😳", "脸红"),
            ("😅", "尴尬 汗"),
            ("😮", "惊讶 wow"),
            ("🥱", "打哈欠"),
            ("😴", "睡觉 晚安 sleep"),
            ("🤯", "震惊"),
            ("😵‍💫", "晕"),
            ("🥳", "庆祝 party"),
            ("😎", "墨镜 酷 cool"),
            ("🤩", "星星眼"),
            ("🫠", "融化"),
            ("🤗", "抱抱 hug"),
            ("🫢", "捂嘴 惊讶"),
            ("🤖", "机器人 桌宠 robot"),
            ("👻", "幽灵"),
            ("👀", "围观 眼睛"),
            ("💤", "困 晚安"),
        ],
    ),
    (
        "手势",
        &[
            ("👋", "打招呼 再见 hello bye"),
            ("🤚", "举手"),
            ("✋", "击掌 停"),
            ("👌", "好的 ok"),
            ("👍", "点赞 同意 赞 like"),
            ("👎", "不赞同"),
            ("✌️", "胜利 耶"),
            ("🤞", "好运"),
            ("🤟", "爱你"),
            ("🤘", "摇滚"),
            ("🤙", "打电话"),
            ("👊", "碰拳"),
            ("✊", "拳头"),
            ("🤝", "握手 成交"),
            ("👏", "鼓掌 棒 clap"),
            ("🙌", "欢呼 双手"),
            ("🙏", "谢谢 拜托 感恩 thanks"),
            ("💪", "加油 力量"),
            ("🫶", "比心 爱心"),
            ("🫰", "比心 手指"),
            ("👉", "向右"),
            ("👈", "向左"),
            ("👆", "向上"),
            ("👇", "向下"),
            ("🤦", "扶额"),
            ("🤷", "摊手"),
            ("🙋", "举手"),
            ("🫡", "敬礼 收到"),
        ],
    ),
    (
        "爱心",
        &[
            ("❤️", "红心 爱你 heart love"),
            ("🧡", "橙心"),
            ("💛", "黄心"),
            ("💚", "绿心"),
            ("💙", "蓝心"),
            ("💜", "紫心"),
            ("🩷", "粉心"),
            ("🤍", "白心"),
            ("🖤", "黑心"),
            ("💔", "心碎"),
            ("❤️‍🔥", "热爱"),
            ("💕", "两颗心"),
            ("💖", "闪亮的心"),
            ("💗", "心动"),
            ("💘", "一箭穿心"),
            ("💌", "情书 信封"),
            ("💋", "唇印 亲亲"),
            ("🌹", "玫瑰"),
            ("💐", "花束"),
            ("💍", "戒指"),
        ],
    ),
    (
        "动物",
        &[
            ("🐶", "小狗 dog"),
            ("🐱", "小猫 cat"),
            ("🐭", "老鼠"),
            ("🐹", "仓鼠"),
            ("🐰", "兔子"),
            ("🦊", "狐狸"),
            ("🐻", "小熊"),
            ("🐼", "熊猫 panda"),
            ("🐨", "考拉"),
            ("🐯", "老虎"),
            ("🐸", "青蛙"),
            ("🐵", "猴子"),
            ("🙈", "捂眼 不看"),
            ("🙉", "捂耳 不听"),
            ("🙊", "捂嘴 不说"),
            ("🐔", "小鸡"),
            ("🐧", "企鹅"),
            ("🐣", "破壳"),
            ("🦆", "鸭子"),
            ("🐢", "乌龟"),
            ("🐝", "蜜蜂"),
            ("🦋", "蝴蝶"),
            ("🦄", "独角兽"),
            ("🐾", "爪印"),
        ],
    ),
    (
        "日常",
        &[
            ("☀️", "太阳 早安"),
            ("🌙", "月亮 晚安"),
            ("⭐", "星星"),
            ("✨", "闪闪"),
            ("🌈", "彩虹"),
            ("☁️", "云"),
            ("🌧️", "下雨"),
            ("☃️", "雪人"),
            ("🌱", "发芽 桌宠"),
            ("🌻", "向日葵"),
            ("🌸", "樱花"),
            ("🍀", "幸运 四叶草"),
            ("☕", "咖啡 coffee"),
            ("🧋", "奶茶"),
            ("🍵", "茶"),
            ("🥤", "饮料"),
            ("🍻", "干杯"),
            ("🎂", "蛋糕 生日 birthday"),
            ("🍰", "甜点"),
            ("🍓", "草莓"),
            ("🍉", "西瓜"),
            ("🍟", "薯条"),
            ("🍕", "披萨"),
            ("🍜", "面条"),
            ("🍚", "米饭"),
            ("🍦", "冰淇淋"),
            ("🎁", "礼物 gift"),
            ("🎉", "庆祝 恭喜"),
            ("🎊", "庆祝 彩带"),
            ("🎈", "气球"),
            ("📷", "照片 相机"),
            ("✉️", "信封 消息"),
            ("🎵", "音乐"),
            ("🎮", "游戏"),
            ("🔥", "火 热门 fire"),
            ("💯", "满分"),
            ("✅", "完成 对勾"),
            ("❌", "错误 叉号"),
            ("❓", "问号"),
            ("❗", "感叹号"),
            ("⏰", "闹钟"),
            ("🚪", "门 传送门"),
        ],
    ),
];

const EMOJI_LIMIT: usize = 80;
const RECENT_LIMIT: usize = 32;
const STICKERS_PER_PAGE: usize = 12;

#[derive(Clone, Default, Serialize, Deserialize)]
struct EmojiHistory {
    recent: Vec<String>,
    favorites: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Recent,
    Favorites,
    Group(usize),
}

fn emoji_info(glyph: &str) -> Option<Emoji> {
    EMOJI
        .iter()
        .flat_map(|(_, group)| group.iter().copied())
        .find(|entry| entry.0 == glyph)
}

fn account_storage(model: Model) -> Option<(web_sys::Storage, String)> {
    let link = model.link.get_untracked();
    let key = format!("portal.emoji.v1.{}.{}", link.server?, link.member?.id);
    Some((web_sys::window()?.local_storage().ok()??, key))
}

fn load_history(model: Model) -> (Tab, EmojiHistory) {
    let history = account_storage(model)
        .and_then(|(storage, key)| storage.get_item(&key).ok().flatten())
        .and_then(|json| serde_json::from_str::<EmojiHistory>(&json).ok())
        .unwrap_or_default();
    let tab = if history.recent.is_empty() {
        Tab::Group(0)
    } else {
        Tab::Recent
    };
    (tab, history)
}

fn choose_emoji(model: Model, input: NodeRef<html::Input>, glyph: &str) -> bool {
    let Some(field) = input.get_untracked() else {
        return false;
    };
    let current = field.value();
    let end = current.encode_utf16().count() as u32;
    let start_selection = field.selection_start().ok().flatten().unwrap_or(end);
    let end_selection = field.selection_end().ok().flatten().unwrap_or(end);
    let byte_offset = |units: u32| {
        let mut passed = 0;
        for (offset, character) in current.char_indices() {
            if passed >= units {
                return offset;
            }
            passed += character.len_utf16() as u32;
        }
        current.len()
    };
    let (start, finish) = (byte_offset(start_selection), byte_offset(end_selection));
    let text = format!("{}{glyph}{}", &current[..start], &current[finish..]);
    if text.chars().count() > 2000 {
        model.error.set("消息最多 2000 个字符".into());
        return false;
    }
    field.set_value(&text);
    model.draft.set(text);
    model.error.set(String::new());
    let _ = field.focus();
    let caret = start_selection + glyph.encode_utf16().count() as u32;
    let _ = field.set_selection_range(caret, caret);
    true
}

#[component]
pub(super) fn EmojiPanel(
    model: Model,
    input: NodeRef<html::Input>,
    open: RwSignal<bool>,
) -> impl IntoView {
    let search = RwSignal::new(String::new());
    let (initial_tab, initial_history) = load_history(model);
    let tab = RwSignal::new(initial_tab);
    let prefs = RwSignal::new(initial_history);
    let entries = RwSignal::new(Vec::<SavedSticker>::new());
    let loaded = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let managing = RwSignal::new(false);
    let page = RwSignal::new(0usize);
    let failure = RwSignal::new(None::<String>);
    let import = NodeRef::<html::Input>::new();

    Effect::watch(
        move || model.link.with(|link| link.revision),
        move |_, _, _| {
            let (initial_tab, initial_history) = load_history(model);
            tab.set(initial_tab);
            prefs.set(initial_history);
            search.set(String::new());
            entries.set(Vec::new());
            loaded.set(false);
            saving.set(false);
            managing.set(false);
            page.set(0);
            failure.set(None);
        },
        false,
    );

    Effect::watch(
        move || {
            (
                model.link.with(|link| link.revision),
                model.stickers_changed.get(),
            )
        },
        move |(revision, change), _, _| {
            let (revision, change) = (*revision, *change);
            spawn_local(async move {
                #[derive(Serialize)]
                struct Args {
                    revision: u64,
                }
                let response =
                    request::<_, Vec<SavedSticker>>("list_stickers", Args { revision }).await;
                if loaded.is_disposed()
                    || model.revision() != revision
                    || model.stickers_changed.get_untracked() != change
                {
                    return;
                }
                loaded.set(true);
                match response {
                    Ok(items) => {
                        entries.set(items);
                        failure.set(None);
                    }
                    Err(error) => failure.set(Some(error)),
                }
            });
        },
        true,
    );

    let store = move || {
        let saved = account_storage(model).and_then(|(storage, key)| {
            serde_json::to_string(&prefs.get_untracked())
                .ok()
                .and_then(|json| storage.set_item(&key, &json).ok())
        });
        if saved.is_none() {
            model.error.set("无法保存表情偏好".into());
        }
    };
    let select_emoji = move |glyph: &'static str| {
        if !model.busy() && choose_emoji(model, input, glyph) {
            prefs.update(|history| {
                history.recent.retain(|entry| entry != glyph);
                history.recent.insert(0, glyph.into());
                history.recent.truncate(RECENT_LIMIT);
            });
            store();
        }
    };
    let toggle_emoji = move |glyph: &'static str| {
        prefs.update(|history| {
            if history.favorites.iter().any(|entry| entry == glyph) {
                history.favorites.retain(|entry| entry != glyph);
            } else {
                history.favorites.insert(0, glyph.into());
                history.favorites.truncate(EMOJI_LIMIT);
            }
        });
        store();
    };

    let shown_emoji = move || {
        let needle = search.get().trim().to_lowercase();
        match tab.get() {
            Tab::Recent => prefs.with(|history| {
                history
                    .recent
                    .iter()
                    .filter_map(|glyph| emoji_info(glyph))
                    .collect::<Vec<_>>()
            }),
            Tab::Favorites => prefs.with(|history| {
                history
                    .favorites
                    .iter()
                    .filter_map(|glyph| emoji_info(glyph))
                    .collect::<Vec<_>>()
            }),
            Tab::Group(index) if needle.is_empty() => EMOJI[index].1.to_vec(),
            Tab::Group(_) => EMOJI
                .iter()
                .flat_map(|(_, group)| group.iter().copied())
                .collect::<Vec<_>>(),
        }
        .into_iter()
        .filter(|(glyph, name)| name.contains(&needle) || glyph.contains(&needle))
        .collect::<Vec<_>>()
    };
    let shown_entries = move || {
        let needle = search.get().trim().to_lowercase();
        entries.with(|items| {
            items
                .iter()
                .filter(|item| item.name.to_lowercase().contains(&needle))
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    Effect::new(move |_| {
        let last = shown_entries().len().saturating_sub(1) / STICKERS_PER_PAGE;
        if page.get_untracked() > last {
            page.set(last);
        }
    });

    let import_images = move |_| {
        let Some(input) = import.get_untracked() else {
            return;
        };
        let Some(files) = input.files() else { return };
        input.set_value("");
        let revision = model.revision();
        saving.set(true);
        failure.set(None);
        spawn_local(async move {
            let mut saved = 0;
            let mut error = None;
            for index in 0..files.length().min(photos::MAX_ATTACHMENTS as u32) {
                let Some(file) = files.get(index) else {
                    continue;
                };
                let file = match photos::ImageDraft::new(file) {
                    Ok(draft) => draft.file,
                    Err(message) => {
                        error = Some(message.into());
                        continue;
                    }
                };
                match photos::save_sticker(revision, &file.name(), &file).await {
                    Ok(_) => saved += 1,
                    Err(message) => {
                        error = Some(message);
                        break;
                    }
                }
            }
            if model.revision() != revision {
                return;
            }
            if saved > 0 {
                model.stickers_changed.update(|value| *value += 1);
                page.try_set(0);
            }
            if files.length() > photos::MAX_ATTACHMENTS as u32 {
                error.get_or_insert("每次最多导入 9 张图片".into());
            }
            if let Some(error) = error {
                model.error.set(error);
            }
            saving.try_set(false);
        });
    };

    view! {
        <section id="emoji-panel" class="emoji-panel" aria-label="表情选择">
            <div class="emoji-search">
                <input type="search" aria-label="搜索表情"
                    placeholder=move || if tab.get() == Tab::Favorites { "搜索收藏" } else { "搜索表情，如「开心」" }
                    prop:value=move || search.get()
                    on:keydown=move |event| if event.key() == "Enter" && !event.is_composing() { event.prevent_default(); }
                    on:input=move |event| {
                        search.set(event_target_value(&event));
                        page.set(0);
                        if !search.get_untracked().trim().is_empty() && tab.get_untracked() == Tab::Recent {
                            tab.set(Tab::Group(0));
                        }
                    }/>
                <button type="button" class="emoji-close" aria-label="关闭表情面板"
                    on:click=move |_| {
                        open.set(false);
                        if let Some(input) = input.get_untracked() { let _ = input.focus(); }
                    }>"×"</button>
            </div>
            <div class="emoji-categories" aria-label="分类">
                <button type="button" class:active=move || tab.get() == Tab::Recent
                    aria-pressed=move || (tab.get() == Tab::Recent).to_string()
                    on:click=move |_| { tab.set(Tab::Recent); search.set(String::new()); }>"最近"</button>
                <button type="button" class:active=move || tab.get() == Tab::Favorites
                    aria-pressed=move || (tab.get() == Tab::Favorites).to_string()
                    on:click=move |_| { tab.set(Tab::Favorites); search.set(String::new()); }>"收藏"</button>
                {EMOJI.iter().enumerate().map(|(index, (label, _))| view! {
                    <button type="button" class:active=move || tab.get() == Tab::Group(index)
                        aria-pressed=move || (tab.get() == Tab::Group(index)).to_string()
                        on:click=move |_| { tab.set(Tab::Group(index)); search.set(String::new()); }>{*label}</button>
                }).collect_view()}
            </div>
            <div class="emoji-results">
                <Show when=move || shown_emoji().is_empty() && tab.get() != Tab::Favorites>
                    <p class="emoji-empty">{move || if search.with(|value| value.trim().is_empty()) { "用过的表情会出现在这里" } else { "没有找到表情" }}</p>
                </Show>
                <div class="emoji-grid">
                    <For each=shown_emoji key=|(glyph, _)| *glyph
                        children=move |(glyph, label)| view! {
                            <div class="emoji-tile">
                                <button type="button" class="emoji-item" title=label aria-label=label
                                    disabled=move || model.busy()
                                    on:click=move |_| select_emoji(glyph)>{glyph}</button>
                                <button type="button" class="emoji-star" title="收藏表情"
                                    aria-pressed=move || prefs.with(|history| history.favorites.iter().any(|item| item == glyph)).to_string()
                                    aria-label=move || if prefs.with(|history| history.favorites.iter().any(|item| item == glyph)) {
                                        format!("取消收藏 {glyph}")
                                    } else { format!("收藏 {glyph}") }
                                    on:click=move |_| toggle_emoji(glyph)>
                                    {move || if prefs.with(|history| history.favorites.iter().any(|item| item == glyph)) { "★" } else { "☆" }}
                                </button>
                            </div>
                        }/>
                </div>
                <Show when=move || tab.get() == Tab::Favorites>
                    <div class="sticker-heading">
                        <strong>"图片表情"</strong>
                        <input node_ref=import type="file" accept=photos::IMAGE_ACCEPT multiple=true hidden=true on:change=import_images/>
                        <button type="button" disabled=move || saving.get() on:click=move |_| {
                            if let Some(input) = import.get_untracked() { input.click(); }
                        }>{move || if saving.get() { "导入中…" } else { "导入图片" }}</button>
                        <button type="button" aria-pressed=move || managing.get().to_string()
                            disabled=move || entries.with(Vec::is_empty)
                            on:click=move |_| managing.update(|value| *value = !*value)>
                            {move || if managing.get() { "完成" } else { "管理" }}
                        </button>
                    </div>
                    <Show when=move || !loaded.get()><p class="emoji-empty" role="status">"正在读取收藏…"</p></Show>
                    <Show when=move || failure.with(Option::is_some)>
                        <button type="button" class="emoji-retry" title=move || failure.get()
                            on:click=move |_| model.stickers_changed.update(|value| *value += 1)>"收藏读取失败，点此重试"</button>
                    </Show>
                    <Show when=move || loaded.get() && failure.with(Option::is_none) && shown_entries().is_empty()>
                        <p class="emoji-empty">{move || if search.with(|value| value.trim().is_empty()) {
                            "点聊天图片上的 ☆，或导入自己的图片"
                        } else { "没有找到图片表情" }}</p>
                    </Show>
                    <div class="sticker-grid">
                        <For each=move || {
                            shown_entries().into_iter().skip(page.get() * STICKERS_PER_PAGE).take(STICKERS_PER_PAGE).collect::<Vec<_>>()
                        } key=|item| item.id.clone()
                            children=move |entry| view! { <StickerTile model=model entry=entry managing=managing open=open/> }/>
                    </div>
                    <Show when=move || { shown_entries().len() > STICKERS_PER_PAGE }>
                        <div class="sticker-pages">
                            <button type="button" disabled=move || page.get() == 0
                                on:click=move |_| page.update(|index| *index = index.saturating_sub(1))>"上一页"</button>
                            <span>{move || format!("{} / {}", page.get() + 1, shown_entries().len().div_ceil(STICKERS_PER_PAGE))}</span>
                            <button type="button" disabled=move || (page.get() + 1) * STICKERS_PER_PAGE >= shown_entries().len()
                                on:click=move |_| page.update(|index| *index += 1)>"下一页"</button>
                        </div>
                    </Show>
                </Show>
            </div>
        </section>
    }
}

#[component]
fn StickerTile(
    model: Model,
    entry: SavedSticker,
    managing: RwSignal<bool>,
    open: RwSignal<bool>,
) -> impl IntoView {
    let file = RwSignal::new_local(None::<web_sys::File>);
    let error = RwSignal::new(false);
    let removing = RwSignal::new(false);
    let attempts = RwSignal::new(0u32);
    let load_entry = entry.clone();
    Effect::new(move |_| {
        let _ = attempts.get();
        let revision = model.revision();
        let entry = load_entry.clone();
        error.set(false);
        spawn_local(async move {
            let result = photos::sticker_file(revision, &entry).await;
            if model.revision() != revision {
                return;
            }
            match result {
                Ok(image) => {
                    file.try_set(Some(image));
                }
                Err(_) => {
                    error.try_set(true);
                }
            }
        });
    });
    let name = entry.name;
    view! {
        <div class="sticker-tile">
            <button type="button" class="sticker-item"
                aria-label=format!("使用表情 {name}") title=name.clone()
                disabled=move || model.busy() || managing.get()
                on:click=move |_| {
                    if let Some(file) = file.get_untracked() {
                        match model.add_attachment(file) {
                            Ok(()) => { model.error.set(String::new()); open.set(false); }
                            Err(message) => model.error.set(message.into()),
                        }
                    } else if error.get_untracked() {
                        attempts.update(|attempt| *attempt += 1);
                    }
                }>
                {move || match file.get() {
                    Some(file) => view! { <photos::DraftThumbnail file=file/> }.into_any(),
                    None if error.get() => view! { <span>"重试"</span> }.into_any(),
                    None => view! { <span>"加载中"</span> }.into_any(),
                }}
            </button>
            <Show when=move || managing.get()>
                <button type="button" class="remove-sticker" aria-label=format!("移除表情 {name}")
                    disabled=move || removing.get()
                    on:click={
                        let id = entry.id.clone();
                        move |_| {
                            let id = id.clone();
                            let revision = model.revision();
                            removing.set(true);
                            spawn_local(async move {
                                let result = photos::remove_sticker(revision, &id).await;
                                if model.revision() != revision { return; }
                                match result {
                                    Ok(()) => model.stickers_changed.update(|value| *value += 1),
                                    Err(error) => model.error.set(error),
                                }
                                removing.try_set(false);
                            });
                        }
                    }>"×"</button>
            </Show>
        </div>
    }
}
