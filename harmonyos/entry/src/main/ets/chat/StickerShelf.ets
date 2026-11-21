import { common } from '@kit.AbilityKit';
import { preferences } from '@kit.ArkData';
import { util } from '@kit.ArkTS';
import { fileIo } from '@kit.CoreFileKit';
import { cryptoFramework } from '@kit.CryptoArchitectureKit';
import { MAX_IMAGE_BYTES } from './Protocol';

export class EmojiItem {
  readonly glyph: string;
  readonly name: string;

  constructor(glyph: string, name: string) {
    this.glyph = glyph;
    this.name = name;
  }
}

export class EmojiGroup {
  readonly name: string;
  readonly items: EmojiItem[];

  constructor(name: string, words: string) {
    this.name = name;
    this.items = words.split('|').map((word: string) => {
      const separator = word.indexOf(' ');
      return new EmojiItem(word.slice(0, separator), word.slice(separator + 1));
    });
  }
}

export const EMOJI_GROUPS: EmojiGroup[] = [
  new EmojiGroup('笑脸', '😀 开心|😂 笑哭|🤣 笑翻|😊 微笑|😉 眨眼|🥰 喜欢|😍 心动|😘 飞吻|😋 好吃|😜 调皮|🤭 偷笑|🤫 保密|🤔 思考|🙄 白眼|🥺 撒娇|🥹 感动|😢 流泪|😭 大哭|😡 生气|😳 脸红|😅 尴尬|😴 晚安|🥳 庆祝|😎 墨镜|🤩 星星眼|🤗 抱抱|🤖 机器人'),
  new EmojiGroup('手势', '👋 你好 再见|👌 好的|👍 点赞|👎 不赞同|✌️ 胜利|🤝 握手|👏 鼓掌|🙌 欢呼|🙏 谢谢|💪 加油|🫶 比心|🤷 摊手|🫡 敬礼'),
  new EmojiGroup('爱心', '❤️ 红心 爱你|🧡 橙心|💛 黄心|💚 绿心|💙 蓝心|💜 紫心|🩷 粉心|🤍 白心|💔 心碎|💕 双心|💖 闪亮|💌 情书 信封|🌹 玫瑰|💐 花束'),
  new EmojiGroup('动物', '🐶 小狗|🐱 小猫|🐰 兔子|🦊 狐狸|🐻 小熊|🐼 熊猫|🐸 青蛙|🙈 捂眼|🐧 企鹅|🦋 蝴蝶'),
  new EmojiGroup('日常', '☀️ 早安 太阳|🌙 晚安 月亮|⭐ 星星|✨ 闪亮|🌈 彩虹|🌱 发芽|🌸 樱花|☕ 咖啡|🧋 奶茶|🎂 生日 蛋糕|🎁 礼物|🎉 庆祝 恭喜|📷 相机|🎵 音乐|🔥 热门|✅ 完成|🚪 传送门')
];
const EMOJI: EmojiItem[] = EMOJI_GROUPS.flatMap((group: EmojiGroup) => group.items);

export class StickerPicture {
  readonly name: string;
  readonly path: string;
  readonly bytes: number;
  readonly modified: number;

  constructor(name: string, path: string, bytes: number, modified: number) {
    this.name = name;
    this.path = path;
    this.bytes = bytes;
    this.modified = modified;
  }
}

export async function readStickerImage(path: string): Promise<ArrayBuffer> {
  const file = await fileIo.open(path, fileIo.OpenMode.READ_ONLY);
  try {
    const size = (await fileIo.stat(file.fd)).size;
    if (size <= 0 || size > MAX_IMAGE_BYTES) {
      throw new Error('请选择一张不超过 8 MiB 的 JPEG、PNG 或 WebP 图片。');
    }
    const data = new ArrayBuffer(size);
    if (await fileIo.read(file.fd, data) !== size) {
      throw new Error('图片未读取完成，请重试。');
    }
    return data;
  } finally {
    await fileIo.close(file);
  }
}

function extension(data: ArrayBuffer): string {
  const bytes = new Uint8Array(data);
  if (bytes.length >= 8 && bytes[0] === 137 && bytes[1] === 80 && bytes[2] === 78 && bytes[3] === 71) {
    return 'png';
  }
  if (bytes.length >= 3 && bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255) {
    return 'jpg';
  }
  if (bytes.length >= 12 && bytes[0] === 82 && bytes[1] === 73 && bytes[2] === 70 && bytes[3] === 70 &&
      bytes[8] === 87 && bytes[9] === 69 && bytes[10] === 66 && bytes[11] === 80) {
    return 'webp';
  }
  throw new Error('请选择 JPEG、PNG 或 WebP 图片。');
}

@Observed
export class StickerShelf {
  pictures: StickerPicture[] = [];
  favorites: string[] = [];
  recent: string[] = [];
  busy: boolean = false;
  private settings?: preferences.Preferences;
  private directory: string = '';
  private member: number = -1;
  private root: string = '';

  initialize(context: common.UIAbilityContext): void {
    this.settings = preferences.getPreferencesSync(context, { name: 'portal_stickers' });
    this.root = context.filesDir;
  }

