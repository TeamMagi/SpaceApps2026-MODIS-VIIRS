# ---- Stage 1: Build Rust backend ----
# Cargo.toml uses edition = "2024", which requires Rust 1.85+ (matches rustc 1.100.0 in dev)
FROM rust:1.100-bookworm AS backend-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# Cache mounts require BuildKit (default in Docker 23+); copy the binary out before the target mount unmounts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p pyrosync-server \
    && cp /build/target/release/pyrosync-server /build/pyrosync-server

# ---- Stage 2: Build frontend ----
FROM node:20-bookworm-slim AS frontend-builder
WORKDIR /build
COPY web/package.json web/package-lock.json ./
# Require package-lock.json with no npm install fallback so frontend builds stay reproducible
RUN npm ci
COPY web/ .
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
COPY --from=frontend-builder --chown=pyrosync:pyrosync /build/dist ./static/

RUN chown -R pyrosync:pyrosync /app

USER pyrosync

ENV RUST_LOG=pyrosync=info
ENV STATIC_DIR=/app/static
ENV ARCHIVE_CACHE_DIR=/app/.cache/firms-archive
EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:3001/api/health || exit 1

ENTRYPOINT ["./pyrosync-server"]
