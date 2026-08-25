# Chrome Web Store 上架准备

Scrobble Bridge 的 Chrome 扩展目前支持开发者模式安装。以下内容用于准备正式上架；创建商店条目、上传 ZIP、提交审核和发布都需要单独授权。

## 建议展示信息

- 扩展名称：`Scrobble Bridge: YouTube Music → Last.fm`
- 简介：`Keep your YouTube Music sign-in connected to Scrobble Bridge so new listening history can be submitted to your Last.fm account.`
- 分类：Productivity / Music
- 单一用途：把用户主动授权的 YouTube Music 登录快照安全传给同机 Scrobble Bridge 桌面应用，或用户明确配对的自有 NAS。
- 支持语言：English、简体中文。
- 隐私政策：仓库根目录 `PRIVACY.md`；上架前改为实际可公开访问的稳定 URL。

## 权限解释

| 权限                          | 用途                                                                         | 授权时机                |
| ----------------------------- | ---------------------------------------------------------------------------- | ----------------------- |
| `alarms`                      | 定期检查是否需要刷新用户已经授权的登录状态                                   | 安装时                  |
| `nativeMessaging`             | 把登录快照交给同一台电脑上的 Scrobble Bridge 桌面应用                        | 安装时                  |
| `storage`                     | 保存目标模式、用户自有 NAS 配对信息和最近连接状态；不保存原始 YouTube Cookie | 安装时                  |
| `cookies`                     | 读取少量明确列出的 YouTube Music 身份验证 Cookie                             | 用户点击启用自动刷新后  |
| `https://music.youtube.com/*` | 识别当前登录的 YouTube Music 账号和账号上下文                                | 用户点击启用自动刷新后  |
| `https://youtube.com/*`       | 读取由 YouTube 父域管理的认证 Cookie                                         | 用户点击启用自动刷新后  |
| `https://*/*`                 | 支持用户自选 NAS 域名；实际只请求用户输入的单个 HTTPS origin                 | 用户主动配对自有 NAS 时 |

扩展不请求其他 YouTube 子域，不使用 content script，不把 Cookie 发送给项目维护者，不出售数据，也不通过分析服务收集播放记录。

## 正式扩展 ID 与桌面安装包

开发版使用固定 ID `nocefljecnigpgfgalgjefcigeidoglj`。运行 `pnpm --filter @scrobble-bridge/extension package:store` 会在 `target/scrobble-bridge-extension-1.0.0.zip` 生成商店专用 ZIP：只从商店包中移除开发环境的固定公钥，保留 `apps/extension/dist` 里的开发版 ID 和现有本地 Chrome 连接。首次上传后由 Chrome Web Store 分配正式扩展 ID；将该 ID 设置为 GitHub 仓库变量 `SCROBBLE_PRODUCTION_EXTENSION_ID`，然后重新构建桌面安装包。

桌面构建会拒绝不是 32 位 `a`–`p` 字符的值。最终 Chrome Native Messaging manifest 同时允许固定开发扩展和经过校验的正式扩展；不能使用通配 origin，也不能只上传扩展而不更新桌面安装包白名单。

## 提交前自查

- `pnpm --filter @scrobble-bridge/extension package:store` 成功，商店 ZIP 不包含开发环境的 `manifest.key`。
- ZIP 包含 Manifest V3、后台 service worker、popup、图标和中英文 locale。
- 商店单一用途说明、权限理由和隐私政策与当前代码一致。
- 正式扩展 ID 已写入仓库变量，并通过桌面安装包白名单验证。
- 真机完成 YouTube Music 登录、扩展权限授予、桌面 Native Messaging 连接和 Last.fm 实际同步。
- NAS 场景仅请求用户指定的 HTTPS origin，并验证忘记设备与服务器端撤销。
- 明确取得 Chrome Web Store 上传、审核提交和发布授权。
