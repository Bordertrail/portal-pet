# Portal Pet 鸿蒙客户端

原生 ArkTS 与 ArkUI 工程，最低兼容 HarmonyOS 5.0（API 12），面向手机和平板。支持安全码激活、成员列表、好友申请与审批、私聊、历史消息、未读数和免打扰。

## 运行

1. 用 DevEco Studio 打开本目录，等待工程同步。选择 `entry` 运行配置和 HarmonyOS 5.0 或更新版本的设备。在 **File > Project Structure > Project > Signing Configs** 中配置调试签名后运行。 [安装 DevEco Studio](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-software-install) · [配置调试签名](https://developer.huawei.com/consumer/cn/doc/doccenter-deveco-studio/ide-signing-auto)
2. 构建前，在 `entry/src/main/ets/chat/Protocol.ets` 中将 `DEFAULT_SERVER_ADDRESS` 改为实际 HTTPS 地址。`https://chat.example.com` 是示例地址。填写或粘贴管理员发放的 6 位安全码后自动登录。 [服务端部署及发码](../deploy/README.md)
3. 鸿蒙端和桌面端或 iPhone 端联调时，分别使用两个成员的安全码。同一成员在新设备重新激活会使旧设备的会话失效。

## 联调

1. 鸿蒙端进入「成员」，发起好友申请。在另一端接受后，打开鸿蒙端「消息」中的对应会话并发送消息。
2. 鸿蒙端返回消息列表，让另一端发来一条消息。前台会显示像素宠物和消息气泡，点击气泡打开该会话。
3. 在「设置」开启免打扰，再从另一端发消息。未读数仍会更新。切到后台后再返回，App 会重连并拉取近期消息。
4. 关闭并重新打开 App，验证登录状态保留。退出登录后，应回到安全码激活页。

传送门动画在 App 前台显示。系统推送需要另行接入华为 Push Kit 和服务端推送。
