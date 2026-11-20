# Portal Pet 桌宠聊天

![原创像素信使机器人](desktop/icons/icon.png)

Rust 服务端使用 Axum、WebSocket 和 SQLite。桌面端使用 Tauri 和 Leptos。好友来信时，发送方的机器人会从传送门走出来，带着可点击的文字气泡。

公网部署使用 Docker Compose 和 Caddy，支持公网 IPv4 加端口或域名。 [Linux 部署、发码和备份步骤](deploy/README.md)

## 在 Mac 本机启动服务

在工作区根目录运行。默认使用 `portal.sqlite` 保存数据，监听 `127.0.0.1:8765`。可用环境变量 `PORTAL_DB` 指定数据库路径。

```sh
cargo run --locked -p portal-server -- serve
```

开另一个终端手动发放安全码。命令会输出成员编号和由大写字母、数字组成的 6 位安全码。安全码可重复使用，重新发码或停用成员会让原码失效。

```sh
cargo run --locked -p portal-server -- invite 示例甲
cargo run --locked -p portal-server -- invite 示例乙
```

重新发码时，将命令中的 `invite <昵称>` 换为 `reissue <成员编号>`。停用成员时换为 `disable <成员编号>`。新设备成功激活后旧凭据失效。服务端会在长连接心跳时复核登录凭据。

运营者可以用 `grant-admin <成员编号>` 为现有成员授予管理员权限。客户端的「设置」支持修改自己的昵称，管理员还可输入昵称创建普通账号。创建成功后，设置页显示该账号的 6 位安全码。

在「设置 → 我的形象」可以切换默认机器人、「蓝色机器人」和「紫色机器人」。选择保存在账号里，好友名单和来信动画会显示所选形象。两种机器人形象会有轻微的待机呼吸，在穿过传送门时交替迈步。

桌面端仅允许用 HTTP 连接本机回环地址。远程地址需要 HTTPS，服务端可放在支持 WSS 的 TLS 反向代理之后。SQLite 3.51.3 及更新版本启用 WAL，较旧版本的新数据库使用回滚日志。可使用带修复的 SQLite 库构建服务端。

## 从源码运行桌面端

需要 Rust 1.92 或更新版本、Trunk、`wasm32-unknown-unknown` 目标和对应平台的 Tauri 构建依赖。首次构建会下载 Cargo 依赖和 WebAssembly 构建工具。 [Tauri 平台构建依赖](https://v2.tauri.app/start/prerequisites/)

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cd interface
trunk build --locked
cd ..
cargo run --locked -p portal-pet
```

单击桌宠打开聊天。桌宠默认连接 `http://127.0.0.1:8765`，输入或粘贴 6 位安全码后自动验证。两人都激活并在线后，在成员列表中发起好友申请，对方同意即可聊天。macOS 将设备凭据保存在钥匙串，Windows 将设备凭据保存在凭据管理器。退出后可以用原安全码再次登录。

连接自己的公网服务时，启动桌宠前将 `PORTAL_PET_SERVER_ADDRESS` 设置为实际 HTTPS 地址。例如 `PORTAL_PET_SERVER_ADDRESS=https://chat.example.com cargo run --locked -p portal-pet`。PowerShell 中可用 `$env:PORTAL_PET_SERVER_ADDRESS = 'https://chat.example.com'`，再从该终端运行桌宠。

## Windows 客户端

为 Windows 10 或 11 的 x64 电脑构建并运行 `Portal Pet_0.1.6_x64-setup.exe`，安装后从开始菜单打开 Portal Pet。安装程序适用于当前用户，提供简体中文和英文。电脑缺少 Microsoft Edge WebView2 运行时时，安装程序会联网获取。首次打开聊天窗口时输入管理员发放的 6 位安全码。

在 Windows 上从源码重新打包时，先安装 Visual Studio C++ 生成工具和 WebView2，再在仓库根目录用 PowerShell 执行以下命令。 [Tauri Windows 构建依赖](https://v2.tauri.app/start/prerequisites/#windows)

```powershell
rustup target add wasm32-unknown-unknown x86_64-pc-windows-msvc
cargo install trunk --locked
cargo install tauri-cli --version 2.12.4 --locked
Set-Location interface
trunk build --locked --release
Set-Location ../desktop
cargo tauri build --target x86_64-pc-windows-msvc --ci -- --locked
```

安装包输出到 `target/x86_64-pc-windows-msvc/release/bundle/nsis/`。

## 验证真实来信

1. B 打开另一款应用，让文本框获得焦点。A 在已建立的好友会话中向 B 发一条消息。
2. B 看到小传送门打开，A 的机器人带着气泡走出来。门关上后，机器人留在桌面上，B 仍可在原应用输入。
3. B 单击气泡或好友机器人打开聊天。在会话里选「结束聊天」，本机会播放把好友机器人踢进门的动画，对方会看到机器人自己走回门里并说 `byebye`。重新打开会话可以查看历史。
4. 右键机器人可分别隐藏。菜单栏或托盘里可恢复自己的机器人和好友机器人。隐藏快捷键收起所有窗口，再按一次恢复允许显示的机器人。

| 操作 | macOS | Windows |
| --- | --- | --- |
| 隐藏或恢复桌宠 | Control + Option + H | Ctrl + Alt + H |

## iPhone 客户端

用 Xcode 打开 `ios/PortalPet.xcodeproj`，选择 `PortalPet` Scheme 和 iOS 17.0 或更新版本的 iPhone 模拟器后运行。安装到自己的 iPhone 时，在 Signing & Capabilities 里选自己的 Team 和可用的 Bundle Identifier，再选择已连接的设备运行。

构建前，在 `ios/PortalPet/PortalAPI.swift` 中将 `defaultServerAddress` 改为实际 HTTPS 地址。`https://chat.example.com` 是示例地址。输入或粘贴管理员发放的 6 位安全码后自动登录。Mac 和 iPhone 同时使用时，分别给两个成员发码。同一个成员重新登录时，服务器会替换该成员的旧会话。

iPhone 客户端支持成员列表、好友审批、私聊、前台传送门来信和免打扰。回到前台时会重连并拉取近期消息。推送通知需要服务端接入 Apple Push Notification service（APNs）。

## 鸿蒙客户端

在 DevEco Studio 中打开 [`harmonyos/`](harmonyos/README.md)。客户端以 HarmonyOS 5.0（API 12）为最低兼容版本，复用现有聊天服务，可与桌面端和 iPhone 端互发消息。

## 许可证

本项目使用 [MIT 许可证](LICENSE)。
