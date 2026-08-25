# Scrobble Bridge 1.0 release checklist

Checked items have current local evidence. Unchecked items are still required before the matching release claim may be made.

The current delivery is an open-source MVP. Native Windows QA, target NAS hardware, binary signing/notarization and public artifact publication remain independent release gates.

## Source gate

- [x] `cargo fmt --all -- --check`
- [x] `cargo test --workspace --all-targets`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `pnpm install --frozen-lockfile && pnpm check && pnpm test && pnpm build` — zero type errors
- [x] Windows x64 `cargo xwin check` and strict workspace Clippy for all targets
- [x] Dependency/security review with `cargo-deny`, `cargo-audit` and `pnpm audit`
- [x] Source/artifact secret-pattern review; diagnostics contain no credential values
- [x] Public documentation, demo fixtures and screenshots exclude real connected-account identifiers
- [x] English source-of-truth README and Simplified Chinese download/install guide; `docs:check` validates both entrypoints and their local links

## Last.fm application and onboarding

- [x] Existing Scrobble Bridge Last.fm API application ownership confirmed
- [x] Official desktop builds support application-wide credentials without requiring users to enter an API key or shared secret
- [x] Missing, partial and user-provided application credentials are handled without overwriting existing configuration
- [x] Returning from browser authorization automatically attempts to complete the user's individual Last.fm session
- [x] Both GitHub Actions repository secrets are configured for official bundled-credential installers
- [x] Source-built desktop apps and NAS preserve the bring-your-own-application option
- [x] Public security documentation discloses that a shared secret embedded in a native installer is extractable

## Desktop artifacts

- [x] macOS Apple Silicon DMG installs, launches, registers Native Messaging, survives window close, uninstalls by removing the App and reinstalls cleanly
- [x] macOS pause persistence and LaunchAgent autostart enable/disable
- [x] macOS hidden-window Dock reopen restores and focuses the main window
- [x] Desktop, extension and NAS Web UI provide Simplified Chinese and English with a persistent selector
- [x] Installed App writes source credentials to Keychain, preserves them across restart, does not repopulate the Cookie field, and emits no credential value to unified logs
- [x] macOS Intel DMG cross-builds as x86_64 and mounts successfully
- [x] Both `.app` bundles pass `codesign --verify --deep --strict` with Developer ID and Hardened Runtime
- [ ] Apple notarization accepted and ticket stapled
- [ ] macOS Intel DMG runtime/install smoke on Rosetta or Intel hardware
- [x] Windows x64 release desktop and Native Messaging PE binaries cross-build with the intended subsystems
- [ ] Windows 10/11 x64 NSIS installs per-user, launches, registers Chrome host and removes its registration on uninstall
- [ ] Installer and contained executables report a valid Authenticode signature
- [x] Checksums match the current local artifacts

## Runtime scenarios

- [x] First snapshot creates a baseline and does not backfill unknown history in deterministic integration tests
- [x] Later unique plays enqueue and submit exactly once in deterministic integration tests
- [x] Consecutive plays of the same track remain distinct
- [x] Process interruption during submission recovers without blind duplicate submission
- [x] Offline/retry/online state-machine recovery passes against controlled clients
- [x] YouTube Cookie expiry and Last.fm authorization expiry require explicit recovery instead of infinite background retries
- [x] Last.fm reauthorization immediately retries auth-blocked outbox entries without disturbing unrelated retry backoff
- [x] A history gap or single rejected song cannot permanently stop later background synchronization
- [x] Incomplete setup and invalid account context cannot leave the UI stuck in a syncing state
- [x] Long wake/suspend intervals, stopped macOS monotonic clocks and normal timer delays have deterministic catch-up coverage
- [x] Real YouTube Music baseline followed by a real Last.fm scrobble — 14-item baseline, then one new play accepted exactly once and confirmed on Last.fm
- [ ] Real consecutive repeat-play scrobble verification
- [ ] Real Mac sleep/wake catch-up without overlap duplication
- [ ] Chrome closed for 24 hours while the stored credential remains valid
- [ ] Expired Cookie attention state followed by Chrome refresh recovery
- [x] Pause survives App and container restart
- [x] Exported diagnostics contain status only and no credential values

## Chrome extension

- [x] Manifest V3 production build and ZIP integrity
- [x] Chrome Web Store ZIP excludes the fixed development key without changing the unpacked local extension ID
- [x] Fixed development ID `nocefljecnigpgfgalgjefcigeidoglj` matches the desktop allowlist
- [x] Install-time permissions exclude YouTube site access; optional Cookie access is limited to `music.youtube.com` plus the required parent Cookie domain `youtube.com`, excludes other subdomains/content scripts, and is user-revocable
- [x] One-time unpacked-extension load in Chrome
- [x] Real Native Messaging Cookie refresh into the installed desktop App and visible provider-specific connected state
- [x] Active Google/YouTube Music account is detected automatically; multiple-account and delegated/brand-account contexts have fail-closed parser/request coverage
- [x] Chrome Web Store permissions, listing copy, privacy disclosures and production-ID handoff are documented
- [ ] Production Chrome Web Store extension ID is assigned and configured in desktop artifacts

## NAS

- [x] Build and inspect both `linux/amd64` and `linux/arm64` images
- [x] Confirm UID/GID `11001`, read-only root, dropped capabilities, `no-new-privileges`, PID limit and bounded tmpfs
- [x] Confirm `/health/live` and configured/unconfigured `/health/ready`
- [x] Recreate the container and confirm volume state remains
- [x] Restore a consistent SQLite backup together with its correct master key and admin token
- [x] HTTPS pair, credential refresh, nonce replay rejection and device revoke
- [x] Ten repeated restart/recovery cycles on both architectures
- [x] OCI archive contains both platforms, SPDX SBOM and SLSA provenance
- [ ] Seven-day automated soak reaches completion with zero failures
- [ ] Seven-day soak on physical amd64 and arm64 NAS hardware

## Policy and publishing

- [ ] Product owner explicitly accepts the non-public YouTube endpoint/Cookie/service-terms risk
- [x] Existing Last.fm API application ownership and individual account-authorization flow confirmed
- [ ] Commercial distribution is separately approved by Last.fm before introducing paid access
- [x] Privacy disclosures match the implemented extension permissions, storage and network destinations
- [x] Current Chrome distribution is explicitly documented as developer-mode sideloading
- [ ] Chrome Web Store listing reviewed and submission separately authorized
- [ ] Git tag, GitHub Release, GHCR push and store submission each receive separate authorization
