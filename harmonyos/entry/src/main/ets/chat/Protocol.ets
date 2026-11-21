export const DEFAULT_SERVER_ADDRESS: string = 'https://chat.example.com';
export const SECURITY_CODE_LENGTH: number = 6;
export const MAX_IMAGE_BYTES: number = 8 * 1024 * 1024;

export interface Member {
  id: number;
  nickname: string;
  avatar?: string;
}

export enum Relationship {
  None = 'none',
  Outgoing = 'outgoing',
  Incoming = 'incoming',
  Friend = 'friend'
}

export interface Person {
  member: Member;
  online: boolean;
  relationship: Relationship;
}

export interface Directory {
  me: Member;
  is_admin?: boolean;
  people: Person[];
}

export interface NicknameChange {
  nickname: string;
}

export interface AvatarChange {
  avatar: string;
}

export interface IssuedAccount {
  member: Member;
  code: string;
}

export interface ChatImage {
  width: number;
  height: number;
  bytes: number;
}

export interface ChatMessage {
  id: number;
  sender: Member;
  recipient_id: number;
  body: string;
  sent_at: number;
  image?: ChatImage;
}

export interface Activation {
  member: Member;
  token: string;
}

export interface Session {
  server: string;
  member: Member;
  token: string;
}

export interface PortalEvent {
  type: string;
  message?: ChatMessage;
  ending?: ConversationEnd;
}

export interface ConversationEnd {
  ended_by: Member;
  peer_id: number;
  through_message_id: number;
}

export interface ActivationRequest {
  code: string;
}

export interface FriendAnswer {
  accept: boolean;
}

export interface OutgoingMessage {
  client_id: string;
  body: string;
}

export interface Failure {
  message: string;
}

export interface Conversation {
  peerId: number;
  messages: ChatMessage[];
  unread: number;
}

export interface Draft {
  body: string;
  request?: OutgoingMessage;
}
