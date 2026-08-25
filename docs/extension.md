# Chrome extension and credential delivery

## Desktop mode

The extension asks for no YouTube site access at installation. When the user clicks **Enable automatic refresh**, Chrome requests the optional `cookies` permission together with `https://music.youtube.com/*` and `https://youtube.com/*`. The second origin is required because Chrome authorizes Cookies API results against the domain that owns each Cookie, while YouTube Music authentication Cookies are parent-domain Cookies. No other YouTube subdomain and no content script are requested. After approval, the extension reads an allowlist of YouTube authentication cookies after startup, relevant Cookie changes, a six-hour alarm, or a manual refresh. It also fetches the signed-in YouTube Music shell and derives the active `SESSION_INDEX` plus stable account/delegated-channel context, so the user does not enter an account label or Google account index. Unknown or inconsistent account context fails closed. It sends the snapshot to `com.scrobblebridge.host` using Manifest V3 Native Messaging. The bundled native host validates the versioned message, starts the App when possible, and forwards it to a same-user local socket. The App validates the credential and stores it in the OS credential vault. **Remove YouTube Music access** revokes both optional permissions.

Chrome may be closed after a successful refresh. The App keeps running and can use the stored snapshot until Google invalidates it. A closed browser cannot refresh a changed or expired Cookie; opening and signing in to YouTube Music is the recovery action.

The unpacked development manifest has a fixed public key, producing extension ID `nocefljecnigpgfgalgjefcigeidoglj`. After Chrome Web Store assigns a production ID, set the GitHub repository variable `SCROBBLE_PRODUCTION_EXTENSION_ID` before building the desktop App so the generated Native Messaging manifest allowlists both exact IDs. The build rejects malformed production IDs; wildcard extension origins are not supported. Store listing copy and permission justifications are documented in [Chrome Web Store preparation](chrome-web-store.md).

## NAS mode

1. Put the daemon behind an HTTPS origin with a Chrome-trusted certificate.
2. Open the Web management UI with the admin token.
3. Create a one-time pairing code and label; it expires after ten minutes and is consumed once.
4. In the extension, choose NAS, enter the exact HTTPS origin and code, and approve that origin permission.
5. The extension pins the returned server instance ID and keeps a random device token in `chrome.storage.local`.

Each credential update includes the device ID, Unix timestamp, random nonce, SHA-256 body hash and HMAC-SHA256 signature. The daemon accepts only a five-minute clock window and atomically records nonces in SQLite, so replay remains blocked after restart. Device access can be revoked from the Web UI.

**Forget this NAS** removes the extension's local device token and optional origin permission. It does not pretend to reach a server that may be offline; use **Revoke** in the NAS Web UI to invalidate the server-side token.

The reverse proxy must set `X-Forwarded-Proto: https`. Insecure HTTP, certificate-warning bypasses and wildcard device enrollment are intentionally unsupported.
