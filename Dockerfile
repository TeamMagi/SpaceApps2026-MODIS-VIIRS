# ---- Stage 1: Build Rust backend ----
# Cargo.toml uses edition = "2024", which requires Rust 1.85+ (matches rustc 1.100.0 in dev)
FROM rust:1.100-bookworm AS backend-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# Cache mounts require BuildKit (default in Docker 23+); copy both binaries out before the target mount unmounts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p pyrosync-server \
    && cargo build --release -p pyrosync-firms --bin archive_seed \
    && cp /build/target/release/pyrosync-server /build/pyrosync-server \
    && cp /build/target/release/archive_seed /build/archive-seed

# ---- Stage 2: Build frontend ----
FROM node:20-bookworm-slim AS frontend-builder
WORKDIR /build
COPY web/package.json web/package-lock.json ./
# Require package-lock.json with no npm install fallback so frontend builds stay reproducible
RUN npm ci
COPY web/ .
# web/src/lib/caseStudy.ts imports ../../../data/case-study/turkey/stats.json, i.e. /data/... from /build/src/lib
COPY data/case-study/turkey/stats.json /data/case-study/turkey/stats.json
RUN npm run build

# ---- Stage 3: Runtime ----
FROM debian:bookworm-slim AS runtime
# curl is required by HEALTHCHECK; ca-certificates is needed for HTTPS requests to NASA FIRMS
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Run as non-root; pre-creating and owning the cache dir ensures named volume mounts remain writable
RUN groupadd --system pyrosync \
    && useradd --system --gid pyrosync --home-dir /app --shell /bin/sh pyrosync \
    && mkdir -p /app/.cache/firms-archive

WORKDIR /app

COPY --from=backend-builder --chown=pyrosync:pyrosync /build/pyrosync-server ./pyrosync-server
COPY --from=backend-builder --chown=pyrosync:pyrosync /build/archive-seed ./archive-seed
COPY --from=frontend-builder --chown=pyrosync:pyrosync /build/dist ./static/
COPY --chown=pyrosync:pyrosync data/archive-set.json ./data/archive-set.json

RUN chown -R pyrosync:pyrosync /app

USER pyrosync

ENV RUST_LOG=pyrosync=info
ENV STATIC_DIR=/app/static
ENV ARCHIVE_CACHE_DIR=/app/.cache/firms-archive
ENV ARCHIVE_SET_FILE=/app/data/archive-set.json
EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:3001/api/health || exit 1

ENTRYPOINT ["./pyrosync-server"]
