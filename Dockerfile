# syntax=docker/dockerfile:1
# Multi-stage optimized build for Roastwarden with cargo-chef + sccache.
#
# Multi-arch (docs/DEPLOY.md, "Images"): linux/amd64 and linux/arm64 from an amd64 builder with
# no emulation. Every stage that runs a command runs on the build machine's platform
# ($BUILDPLATFORM): the dashboard and the notices are the same for both, and Rust cross-compiles
# for the target ($TARGETARCH). The runtime stage only copies, so an arm64 image needs no QEMU;
# only the amd64 one adds curl (runtime-amd64), which the owner's deploy script uses.

# ============================================
# Frontend: Build SvelteKit SPA
# ============================================
FROM --platform=$BUILDPLATFORM node:26-alpine AS frontend

WORKDIR /app/apps/dashboard
COPY apps/dashboard/package.json apps/dashboard/package-lock.json ./
RUN npm ci
COPY apps/dashboard/ ./
# Roastwarden's licence, which the build copies to build/licenses/LICENSE.txt (notices.js)
COPY LICENSE /app/LICENSE
# The dashboard knows its own revision, so Settings → About can say when an open tab is older
# than the server (after npm ci, so a new commit does not undo its cache)
ARG ROASTWARDEN_REVISION=unknown
ENV ROASTWARDEN_REVISION=$ROASTWARDEN_REVISION
RUN npm run build

# ============================================
# Notices: the Rust crates' licences (scripts/notices.sh)
# ============================================
# The third-party notices the image serves at /licenses/server.txt: every crate in the server's
# graph with its licence, the NOTICE files, the embedded fonts. Text only, so it runs on the
# build machine's own platform.
FROM --platform=$BUILDPLATFORM rust:1.99-bookworm AS notices
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo install cargo-about --version 0.9.0 --features cli --locked --root /usr/local
WORKDIR /app
COPY Cargo.toml Cargo.lock about.toml ./
COPY crates/ crates/
COPY scripts/notices.sh scripts/notices.py scripts/
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo fetch --locked && scripts/notices.sh /notices

# ============================================
# Base: Install Rust build tools
# ============================================
# Pinned: a toolchain change is a deliberate commit. On the build platform,
# with the aarch64 cross linker and target for an arm64 image.
# Built on Debian bookworm though the image runs on trixie: a binary needs the glibc it was
# built against or a newer one, and `roast` is also copied out of the image to the machine an
# agent runs on (docs/agents.md), which may be older than the image. Built here it asks for
# glibc 2.36, like the release's binaries.
FROM --platform=$BUILDPLATFORM rust:1.99-bookworm AS base

RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add aarch64-unknown-linux-gnu
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar

# Install cargo-chef and sccache for optimal caching
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo install cargo-chef --locked && \
    cargo install sccache --version ^0.19 --locked

# Configure sccache
ENV RUSTC_WRAPPER=sccache
ENV SCCACHE_DIR=/sccache

WORKDIR /app

# ============================================
# Planner: Generate dependency recipe
# ============================================
FROM base AS planner

# Copy entire workspace to analyze dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Generate recipe.json containing all workspace dependencies
RUN cargo chef prepare --recipe-path recipe.json

# ============================================
# Builder: Cook dependencies + build app
# ============================================
FROM base AS builder

# The target's Rust triple: amd64 → x86_64, arm64 → aarch64 (cross-compiled)
ARG TARGETARCH
RUN case "${TARGETARCH}" in \
      amd64) echo x86_64-unknown-linux-gnu ;; \
      arm64) echo aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported TARGETARCH ${TARGETARCH}" >&2; exit 1 ;; \
    esac > /triple

# Copy dependency recipe from planner
COPY --from=planner /app/recipe.json recipe.json

# Cook dependencies with cache mounts
# This layer is cached until Cargo.toml/Cargo.lock change
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/sccache,sharing=locked \
    cargo chef cook --release --target "$(cat /triple)" --recipe-path recipe.json

