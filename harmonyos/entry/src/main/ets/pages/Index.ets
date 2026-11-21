import { common } from '@kit.AbilityKit';
import { fileUri } from '@kit.CoreFileKit';
import { unifiedDataChannel, uniformTypeDescriptor } from '@kit.ArkData';
import { photoAccessHelper } from '@kit.MediaLibraryKit';
import { ChatStore } from '../chat/ChatStore';
import { ChatMessage, Member, Person, Relationship, SECURITY_CODE_LENGTH } from '../chat/Protocol';
import { EMOJI_GROUPS, EmojiGroup, EmojiItem, StickerPicture, StickerShelf, readStickerImage } from '../chat/StickerShelf';
import { ArrivalCard, CountBadge, MessageHistory, PetAvatar, PortalHero } from './ChatViews';

enum Tab {
  Messages = '消息',
  Members = '成员',
  Settings = '设置'
}

@Entry
@Component
struct Index {
  @State private chat: ChatStore = new ChatStore();
  @State private tab: Tab = Tab.Messages;
  @State private code: string = '';
  @State private codeFocused: boolean = false;
  @State private nicknameDraft: string = '';
  @State private savingNickname: boolean = false;
  @State private nicknameSaved: boolean = false;
  @State private savingAvatar: boolean = false;
  @State private newNickname: string = '';
  @State private creatingAccount: boolean = false;
  @State private issuedCode: string = '';
  @State private issuedName: string = '';
  @State private selectingImage: boolean = false;
  @State private shelf: StickerShelf = new StickerShelf();
  @State private showStickers: boolean = false;
  @State private stickerCategory: string = '笑脸';
  @State private stickerQuery: string = '';
  @State private manageStickers: boolean = false;
  @State private droppingImage: boolean = false;

  private async pickImage(collect: boolean = false): Promise<void> {
    const peer = this.chat.peer?.id;
    const account = this.chat.member?.id;
    if (peer === undefined || this.selectingImage) {
      return;
    }
    this.selectingImage = true;
    try {
      const options = new photoAccessHelper.PhotoSelectOptions();
      options.MIMEType = photoAccessHelper.PhotoViewMIMETypes.IMAGE_TYPE;
      options.maxSelectNumber = 1;
      const selected = await new photoAccessHelper.PhotoViewPicker().select(options);
      const uri = selected.photoUris[0];
      if (!uri || this.chat.peer?.id !== peer) {
        return;
      }
      const data = await readStickerImage(uri);
      if (this.chat.member?.id !== account || this.chat.peer?.id !== peer) { return; }
      if (collect) {
        await this.saveSticker(data);
      } else {
        this.chat.selectImage(data);
      }
    } catch (error) {
      if (this.chat.peer?.id === peer) {
        this.chat.notice = (error as Error).message || '无法读取所选图片。';
      }
    } finally {
      this.selectingImage = false;
    }
  }

  aboutToAppear(): void {
    const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
    try {
      this.shelf.initialize(context);
    } catch {
      this.chat.notice = '无法读取本机表情收藏。';
    }
    this.chat.restore(context);
  }

  onPageShow(): void {
    this.chat.setActive(true);
  }

  onPageHide(): void {
    this.chat.setActive(false);
  }

  aboutToDisappear(): void {
    this.chat.setActive(false);
  }

  onBackPress(): boolean {
    if (this.showStickers) {
      this.showStickers = false;
      return true;
    }
    if (this.chat.peer) {
      this.chat.closeChat();
      return true;
    }
    if (this.tab !== Tab.Messages && this.chat.member) {
      this.tab = Tab.Messages;
      return true;
    }
    return false;
  }

  private async toggleStickers(): Promise<void> {
    if (this.showStickers) {
      this.showStickers = false;
      return;
    }
    const account = this.chat.member?.id ?? -1;
    const peer = this.chat.peer?.id;
    try {
      await this.shelf.selectAccount(account, true);
      if (this.chat.member?.id !== account || this.chat.peer?.id !== peer) { return; }
      this.stickerCategory = this.shelf.recent.length ? '最近' : '笑脸';
      this.stickerQuery = '';
      this.manageStickers = false;
      this.showStickers = true;
    } catch {
      this.chat.notice = '无法读取本机表情收藏。';
    }
  }

