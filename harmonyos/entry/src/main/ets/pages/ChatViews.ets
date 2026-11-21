import { image } from '@kit.ImageKit';
import { ChatMessage } from '../chat/Protocol';

function petImage(memberId: number, avatar: string = 'default'): Resource {
  if (avatar === 'blue') {
    return $r('app.media.avatar_blue');
  }
  if (avatar === 'purple') {
    return $r('app.media.avatar_purple');
  }
  switch (memberId % 3) {
    case 0:
      return $r('app.media.pet_peach');
    case 1:
      return $r('app.media.pet_blue');
    case 2:
      return $r('app.media.pet_purple');
    default:
      return $r('app.media.pet_green');
  }
}

@Component
export struct PetAvatar {
  @Prop memberId: number = -1;
  @Prop avatar: string = 'default';
  size: number = 46;
  animate: boolean = false;
  @State breathing: boolean = false;

  build() {
    Image(petImage(this.memberId, this.avatar))
      .width(this.size)
      .height(this.size)
      .objectFit(ImageFit.Contain)
      .scale({ x: this.breathing ? 1.014 : 1, y: this.breathing ? 0.965 : 1, centerY: '96%' })
      .backgroundColor($r('app.color.wash'))
      .borderRadius(this.size * 0.3)
      .accessibilityLevel('no')
      .onAppear(() => {
        if (this.animate) {
          this.getUIContext().animateTo({
            duration: 1600, curve: Curve.EaseInOut, iterations: -1, playMode: PlayMode.Alternate
          }, () => { this.breathing = true; })
        }
      })
  }
}

@Component
struct WalkingAvatar {
  @Prop memberId: number = -1;
  @Prop avatar: string = 'default';
  size: number = 47;
  @State step: number = 0;

  private legLine(): number {
    return this.size * 0.82;
  }

  private joint(): number {
    return this.legLine() - this.size * 0.02;
  }

  @Builder
  leg(left: boolean) {
    Stack({ alignContent: Alignment.TopStart }) {
      Image(petImage(this.memberId, this.avatar))
        .width(this.size)
        .height(this.size)
        .position({ x: left ? 0 : -this.size / 2, y: -this.joint() })
    }
    .width(this.size / 2)
    .height(this.size - this.joint())
    .clip(true)
    .position({ x: left ? 0 : this.size / 2, y: this.joint() })
    .translate({
      x: this.step === (left ? 1 : 2) ? (left ? -1 : 1) : 0,
      y: this.step === (left ? 1 : 2) ? -2 : 0
    })
  }

  build() {
    Stack({ alignContent: Alignment.TopStart }) {
      if (this.avatar === 'default') {
        Image(petImage(this.memberId))
          .width(this.size)
          .height(this.size)
      } else {
        Stack({ alignContent: Alignment.TopStart }) {
          Image(petImage(this.memberId, this.avatar))
            .width(this.size)
            .height(this.size)
            .position({ x: 0, y: 0 })
        }
        .width(this.size)
        .height(this.legLine())
        .clip(true)
        this.leg(true)
        this.leg(false)
      }
    }
    .width(this.size)
    .height(this.size)
    .onAppear(() => {
      if (this.avatar === 'default') {
        return;
      }
      this.getUIContext().keyframeAnimateTo({ iterations: 2 }, [
        { duration: 55, event: () => { this.step = 1; } },
        { duration: 55, event: () => { this.step = 0; } },
        { duration: 55, event: () => { this.step = 2; } },
        { duration: 55, event: () => { this.step = 0; } }
      ])
    })
  }
}

@Component
export struct CountBadge {
  @Prop count: number = 0;

  build() {
    if (this.count > 0) {
      Text(this.count > 99 ? '99+' : this.count.toString())
        .fontSize(11)
        .fontWeight(FontWeight.Bold)
        .fontColor(Color.White)
        .padding({ left: 7, right: 7, top: 3, bottom: 3 })
        .backgroundColor('#6241BC')
        .borderRadius(10)
        .accessibilityText(this.count + ' 条待处理')
    }
  }
}

@Component
export struct PortalHero {
  build() {
    Stack({ alignContent: Alignment.Bottom }) {
      Column()
        .width(104)
        .height(137)
        .borderRadius({ topLeft: 54, topRight: 54, bottomLeft: 12, bottomRight: 12 })
        .border({ width: 2, color: '#B99AFF' })
        .linearGradient({
          direction: GradientDirection.RightBottom,
          colors: [['#DDC2FF', 0], ['#9572EB', 0.46], ['#543396', 1]]
        })
        .shadow({ radius: 24, color: '#B28AF4' })
        .margin({ bottom: 22 })
      Image($r('app.media.pet_green'))
        .width(115)
        .height(115)
        .objectFit(ImageFit.Contain)
        .offset({ x: 29, y: 4 })
    }
    .width('100%')
    .height(184)
    .borderRadius(26)
    .linearGradient({ direction: GradientDirection.RightBottom, colors: [['#F4E7FF', 0], ['#E1DAFF', 1]] })
    .accessibilityLevel('no')
  }
}