  async selectAccount(member: number, refresh: boolean = false): Promise<void> {
    if (this.member !== member) {
      this.member = member;
      this.directory = this.root && member >= 0 ? this.root + '/stickers-' + member : '';
      this.pictures = [];
      this.favorites = [];
      this.recent = [];
      if (this.directory) {
        this.favorites = this.readEmoji('favorites');
        this.recent = this.readEmoji('recent');
      }
    }
    if (refresh && this.directory) {
      await this.refresh(this.directory);
    }
  }

  choices(category: string, query: string): EmojiItem[] {
    const needle = query.trim().toLowerCase();
    let items: EmojiItem[];
    if (category === '最近' || category === '收藏') {
      const order = category === '最近' ? this.recent : this.favorites;
      items = order.flatMap((glyph: string) => EMOJI.filter((item: EmojiItem) => item.glyph === glyph));
    } else {
      items = needle ? EMOJI : (EMOJI_GROUPS.find((group: EmojiGroup) => group.name === category)?.items ?? []);
    }
    return !needle ? items : items.filter((item: EmojiItem) => item.name.includes(needle) || item.glyph.includes(needle));
  }

  async used(glyph: string): Promise<void> {
    this.recent = [glyph, ...this.recent.filter((value: string) => value !== glyph)].slice(0, 32);
    await this.writeEmoji('recent', this.recent);
  }

  async toggle(glyph: string): Promise<void> {
    const remaining = this.favorites.filter((value: string) => value !== glyph);
    this.favorites = remaining.length === this.favorites.length ? [glyph, ...remaining].slice(0, 80) : remaining;
    await this.writeEmoji('favorites', this.favorites);
  }

  async save(data: ArrayBuffer): Promise<void> {
    if (this.busy || !this.directory) { throw new Error('表情收藏暂不可用。'); }
    if (data.byteLength <= 0 || data.byteLength > MAX_IMAGE_BYTES) {
      throw new Error('每张图片不能超过 8 MiB。');
    }
    const kind = extension(data);
    const directory = this.directory;
    this.busy = true;
    try {
      const md = cryptoFramework.createMd('SHA256');
      await md.update({ data: new Uint8Array(data) });
      const digest = await md.digest();
      const name = Array.from(digest.data).map((value: number) => value.toString(16).padStart(2, '0')).join('') + '.' + kind;
      await this.refresh(directory);
      if (directory !== this.directory) { return; }
      if (this.pictures.some((item: StickerPicture) => item.name === name)) { return; }
      const total = this.pictures.reduce((size: number, item: StickerPicture) => size + item.bytes, data.byteLength);
      if (this.pictures.length >= 80 || total > 64 * 1024 * 1024) {
        throw new Error('表情收藏已满，请先移除一些图片。');
      }
      if (!(await fileIo.access(directory))) { await fileIo.mkdir(directory); }
      const temporary = directory + '/tmp-' + util.generateRandomUUID();
      try {
        const file = await fileIo.open(temporary, fileIo.OpenMode.CREATE | fileIo.OpenMode.WRITE_ONLY);
        try {
          if (await fileIo.write(file.fd, data) !== data.byteLength) { throw new Error('无法保存表情。'); }
        } finally {
          await fileIo.close(file);
        }
        await fileIo.rename(temporary, directory + '/' + name);
      } finally {
        if (await fileIo.access(temporary)) { await fileIo.unlink(temporary); }
      }
      await this.refresh(directory);
    } finally {
      this.busy = false;
    }
  }

  async remove(picture: StickerPicture): Promise<void> {
    const directory = this.directory;
    if (this.busy || !this.pictures.some((item: StickerPicture) => item.path === picture.path)) { return; }
    this.busy = true;
    try {
      await fileIo.unlink(picture.path);
      await this.refresh(directory);
    } finally {
      this.busy = false;
    }
  }

  private async refresh(directory: string): Promise<void> {
    const images: StickerPicture[] = [];
    if (await fileIo.access(directory)) {
      const names = await fileIo.listFile(directory);
      for (const name of names) {
        if (!/^[0-9a-f]{64}\.(png|jpg|webp)$/.test(name)) { continue; }
        const path = directory + '/' + name;
        const stat = await fileIo.stat(path);
        if (stat.size > 0 && stat.size <= MAX_IMAGE_BYTES) {
          images.push(new StickerPicture(name, path, stat.size, stat.mtime));
        }
      }
    }
    if (this.directory === directory) {
      this.pictures = images.sort((a: StickerPicture, b: StickerPicture) => b.modified - a.modified);
    }
  }

  private readEmoji(section: string): string[] {
    try {
      const value = this.settings?.getSync(section + ':' + this.member, '[]');
      return typeof value === 'string' ? JSON.parse(value) as string[] : [];
    } catch {
      return [];
    }
  }

  private async writeEmoji(section: string, list: string[]): Promise<void> {
    if (this.member < 0 || !this.settings) { return; }
    this.settings.putSync(section + ':' + this.member, JSON.stringify(list));
    await this.settings.flush();
  }
}