  private async saveSticker(data: ArrayBuffer): Promise<void> {
    const account = this.chat.member?.id ?? -1;
    await this.shelf.selectAccount(account);
    if (this.chat.member?.id !== account) { return; }
    await this.shelf.save(data);
    if (this.chat.member?.id === account) { this.chat.notice = '已收藏为表情。'; }
  }

  private async saveChatImage(message: ChatMessage): Promise<void> {
    try {
      await this.saveSticker(await this.chat.imageData(message));
    } catch (error) {
      this.chat.notice = (error as Error).message || '无法收藏这张图片。';
    }
  }

  private async usePicture(picture: StickerPicture): Promise<void> {
    const peer = this.chat.peer?.id;
    const account = this.chat.member?.id;
    if (peer === undefined || this.selectingImage) { return; }
    if (this.manageStickers) {
      await this.shelf.remove(picture).catch(() => { this.chat.notice = '无法移除表情。'; });
      return;
    }
    this.selectingImage = true;
    try {
      const data = await readStickerImage(picture.path);
      if (this.chat.member?.id === account && this.chat.peer?.id === peer) {
        this.chat.selectImage(data);
        this.showStickers = false;
      }
    } catch {
      this.chat.notice = '无法读取这个收藏表情。';
    } finally {
      this.selectingImage = false;
    }
  }

  private dropImage(event: DragEvent): void {
    this.droppingImage = false;
    const peer = this.chat.peer?.id;
    const account = this.chat.member?.id;
    if (peer === undefined || this.selectingImage || this.chat.ending || this.chat.sendingPeers.includes(peer)) { return; }
    try {
      const record = event.getData().getRecords()[0] as unifiedDataChannel.Image | undefined;
      if (!record?.imageUri) { return; }
      event.setResult(DragResult.DRAG_SUCCESSFUL);
      this.selectingImage = true;
      readStickerImage(record.imageUri).then((data: ArrayBuffer) => {
        if (this.chat.member?.id === account && this.chat.peer?.id === peer) {
          this.showStickers = false;
          this.chat.selectImage(data);
        }
      }).catch(() => {
        this.chat.notice = '无法读取拖入的图片。';
      }).finally(() => { this.selectingImage = false; });
    } catch {
      this.chat.notice = '无法读取拖入的图片。';
    }
  }

  private async activate(): Promise<void> {
    await this.chat.activate(this.code);
    this.code = '';
    if (this.chat.member) {
      this.tab = Tab.Messages;
      this.nicknameDraft = '';
      this.nicknameSaved = false;
      this.newNickname = '';
      this.issuedCode = '';
      this.issuedName = '';
    }
  }

  private enterCode(input: string): void {
    if (this.chat.activating || this.chat.signingOut) {
      return;
    }
    const value = input.replace(/[^A-Za-z0-9]/g, '').toUpperCase();
    if (value.length > SECURITY_CODE_LENGTH) {
      this.chat.notice = '请输入 6 位安全码。';
      return;
    }
    if (value === this.code) {
      return;
    }
    this.chat.notice = '';
    this.code = value;
    if (value.length === SECURITY_CODE_LENGTH) {
      this.activate();
    }
  }

  private async saveAvatar(avatar: string): Promise<void> {
    if (this.savingAvatar) {
      return;
    }
    this.savingAvatar = true;
    await this.chat.changeAvatar(avatar);
    this.savingAvatar = false;
  }

  @Builder
  avatarOption(label: string, avatar: string, current: string) {
    Button(label)
      .fontSize(14)
      .layoutWeight(1)
      .enabled(!this.savingAvatar && current !== avatar)
      .onClick(() => { this.saveAvatar(avatar); })
  }

  private open(peer: Member): void {
    this.showStickers = false;
    this.tab = Tab.Messages;
    this.chat.open(peer);
  }

  private async saveNickname(): Promise<void> {
    if (this.savingNickname || !this.nicknameDraft.trim()) {
      return;
    }
    this.savingNickname = true;
    this.nicknameSaved = false;
    const saved = await this.chat.changeNickname(this.nicknameDraft);
    if (saved) {
      this.nicknameDraft = '';
      this.nicknameSaved = true;
    }
    this.savingNickname = false;
  }

