import { common } from '@kit.AbilityKit';
import { util } from '@kit.ArkTS';
import { webSocket } from '@kit.NetworkKit';
import { ChatPreferences, clearSession, loadSession, saveSession } from './LocalData';
import { PortalApi, PortalError, portalOrigin } from './PortalApi';
import {
  ChatMessage, Conversation, ConversationEnd, DEFAULT_SERVER_ADDRESS, Draft, IssuedAccount, Member, OutgoingMessage, Person,
  PortalEvent, Relationship, Session
} from './Protocol';

enum Connection {
  Offline = '离线',
  Connecting = '连接中',
  Online = '已连接'
}

class SocketHeaders {
  readonly Authorization: string;

  constructor(token: string) {
    this.Authorization = 'Bearer ' + token;
  }
}

@Observed
export class ChatStore {
  member?: Member;
  isAdmin: boolean = false;
  people: Person[] = [];
  conversations: Conversation[] = [];
  peer?: Member;
  arrival?: ChatMessage;
  notice: string = '';
  connection: Connection = Connection.Offline;
  quiet: boolean = false;
  starting: boolean = true;
  activating: boolean = false;
  signingOut: boolean = false;
  ending: boolean = false;
  refreshing: boolean = false;
  directoryLoaded: boolean = false;
  draft: string = '';
  sendingPeers: number[] = [];
  pendingFriends: number[] = [];

  private readonly api: PortalApi = new PortalApi();
  private preferences?: ChatPreferences;
  private session?: Session;
  private active: boolean = false;
  private socket?: webSocket.WebSocket;
  private retryMs: number = 1000;
  private retryTimer: number = -1;
  private readyTimer: number = -1;
  private arrivalTimer: number = -1;
  private directoryRevision: number = 0;
  private appliedDirectoryRevision: number = 0;
  private syncRevision: number = 0;
  private readThrough: Map<number, number> = new Map();
  private drafts: Map<number, Draft> = new Map();

  async restore(context: common.UIAbilityContext): Promise<void> {
    try {
      this.preferences = new ChatPreferences(context);
      this.quiet = this.preferences.quiet();
    } catch {
      this.notice = '无法读取本机设置。';
    }
    try {
      const session = await loadSession();
      if (session && portalOrigin(DEFAULT_SERVER_ADDRESS) === session.server &&
        Number.isSafeInteger(session.member?.id) && session.token) {
        this.useSession(session);
      }
    } catch (error) {
      this.notice = (error as Error).message;
    } finally {
      this.starting = false;
    }
  }

  setActive(active: boolean): void {
    this.active = active;
    if (active) {
      this.connect();
      const current = this.peer;
      if (current) {
        this.markRead(current.id);
      }
    } else {
      this.stopConnection();
      this.dismissArrival();
    }
  }

  async activate(code: string): Promise<void> {
    if (this.session || this.starting || this.activating || this.signingOut) {
      return;
    }
    this.activating = true;
    this.notice = '';
    try {
      const session = await this.api.activate(DEFAULT_SERVER_ADDRESS, code);
      try {
        await saveSession(session);
      } catch {
        this.notice = '无法保存登录凭据，本次登录将在关闭 App 后失效。';
      }
      this.useSession(session);
    } catch (error) {
      this.notice = (error as Error).message;
    } finally {
      this.activating = false;
    }
  }

  async logout(): Promise<void> {
    const session = this.session;
    if (!session || this.signingOut) {
      return;
    }
    this.signingOut = true;
    this.stopConnection();
    this.dismissArrival();
    try {
      await clearSession();
    } catch (error) {
      this.signingOut = false;
      this.notice = (error as Error).message;
      this.connect();
      return;
    }

    this.reset();
    this.notice = '';
    try {
      await this.api.logout(session);
    } catch (error) {
      if (!(error instanceof PortalError && error.status === 401)) {
        this.notice = '已退出本机，服务器暂时未确认注销。';
      }
    } finally {
      this.signingOut = false;
    }
  }

  async changeNickname(nickname: string): Promise<boolean> {
    return this.updateProfile((session: Session) => this.api.changeNickname(session, nickname));
  }

  async changeAvatar(avatar: string): Promise<boolean> {
    return this.updateProfile((session: Session) => this.api.changeAvatar(session, avatar));
  }

  private async updateProfile(action: (session: Session) => Promise<Member>): Promise<boolean> {
    const session = this.session;
    if (!session || this.signingOut) {
      return false;
    }
    this.notice = '';
    try {
      const member = await action(session);
      if (this.session !== session || this.signingOut) {
        return false;
      }
      this.member = member;
      this.appliedDirectoryRevision = ++this.directoryRevision;
      await this.fetchDirectory(session).catch((error: Error) => this.report(session, error, false));
      return this.session === session;
    } catch (error) {
      this.report(session, error as Error, true);
      return false;
    }
  }

