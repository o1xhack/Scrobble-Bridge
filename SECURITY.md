# Security policy

## Supported version

Security fixes target the latest `1.x` release branch. This repository currently contains the 1.0 implementation.

## Reporting a vulnerability

Do not open a public issue containing credentials, proof-of-concept account access, or private listening history. Use GitHub private vulnerability reporting when it is enabled for the repository. If it is not enabled, contact the repository owner through a private channel listed on the GitHub profile and include only enough detail to establish impact.

Never include cookies, API keys, shared secrets, session keys, admin tokens, pairing codes or device tokens in a report. Replace them with `[REDACTED]`.

## Last.fm application credentials

The public source tree contains no project-owned Last.fm API key or shared secret. Maintainer-built desktop installers can receive an application-wide key and shared secret through the `SCROBBLE_LASTFM_API_KEY` and `SCROBBLE_LASTFM_SHARED_SECRET` repository secrets. They are copied into the user's OS credential vault on first launch only when the user has not already configured a custom application.

An application secret embedded in a distributed native binary is extractable by a determined user. Treat it as a revocable application identifier, not as a protected server-side secret: monitor usage, rotate it if abused, never print it in CI logs, and never confuse it with an individual listener's Last.fm session. Each listener still authorizes their own account directly with Last.fm, and their session remains on their own device. Source builds without both settings retain the bring-your-own-application path.

## Deployment boundary

- Do not expose port `8787` directly to the public internet.
- Put NAS extension traffic behind HTTPS with a certificate Chrome trusts.
- Protect `/data/secrets/master.key` and `/data/secrets/admin.token`; backups of encrypted credentials are unusable without the master key, while theft of both files permits decryption.
- Use the container's non-root user, read-only root filesystem and dropped capabilities from the supplied Compose file.
- Revoke a paired device immediately if its Chrome profile or computer is lost.
- Treat exported diagnostics as private operational data even though secret values are excluded.

## Security design

Credential-bearing Rust and TypeScript structures avoid raw debug output. Last.fm transport errors strip request URLs, upstream HTTP bodies are not echoed, native messages are capped at 1 MiB, Unix IPC lives under a mode-`0700` directory, NAS requests use HTTPS plus a timestamped HMAC signature, and nonces are atomically persisted to prevent replay across restarts.

The app cannot protect credentials from malware already running as the same signed-in OS user, a compromised browser profile, a compromised NAS administrator, or a trusted root certificate installed by an attacker.
