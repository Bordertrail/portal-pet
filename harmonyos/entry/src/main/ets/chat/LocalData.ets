import { common } from '@kit.AbilityKit';
import { asset } from '@kit.AssetStoreKit';
import { util } from '@kit.ArkTS';
import { BusinessError } from '@kit.BasicServicesKit';
import { preferences } from '@kit.ArkData';
import { Session } from './Protocol';

const SESSION_ALIAS: Uint8Array = new util.TextEncoder().encodeInto('portal-pet-session');

function sessionQuery(): asset.AssetMap {
  const query: asset.AssetMap = new Map();
  query.set(asset.Tag.ALIAS, SESSION_ALIAS);
  return query;
}

export async function loadSession(): Promise<Session | undefined> {
  const query = sessionQuery();
  query.set(asset.Tag.RETURN_TYPE, asset.ReturnType.ALL);
  try {
    const results = await asset.query(query);
    const secret = results[0]?.get(asset.Tag.SECRET);
    if (!(secret instanceof Uint8Array)) {
      throw new Error('无法读取登录凭据。');
    }
    return JSON.parse(new util.TextDecoder('utf-8').decodeToString(secret)) as Session;
  } catch (error) {
    if ((error as BusinessError).code !== asset.ErrorCode.NOT_FOUND) {
      throw new Error('无法读取登录凭据，请重试。');
    }
    return undefined;
  }
}

export async function saveSession(session: Session): Promise<void> {
  const query = sessionQuery();
  query.set(asset.Tag.SECRET, new util.TextEncoder().encodeInto(JSON.stringify(session)));
  query.set(asset.Tag.CONFLICT_RESOLUTION, asset.ConflictResolution.OVERWRITE);
  await asset.add(query);
}

export async function clearSession(): Promise<void> {
  try {
    await asset.remove(sessionQuery());
  } catch (error) {
    if ((error as BusinessError).code !== asset.ErrorCode.NOT_FOUND) {
      throw new Error('无法清除本机登录凭据，请重试。');
    }
  }
}

export class ChatPreferences {
  private store: preferences.Preferences;

  constructor(context: common.UIAbilityContext) {
    this.store = preferences.getPreferencesSync(context, { name: 'portal_chat' });
  }

  quiet(): boolean {
    return this.store.getSync('quiet', false) as boolean;
  }

  async saveSetting(key: string, value: string | boolean): Promise<void> {
    this.store.putSync(key, value);
    await this.store.flush();
  }

  readThrough(session: Session, peerId: number): number {
    return this.store.getSync(this.readKey(session, peerId), 0) as number;
  }

  async markRead(session: Session, peerId: number, messageId: number): Promise<void> {
    if (messageId > this.readThrough(session, peerId)) {
      this.store.putSync(this.readKey(session, peerId), messageId);
      await this.store.flush();
    }
  }

  private readKey(session: Session, peerId: number): string {
    return `read:${session.server}:${session.member.id}:${peerId}`;
  }
}