  async createAccount(nickname: string): Promise<IssuedAccount | undefined> {
    const session = this.session;
    if (!session || this.signingOut || !this.isAdmin) {
      return undefined;
    }
    this.notice = '';
    try {
      const account = await this.api.createAccount(session, nickname);
      return this.session === session && !this.signingOut ? account : undefined;
    } catch (error) {
      this.report(session, error as Error, true);
      return undefined;
    }
  }

  async setQuiet(quiet: boolean): Promise<void> {
    this.quiet = quiet;
    if (quiet) {
      this.dismissArrival();
    }
    try {
      await this.preferences?.saveSetting('quiet', quiet);
    } catch {
      this.notice = '无法保存免打扰设置。';
    }
  }

  getUnreadCount(): number {
    return this.conversations.reduce((sum: number, conversation: Conversation) => sum + conversation.unread, 0);
  }

  getRequestCount(): number {
    return this.people.filter((person: Person) => person.relationship === Relationship.Incoming).length;
  }

  friends(): Person[] {
    return this.people.filter((person: Person) => person.relationship === Relationship.Friend)
      .sort((first: Person, second: Person) => {
        const recent = (this.latest(second.member.id)?.id ?? 0) - (this.latest(first.member.id)?.id ?? 0);
        return recent || first.member.nickname.localeCompare(second.member.nickname);
      });
  }

  messagesFor(peerId: number): ChatMessage[] {
    return this.conversation(peerId)?.messages ?? [];
  }

  unreadFor(peerId: number): number {
    return this.conversation(peerId)?.unread ?? 0;
  }

  latest(peerId: number): ChatMessage | undefined {
    const messages = this.messagesFor(peerId);
    return messages[messages.length - 1];
  }

  async refresh(): Promise<void> {
    const session = this.session;
    if (session && !this.refreshing && !this.signingOut) {
      this.notice = '';
      await this.sync(session, true);
    }
  }

  requestFriend(peerId: number): Promise<void> {
    return this.updateFriend(peerId, (session: Session) => this.api.requestFriend(session, peerId));
  }

  answerFriend(peerId: number, accept: boolean): Promise<void> {
    return this.updateFriend(peerId, (session: Session) => this.api.answerFriend(session, peerId, accept));
  }

  async open(peer: Member): Promise<void> {
    const session = this.session;
    if (!session || this.signingOut) {
      return;
    }
    this.peer = peer;
    this.draft = this.drafts.get(peer.id)?.body ?? '';
    this.dismissArrival();
    this.markRead(peer.id);
    try {
      await this.fetchHistory(session, peer.id);
    } catch (error) {
      this.report(session, error as Error, true);
    }
  }

  closeChat(): void {
    this.peer = undefined;
    this.draft = '';
  }

  async endConversation(): Promise<void> {
    const session = this.session;
    const peer = this.peer;
    if (!session || !peer || this.ending || this.sendingPeers.includes(peer.id)) {
      return;
    }
    this.ending = true;
    this.notice = '';
    try {
      const ending = await this.api.endConversation(session, peer.id);
      if (this.session === session) {
        this.applyEnding(session, ending);
      }
    } catch (error) {
      this.report(session, error as Error, true);
    } finally {
      this.ending = false;
    }
  }

  private applyEnding(session: Session, ending: ConversationEnd): void {
    const peerId = ending.ended_by.id === session.member.id ? ending.peer_id : ending.ended_by.id;
    const messages = this.messagesFor(peerId);
    const arrival = this.arrival;
    if (arrival && arrival.sender.id === peerId && arrival.id <= ending.through_message_id) {
      this.dismissArrival();
    }
    if (this.peer?.id === peerId && (messages[messages.length - 1]?.id ?? 0) <= ending.through_message_id) {
      this.closeChat();
    }
  }

  editDraft(body: string): void {
    const peer = this.peer;
    if (!peer) {
      return;
    }
    this.draft = body;
    const previous = this.drafts.get(peer.id)?.request;
    this.drafts.set(peer.id, { body, request: previous?.body === body.trim() ? previous : undefined });
  }

  async send(): Promise<void> {
    const session = this.session;
    const peer = this.peer;
    const body = this.draft.trim();
    if (!session || !peer || !body || this.signingOut || this.ending || this.sendingPeers.includes(peer.id)) {
      return;
    }
    if (Array.from(body).length > 2000) {
      this.notice = '每条消息最多 2000 个字符。';
      return;
    }
    this.notice = '';
    const request: OutgoingMessage = this.drafts.get(peer.id)?.request ??
      { client_id: util.generateRandomUUID(), body };
    this.drafts.set(peer.id, { body: this.draft, request });
    this.sendingPeers = [...this.sendingPeers, peer.id];
    try {
      const sent = await this.api.send(session, peer.id, request);
      if (this.session === session && !this.signingOut) {
        this.merge(session, peer.id, [sent]);
        this.drafts.delete(peer.id);
        if (this.peer?.id === peer.id) {
          this.draft = '';
        }
      }
    } catch (error) {
      this.report(session, error as Error, true);
    } finally {
      if (this.session === session) {
        this.sendingPeers = this.sendingPeers.filter((id: number) => id !== peer.id);
      }
    }
  }

