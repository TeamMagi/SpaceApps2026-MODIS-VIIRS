# ---- Stage 1: Build Rust backend ----
FROM rust:1.83-bookworm AS backend-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p pyrosync-server

# ---- Stage 2: Build frontend ----
FROM node:20-bookworm-slim AS frontend-builder
WORKDIR /build
COPY web/package.json web/package-lock.json* ./
RUN npm ci || npm install
COPY web/ .
RUN npm run build

# ---- Stage 3: Runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /build/target/release/pyrosync-server ./pyrosync-server
COPY --from=frontend-builder /build/dist ./static/

ENV RUST_LOG=pyrosync=info
ENV STATIC_DIR=/app/static
EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -f http://localhost:3001/api/health || exit 1

ENTRYPOINT ["./pyrosync-server"]