# Copy source code (invalidates cache only when source changes)
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# roast embeds the OpenAPI document (openapi --offline) and the Agent Skill (skill install)
COPY docs/openapi.json docs/openapi.json
COPY skills/roastwarden/SKILL.md skills/roastwarden/label-design.md skills/roastwarden/
# The firmware download fills in the example sketches (firmware.rs)
COPY examples/roasters/ examples/roasters/

# Build the server and the roast CLI
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/sccache,sharing=locked \
    cargo build --release --target "$(cat /triple)" -p roastwarden-server \
    && mkdir -p /out \
    && cp "target/$(cat /triple)/release/roastwarden-server" "target/$(cat /triple)/release/roast" /out/

# Show sccache statistics for debugging
RUN sccache --show-stats || true

# ============================================
# Accounts: the runtime's user, certificates and folders, made natively
# ============================================
# The same Debian release as the runtime, so its passwd and group files fit either platform's
# image. The roastwarden user is the same system uid as before (a data volume keeps its owner).
FROM --platform=$BUILDPLATFORM debian:trixie-slim AS accounts
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -r roastwarden && useradd -r -g roastwarden roastwarden \
    && mkdir -p /app/data

# ============================================
# Runtime: Minimal production image
# ============================================
FROM debian:trixie-slim AS runtime-base

# Only copies: no command runs on the target platform
COPY --from=accounts /etc/passwd /etc/group /etc/
COPY --from=accounts /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=accounts --chown=roastwarden:roastwarden /app /app

# amd64 adds curl, for a deploy script that checks health inside the container
FROM runtime-base AS runtime-amd64
RUN apt-get update && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*

FROM runtime-base AS runtime-arm64

FROM runtime-${TARGETARCH}

WORKDIR /app

# Copy the binaries from builder: the server, and roast (CLI, healthcheck, offline backup and
# restore: docker exec roastwarden roast backup)
COPY --from=builder /out/roastwarden-server /usr/local/bin/roastwarden-server
COPY --from=builder /out/roast /usr/local/bin/roast

# Copy SvelteKit SPA build from frontend stage
COPY --from=frontend --chown=roastwarden:roastwarden /app/apps/dashboard/build/ /app/static/
# Third-party notices beside the dashboard's (Settings → About links /licenses/)
COPY --from=notices --chown=roastwarden:roastwarden /notices/server.txt /app/static/licenses/server.txt

# Switch to non-root user
USER roastwarden

# Health check (exit 0 when /healthz answers)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["roast", "health", "--json"]

# Expose port
EXPOSE 8080

# Revision stamped by CI (build-arg); surfaced by GET /version. `git describe` and the commit's
# date: Settings → About's "0.4.0 + 66 commits" (GET /api/about, `build`)
ARG ROASTWARDEN_REVISION=unknown
ENV ROASTWARDEN_REVISION=$ROASTWARDEN_REVISION
ARG ROASTWARDEN_DESCRIBE=
ENV ROASTWARDEN_DESCRIBE=$ROASTWARDEN_DESCRIBE
ARG ROASTWARDEN_COMMITTED_AT=
ENV ROASTWARDEN_COMMITTED_AT=$ROASTWARDEN_COMMITTED_AT
# The repository it was built from, for About's CHANGELOG link (CI: its own server and repo)
ARG ROASTWARDEN_REPO_URL=
ENV ROASTWARDEN_REPO_URL=$ROASTWARDEN_REPO_URL

# Set default environment variables
ENV ROASTWARDEN_APP_DIR=/app/static
ENV ROASTWARDEN_HTTP_ADDR=0.0.0.0:8080
ENV MQTT_BROKER_HOST=mosquitto
ENV MQTT_BROKER_PORT=1883
ENV ROASTWARDEN_DB_PATH=/app/data/roastwarden.db
ENV RUST_LOG=info

CMD ["roastwarden-server"]