  dismissArrival(): void {
    clearTimeout(this.arrivalTimer);
    this.arrival = undefined;
  }

  private useSession(session: Session): void {
    this.session = session;
    this.member = session.member;
    this.connect();
  }

  private connect(): void {
    const session = this.session;
    if (!session || !this.active || this.socket || this.signingOut || this.retryTimer !== -1) {
      return;
    }
    const socket = webSocket.createWebSocket();
    this.socket = socket;
    this.connection = Connection.Connecting;
    socket.on('message', (error, data) => {
      if (this.socket !== socket || error || typeof data !== 'string') {
        return;
      }
      let event: PortalEvent;
      try {
        event = JSON.parse(data) as PortalEvent;
      } catch {
        return;
      }
      if (event) {
        this.handleEvent(session, event);
      }
    });
    socket.on('close', () => this.reconnect(session, socket));
    socket.on('error', () => this.reconnect(session, socket));
    const endpoint = 'wss' + session.server.substring(5) + '/api/v1/events';
    const options: webSocket.WebSocketRequestOptions = { header: new SocketHeaders(session.token) };
    this.readyTimer = setTimeout(() => this.reconnect(session, socket), 12000);
    socket.connect(endpoint, options).then((connected: boolean) => {
      if (!connected) {
        this.reconnect(session, socket);
      } else if (this.socket !== socket) {
        socket.close().catch(() => {});
      }
    }).catch(() => this.reconnect(session, socket));
  }

  private handleEvent(session: Session, event: PortalEvent): void {
    switch (event.type) {
      case 'ready':
        clearTimeout(this.readyTimer);
        this.connection = Connection.Online;
        this.retryMs = 1000;
        this.sync(session, false);
        break;
      case 'directory_changed':
        this.fetchDirectory(session).catch((error: Error) => this.report(session, error, false));
        break;
      case 'resync':
        this.sync(session, false);
        break;
      case 'message':
        if (event.message) {
          this.receive(session, event.message);
        }
        break;
      case 'conversation_ended':
        if (event.ending) {
          this.applyEnding(session, event.ending);
        }
        break;
      case 'session_ended':
        this.expireSession(session);
        break;
    }
  }

  private reconnect(session: Session, socket: webSocket.WebSocket): void {
    if (this.socket !== socket) {
      return;
    }
    this.closeSocket();
    this.connection = Connection.Connecting;
    this.fetchDirectory(session).catch((error: Error) => this.report(session, error, false));
    this.retryTimer = setTimeout(() => {
      this.retryTimer = -1;
      this.connect();
    }, this.retryMs);
    this.retryMs = Math.min(this.retryMs * 2, 8000);
  }

  private stopConnection(): void {
    this.closeSocket();
    clearTimeout(this.retryTimer);
    this.retryTimer = -1;
    this.retryMs = 1000;
    this.connection = Connection.Offline;
    this.syncRevision += 1;
    this.refreshing = false;
  }

  private closeSocket(): void {
    clearTimeout(this.readyTimer);
    const socket = this.socket;
    this.socket = undefined;
    if (socket) {
      socket.off('message');
      socket.off('close');
      socket.off('error');
      socket.close().catch(() => {});
    }
  }

  private async sync(session: Session, showError: boolean): Promise<void> {
    const revision = ++this.syncRevision;
    this.refreshing = true;
    try {
      await this.fetchDirectory(session);
      for (const person of this.friends()) {
        if (revision !== this.syncRevision || this.session !== session || this.signingOut) {
          break;
        }
        try {
          await this.fetchHistory(session, person.member.id);
        } catch (error) {
          this.report(session, error as Error, showError);
        }
      }
    } catch (error) {
      this.report(session, error as Error, showError);
    } finally {
      if (revision === this.syncRevision) {
        this.refreshing = false;
      }
    }
  }

  private async fetchDirectory(session: Session): Promise<void> {
    const revision = ++this.directoryRevision;
    const directory = await this.api.directory(session);
    if (this.session === session && !this.signingOut && revision > this.appliedDirectoryRevision) {
      this.appliedDirectoryRevision = revision;
      this.people = directory.people;
      this.member = directory.me;
      this.isAdmin = directory.is_admin === true;
      this.directoryLoaded = true;
      if (this.peer) {
        const current = directory.people.find((person: Person) =>
          person.member.id === this.peer?.id && person.relationship === Relationship.Friend);
        if (current) {
          this.peer = current.member;
        } else {
          this.closeChat();
          this.notice = '这个会话已不可用。';
        }
      }
    }
  }

