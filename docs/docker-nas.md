# Docker / NAS deployment

The container is a complete, always-on Scrobble Bridge runtime for `linux/amd64` and `linux/arm64`. It does not need Chrome, a display server, Python, Node.js, or a project-operated cloud service after the image is built.

## Start it

```bash
docker compose -f deploy/docker/compose.yaml up -d --build
docker compose -f deploy/docker/compose.yaml exec scrobble-bridge \
  sh -c 'cat /data/secrets/admin.token'
```

Open `http://NAS_ADDRESS:8787` on your private network and enter that one-time local admin credential. Complete YouTube Music and Last.fm setup in the Web UI. The token and encrypted credential vault are mode `0600` inside the persistent volume.

The source tree never contains a Last.fm shared secret. Self-built deployments use a Last.fm API application owned by the user. YouTube Music browser credentials are encrypted at rest with ChaCha20-Poly1305; the 32-byte master key lives at `/data/secrets/master.key` by default. Set `SCROBBLE_MASTER_KEY_FILE` to a mounted Docker secret if your NAS supports it.

## Storage and lifecycle

- `/data/state.sqlite3`: history window and idempotent outbox;
- `/data/credentials.enc`: encrypted credential values;
- `/data/secrets/master.key`: generated encryption key unless an external secret is mounted;
- `/data/secrets/admin.token`: Web/API access token;
- `/data/backups/state-*.sqlite3`: consistent daily SQLite backups; newest seven retained;
- `restart: unless-stopped`: resumes after NAS restart;
- `/health/live`: process liveness;
- `/health/ready`: returns `503` until both services are configured.

The root filesystem is read-only, the service runs as UID/GID `11001`, all Linux capabilities are dropped, and `/tmp` is a bounded tmpfs. Pause/resume state is stored in SQLite and survives restart. Back up the entire volume, including the master key. A database without its key cannot recover the credentials; a key without the database cannot recover sync state.

## HTTPS requirement for the Chrome extension

Port `8787` is plain HTTP and is intended only for a trusted private LAN during manual administration. Do not expose it directly to the internet. The Chrome extension's NAS credential endpoint must use HTTPS through one of:

- the NAS vendor's trusted reverse proxy/certificate;
- Tailscale Serve;
- another reverse proxy with a certificate already trusted by Chrome.

Self-signed certificate bypasses and insecure HTTP credential delivery are intentionally unsupported. Native Messaging is only for a desktop App on the same computer and cannot connect directly to a NAS.

## Supported targets

- Synology Container Manager on 64-bit Intel/AMD or ARM;
- QNAP Container Station on 64-bit Intel/AMD or ARM;
- TrueNAS SCALE and standard Docker Compose hosts.

32-bit ARM (`arm/v7`) is not supported. The multi-architecture release workflow builds both platforms; real-device seven-day soak tests remain a release gate and cannot be replaced by emulation.
