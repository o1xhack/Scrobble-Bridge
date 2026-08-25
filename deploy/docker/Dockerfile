# syntax=docker/dockerfile:1.8
FROM --platform=$BUILDPLATFORM node:25-bookworm-slim AS web-builder
WORKDIR /source
RUN corepack enable && corepack prepare pnpm@10.34.5 --activate
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json apps/web/package.json
COPY packages/ui/package.json packages/ui/package.json
RUN pnpm install --frozen-lockfile
COPY apps/web apps/web
COPY packages/ui packages/ui
RUN pnpm --filter @scrobble-bridge/web build

FROM --platform=$BUILDPLATFORM rust:1.97-bookworm AS rust-builder
ARG TARGETARCH
WORKDIR /source
# Debian security updates should remain installable across both release architectures;
# pinning snapshot-specific package revisions would make those builds brittle.
# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        cmake \
        gcc-aarch64-linux-gnu \
        gcc-x86-64-linux-gnu \
        libc6-dev-amd64-cross \
        libc6-dev-arm64-cross \
        ninja-build \
        perl \
        pkg-config \
    && rustup target add aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu \
    && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
# Cargo resolves every workspace member even when only the daemon is built.
COPY apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.toml
COPY apps/desktop/src-tauri/build.rs apps/desktop/src-tauri/build.rs
COPY apps/desktop/src-tauri/src apps/desktop/src-tauri/src
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc \
    AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar \
    AR_x86_64_unknown_linux_gnu=x86_64-linux-gnu-ar
RUN case "$TARGETARCH" in \
        arm64) rust_target=aarch64-unknown-linux-gnu ;; \
        amd64) rust_target=x86_64-unknown-linux-gnu ;; \
        *) echo "unsupported Docker target architecture: $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && cargo build --locked --release --package scrobble-daemon --target "$rust_target" \
    && mkdir -p /out \
    && cp "target/$rust_target/release/scrobble-daemon" /out/scrobble-daemon

FROM debian:bookworm-slim AS runtime
# Keep ca-certificates on the current Bookworm security revision.
# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 11001 scrobble \
    && useradd --uid 11001 --gid scrobble --home-dir /nonexistent --shell /usr/sbin/nologin scrobble \
    && mkdir -p /app/web /data \
    && chown -R scrobble:scrobble /data
COPY --from=rust-builder /out/scrobble-daemon /usr/local/bin/scrobble-daemon
COPY --from=web-builder /source/apps/web/dist /app/web
ENV SCROBBLE_BIND=0.0.0.0:8787 \
    SCROBBLE_DATA_DIR=/data \
    SCROBBLE_WEB_DIR=/app/web \
    RUST_LOG=info
USER 11001:11001
EXPOSE 8787
VOLUME ["/data"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD ["scrobble-daemon", "healthcheck"]
ENTRYPOINT ["scrobble-daemon"]
