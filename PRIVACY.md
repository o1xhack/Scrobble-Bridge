# Privacy

Scrobble Bridge is local-first software. The project does not operate an account service, analytics endpoint, credential relay, or hosted sync database.

## Data processed

The software processes a selected YouTube Music account label, selected Google account index, allowlisted YouTube authentication cookies, YouTube Music listening-history metadata, Last.fm API application credentials, Last.fm session credentials, inferred play timestamps, and local operational status.

Official desktop installers may include the maintainer-operated Last.fm application credentials so listeners can authorize their own account without creating an API application. Those application-level credentials identify Scrobble Bridge; they are not a listener password or account session. Each listener approves access directly on Last.fm, and the resulting individual session is stored only in that listener's operating-system credential vault. Self-built desktop and NAS deployments may instead use an application owned by their operator.

The Chrome extension does not receive YouTube site access at install time. When the user explicitly enables automatic refresh, Chrome asks for the optional `cookies` permission plus `https://music.youtube.com/*` and the parent `https://youtube.com/*` origin that owns the authentication cookies. The extension then reads only named authentication cookies needed by the YouTube Music Web request. It does not request other YouTube subdomains or inject content scripts. It sends a credential snapshot either to the desktop App through Chrome Native Messaging or directly to the user's own NAS endpoint over HTTPS. Raw YouTube cookies are never written to `chrome.storage`, and the optional YouTube permission can be removed from the extension UI. NAS pairing device credentials do remain in `chrome.storage.local` so the extension can reconnect after Chrome restarts; removing the extension clears that local data, and the NAS administrator can independently revoke the device.

The manifest also declares `https://*/*` as an optional host-permission pattern so an operator can choose their own NAS domain; it is not granted at installation and the extension requests only the exact HTTPS origin entered during pairing.

## Storage

- macOS desktop secrets: macOS Keychain;
- Windows desktop secrets: Windows Credential Manager;
- NAS secrets: ChaCha20-Poly1305 encrypted file, with a separate 32-byte master key;
- non-secret sync state: local SQLite database;
- browser admin token: `sessionStorage`, cleared when the browser tab/session ends;
- diagnostics: status, counters, timestamps, platform and version only; no raw cookies, API shared secrets, session keys, pairing tokens or device tokens.

## Network destinations

The runtime contacts YouTube Music and Last.fm. In NAS mode the extension also contacts the HTTPS origin explicitly approved during pairing. No project-operated server receives this data.

## Retention and deletion

The runtime keeps the minimum history window and outbox required for idempotent synchronization. NAS SQLite backups run daily and retain the newest seven copies. Desktop users can remove application data and related Keychain/Credential Manager entries; NAS users can delete the persistent volume. Revoking a paired NAS device removes its server-side device token.

## Important limitation

Closing Chrome does not invalidate the last snapshot already stored by the runtime. If Google expires or revokes that credential, synchronization stops and asks the user to reconnect after Chrome is opened again. Scrobble Bridge does not attempt to bypass account security or promise permanent authentication.

## Chrome Web Store Limited Use disclosure

Scrobble Bridge's use and transfer of information received from Google services adheres to the [Chrome Web Store User Data Policy](https://developer.chrome.com/docs/webstore/program-policies/user-data-faq), including the Limited Use requirements. Authentication information, account identifiers, and YouTube Music page data are used only to provide the user-facing credential-refresh and scrobbling features described above. They are not used for advertising, profiling, creditworthiness, lending, or any unrelated purpose; they are not sold; and they are not made available for human reading except when the user explicitly provides specific diagnostic material for support or when required for security or legal compliance.
