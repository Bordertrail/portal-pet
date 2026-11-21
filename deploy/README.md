# 在 Linux 服务器部署 Portal Pet

准备一台安装了 Docker Engine 和 Docker Compose v2 的 Linux 服务器。可以使用固定公网 IPv4 加端口，也可以使用域名。 [Docker Engine 安装文档](https://docs.docker.com/engine/install/)

## 用公网 IP 和端口启动

1. 在云平台安全组和服务器防火墙放行 **TCP 80** 和 **TCP 8443**。桌宠走 8443，Caddy 通过 80 自动申请及续期 Let's Encrypt 公网 IP 证书。使用其他聊天端口时，修改 `.env` 的 `PORTAL_HTTPS_PORT` 并放行对应端口。 [Let's Encrypt IP 证书](https://letsencrypt.org/2026/01/15/6day-and-ip-general-availability) [证书验证方式](https://letsencrypt.org/docs/challenge-types/#http-01-challenge)
2. 把部署包上传到服务器并解包。运行 `nano .env` 时，把 `PORTAL_HOST` 改成服务器的实际公网 IPv4，只填 IP。

   ```sh
   mkdir -p portal-pet
   tar -xzf portal-server-deploy.tar.gz -C portal-pet
   cd portal-pet
   cp -n .env.ip.example .env
   nano .env
   docker compose pull caddy
   docker compose up -d --build
   docker compose ps
   ```

3. 从另一台电脑检查 HTTPS 入口。把命令中的 IP 换成实际公网 IP。返回 `ok` 后，客户端即可使用这个 HTTPS 入口。

   ```sh
   curl -fsS https://203.0.113.10:8443/health
   ```

### 用域名启动

将域名指向服务器，放行 TCP 80 和 443。解包后从 `.env.example` 创建 `.env`，把 `PORTAL_DOMAIN` 改为实际域名。运行 `docker compose up -d --build` 后，用 `curl -fsS https://chat.example.com/health` 验证，把示例域名换成实际域名。 [Caddy 自动 HTTPS 的前提](https://caddyserver.com/docs/automatic-https#overview)

### 启动排查

Caddy 同时代理 HTTP 接口和 WebSocket。Rust 服务的 8765 端口只用于 Compose 内部网络。HTTPS 未就绪时，用 `docker compose logs --tail=100 caddy portal` 查看证书签发和应用启动日志。 [Caddy WebSocket 代理](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy#streaming)

首次构建需要联网拉取基础镜像、系统依赖和 Rust 依赖。如果旧版 Docker 的 BuildKit 拉取公开基础镜像时返回 401，可在仍支持旧构建器的版本上重试 `DOCKER_BUILDKIT=0 docker compose up -d --build`。

如果服务器已有服务占用这些端口，可以由既有 HTTPS 入口负责证书签发，并代理 Rust 服务的 HTTP 和 WebSocket。只有当入口覆盖 `X-Portal-Client-IP` 请求头并且 Rust 端口仅对可信代理可达时，才启用 `PORTAL_TRUST_PROXY=1`。

## 发码和连接桌宠

在服务器解包目录运行。每条命令会输出一个成员编号和可重复使用的 6 位安全码。把各自的码单独交给对应成员。

```sh
docker compose exec portal portal-server invite 示例甲
docker compose exec portal portal-server invite 示例乙
```

打开桌宠并输入个人安全码。部署到公网后，用启动环境变量 `PORTAL_PET_SERVER_ADDRESS` 配置桌面端的实际 HTTPS 地址。移动端在源码中更改示例地址后重新构建。两人都上线后，从成员列表发起好友申请。对方同意后即可聊天。

重新发码时，在服务器上执行 `docker compose exec portal portal-server reissue <成员编号>`。停用成员时执行 `docker compose exec portal portal-server disable <成员编号>`。填写实际成员编号时省略尖括号。同一成员重新登录会替换旧设备的登录会话。旧版 32 位码仍可在旧客户端一次性激活，新客户端请为原成员重新发码。

为已启用的成员授予管理员权限时，执行 `docker compose exec portal portal-server grant-admin <成员编号>`。管理员可以从客户端设置创建普通账号，所有成员都可以在设置中修改自己的昵称。

## 在阿里云 OSS 启用图片消息

在与 ECS 相同的地域创建私有 Bucket，保持阻止公共访问开启。服务端用 RAM 凭据读写 `portal-pet/images/`，聊天客户端通过 Portal Pet 的 HTTPS 接口访问图片。 [OSS 阻止公共访问](https://www.alibabacloud.com/help/en/oss/user-guide/block-public-access)

给专用 RAM 身份授权 `oss:PutObject`、`oss:GetObject` 和 `oss:DeleteObject`，资源限定为 `acs:oss:*:*:你的-bucket/portal-pet/images/*`。删除权限用于清理提交失败或被重复提交替代的对象。 [OSS RAM 权限](https://www.alibabacloud.com/help/zh/oss/user-guide/authorization-syntax-and-elements)

把以下值写在服务器上的配置文件中，Bucket 名和地域替换为 OSS 控制台显示的值，凭据使用专用 RAM 身份的实际值。配置文件权限设为 `0600`。

```dotenv
PORTAL_OSS_BUCKET=your-bucket
PORTAL_OSS_REGION=cn-hangzhou
PORTAL_OSS_INTERNAL=1
OSS_ACCESS_KEY_ID=your-ram-access-key-id
OSS_ACCESS_KEY_SECRET=your-ram-access-key-secret
```

- **ECS 同地域**用 `PORTAL_OSS_INTERNAL=1` 通过 OSS 内网 Endpoint 访问，无需自定义域名。中国内地新开通的 OSS 服务限制默认外网域名调用数据 API。使用 STS 临时凭据时同时设置 `OSS_SECURITY_TOKEN`，并在凭据到期前换新。 [OSS 地域与 Endpoint](https://www.alibabacloud.com/help/zh/oss/user-guide/regions-and-endpoints)
- **原生 systemd 部署**使用 `sudo systemctl edit portal-pet`，在 `[Service]` 下设置 `EnvironmentFile=/etc/portal-pet/oss.env`，把上述配置存到该路径。更新服务端二进制后重载 systemd 并重启 `portal-pet`。在 Alibaba Cloud Linux 3 上编译新版时，安装 `openssl-devel`。
- **Docker Compose 部署**把配置加入项目的 `.env`，执行 `docker compose up -d --build`。`portal` 容器会通过 `oss_egress` 网络访问 OSS。

配置完成后，用两个好友账号发送并打开一张图片确认。当前支持 JPEG、PNG 和 WebP，单张上限 8 MiB。`PORTAL_OSS_BUCKET` 留空时仍可正常使用文字聊天。将 OSS 对象与 SQLite 数据库一并备份可完整恢复图片历史。

## 备份和更新

成员、好友关系和消息保存在 `portal_data` 卷中。Caddy 的证书和配置也放在持久卷。备份时使用 SQLite 的在线备份命令取得一致性快照。 [SQLite 在线备份](https://sqlite.org/backup.html)

```sh
umask 077
docker compose exec -T portal sh -ec '
    dir=$(mktemp -d)
    trap "rm -f \"$dir/portal.sqlite\"; rmdir \"$dir\"" EXIT
    sqlite3 "$PORTAL_DB" ".backup $dir/portal.sqlite"
    cat "$dir/portal.sqlite"
' > "portal-$(date +%Y%m%d-%H%M%S).sqlite"
```

更新时先备份，再替换部署目录里的源文件，保留 `.env`，然后重新运行 `docker compose up -d --build`。保留项目目录名和 Compose 数据卷，现有成员和聊天记录会继续使用原数据库。

### 更新原生 systemd 部署

使用 `portal-pet.service` 和 `/usr/local/bin/portal-server` 的服务器可将新版部署包解到一个新目录，在该目录编译并替换服务端。启动会自动更新现有 SQLite 数据库结构。

```sh
mkdir -p desktop/src interface/src
touch desktop/src/main.rs interface/src/main.rs
cargo build --release --locked -p portal-server
sudo install -m 0755 target/release/portal-server /usr/local/bin/portal-server.new
sudo mv /usr/local/bin/portal-server.new /usr/local/bin/portal-server
sudo systemctl restart portal-pet
curl -fsS http://127.0.0.1:8765/health
```

为原成员生成新版安全码时使用 `sudo -u portal-pet env PORTAL_DB=/var/lib/portal-pet/portal.sqlite /usr/local/bin/portal-server reissue <成员编号>`。

原生部署中，把上条命令中的 `reissue` 换成 `grant-admin` 即可授予管理员权限。已登录客户端刷新成员列表后可在设置中看到管理员入口。
