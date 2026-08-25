# Contributing

## Development setup

Install Rust 1.94.1, Node.js 24 and pnpm 10.34.5. Platform-specific desktop builds also require the current Tauri 2 prerequisites for macOS or Windows.

```bash
corepack enable
pnpm install --frozen-lockfile
cargo test --workspace
pnpm check
pnpm test
pnpm build
```

Before submitting a change, run Rust formatting and strict Clippy:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Change rules

- Never commit a real Cookie, token, key, browser export, SQLite database or diagnostic bundle.
- Add sanitized fixtures for every parser shape change; unknown upstream responses must fail closed.
- Preserve outbox idempotency and crash-recovery tests when changing synchronization behavior.
- Any new Chrome permission needs a user-facing reason and a matching privacy-document update.
- Keep desktop and Docker credential backends separate from the pure synchronization core.
- Set both `SCROBBLE_LASTFM_API_KEY` and `SCROBBLE_LASTFM_SHARED_SECRET` only for authorized maintainer desktop builds; omit both for ordinary open-source development.
- Never commit connected-account screenshots, personal listening history, production extension IDs inferred from an unassigned listing, or real application credentials.
- Do not describe an unsigned, unnotarized, untested artifact as a public release.

By contributing, you agree that your contribution is licensed under the repository's MIT License.
