import { YOUTUBE_COOKIE_ORIGINS, type ExtensionSettings } from "./protocol";
import {
  POPUP_COPY,
  resolveExtensionLocale,
  UI_LOCALE_KEY,
  type ExtensionLocale,
} from "./i18n";
import "./popup.css";

const target = element<HTMLSelectElement>("target");
const nasFields = element<HTMLElement>("nas-fields");
const nasUrl = element<HTMLInputElement>("nas-url");
const pairingCode = element<HTMLInputElement>("pairing-code");
const status = element<HTMLElement>("status");
const message = element<HTMLElement>("message");
const localeSelect = element<HTMLSelectElement>("locale");
const pairButton = element<HTMLButtonElement>("pair");
const forgetButton = element<HTMLButtonElement>("forget");
const refreshButton = element<HTMLButtonElement>("refresh");
const revokeYouTubeButton = element<HTMLButtonElement>("revoke-youtube");

let locale: ExtensionLocale = "en";
let currentSettings: ExtensionSettings | undefined;
let youtubeAccess = false;

void initialize();
localeSelect.addEventListener("change", () => {
  locale = resolveExtensionLocale(localeSelect.value);
  void chrome.storage.local.set({ [UI_LOCALE_KEY]: locale });
  applyCopy();
  renderState();
});
target.addEventListener("change", () => {
  nasFields.hidden = target.value !== "nas";
  void persistInputs();
});
refreshButton.addEventListener(
  "click",
  () =>
    void run(async () => {
      await ensureYouTubeAccess();
      await persistInputs();
      const response = await chrome.runtime.sendMessage({ type: "refresh" });
      if (!response?.ok)
        throw new Error(response?.error ?? POPUP_COPY[locale].refreshFailed);
      currentSettings = await readSettings();
      message.textContent = POPUP_COPY[locale].refreshed;
      renderState();
    }),
);
revokeYouTubeButton.addEventListener(
  "click",
  () =>
    void run(async () => {
      await chrome.permissions.remove({
        permissions: ["cookies"],
        origins: YOUTUBE_COOKIE_ORIGINS,
      });
      youtubeAccess = await hasYouTubeAccess();
      message.textContent = POPUP_COPY[locale].youtubeRevoked;
      renderState();
    }),
);
pairButton.addEventListener(
  "click",
  () =>
    void run(async () => {
      const endpoint = new URL(nasUrl.value);
      if (endpoint.protocol !== "https:")
        throw new Error(POPUP_COPY[locale].httpsOnly);
      const granted = await chrome.permissions.request({
        permissions: ["cookies"],
        origins: [...YOUTUBE_COOKIE_ORIGINS, `${endpoint.origin}/*`],
      });
      if (!granted)
        throw new Error("Permission for this NAS endpoint was not granted.");
      youtubeAccess = await hasYouTubeAccess();
      if (!youtubeAccess) throw new Error(POPUP_COPY[locale].permissionDenied);
      await persistInputs();
      const response = await chrome.runtime.sendMessage({
        type: "pair_nas",
        endpoint: endpoint.origin,
        code: pairingCode.value.trim(),
        deviceName: navigator.platform || "Chrome",
      });
      if (!response?.ok)
        throw new Error(response?.error ?? POPUP_COPY[locale].pairingFailed);
      currentSettings = await readSettings();
      message.textContent = POPUP_COPY[locale].paired;
      renderState();
    }),
);
forgetButton.addEventListener(
  "click",
  () =>
    void run(async () => {
      const response = await chrome.runtime.sendMessage({ type: "forget_nas" });
      if (!response?.ok)
        throw new Error(response?.error ?? POPUP_COPY[locale].forgetFailed);
      if (response.endpoint)
        await chrome.permissions.remove({
          origins: [`${response.endpoint}/*`],
        });
      target.value = "desktop";
      currentSettings = await readSettings();
      message.textContent = POPUP_COPY[locale].nasForgotten;
      renderState();
    }),
);

async function initialize(): Promise<void> {
  const stored = await chrome.storage.local.get(["settings", UI_LOCALE_KEY]);
  locale = resolveExtensionLocale(
    (stored[UI_LOCALE_KEY] as string | undefined) ??
      chrome.i18n.getUILanguage(),
  );
  localeSelect.value = locale;
  applyCopy();
  currentSettings = stored.settings as ExtensionSettings | undefined;
  youtubeAccess = await hasYouTubeAccess();
  if (currentSettings) {
    target.value = currentSettings.target;
    nasUrl.value = currentSettings.device?.endpoint ?? "";
    if (currentSettings.lastError)
      message.textContent = localizeError(currentSettings.lastError);
  }
  renderState();
}