@Component
export struct ArrivalCard {
  @Prop senderId: number = -1;
  @Prop senderAvatar: string = 'default';
  @Prop sender: string = '';
  @Prop body: string = '';
  onOpen: () => void = () => {};
  onDismiss: () => void = () => {};
  @State entered: boolean = false;

  build() {
    Row({ space: 8 }) {
      Button({ type: ButtonType.Normal }) {
        Row({ space: 11 }) {
          Stack() {
            Circle()
              .width(44)
              .height(44)
              .fill('#A883F3')
              .shadow({ radius: 9, color: '#AD85FF' })
            WalkingAvatar({ memberId: this.senderId, avatar: this.senderAvatar })
              .translate({ x: this.entered ? 4 : -10, y: this.entered ? 1 : 9 })
              .opacity(this.entered ? 1 : 0)
          }
          .width(50)
          .height(54)
          .accessibilityLevel('no')
          Column({ space: 4 }) {
            Text(this.sender)
              .fontSize(14)
              .fontWeight(FontWeight.Bold)
              .fontColor($r('app.color.ink'))
              .maxLines(1)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
            Text(this.body)
              .fontSize(14)
              .fontColor($r('app.color.muted'))
              .maxLines(2)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
          }
          .layoutWeight(1)
          .alignItems(HorizontalAlign.Start)
        }
        .width('100%')
      }
      .layoutWeight(1)
      .padding({ left: 0, right: 0 })
      .backgroundColor(Color.Transparent)
      .accessibilityText('打开' + this.sender + '的来信，' + this.body)
      .onClick(() => this.onOpen())

      Button('×', { type: ButtonType.Circle })
        .width(44)
        .height(44)
        .fontSize(22)
        .fontColor($r('app.color.muted'))
        .backgroundColor($r('app.color.wash'))
        .accessibilityText('关闭来信提示')
        .onClick(() => this.onDismiss())
    }
    .width('100%')
    .padding({ left: 10, right: 12, top: 8, bottom: 8 })
    .backgroundColor($r('app.color.surface'))
    .borderRadius(20)
    .border({ width: 1, color: $r('app.color.line') })
    .shadow({ radius: 22, color: '#38493167', offsetY: 6 })
    .translate({ y: this.entered ? 0 : -18 })
    .opacity(this.entered ? 1 : 0)
    .onAppear(() => {
      this.getUIContext().animateTo({ duration: 260, curve: Curve.EaseOut }, () => {
        this.entered = true;
      });
    })
  }
}

@Component
struct ChatPhoto {
  @Prop message: ChatMessage;
  loadImage: (message: ChatMessage) => Promise<ArrayBuffer> = async () => new ArrayBuffer(0);
  saveImage: (message: ChatMessage) => void = () => {};
  @State bitmap?: image.PixelMap;
  @State failed: boolean = false;
  private loading: boolean = false;
  private visible: boolean = false;

  aboutToAppear(): void {
    this.visible = true;
  }

  aboutToDisappear(): void {
    this.visible = false;
    const bitmap = this.bitmap;
    this.bitmap = undefined;
    if (bitmap) {
      bitmap.release().catch(() => {});
    }
  }

  private async load(): Promise<void> {
    if (this.loading || this.bitmap || !this.visible) {
      return;
    }
    this.loading = true;
    this.failed = false;
    try {
      const data = await this.loadImage(this.message);
      const source = image.createImageSource(data);
      try {
        const info = this.message.image;
        const scale = Math.min(1, 640 / Math.max(info?.width ?? 1, info?.height ?? 1));
        const size: image.Size = {
          width: Math.max(1, Math.round((info?.width ?? 1) * scale)),
          height: Math.max(1, Math.round((info?.height ?? 1) * scale))
        };
        const bitmap = await source.createPixelMap({ desiredSize: size });
        if (this.visible) {
          this.bitmap = bitmap;
        } else {
          await bitmap.release();
        }
      } finally {
        await source.release();
      }
    } catch {
      if (this.visible) {
        this.failed = true;
      }
    } finally {
      this.loading = false;
    }
  }

