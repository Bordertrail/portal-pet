use serde::{Deserialize, Serialize};

pub const DEFAULT_SERVER_ADDRESS: &str = "http://127.0.0.1:8765";
pub const SECURITY_CODE_LENGTH: usize = 6;
pub const IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSticker {
    pub id: String,
    pub name: String,
    pub content_type: String,
    pub bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: i64,
    pub nickname: String,
    #[serde(default)]
    pub avatar: Avatar,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Avatar {
    #[default]
    Default,
    Blue,
    Purple,
}

impl Avatar {
    pub fn from_storage(value: &str) -> Self {
        match value {
            "blue" => Self::Blue,
            "purple" => Self::Purple,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Blue => "blue",
            Self::Purple => "purple",
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvatarChange {
    pub avatar: Avatar,
}

#[derive(Serialize, Deserialize)]
pub struct Activate {
    pub code: String,
}

#[derive(Serialize, Deserialize)]
pub struct Activated {
    pub member: Member,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relationship {
    None,
    Outgoing,
    Incoming,
    Friend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub member: Member,
    pub online: bool,
    pub relationship: Relationship,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Directory {
    pub me: Member,
    #[serde(default)]
    pub is_admin: bool,
    pub people: Vec<Person>,
    #[serde(default)]
    pub endings: Vec<ConversationEnd>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NicknameChange {
    pub nickname: String,
}

#[derive(Serialize, Deserialize)]
pub struct IssuedAccount {
    pub member: Member,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationEnd {
    pub ended_by: Member,
    pub peer_id: i64,
    pub through_message_id: i64,
}

impl ConversationEnd {
    pub fn other_id(&self, me: i64) -> i64 {
        if self.ended_by.id == me {
            self.peer_id
        } else {
            self.ended_by.id
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct FriendAnswer {
    pub accept: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SendMessage {
    pub client_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInfo {
    pub content_type: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub sender: Member,
    pub recipient_id: i64,
    pub body: String,
    pub sent_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageInfo>,
}

impl Message {
    pub fn peer_id(&self, member_id: i64) -> i64 {
        if self.sender.id == member_id {
            self.recipient_id
        } else {
            self.sender.id
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Ready,
    DirectoryChanged,
    Message { message: Message },
    ConversationEnded { ending: ConversationEnd },
    Resync,
    SessionEnded,
}

#[derive(Serialize, Deserialize)]
pub struct ApiError {
    pub message: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Link {
    pub member: Option<Member>,
    pub server: Option<String>,
    pub connected: bool,
    pub revision: u64,
    pub notice: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arrival {
    pub generation: u64,
    pub peer: Member,
    pub body: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisitAction {
    Arrive,
    WalkAway,
    KickAway,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VisitAnimation {
    pub generation: u64,
    pub peer: Member,
    pub action: VisitAction,
}
