export type ExtensionLocale = "en" | "zh-CN";

export const UI_LOCALE_KEY = "uiLocale";

export const POPUP_COPY = {
  en: {
    language: "Language",
    checking: "Checking connection…",
    sendTo: "Send credentials to",
    desktop: "Desktop App",
    nas: "NAS / Docker",
    accountDetection: "YouTube Music account",
    accountDetectionValue: "Detected automatically",
    endpoint: "HTTPS endpoint",
    pairingCode: "Pairing code",
    pair: "Pair with NAS",
    forget: "Forget this NAS",
    enableRefresh: "Enable automatic refresh",
    refreshNow: "Refresh credentials now",
    removeAccess: "Remove YouTube Music access",
    website: "Website",
    privacyPolicy: "Privacy",
    privacy:
      "Access is requested only when you enable automatic refresh and is limited to music.youtube.com plus the parent youtube.com cookie domain required by Chrome. Cookie values go directly to your selected runtime and are never stored by this extension.",
    revokeNote:
      "Forgetting a NAS removes the local device token. Revoke the device in the NAS Web UI to invalidate its server-side token.",
    ready: "Ready to connect",
    permissionRequired: "Automatic refresh is off",
    connected: "Connected",
    attention: "Needs attention",
    refreshed: "Credential snapshot refreshed.",
    paired: "Paired and credentials refreshed.",
    nasForgotten:
      "Local NAS access removed. Revoke the device in the NAS Web UI as well.",
    youtubeRevoked: "YouTube Music access removed. Automatic refresh is off.",
    refreshFailed: "Refresh failed",
    pairingFailed: "Pairing failed",
    forgetFailed: "Could not forget NAS",
    httpsOnly: "Use an HTTPS endpoint.",
    permissionDenied: "YouTube Music permission was not granted.",
  },
  "zh-CN": {
    language: "语言",
    checking: "正在检查连接…",
    sendTo: "将凭据发送到",
    desktop: "桌面 App",
    nas: "NAS / Docker",
    accountDetection: "YouTube Music 账号",
    accountDetectionValue: "自动识别当前账号",
    endpoint: "HTTPS 地址",
    pairingCode: "配对码",
    pair: "与 NAS 配对",
    forget: "忘记这台 NAS",
    enableRefresh: "启用自动刷新",
    refreshNow: "立即刷新凭据",
    removeAccess: "移除 YouTube Music 访问权限",
    website: "官网",
    privacyPolicy: "隐私政策",
    privacy:
      "只有在你启用自动刷新时才会请求权限，范围仅限 music.youtube.com 以及 Chrome 读取认证 Cookie 所需的父域 youtube.com。Cookie 值会直接发送到你选择的运行端，本扩展不会保存。",
    revokeNote:
      "忘记 NAS 只会删除本地设备令牌；还需在 NAS 网页中撤销设备，才能让服务端令牌失效。",
    ready: "可以连接",
    permissionRequired: "自动刷新未启用",
    connected: "已连接",
    attention: "需要处理",
    refreshed: "登录凭据已刷新。",
    paired: "NAS 已配对，登录凭据已刷新。",
    nasForgotten: "已移除本地 NAS 访问；请同时在 NAS 网页中撤销该设备。",
    youtubeRevoked: "已移除 YouTube Music 访问权限，自动刷新已关闭。",
    refreshFailed: "刷新失败",
    pairingFailed: "配对失败",
    forgetFailed: "无法忘记 NAS",
    httpsOnly: "请使用 HTTPS 地址。",
    permissionDenied: "未授予 YouTube Music 权限。",
  },
} as const;

export function resolveExtensionLocale(value?: string | null): ExtensionLocale {
  return value?.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