  private async fetchHistory(session: Session, peerId: number): Promise<void> {
    const history = await this.api.history(session, peerId);
    if (this.session === session && !this.signingOut) {
      this.merge(session, peerId, history);
    }
  }

  private async updateFriend(peerId: number, action: (session: Session) => Promise<void>): Promise<void> {
    const session = this.session;
    if (!session || this.signingOut || this.pendingFriends.includes(peerId)) {
      return;
    }
    this.notice = '';
    this.pendingFriends = [...this.pendingFriends, peerId];
    try {
      await action(session);
      if (this.session === session && !this.signingOut) {
        await this.fetchDirectory(session);
      }
    } catch (error) {
      this.report(session, error as Error, true);
    } finally {
      if (this.session === session) {
        this.pendingFriends = this.pendingFriends.filter((id: number) => id !== peerId);
      }
    }
  }

  private receive(session: Session, message: ChatMessage): void {
    const peerId = message.sender.id;
    const isNew = !this.messagesFor(peerId).some((stored: ChatMessage) => stored.id === message.id);
    this.merge(session, peerId, [message]);
    if (isNew && this.active && !this.quiet && this.peer?.id !== peerId &&
      message.id > this.lastRead(session, peerId)) {
      this.dismissArrival();
      this.arrival = message;
      this.arrivalTimer = setTimeout(() => this.dismissArrival(), 8000);
    }
  }

  private merge(session: Session, peerId: number, incoming: ChatMessage[]): void {
    const unique: Map<number, ChatMessage> = new Map();
    this.messagesFor(peerId).forEach((message: ChatMessage) => unique.set(message.id, message));
    incoming.forEach((message: ChatMessage) => unique.set(message.id, message));
    const messages = Array.from(unique.values())
      .sort((first: ChatMessage, second: ChatMessage) => first.id - second.id).slice(-100);
    const unread = messages.filter((message: ChatMessage) =>
      message.sender.id !== session.member.id && message.id > this.lastRead(session, peerId)).length;
    const conversation: Conversation = { peerId, messages, unread };
    this.conversations = [...this.conversations.filter((existing: Conversation) => existing.peerId !== peerId),
      conversation];
    if (this.active && this.peer?.id === peerId) {
      this.markRead(peerId);
    }
  }

  private conversation(peerId: number): Conversation | undefined {
    return this.conversations.find((conversation: Conversation) => conversation.peerId === peerId);
  }

  private lastRead(session: Session, peerId: number): number {
    const cached = this.readThrough.get(peerId);
    if (cached !== undefined) {
      return cached;
    }
    let last = 0;
    try {
      last = this.preferences?.readThrough(session, peerId) ?? 0;
    } catch {
      // Keep read progress in memory if preferences are unavailable.
    }
    this.readThrough.set(peerId, last);
    return last;
  }

  private markRead(peerId: number): void {
    const session = this.session;
    const latest = this.latest(peerId);
    if (!session || !latest || latest.id <= this.lastRead(session, peerId)) {
      return;
    }
    this.readThrough.set(peerId, latest.id);
    const updated: Conversation = { peerId, messages: this.messagesFor(peerId), unread: 0 };
    this.conversations = [...this.conversations.filter((existing: Conversation) => existing.peerId !== peerId), updated];
    this.preferences?.markRead(session, peerId, latest.id).catch(() => {});
  }

  private report(session: Session, error: Error, showError: boolean): void {
    if (this.session !== session || this.signingOut) {
      return;
    }
    if (error instanceof PortalError && error.status === 401) {
      this.expireSession(session);
    } else if (showError) {
      this.notice = error.message || '操作失败，请重试。';
    }
  }

  private async expireSession(session: Session): Promise<void> {
    if (this.session !== session || this.signingOut) {
      return;
    }
    this.signingOut = true;
    this.reset();
    this.notice = '登录凭据已失效，请重新激活。';
    try {
      await clearSession();
    } catch (error) {
      this.notice = (error as Error).message;
    } finally {
      this.signingOut = false;
    }
  }

  private reset(): void {
    this.stopConnection();
    this.dismissArrival();
    this.session = undefined;
    this.member = undefined;
    this.isAdmin = false;
    this.people = [];
    this.directoryLoaded = false;
    this.conversations = [];
    this.pendingFriends = [];
    this.sendingPeers = [];
    this.readThrough.clear();
    this.drafts.clear();
    this.closeChat();
  }
}
