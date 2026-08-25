import {
  cookieHeader,
  hasSapisidCookie,
  parseYouTubeAccountContext,
  shouldRefreshChangedCookie,
  signedHeaders,
  YTMUSIC_COOKIE_URL,
  YOUTUBE_COOKIE_ORIGINS,
  type CredentialSnapshot,
  type ExtensionSettings,
} from "./protocol";

const NATIVE_HOST = "com.scrobblebridge.host";
const PERIODIC_ALARM = "periodic-credential-refresh";
const DEFAULT_SETTINGS: ExtensionSettings = {
  target: "desktop",
};
let refreshTimer: number | undefined;
let refreshRunning: Promise<void> | null = null;

chrome.runtime.onInstalled.addListener(() => {
  void chrome.alarms.create(PERIODIC_ALARM, { periodInMinutes: 360 });
  scheduleRefresh();
});
chrome.runtime.onStartup.addListener(scheduleRefresh);
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === PERIODIC_ALARM) scheduleRefresh();
});
chrome.cookies.onChanged.addListener(({ cookie }) => {
  if (shouldRefreshChangedCookie(cookie)) scheduleRefresh();
});

chrome.runtime.onMessage.addListener(
  (message: unknown, _sender, sendResponse) => {
    void handleMessage(message).then(sendResponse, (error: unknown) => {
      sendResponse({
        ok: false,
        error: error instanceof Error ? error.message : String(error),
      });
    });
    return true;
  },
);

function scheduleRefresh(): void {
  if (refreshTimer !== undefined) clearTimeout(refreshTimer);
  refreshTimer = setTimeout(() => {
    refreshTimer = undefined;
    void hasYouTubeAccess()
      .then((granted) => {
        if (granted) return refreshCredentials();
      })
      .catch(() => undefined);
  }, 1_500) as unknown as number;
}

async function handleMessage(message: unknown): Promise<unknown> {
  if (!message || typeof message !== "object" || !("type" in message))
    throw new Error("Invalid extension message");
  const request = message as Record<string, unknown>;
  if (request.type === "refresh") {
    await refreshCredentials();
    return { ok: true };
  }
  if (request.type === "pair_nas") {
    return pairNas(
      String(request.endpoint ?? ""),
      String(request.code ?? ""),
      String(request.deviceName ?? "Chrome"),
    );
  }
  if (request.type === "forget_nas") {
    const settings = await readSettings();
    const endpoint = settings.device?.endpoint;
    const { device: _device, ...withoutDevice } = settings;
    await saveSettings({
      ...withoutDevice,
      target: "desktop",
      lastError: undefined,
    });
    return { ok: true, endpoint };
  }
  throw new Error("Unsupported extension message");
}

async function refreshCredentials(): Promise<void> {
  if (refreshRunning) return refreshRunning;
  refreshRunning = performRefresh().finally(() => {
    refreshRunning = null;
  });
  return refreshRunning;
}

async function performRefresh(): Promise<void> {
  const settings = await readSettings();
  try {
    if (!(await hasYouTubeAccess()))
      throw new Error(
        "Enable YouTube Music access before refreshing credentials.",
      );
    const cookies = await chrome.cookies.getAll({ url: YTMUSIC_COOKIE_URL });
    const header = cookieHeader(cookies);
    if (!hasSapisidCookie(cookies)) {
      throw new Error(
        "Open YouTube Music and sign in before refreshing credentials.",
      );
    }
    const accountContext = await fetchYouTubeAccountContext();
    const snapshot: CredentialSnapshot = {
      account_id: accountContext.accountId,
      auth_user: accountContext.authUser,
      delegated_session_id: accountContext.delegatedSessionId,
      cookie_header: header,
    };
    if (settings.target === "nas") await sendToNas(snapshot, settings);
    else await sendToDesktop(snapshot);
    await saveSettings({
      ...settings,
      detectedAccountId: accountContext.accountId,
      detectedAuthUser: accountContext.authUser,
      detectedDelegatedSessionId: accountContext.delegatedSessionId,
      lastSuccessAt: new Date().toISOString(),
      lastError: undefined,
    });
  } catch (error) {
    await saveSettings({
      ...settings,
      lastError: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
}

async function fetchYouTubeAccountContext() {
  const response = await fetch(YTMUSIC_COOKIE_URL, {
    cache: "no-store",
    credentials: "include",
  });
  if (!response.ok)
    throw new Error("Could not detect the active YouTube Music account.");
  return parseYouTubeAccountContext(await response.text());
}

async function hasYouTubeAccess(): Promise<boolean> {
  return chrome.permissions.contains({
    permissions: ["cookies"],
    origins: YOUTUBE_COOKIE_ORIGINS,
  });
}

async function sendToDesktop(snapshot: CredentialSnapshot): Promise<void> {
  const response = await chrome.runtime.sendNativeMessage(NATIVE_HOST, {
    version: 1,
    type: "credential_snapshot",
    payload: snapshot,
  });
  if (!response?.ok)
    throw new Error(
      response?.error ?? "Desktop App did not accept the credential snapshot.",
    );
}

async function sendToNas(
  snapshot: CredentialSnapshot,
  settings: ExtensionSettings,
): Promise<void> {
  if (!settings.device)
    throw new Error("Pair this extension with your NAS first.");
  const body = JSON.stringify(snapshot);
  const headers = await signedHeaders(body, settings.device);
  const response = await fetch(
    `${settings.device.endpoint}/api/v1/extension/credentials/ytmusic`,
    {
      method: "PUT",
      headers,
      body,
    },
  );
  const payload = await response.json().catch(() => ({}));
  if (!response.ok)
    throw new Error(payload.message ?? `NAS returned HTTP ${response.status}`);
  if (payload.server_id !== settings.device.serverId)
    throw new Error("NAS identity changed; revoke this device and pair again.");
}

async function pairNas(
  endpointInput: string,
  code: string,
  deviceName: string,
): Promise<{ ok: true }> {
  const endpoint = new URL(endpointInput);
  if (endpoint.protocol !== "https:")
    throw new Error("NAS endpoint must use HTTPS.");
  const response = await fetch(`${endpoint.origin}/api/v1/pairing/exchange`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code, device_name: deviceName }),
  });
  const payload = await response.json().catch(() => ({}));
  if (!response.ok)
    throw new Error(
      payload.message ?? `Pairing failed with HTTP ${response.status}`,
    );
  const settings = await readSettings();
  await saveSettings({
    ...settings,
    target: "nas",
    device: {
      endpoint: endpoint.origin,
      deviceId: payload.device_id,
      deviceToken: payload.device_token,
      serverId: payload.server_id,
    },
    lastError: undefined,
  });
  await refreshCredentials();
  return { ok: true };
}

async function readSettings(): Promise<ExtensionSettings> {
  const stored = await chrome.storage.local.get("settings");
  return {
    ...DEFAULT_SETTINGS,
    ...(stored.settings as Partial<ExtensionSettings> | undefined),
  };
}

async function saveSettings(settings: ExtensionSettings): Promise<void> {
  await chrome.storage.local.set({ settings });
}