  build() {
    Stack({ alignContent: Alignment.TopEnd }) {
      Stack() {
        if (this.bitmap) {
          Image(this.bitmap).width('100%').height('100%').objectFit(ImageFit.Contain).accessibilityText('聊天图片')
        } else if (this.failed) {
          Button('图片未加载，点此重试').fontSize(12).onClick(() => { this.load(); })
        } else {
          LoadingProgress().width(22).height(22).accessibilityText('正在加载图片')
        }
      }.width('100%').height('100%')
      if (this.bitmap) {
        Button('☆', { type: ButtonType.Circle })
          .width(32).height(32).fontSize(18)
          .fontColor($r('app.color.accent'))
          .backgroundColor($r('app.color.surface'))
          .margin(5)
          .accessibilityText('收藏为表情')
          .onClick(() => this.saveImage(this.message))
      }
    }
    .width(185)
    .height(Math.min(185, Math.max(68, 185 * (this.message.image?.height ?? 1) / (this.message.image?.width ?? 1))))
    .backgroundColor($r('app.color.wash'))
    .borderRadius(12)
    .clip(true)
    .onVisibleAreaChange([0.1], (visible: boolean) => {
      if (visible && !this.failed) {
        this.load();
      }
    })
  }
}

@Component
export struct MessageHistory {
  @Prop @Watch('messagesChanged') messages: ChatMessage[] = [];
  @Prop memberId: number = -1;
  @Prop @Watch('peerChanged') peerId: number = -1;
  loadImage: (message: ChatMessage) => Promise<ArrayBuffer> = async () => new ArrayBuffer(0);
  saveImage: (message: ChatMessage) => void = () => {};
  private scroller: Scroller = new Scroller();
  private timer: number = -1;
  private endIndex: number = -1;
  private previousCount: number = 0;

  private messagesChanged(): void {
    const atBottom = this.endIndex >= this.previousCount - 2;
    this.previousCount = this.messages.length;
    if (atBottom || this.messages[this.messages.length - 1]?.sender.id === this.memberId) {
      this.scrollToLatest();
    }
  }

  private peerChanged(): void {
    this.endIndex = -1;
    this.scrollToLatest();
  }

  private scrollToLatest(): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.scroller.scrollEdge(Edge.Bottom), 30);
  }

  aboutToDisappear(): void {
    clearTimeout(this.timer);
  }

  private timeLabel(seconds: number): string {
    const date = new Date(seconds * 1000);
    const hours = date.getHours().toString().padStart(2, '0');
    const minutes = date.getMinutes().toString().padStart(2, '0');
    const time = hours + ':' + minutes;
    const today = new Date();
    if (date.getDate() === today.getDate() && date.getMonth() === today.getMonth() &&
      date.getFullYear() === today.getFullYear()) {
      return time;
    }
    return (date.getMonth() + 1) + '月' + date.getDate() + '日 ' + time;
  }

  @Builder
  messageBubble(message: ChatMessage) {
    Row({ space: 8 }) {
      if (message.sender.id !== this.memberId) {
        PetAvatar({ memberId: this.peerId, avatar: message.sender.avatar ?? 'default', size: 34 })
      }
      Column({ space: 5 }) {
        if (message.image) {
          ChatPhoto({ message: message, loadImage: this.loadImage, saveImage: this.saveImage })
        } else {
          Text(message.body)
            .fontSize(16)
            .fontColor(message.sender.id === this.memberId ? Color.White : $r('app.color.ink'))
            .wordBreak(WordBreak.BREAK_WORD)
            .padding({ left: 14, right: 14, top: 9, bottom: 9 })
            .backgroundColor(message.sender.id === this.memberId ? '#6241BC' : $r('app.color.surface'))
            .borderRadius(16)
        }
        Text(this.timeLabel(message.sent_at))
          .fontSize(11)
          .fontColor($r('app.color.muted'))
      }
      .constraintSize({ maxWidth: '78%' })
      .alignItems(message.sender.id === this.memberId ? HorizontalAlign.End : HorizontalAlign.Start)
    }
    .width('100%')
    .alignItems(VerticalAlign.Top)
    .justifyContent(message.sender.id === this.memberId ? FlexAlign.End : FlexAlign.Start)
  }

  build() {
    List({ space: 13, scroller: this.scroller, initialIndex: Math.max(0, this.messages.length - 1) }) {
      if (this.messages.length === 0) {
        ListItem() {
          Text('发一条消息，和好友打个招呼。')
            .width('100%')
            .textAlign(TextAlign.Center)
            .fontSize(14)
            .fontColor($r('app.color.muted'))
            .padding(16)
        }
      }
      ForEach(this.messages, (message: ChatMessage) => {
        ListItem() {
          this.messageBubble(message)
        }
      }, (message: ChatMessage) => message.id.toString())
    }
    .width('100%')
    .height('100%')
    .padding({ left: 18, right: 18, top: 12, bottom: 10 })
    .scrollBar(BarState.Auto)
    .onScrollIndex((_start: number, end: number) => {
      this.endIndex = end;
    })
    .onAppear(() => {
      this.previousCount = this.messages.length;
      this.scrollToLatest();
    })
  }
}
