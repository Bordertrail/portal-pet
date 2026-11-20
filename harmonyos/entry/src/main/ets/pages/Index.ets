import { common } from '@kit.AbilityKit';
import { ChatStore } from '../chat/ChatStore';
import { ChatMessage, Member, Person, Relationship, SECURITY_CODE_LENGTH } from '../chat/Protocol';
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

  aboutToAppear(): void {
    const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
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
          .onClick(() => this.chat.closeChat())
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

      Column() {
        MessageHistory({
          messages: this.chat.messagesFor(this.chat.peer?.id ?? -1),
          memberId: this.chat.member?.id ?? -1,
          peerId: this.chat.peer?.id ?? -1
        })
      }
      .width('100%')
      .layoutWeight(1)

      Row({ space: 8 }) {
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
        Button(this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1) ? '发送中' : '发送')
          .height(44)
          .backgroundColor('#6241BC')
          .enabled(!!this.chat.draft.trim() && !this.chat.ending &&
            !this.chat.sendingPeers.includes(this.chat.peer?.id ?? -1))
          .onClick(() => { this.chat.send(); })
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