  private async createAccount(): Promise<void> {
    if (this.creatingAccount || !this.newNickname.trim()) {
      return;
    }
    this.creatingAccount = true;
    const account = await this.chat.createAccount(this.newNickname);
    if (account) {
      this.issuedCode = account.code;
      this.issuedName = account.member.nickname;
      this.newNickname = '';
    }
    this.creatingAccount = false;
  }

  @Builder
  fieldLabel(label: string) {
    Text(label)
      .fontSize(13)
      .fontWeight(FontWeight.Medium)
      .fontColor($r('app.color.accent'))
  }

  @Builder
  notice() {
    if (this.chat.notice) {
      Row({ space: 8 }) {
        Text(this.chat.notice)
          .layoutWeight(1)
          .fontSize(13)
          .fontColor($r('app.color.negative'))
        Button('×', { type: ButtonType.Circle })
          .height(44)
          .width(44)
          .fontSize(20)
          .backgroundColor(Color.Transparent)
          .fontColor($r('app.color.negative'))
          .accessibilityText('关闭提示')
          .onClick(() => { this.chat.notice = ''; })
      }
      .width('100%')
      .padding({ left: 14, right: 4 })
      .backgroundColor($r('app.color.wash'))
      .borderRadius(12)
    }
  }

  @Builder
  login() {
    Scroll() {
      Column({ space: 20 }) {
        PortalHero()
        Column({ space: 6 }) {
          Text('Portal Pet')
            .fontSize(34)
            .fontWeight(FontWeight.Bold)
            .fontColor($r('app.color.ink'))
          Text('输入管理员发放的 6 位安全码。')
            .fontSize(15)
            .fontColor($r('app.color.muted'))
        }
        .width('100%')
        .alignItems(HorizontalAlign.Start)

        Column({ space: 8 }) {
          this.fieldLabel('安全码')
          Stack() {
            Row({ space: 6 }) {
              ForEach([0, 1, 2, 3, 4, 5], (index: number) => {
                Text(index < this.code.length ? '•' : ' ')
                  .fontSize(20)
                  .fontColor($r('app.color.ink'))
                  .height(46)
                  .layoutWeight(1)
                  .textAlign(TextAlign.Center)
                  .backgroundColor($r('app.color.background'))
                  .borderRadius(10)
                  .border({
                    width: this.codeFocused && index === this.code.length ? 2 : 1,
                    color: this.codeFocused && index === this.code.length ? $r('app.color.accent') : $r('app.color.line')
                  })
              }, (index: number) => index.toString())
            }
            .width('100%')
            .accessibilityLevel('no-hide-descendants')
            TextInput({ text: this.code })
              .width('100%')
              .height(46)
              .type(InputType.Password)
              .showPasswordIcon(false)
              .backgroundColor(Color.Transparent)
              .fontColor(Color.Transparent)
              .caretColor(Color.Transparent)
              .enabled(!this.chat.activating && !this.chat.signingOut)
              .accessibilityText('6 位安全码，输满后自动登录')
              .onFocus(() => { this.codeFocused = true; })
              .onBlur(() => { this.codeFocused = false; })
              .onChange((value: string) => { this.enterCode(value); })
          }
          .width('100%')

          this.notice()
          if (this.chat.activating) {
            Text('正在验证…').fontSize(13).fontColor($r('app.color.muted'))
          }
        }
        .width('100%')
        .alignItems(HorizontalAlign.Start)
        .padding(16)
        .backgroundColor($r('app.color.surface'))
        .border({ width: 1, color: $r('app.color.line') })
        .borderRadius(22)
      }
      .width('100%')
      .constraintSize({ maxWidth: 500 })
      .padding({ left: 20, right: 20, top: 24, bottom: 30 })
    }
    .width('100%')
    .height('100%')
    .align(Alignment.Top)
    .scrollBar(BarState.Off)
  }

