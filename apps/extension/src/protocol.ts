export const AUTH_COOKIE_NAMES = new Set([
  "APISID",
  "HSID",
  "LOGIN_INFO",
  "PREF",
  "SAPISID",
  "SID",
  "SSID",
  "__Secure-1PAPISID",
  "__Secure-1PSID",
  "__Secure-1PSIDCC",
  "__Secure-1PSIDTS",
  "__Secure-3PAPISID",
  "__Secure-3PSID",
  "__Secure-3PSIDCC",
  "__Secure-3PSIDTS",
]);

export const YTMUSIC_COOKIE_URL = "https://music.youtube.com/";
export const YOUTUBE_COOKIE_ORIGINS = [
  "https://music.youtube.com/*",
  "https://youtube.com/*",
];

const SAPISID_COOKIE_NAMES = new Set([
  "__Secure-3PAPISID",
  "SAPISID",
  "__Secure-1PAPISID",
]);

export interface CredentialSnapshot {
  account_id: string;
  auth_user: number;
  delegated_session_id?: string;
  cookie_header: string;
}

export interface YouTubeAccountContext {
  accountId: string;
  authUser: number;
  delegatedSessionId?: string;
}

export interface DeviceConnection {
  endpoint: string;
  deviceId: string;
  deviceToken: string;
  serverId: string;
}

export interface ExtensionSettings {
  target: "desktop" | "nas";
  device?: DeviceConnection;
  detectedAccountId?: string;
  detectedAuthUser?: number;
  detectedDelegatedSessionId?: string;
  lastSuccessAt?: string;
  lastError?: string;
}

export function parseYouTubeAccountContext(
  html: string,
): YouTubeAccountContext {
  const dataSyncId = embeddedJsonString(html, "DATASYNC_ID");
  if (!dataSyncId)
    throw new Error("Could not detect the active YouTube Music account.");

  const sessionMatch = html.match(/"SESSION_INDEX"\s*:\s*(?:"(\d+)"|(\d+))/);
  const authUser = Number.parseInt(
    sessionMatch?.[1] ?? sessionMatch?.[2] ?? "",
    10,
  );
  if (!Number.isInteger(authUser) || authUser < 0 || authUser > 255)
    throw new Error("Could not detect the active YouTube Music account.");

  const [first, second = ""] = dataSyncId.split("||", 2);
  if (!/^\d{1,128}$/.test(first) || (second && !/^\d{1,128}$/.test(second)))
    throw new Error("Could not detect the active YouTube Music account.");

  return {
    accountId: second ? first : first,
    authUser,
    delegatedSessionId: second ? first : undefined,
  };
}

function embeddedJsonString(html: string, key: string): string | undefined {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = html.match(
    new RegExp(`"${escapedKey}"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`),
  );
  if (!match) return undefined;
  try {
    return JSON.parse(`"${match[1]}"`) as string;
  } catch {
    return undefined;
  }
}

export function cookieHeader(
  cookies: Pick<chrome.cookies.Cookie, "name" | "value">[],
): string {
  return cookies
    .filter((cookie) => AUTH_COOKIE_NAMES.has(cookie.name))
    .sort((left, right) => left.name.localeCompare(right.name))
    .map((cookie) => `${cookie.name}=${cookie.value}`)
    .join("; ");
}

export function hasSapisidCookie(
  cookies: Pick<chrome.cookies.Cookie, "name" | "value">[],
): boolean {
  return cookies.some(
    (cookie) =>
      SAPISID_COOKIE_NAMES.has(cookie.name) && cookie.value.trim().length > 0,
  );
}

export function shouldRefreshChangedCookie(
  cookie: Pick<chrome.cookies.Cookie, "name" | "domain">,
): boolean {
  if (!AUTH_COOKIE_NAMES.has(cookie.name)) return false;
  const domain = cookie.domain.toLowerCase();
  return domain === "youtube.com" || domain.endsWith(".youtube.com");
}

export async function signedHeaders(
  body: string,
  connection: DeviceConnection,
  timestamp = Math.floor(Date.now() / 1000),
  nonce = crypto.randomUUID().replaceAll("-", ""),
): Promise<Record<string, string>> {
  const encoder = new TextEncoder();
  const bodyDigest = await crypto.subtle.digest(
    "SHA-256",
    encoder.encode(body),
  );
  const bodyHash = [...new Uint8Array(bodyDigest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
  const canonical = `${timestamp}\n${nonce}\n${bodyHash}`;
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(connection.deviceToken),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign(
    "HMAC",
    key,
    encoder.encode(canonical),
  );
  return {
    "Content-Type": "application/json",
    "X-Scrobble-Device": connection.deviceId,
    "X-Scrobble-Timestamp": String(timestamp),
    "X-Scrobble-Nonce": nonce,
    "X-Scrobble-Signature": base64Url(new Uint8Array(signature)),
  };
}

function base64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary)
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replace(/=+$/, "");
}
