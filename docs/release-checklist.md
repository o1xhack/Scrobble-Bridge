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
- [x] Release-candidate dependency review completed across Cargo, pnpm, Docker and GitHub Actions; accepted only current-Rust-compatible patch/minor updates and regenerated locks. Deferred to a later train: Node 25, Rust 1.97, TypeScript 7, `@types/node` 26, `@types/chrome` 0.2, `prettier-plugin-svelte` 4, `winreg` 0.56 and pnpm 11
- [x] GitHub vulnerability alerts enabled; recurring Dependabot version-update PRs disabled so routine upgrades are handled in the release train
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

## Software updates

- [x] Tauri updater uses the permanent GitHub Releases `latest.json` endpoint and an embedded public verification key; the private key is absent from the repository
- [x] App startup, foreground return and sleep/wake recovery trigger a check only when the persisted 24-hour interval is due; Settings also provides an explicit manual check
- [x] A new version appears in a prominent home-screen banner with bilingual controls and release notes
- [x] Download, signature verification, and **Update now and restart** are separate user-controlled steps; there is no silent download or install
- [x] Release builds require signed updater artifacts, normalize both Mac architecture filenames, and generate a static two-architecture manifest as an internal GitHub Actions artifact
- [x] The current Apple Silicon and Intel updater archives were built from Developer ID-signed Apps and independently verified against the embedded Tauri public key
- [x] `pnpm version:set <semver>` updates the workspace, desktop, extension, Tauri and public download versions and creates a bilingual release-note stub without committing, tagging or publishing
- [x] Both current Mac updater archives and `.sig` files are generated from the final Developer ID-signed candidate and the generated `latest.json` points to their exact future Release asset names
- [ ] End-to-end update from an installed older signed build downloads, verifies, replaces the App and relaunches while preserving Keychain and SQLite state

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

## Test policy

- Pull requests run the existing checks relevant to their changed surfaces.
- Add focused tests only for changed behavior, regressions, parser contracts or uncovered release risk; do not add tests mechanically per PR.
- The release candidate runs and records the complete source, desktop, extension, NAS, signing and real-device gates once before publication.
