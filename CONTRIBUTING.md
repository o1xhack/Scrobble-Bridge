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
- Run the existing checks that cover the code you changed. A pull request does not need to add a new test merely because it is a pull request.
- Add or change tests when behavior changes, a bug needs a regression guard, an external parser contract changes, or the release risk cannot be covered by existing tests. Keep tests focused on the behavior at risk.
- Add sanitized fixtures when a parser shape changes; unknown upstream responses must fail closed.
- Preserve the existing outbox idempotency and crash-recovery coverage when changing synchronization behavior.
- Run the complete release matrix once for the release candidate and record the exact commands and real-device checks in the release notes/checklist instead of duplicating broad test-only commits across every pull request.
- Routine dependency version updates are reviewed as one explicit release-preparation step. The repository keeps vulnerability alerts enabled but does not open recurring Dependabot version-update pull requests.
- Any new Chrome permission needs a user-facing reason and a matching privacy-document update.
- Keep desktop and Docker credential backends separate from the pure synchronization core.
- Set both `SCROBBLE_LASTFM_API_KEY` and `SCROBBLE_LASTFM_SHARED_SECRET` only for authorized maintainer desktop builds; omit both for ordinary open-source development.
- Never commit connected-account screenshots, personal listening history, production extension IDs inferred from an unassigned listing, or real application credentials.
- Do not describe an unsigned, unnotarized, untested artifact as a public release.

By contributing, you agree that your contribution is licensed under the repository's MIT License.
