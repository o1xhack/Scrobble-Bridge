# Third-party notices

Scrobble Bridge is built with third-party Rust and JavaScript packages. Exact resolved versions are recorded in `Cargo.lock` and `pnpm-lock.yaml`; those lockfiles are the authoritative dependency inventory for a given source revision.

Major components include Tauri, Svelte, Tokio, Axum, Reqwest, Rusqlite/SQLite, RustCrypto primitives, Keyring, Vite and Vitest. Each component remains subject to its own license and copyright notices. The project does not relicense third-party code under the Scrobble Bridge MIT license.

Release automation produces dependency metadata and a software bill of materials for the multi-architecture container image. Distributors are responsible for preserving all notices required by the exact dependency versions they ship and for reviewing lockfile changes before release.