  @Builder
  header() {
    Row({ space: 10 }) {
      Text(this.tab)
        .layoutWeight(1)
        .fontSize(30)
        .fontWeight(FontWeight.Bold)
        .fontColor($r('app.color.ink'))
      Text(this.chat.connection)
        .fontSize(12)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.chat.connection === '已连接' ? $r('app.color.positive') : $r('app.color.muted'))
        .padding({ left: 10, right: 10, top: 6, bottom: 6 })
        .backgroundColor($r('app.color.wash'))
        .borderRadius(12)
      if (this.tab !== Tab.Settings) {
        Button(this.chat.refreshing ? '同步中' : '刷新', { type: ButtonType.Normal })
          .height(44)
          .fontSize(13)
          .fontColor($r('app.color.accent'))
          .backgroundColor(Color.Transparent)
          .borderRadius(12)
          .enabled(!this.chat.refreshing)
          .onClick(() => { this.chat.refresh(); })
      }
    }
    .width('100%')
    .padding({ top: 10, bottom: 12 })
  }

  @Builder
  tabButton(tab: Tab) {
    Button({ type: ButtonType.Normal }) {
      Row({ space: 6 }) {
        Text(tab)
          .fontSize(15)
          .fontWeight(this.tab === tab ? FontWeight.Bold : FontWeight.Medium)
          .fontColor(this.tab === tab ? $r('app.color.accent') : $r('app.color.muted'))
        if (tab === Tab.Messages) {
          CountBadge({ count: this.chat.getUnreadCount() })
        } else if (tab === Tab.Members) {
          CountBadge({ count: this.chat.getRequestCount() })
        }
      }
    }
    .layoutWeight(1)
    .height(46)
    .padding({ left: 6, right: 6 })
    .borderRadius(16)
    .backgroundColor(this.tab === tab ? $r('app.color.wash') : Color.Transparent)
    .onClick(() => { this.tab = tab; })
  }

  @Builder
  empty(title: string, detail: string) {
    Column({ space: 9 }) {
      PetAvatar({ size: 76 })
      Text(title)
        .fontSize(19)
        .fontWeight(FontWeight.Bold)
        .fontColor($r('app.color.ink'))
      Text(detail)
        .fontSize(14)
        .textAlign(TextAlign.Center)
        .fontColor($r('app.color.muted'))
    }
    .width('100%')
    .padding({ left: 18, right: 18, top: 26, bottom: 26 })
    .backgroundColor($r('app.color.surface'))
    .borderRadius(22)
  }

  @Builder
  chatRow(person: Person) {
    Button({ type: ButtonType.Normal }) {
      Row({ space: 12 }) {
        PetAvatar({ memberId: person.member.id, avatar: person.member.avatar ?? 'default' })
        Column({ space: 4 }) {
          Text(person.member.nickname)
            .fontSize(17)
            .fontWeight(FontWeight.Bold)
            .fontColor($r('app.color.ink'))
            .maxLines(1)
            .textOverflow({ overflow: TextOverflow.Ellipsis })
          Text(this.chat.latest(person.member.id)?.body ?? (person.online ? '在线' : '离线'))
            .fontSize(13)
            .fontColor($r('app.color.muted'))
            .maxLines(1)
            .textOverflow({ overflow: TextOverflow.Ellipsis })
        }
        .layoutWeight(1)
        .alignItems(HorizontalAlign.Start)
        CountBadge({ count: this.chat.unreadFor(person.member.id) })
      }
      .width('100%')
      .padding(12)
    }
    .width('100%')
    .padding(0)
    .borderRadius(16)
    .backgroundColor($r('app.color.surface'))
    .onClick(() => this.open(person.member))
  }

  @Builder
  smallAction(label: string, peerId: number, action: () => void) {
    Button(label, { type: ButtonType.Normal })
      .height(44)
      .borderRadius(10)
      .fontSize(13)
      .fontColor($r('app.color.accent'))
      .backgroundColor($r('app.color.wash'))
      .enabled(!this.chat.pendingFriends.includes(peerId))
      .onClick(action)
  }

  @Builder
  memberRow(person: Person) {
    Column({ space: 8 }) {
      Row({ space: 12 }) {
        PetAvatar({ memberId: person.member.id, avatar: person.member.avatar ?? 'default' })
        Column({ space: 4 }) {
          Text(person.member.nickname)
            .fontSize(17)
            .fontWeight(FontWeight.Bold)
            .fontColor($r('app.color.ink'))
            .maxLines(1)
            .textOverflow({ overflow: TextOverflow.Ellipsis })
          Text(person.online ? '在线' : '离线')
            .fontSize(12)
            .fontColor(person.online ? $r('app.color.positive') : $r('app.color.muted'))
        }
        .alignItems(HorizontalAlign.Start)
        .layoutWeight(1)
        if (person.relationship === Relationship.Friend) {
          this.smallAction('聊天', person.member.id, () => this.open(person.member))
        } else if (person.relationship === Relationship.None) {
          this.smallAction('加好友', person.member.id, () => { this.chat.requestFriend(person.member.id); })
        } else if (person.relationship === Relationship.Outgoing) {
          Text('已申请').fontSize(13).fontColor($r('app.color.muted'))
        }
      }
      .width('100%')

      if (person.relationship === Relationship.Incoming) {
        Row({ space: 8 }) {
          Text('好友申请')
            .layoutWeight(1)
            .fontSize(13)
            .fontColor($r('app.color.accent'))
          this.smallAction('拒绝', person.member.id, () => { this.chat.answerFriend(person.member.id, false); })
          this.smallAction('同意', person.member.id, () => { this.chat.answerFriend(person.member.id, true); })
        }
        .width('100%')
      }
    }
    .width('100%')
    .padding(12)
    .backgroundColor($r('app.color.surface'))
    .borderRadius(16)
  }

  @Builder
  settings(member: Member) {
    Row({ space: 12 }) {
      PetAvatar({ memberId: member.id, avatar: member.avatar ?? 'default', size: 54, animate: true })
      Text(member.nickname)
        .fontSize(19)
        .fontWeight(FontWeight.Bold)
        .fontColor($r('app.color.ink'))
    }
    .width('100%')
    .padding(14)
    .borderRadius(16)
    .backgroundColor($r('app.color.surface'))

    Column({ space: 8 }) {
      this.fieldLabel('我的昵称')
      TextInput({ text: this.nicknameDraft, placeholder: member.nickname })
        .maxLength(64)
        .enabled(!this.savingNickname)
        .accessibilityText('我的昵称')
        .onChange((value: string) => {
          this.nicknameDraft = value;
          this.nicknameSaved = false;
        })
      Button(this.savingNickname ? '正在保存…' : '保存昵称')
        .enabled(!this.savingNickname && !!this.nicknameDraft.trim())
        .onClick(() => { this.saveNickname(); })
      if (this.nicknameSaved) {
        Text('昵称已保存').fontSize(13).fontColor($r('app.color.muted'))
      }
    }
    .width('100%')
    .padding(14)
    .alignItems(HorizontalAlign.Start)
    .borderRadius(16)
    .backgroundColor($r('app.color.surface'))

    Column({ space: 8 }) {
      this.fieldLabel('我的形象')
      Row({ space: 8 }) {
        this.avatarOption('默认', 'default', member.avatar ?? 'default')
        this.avatarOption('蓝色机器人', 'blue', member.avatar ?? 'default')
        this.avatarOption('紫色机器人', 'purple', member.avatar ?? 'default')
      }
    }
    .width('100%')
    .padding(14)
    .alignItems(HorizontalAlign.Start)
    .borderRadius(16)
    .backgroundColor($r('app.color.surface'))

    if (this.chat.isAdmin) {
      Column({ space: 8 }) {
        this.fieldLabel('管理员')
        TextInput({ text: this.newNickname, placeholder: '新账号昵称' })
          .maxLength(64)
          .enabled(!this.creatingAccount)
          .onChange((value: string) => { this.newNickname = value; })
        Button(this.creatingAccount ? '正在创建…' : '创建账号')
          .enabled(!this.creatingAccount && !!this.newNickname.trim())
          .onClick(() => { this.createAccount(); })
        if (this.issuedCode) {
          Text(this.issuedName + ' 的安全码  ' + this.issuedCode)
            .fontSize(16)
            .fontColor($r('app.color.ink'))
        }
      }
      .width('100%')
      .padding(14)
      .alignItems(HorizontalAlign.Start)
      .borderRadius(16)
      .backgroundColor($r('app.color.surface'))
    }

    Row({ space: 12 }) {
      Column({ space: 5 }) {
        Text('免打扰')
          .fontSize(16)
          .fontWeight(FontWeight.Medium)
          .fontColor($r('app.color.ink'))
        Text('隐藏前台传送门提示，消息仍会计入未读。')
          .fontSize(13)
          .fontColor($r('app.color.muted'))
      }
      .layoutWeight(1)
      .alignItems(HorizontalAlign.Start)
      Toggle({ type: ToggleType.Switch, isOn: this.chat.quiet })
        .selectedColor('#6241BC')
        .accessibilityText('免打扰')
        .onChange((quiet: boolean) => { this.chat.setQuiet(quiet); })
    }
    .width('100%')
    .padding(14)
    .borderRadius(16)
    .backgroundColor($r('app.color.surface'))

    Button(this.chat.signingOut ? '正在退出…' : '退出登录', { type: ButtonType.Normal })
      .width('100%')
      .height(48)
      .fontColor($r('app.color.negative'))
      .borderRadius(15)
      .backgroundColor($r('app.color.surface'))
      .enabled(!this.chat.signingOut)
      .onClick(() => { this.chat.logout(); })
  }

  @Builder
  dashboard(member: Member) {
    Column() {
      this.header()
      this.notice()
      List({ space: 10 }) {
        if (this.tab === Tab.Settings) {
          ListItem() {
            Column({ space: 10 }) {
              this.settings(member)
            }
            .width('100%')
          }
        } else if (!this.chat.directoryLoaded) {
          ListItem() {
            this.empty('正在同步', '正在读取服务器上的成员与会话。')
          }
        } else if (this.tab === Tab.Messages) {
          if (this.chat.friends().length === 0) {
            ListItem() {
              Column({ space: 10 }) {
                this.empty('还没有好友会话', '添加好友后，就能在这里收发消息。')
                Button('查看成员')
                  .height(44)
                  .backgroundColor('#6241BC')
                  .onClick(() => { this.tab = Tab.Members; })
              }
              .width('100%')
            }
          }
          ForEach(this.chat.friends(), (person: Person) => {
            ListItem() {
              this.chatRow(person)
            }
          }, (person: Person) => person.member.id + ':' + person.member.nickname + ':' + person.online)
        } else {
          if (this.chat.people.length === 0) {
            ListItem() {
              this.empty('暂无其他成员', '其他成员激活后会出现在这里。')
            }
          }
          ForEach(this.chat.people, (person: Person) => {
            ListItem() {
              this.memberRow(person)
            }
          }, (person: Person) =>
            person.member.id + ':' + person.member.nickname + ':' + person.online + ':' + person.relationship)
        }
      }
      .width('100%')
      .layoutWeight(1)
      .padding({ top: 8, bottom: 10 })
      .scrollBar(BarState.Auto)

      Row({ space: 6 }) {
        this.tabButton(Tab.Messages)
        this.tabButton(Tab.Members)
        this.tabButton(Tab.Settings)
      }
      .width('100%')
      .padding({ left: 5, right: 5, top: 5, bottom: 5 })
      .backgroundColor($r('app.color.surface'))
      .borderRadius(20)
      .margin({ bottom: 10 })
    }
    .width('100%')
    .height('100%')
    .padding({ left: 16, right: 16 })
    .constraintSize({ maxWidth: 660 })
  }

  @Builder
  stickerPicker() {
    Column({ space: 6 }) {
      Row({ space: 6 }) {
        TextInput({ placeholder: '搜索表情', text: this.stickerQuery })
          .height(36)
          .layoutWeight(1)
          .onChange((text: string) => {
            this.stickerQuery = text;
            if (text.trim() && this.stickerCategory === '最近') { this.stickerCategory = '笑脸'; }
          })
        Button('关闭').fontSize(13).onClick(() => { this.showStickers = false; })
      }
      Scroll() {
        Row({ space: 5 }) {
          ForEach(['最近', '收藏', ...EMOJI_GROUPS.map((group: EmojiGroup) => group.name)], (name: string) => {
            Button(name, { type: ButtonType.Normal })
              .height(30).fontSize(12).borderRadius(8)
              .fontColor($r('app.color.accent'))
              .backgroundColor(name === this.stickerCategory ? '#e9dcfa' : $r('app.color.wash'))
              .onClick(() => { this.stickerCategory = name; this.stickerQuery = ''; })
          }, (name: string) => name)
        }
      }.scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Off).width('100%')
      Scroll() {
        Column({ space: 7 }) {
          if (this.shelf.choices(this.stickerCategory, this.stickerQuery).length === 0 && this.stickerCategory !== '收藏') {
            Text(this.stickerQuery ? '没有找到表情' : '用过的表情会出现在这里')
              .fontSize(13).fontColor($r('app.color.muted'))
          }
          Flex({ wrap: FlexWrap.Wrap }) {
            ForEach(this.shelf.choices(this.stickerCategory, this.stickerQuery), (item: EmojiItem) => {
              Stack({ alignContent: Alignment.TopEnd }) {
                Button(item.glyph, { type: ButtonType.Normal })
                  .width(44).height(39).fontSize(23)
                  .backgroundColor(Color.Transparent)
                  .accessibilityText(item.name)
                  .onClick(() => {
                    const text = this.chat.draft + item.glyph;
                    if (Array.from(text).length <= 2000) {
                      this.chat.editDraft(text);
                      this.shelf.used(item.glyph).catch(() => { this.chat.notice = '无法保存最近使用。'; });
                    }
                  })
                Button(this.shelf.favorites.includes(item.glyph) ? '★' : '☆', { type: ButtonType.Normal })
                  .width(23).height(22).padding(0).fontSize(10)
                  .fontColor('#76501F').backgroundColor(Color.Transparent)
                  .accessibilityText(this.shelf.favorites.includes(item.glyph) ? '取消收藏 ' + item.name : '收藏 ' + item.name)
                  .onClick(() => {
                    this.shelf.toggle(item.glyph).catch(() => { this.chat.notice = '无法保存表情收藏。'; });
                  })
              }
              .width(47).height(39)
            }, (item: EmojiItem) => item.glyph)
          }
          if (this.stickerCategory === '收藏' && !this.stickerQuery.trim()) {
            Row({ space: 6 }) {
              Text('图片表情').layoutWeight(1).fontSize(14).fontWeight(FontWeight.Medium)
              Button('导入图片').fontSize(12)
                .enabled(!this.selectingImage && !this.shelf.busy).onClick(() => { this.pickImage(true); })
              Button(this.manageStickers ? '完成' : '管理').fontSize(12)
                .enabled(this.shelf.pictures.length > 0).onClick(() => { this.manageStickers = !this.manageStickers; })
            }.width('100%')
            if (this.shelf.pictures.length === 0) {
              Text('点聊天图片的 ☆，或导入自己的图片').fontSize(13).fontColor($r('app.color.muted'))
            }
            Flex({ wrap: FlexWrap.Wrap }) {
              ForEach(this.shelf.pictures, (picture: StickerPicture) => {
                Button({ type: ButtonType.Normal }) {
                  Stack({ alignContent: Alignment.TopEnd }) {
                    Image(fileUri.getUriFromPath(picture.path))
                      .width(48).height(48).objectFit(ImageFit.Contain)
                    if (this.manageStickers) { Text('×').fontSize(13).fontColor('#76501F') }
                  }
                }
                .width(55).height(55).padding(3).margin(2)
                .backgroundColor($r('app.color.wash'))
                .enabled(!this.shelf.busy && !this.selectingImage)
                .accessibilityText(this.manageStickers ? '移除图片表情' : '使用图片表情')
                .onClick(() => { this.usePicture(picture); })
              }, (picture: StickerPicture) => picture.name)
            }
          }
        }.width('100%').alignItems(HorizontalAlign.Start)
      }.height(155).width('100%').align(Alignment.TopStart)
    }
    .padding({ left: 12, right: 12, top: 8, bottom: 8 })
    .width('100%')
    .backgroundColor($r('app.color.surface'))
  }

  @Builder
  conversation() {
    Column() {
      Row({ space: 10 }) {
        Button('‹', { type: ButtonType.Circle })
          .width(44)
          .height(44)
          .fontSize(28)
          .fontColor($r('app.color.accent'))
          .backgroundColor($r('app.color.wash'))
          .accessibilityText('返回消息列表')
          .onClick(() => { this.showStickers = false; this.chat.closeChat(); })
        PetAvatar({ memberId: this.chat.peer?.id ?? -1, avatar: this.chat.peer?.avatar ?? 'default', size: 40 })
        Text(this.chat.peer?.nickname ?? '')
          .layoutWeight(1)
          .fontSize(20)
          .fontWeight(FontWeight.Bold)
          .fontColor($r('app.color.ink'))
          .maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis })
        Button(this.chat.ending ? '正在结束…' : '结束聊天')
          .fontSize(13)
          .fontColor($r('app.color.accent'))
          .backgroundColor($r('app.color.wash'))
          .enabled(!this.chat.ending && !this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1))
          .onClick(() => { this.chat.endConversation(); })
      }
      .width('100%')
      .padding({ left: 16, right: 16, top: 8, bottom: 8 })
      this.notice()

      Stack() {
        MessageHistory({
          messages: this.chat.messagesFor(this.chat.peer?.id ?? -1),
          memberId: this.chat.member?.id ?? -1,
          peerId: this.chat.peer?.id ?? -1,
          loadImage: (message: ChatMessage) => this.chat.imageData(message),
          saveImage: (message: ChatMessage) => { this.saveChatImage(message); }
        })
        if (this.droppingImage) {
          Text('松开添加图片')
            .padding(16).borderRadius(14)
            .fontColor($r('app.color.accent')).backgroundColor($r('app.color.surface'))
            .hitTestBehavior(HitTestMode.None)
        }
      }
      .width('100%')
      .layoutWeight(1)
      .allowDrop([uniformTypeDescriptor.UniformDataType.IMAGE])
      .onDragEnter(() => { this.droppingImage = true; })
      .onDragLeave(() => { this.droppingImage = false; })
      .onDrop((event: DragEvent) => { this.dropImage(event); })

      if (this.showStickers) { this.stickerPicker() }
      Row({ space: 8 }) {
        Button('☺')
          .height(44).width(42).fontSize(18)
          .backgroundColor($r('app.color.wash')).fontColor($r('app.color.accent'))
          .accessibilityText('表情与收藏')
          .enabled(!this.chat.ending && !this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1))
          .onClick(() => { this.toggleStickers(); })
        Button('图片')
          .height(44)
          .backgroundColor($r('app.color.wash'))
          .fontColor($r('app.color.accent'))
          .enabled(!this.selectingImage && !this.chat.ending && !this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1))
          .onClick(() => { this.showStickers = false; this.pickImage(); })
        if (this.chat.imageDraft) {
          Text('已选图片').layoutWeight(1).fontColor($r('app.color.ink'))
          Button('移除', { type: ButtonType.Normal })
            .enabled(!this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1))
            .onClick(() => { this.chat.imageDraft = undefined; })
        } else {
          TextArea({ placeholder: '输入消息', text: this.chat.draft })
            .layoutWeight(1)
            .height(50)
            .maxLength(4000)
            .fontSize(16)
            .fontColor($r('app.color.ink'))
            .placeholderColor($r('app.color.muted'))
            .backgroundColor($r('app.color.wash'))
            .borderRadius(14)
            .enabled(!this.chat.ending && !this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1))
            .accessibilityText('输入消息')
            .onChange((value: string) => this.chat.editDraft(value))
        }
        Button(this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1) ? '发送中' : '发送')
          .height(44)
          .backgroundColor('#6241BC')
          .enabled((!!this.chat.imageDraft || !!this.chat.draft.trim()) && !this.selectingImage && !this.chat.ending &&
            !this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1))
          .onClick(() => { this.showStickers = false; this.chat.send(); })
      }
      .width('100%')
      .padding({ left: 14, right: 14, top: 8, bottom: 8 })
      .backgroundColor($r('app.color.surface'))
    }
    .width('100%')
    .height('100%')
    .constraintSize({ maxWidth: 760 })
  }

  build() {
    Stack({ alignContent: Alignment.Top }) {
      if (this.chat.starting) {
        Column({ space: 12 }) {
          LoadingProgress().width(36).height(36).color($r('app.color.accent'))
          Text('正在读取登录状态…').fontSize(14).fontColor($r('app.color.muted'))
        }
        .width('100%')
        .height('100%')
        .justifyContent(FlexAlign.Center)
      } else if (!this.chat.member) {
        this.login()
      } else if (this.chat.peer) {
        this.conversation()
      } else {
        this.dashboard(this.chat.member)
      }

      if (this.chat.member && this.chat.arrival) {
        ForEach([this.chat.arrival], (message: ChatMessage) => {
          Column() {
            ArrivalCard({
              senderId: message.sender.id,
              senderAvatar: message.sender.avatar ?? 'default',
              sender: message.sender.nickname,
              body: message.body,
              onOpen: () => this.open(message.sender),
              onDismiss: () => this.chat.dismissArrival()
            })
          }
          .width('100%')
          .constraintSize({ maxWidth: 490 })
          .padding({ top: 5, left: 14, right: 14 })
        }, (message: ChatMessage) => message.id.toString())
      }
    }
    .width('100%')
    .height('100%')
    .backgroundColor($r('app.color.background'))
  }
}