function applyCopy(): void {
  const text = POPUP_COPY[locale];
  document.documentElement.lang = locale;
  setText("language-label", text.language);
  localeSelect.setAttribute("aria-label", text.language);
  setText("target-label", text.sendTo);
  setText("target-desktop", text.desktop);
  setText("target-nas", text.nas);
  setText("account-detection-label", text.accountDetection);
  setText("account-detection-value", text.accountDetectionValue);
  setText("nas-url-label", text.endpoint);
  setText("pairing-code-label", text.pairingCode);
  pairButton.textContent = text.pair;
  forgetButton.textContent = text.forget;
  revokeYouTubeButton.textContent = text.removeAccess;
  setText("privacy-note", text.privacy);
  setText("revoke-note", text.revokeNote);
}

function renderState(): void {
  const text = POPUP_COPY[locale];
  nasFields.hidden = target.value !== "nas";
  forgetButton.hidden = !currentSettings?.device;
  revokeYouTubeButton.hidden = !youtubeAccess;
  refreshButton.textContent = youtubeAccess
    ? text.refreshNow
    : text.enableRefresh;
  status.textContent = !youtubeAccess
    ? text.permissionRequired
    : currentSettings?.lastError
      ? text.attention
      : currentSettings?.lastSuccessAt
        ? text.connected
        : text.ready;
}

async function ensureYouTubeAccess(): Promise<void> {
  if (!youtubeAccess) {
    youtubeAccess = await chrome.permissions.request({
      permissions: ["cookies"],
      origins: YOUTUBE_COOKIE_ORIGINS,
    });
  }
  if (!youtubeAccess) throw new Error(POPUP_COPY[locale].permissionDenied);
  renderState();
}

async function hasYouTubeAccess(): Promise<boolean> {
  return chrome.permissions.contains({
    permissions: ["cookies"],
    origins: YOUTUBE_COOKIE_ORIGINS,
  });
}

async function readSettings(): Promise<ExtensionSettings | undefined> {
  const stored = await chrome.storage.local.get("settings");
  return stored.settings as ExtensionSettings | undefined;
}

async function persistInputs(): Promise<void> {
  const stored = await chrome.storage.local.get("settings");
  const current = (stored.settings ?? {}) as Partial<ExtensionSettings>;
  currentSettings = {
    ...current,
    target: target.value as ExtensionSettings["target"],
  };
  await chrome.storage.local.set({ settings: currentSettings });
}

async function run(action: () => Promise<void>): Promise<void> {
  pairButton.disabled = true;
  forgetButton.disabled = true;
  refreshButton.disabled = true;
  revokeYouTubeButton.disabled = true;
  message.textContent = "";
  try {
    await action();
  } catch (error) {
    message.textContent = localizeError(
      error instanceof Error ? error.message : String(error),
    );
  } finally {
    pairButton.disabled = false;
    forgetButton.disabled = false;
    refreshButton.disabled = false;
    revokeYouTubeButton.disabled = false;
  }
}

function localizeError(value: string): string {
  if (locale === "en") return value;
  const known: Record<string, string> = {
    "Open YouTube Music and sign in before refreshing credentials.":
      "请先打开 YouTube Music 并登录，再刷新凭据。",
    "Enable YouTube Music access before refreshing credentials.":
      "请先启用 YouTube Music 访问权限。",
    "Desktop App did not accept the credential snapshot.":
      "桌面 App 未接受登录凭据。",
    "Pair this extension with your NAS first.": "请先将扩展与 NAS 配对。",
    "NAS endpoint must use HTTPS.": "NAS 地址必须使用 HTTPS。",
    "NAS identity changed; revoke this device and pair again.":
      "NAS 身份已变化，请撤销设备后重新配对。",
    "Permission for this NAS endpoint was not granted.":
      "未授予此 NAS 地址的访问权限。",
    "Could not detect the active YouTube Music account.":
      "无法识别当前 YouTube Music 账号，请先打开 YouTube Music 后重试。",
  };
  return known[value] ?? value;
}

function setText(id: string, value: string): void {
  element<HTMLElement>(id).textContent = value;
}

function element<T extends HTMLElement>(id: string): T {
  const value = document.getElementById(id);
  if (!value) throw new Error(`Missing element ${id}`);
  return value as T;
}
