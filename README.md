# Scrobble Bridge

**YouTube Music → Last.fm · 本地优先的跨设备收听同步**

Scrobble Bridge 是一个本地优先、MIT 开源的 YouTube Music → Last.fm scrobbler。它可以作为 macOS / Windows 桌面 App 常驻后台，也可以在 24 小时在线的 NAS 上以 Docker 容器运行。只要歌曲进入同一个 YouTube Music 账号的云端历史，手机、平板和其他电脑上的播放也可以被后台运行的 Scrobble Bridge 发现并同步。

> 这是非官方独立项目，与 Google、YouTube 或 Last.fm 没有隶属或合作关系。YouTube Music 没有提供本项目所需的公开历史 API；当前实现使用浏览器登录凭据访问内部 Web endpoint，因此可能随上游改版失效。播放时间是根据历史窗口估算，不应当视为精确收听日志。

## 1.0 包含什么

- Rust 同步核心：有序历史窗口、baseline、间隙保护、重复播放、确定性 fingerprint；
- SQLite outbox：提交状态、崩溃恢复、Last.fm recent tracks 对照、退避重试、每日备份；
- Tauri 2 + Svelte 桌面 App：macOS 12+、Windows 10/11 x64、简体中文/英文、Menu Bar / System Tray、登录启动、关闭窗口后常驻、Dock 重新打开、睡眠唤醒补跑；
- Chrome Manifest V3 扩展：简体中文/英文；自动识别当前 YouTube Music 账号及多账号上下文；安装时不索取 YouTube 站点访问，用户启用自动刷新时才按需申请 `cookies`、`music.youtube.com` 和认证 Cookie 所属的父域 `youtube.com`，且不把 Cookie 写入扩展存储；
- Native Messaging：扩展把短期凭据快照交给同机桌面 App，App 存入 macOS Keychain / Windows Credential Manager；
- Last.fm 授权：官方配置的桌面安装包预置项目级 API application；普通用户只需打开 Last.fm 登录授权，不需要自己填写 API Key 或 Shared Secret；
- Docker / NAS：`linux/amd64`、`linux/arm64`、非 root、只读根文件系统、健康检查、持久化卷、Web 管理和 HTTPS 设备配对；
- CI：Rust/TypeScript 测试、macOS/Windows 原生编译、双架构容器、DMG/NSIS/扩展发布产物。

实现详情见 [1.0 实施状态](docs/1.0-implementation-status.md)，测试边界见 [1.0 QA 报告](docs/1.0-qa-report.md)，设计依据见 [产品与技术架构](docs/1.0-product-architecture-plan.md)。

## 选择运行方式

| 方式         | 适合场景                            | Chrome 关闭后                                         | 凭据保存位置                                    |
| ------------ | ----------------------------------- | ----------------------------------------------------- | ----------------------------------------------- |
| 桌面 App     | Mac mini、日常 Mac/Windows PC       | App 继续按已有快照同步；Chrome 下次启动时扩展刷新快照 | Keychain / Credential Manager                   |
| Docker / NAS | Synology、QNAP、TrueNAS、常开服务器 | 容器继续按已有快照同步；扩展下次启动后通过 HTTPS 更新 | `/data/credentials.enc`，ChaCha20-Poly1305 加密 |

Chrome 不需要一直开。没有浏览器时，Scrobble Bridge 仍可使用最后一个有效快照；Cookie 真正失效后会进入 `needs_attention`，必须重新打开并登录 YouTube Music，扩展才可刷新，项目不会声称“永久 Cookie”。

## 桌面 App

发布产物由 `.github/workflows/artifacts.yml` 生成：

- macOS Apple Silicon / Intel：DMG；
- Windows x64：当前用户 NSIS installer；
- Chrome Extension：可审阅 ZIP。

