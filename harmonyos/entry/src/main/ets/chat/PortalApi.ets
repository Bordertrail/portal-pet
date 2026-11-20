import { url } from '@kit.ArkTS';
import { rcp } from '@kit.RemoteCommunicationKit';
import {
  Activation, ActivationRequest, AvatarChange, ChatMessage, ConversationEnd, Directory, Failure, FriendAnswer, IssuedAccount, Member,
  NicknameChange, OutgoingMessage, Session
} from './Protocol';

export class PortalError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export function portalOrigin(input: string): string {
  const address = input.trim();
  const invalid = '请输入完整的 HTTPS 服务器地址，例如 https://chat.example.com:8443。';
  if (address.length > 300 || !/^https:\/\/[^/?#@\s\\]+\/?$/i.test(address)) {
    throw new Error(invalid);
  }
  try {
    const origin = url.URL.parseURL(address);
    if (!origin.hostname || origin.protocol !== 'https:' || origin.port === '0') {
      throw new Error(invalid);
    }
    return origin.origin;
  } catch {
    throw new Error(invalid);
  }
}

export class PortalApi {
  async activate(address: string, input: string): Promise<Session> {
    const server = portalOrigin(address);
    const code = input.trim().toUpperCase();
    if (!/^[0-9A-Z]{6}$/.test(code)) {
      throw new Error('请输入管理员发放的 6 位安全码。');
    }
    const payload: ActivationRequest = { code };
    const result = await this.read<Activation>(server, 'activate', 'POST', '', JSON.stringify(payload));
    if (!result.member || !Number.isSafeInteger(result.member.id) || !result.token) {
      throw new Error('服务器未返回有效的登录凭据。');
    }
    return { server, member: result.member, token: result.token };
  }

  directory(session: Session): Promise<Directory> {
    return this.read<Directory>(session.server, 'directory', 'GET', session.token);
  }

  changeNickname(session: Session, nickname: string): Promise<Member> {
    const payload: NicknameChange = { nickname };
    return this.read<Member>(session.server, 'me/nickname', 'POST', session.token, JSON.stringify(payload));
  }

  changeAvatar(session: Session, avatar: string): Promise<Member> {
    const payload: AvatarChange = { avatar };
    return this.read<Member>(session.server, 'me/avatar', 'POST', session.token, JSON.stringify(payload));
  }

  createAccount(session: Session, nickname: string): Promise<IssuedAccount> {
    const payload: NicknameChange = { nickname };
    return this.read<IssuedAccount>(session.server, 'admin/accounts', 'POST', session.token, JSON.stringify(payload));
  }

  history(session: Session, peerId: number): Promise<ChatMessage[]> {
    return this.read<ChatMessage[]>(session.server, 'messages/' + peerId, 'GET', session.token);
  }

  async requestFriend(session: Session, peerId: number): Promise<void> {
    await this.request(session.server, 'friends/' + peerId + '/request', 'POST', session.token);
  }

  async answerFriend(session: Session, peerId: number, accept: boolean): Promise<void> {
    const payload: FriendAnswer = { accept };
    await this.request(session.server, 'friends/' + peerId + '/answer', 'POST', session.token,
      JSON.stringify(payload));
  }

  send(session: Session, peerId: number, payload: OutgoingMessage): Promise<ChatMessage> {
    return this.read<ChatMessage>(session.server, 'messages/' + peerId, 'POST', session.token,
      JSON.stringify(payload));
  }

  async logout(session: Session): Promise<void> {
    await this.request(session.server, 'session/logout', 'POST', session.token);
  }

  endConversation(session: Session, peerId: number): Promise<ConversationEnd> {
    return this.read<ConversationEnd>(session.server, 'messages/' + peerId + '/end', 'POST', session.token);
  }

  private async read<T>(server: string, path: string, method: rcp.HttpMethod, token: string,
    body?: string): Promise<T> {
    const content = await this.request(server, path, method, token, body);
    try {
      return JSON.parse(content) as T;
    } catch {
      throw new Error('无法读取服务器响应。');
    }
  }

  private async request(server: string, path: string, method: rcp.HttpMethod, token: string,
    body?: string): Promise<string> {
    const headers: rcp.RequestHeaders = { 'accept': 'application/json' };
    if (token) {
      headers.authorization = 'Bearer ' + token;
    }
    if (body !== undefined) {
      headers['content-type'] = 'application/json';
    }
    const request = new rcp.Request(server + '/api/v1/' + path, method, headers, body);
    const connection = rcp.createSession({
      requestConfiguration: { transfer: { autoRedirect: false, timeout: { connectMs: 8000, transferMs: 12000 } } }
    });
    let response: rcp.Response;
    try {
      response = await connection.fetch(request);
    } catch {
      throw new Error('无法连接服务器，请检查 HTTPS 地址、证书和网络。');
    } finally {
      connection.close();
    }
    const content = response.toString() ?? '';
    if (response.statusCode < 200 || response.statusCode >= 300) {
      let message = '服务器返回 HTTP ' + response.statusCode + '。';
      try {
        const failure = JSON.parse(content) as Failure;
        if (failure?.message) {
          message = failure.message;
        }
      } catch {
        // Reverse proxies can return plain text or HTML error pages.
      }
      throw new PortalError(response.statusCode, message);
    }
    return content;
  }
}