开发构建：

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm --filter @scrobble-bridge/desktop bundle:mac   # macOS
pnpm --filter @scrobble-bridge/desktop tauri build --bundles nsis --config src-tauri/tauri.release.conf.json  # Windows
```

第一次打开 App 会注册随包提供的 Chrome Native Messaging host。关闭主窗口只会隐藏窗口；重新点击 macOS Dock 图标或托盘中的 **打开 / Open** 会恢复主窗口，从托盘选择 **退出 / Quit** 才会停止后台运行。

### Last.fm 登录方式

官方安装包由维护者在构建时注入项目的 Last.fm API application。首次使用时：

1. 安装并启用 Chrome 扩展，让它连接当前 YouTube Music 账号。
2. 在桌面 App 中点击 **前往 Last.fm 授权**。
3. 在 Last.fm 完成登录并允许 Scrobble Bridge 访问。
4. 切回桌面 App 后会自动完成连接；必要时也可以点击备用的 **我已批准访问，完成连接**。

源码仓库不包含实际 API Key、Shared Secret、Cookie 或用户 session。开源自行构建时如果没有设置项目级构建凭据，App 会提供高级表单，让维护者或使用者连接自己创建的 Last.fm API application。自己的账号 session 仍只保存在本机系统凭据库中，不会被上传给项目维护者。

## Chrome Extension

1. 构建：`pnpm --filter @scrobble-bridge/extension build`；
2. 在 `chrome://extensions` 打开 Developer mode；
3. 选择 **Load unpacked**，加载 `apps/extension/dist`；
4. 选择 Desktop 或 NAS，点击 **启用自动刷新**；扩展会自动识别当前 YouTube Music 账号，不需要填写账号标签或 Google account index；
5. Chrome 此时才会请求读取 `music.youtube.com` 及认证 Cookie 所属父域 `youtube.com` 的最小权限。授权后可随时在扩展中点击 **移除 YouTube Music 访问权限**。

NAS 模式必须从 Web 管理页生成十分钟有效的一次性配对码，并使用 Chrome 已信任证书的 HTTPS 地址。扩展仅在用户点击配对时请求该精确 origin 的访问权限。详见 [扩展与凭据连接](docs/extension.md) 与 [Chrome Web Store 上架准备](docs/chrome-web-store.md)。Chrome Web Store 提交本身不属于当前仓库公开动作。

## Docker / NAS

```bash
docker compose -f deploy/docker/compose.yaml up -d --build
docker compose -f deploy/docker/compose.yaml exec scrobble-bridge \
  sh -c 'cat /data/secrets/admin.token'
```

打开 `http://NAS_ADDRESS:8787` 完成管理设置。不要把这个 HTTP 端口直接暴露到公网；Chrome 配对必须走可信 HTTPS reverse proxy 或 Tailscale Serve。完整说明见 [Docker / NAS 部署](docs/docker-nas.md)。

## 本地开发与验证

需要 Rust 1.94.1、Node.js 24、pnpm 10.34.5，以及对应平台的 Tauri 系统依赖。

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm check
pnpm test
pnpm build
```

仓库结构：

- `crates/scrobble-core`：纯同步算法；
- `crates/scrobble-storage`：SQLite 和加密 vault；
- `crates/ytmusic-client`、`crates/lastfm-client`：上游适配器；
- `crates/scrobble-engine`：outbox 执行器；
- `apps/desktop`：Tauri App；
- `apps/extension`：Chrome Extension；
- `crates/scrobble-daemon`、`apps/web`：Docker runtime 和管理 UI。

### 维护者构建配置

官方桌面构建需要在 GitHub 仓库配置两个 Actions secrets：

- `SCROBBLE_LASTFM_API_KEY`
- `SCROBBLE_LASTFM_SHARED_SECRET`

两个值必须同时存在，不能写入 Git 仓库或 Actions 日志。默认 artifact workflow 会拒绝生成缺少项目级 Last.fm application 的官方安装包；需要明确构建自行配置版本时，可以选择 `self-provided` 模式。Chrome Web Store 分配正式扩展 ID 后，再把它设置为仓库变量 `SCROBBLE_PRODUCTION_EXTENSION_ID`，桌面安装包会同时允许经校验的正式扩展和固定的开发扩展。

## 安全、隐私与发布状态

请先阅读 [隐私说明](PRIVACY.md) 与 [安全策略](SECURITY.md)。公开分发前仍必须由发布者提供 Apple Developer ID / notarization 凭据和 Windows code-signing certificate，并在真实 Intel Mac、Windows x64 与 amd64/arm64 NAS 上完成验收。源码、自动化和未签名本地产物不等于已经公开发布；详细门禁见 [1.0 发布清单](docs/release-checklist.md)。

本项目不提供托管账户、云端凭据中转或订阅服务。官方桌面安装包可以包含项目级 Last.fm API application，以便用户直接授权自己的 Last.fm 账号；源码构建和 NAS 自托管仍支持用户自备 API application。原生客户端里打包的 shared secret 可以被有能力的人从安装包提取，因此不能把它视为服务器端机密，需要监控、轮换并遵守 Last.fm 服务条款。Last.fm API 默认仅许可非商业用途；如需收费分发、订阅、商业服务或研究用途，必须先向 Last.fm 获得相应许可。

## License

[MIT](LICENSE)。贡献方式见 [CONTRIBUTING.md](CONTRIBUTING.md)，第三方依赖说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
