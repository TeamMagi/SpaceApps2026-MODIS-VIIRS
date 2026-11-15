.PHONY: dev dev-backend dev-frontend build build-backend build-frontend test test-backend test-frontend lint lint-backend check docker docker-up docker-down docker-logs docker-shell data docker-seed seed precompute precompute-local bundle-derived fetch-sample verify-data manifest smoke clean clean-cache clean-derived help

# Shared host directory for the FIRMS country archive (override via environment or make args)
ARCHIVE_CACHE_HOST_DIR ?= .cache/firms-archive
# Shared host directory for precomputed /api/series disk cache (override via environment or make args)
DERIVED_CACHE_HOST_DIR ?= .cache/pyrosync-derived
# Target URL for 'make smoke'
SMOKE_URL ?= http://localhost:3001

# The legacy 32-bit MinGW in the machine PATH ships binutils 2.28, whose dlltool
# cannot emit x86-64 import libraries, so every cargo link step fails before it starts.
# Put MinGW-w64 ahead of it for all targets. Empty (and harmless) on Linux/Docker.
MINGW64_BIN ?= $(firstword $(wildcard $(HOME)/AppData/Local/Microsoft/WinGet/Packages/BrechtSanders.WinLibs.*/mingw64/bin))
ifneq ($(MINGW64_BIN),)
export PATH := $(MINGW64_BIN):$(PATH)
endif

# ---- Development ----

dev-backend:
	RUST_LOG=pyrosync=debug cargo run -p pyrosync-server

dev-frontend:
	cd web && npm run dev

dev: ## Start backend + frontend (run in separate terminals)
	@echo "Terminal 1: make dev-backend"
	@echo "Terminal 2: make dev-frontend"

# ---- Build ----

build-backend:
	cargo build --release -p pyrosync-server

build-frontend:
	cd web && npm ci && npm run build

build: build-backend build-frontend ## Build everything

# ---- Test ----

test-backend:
	cargo test --workspace

test-frontend:
	cd web && npm run build

test: test-backend test-frontend ## Run all tests

# ---- Lint ----

lint-backend:
	cargo clippy --workspace -- -D warnings

lint: lint-backend ## Run all lints

# ---- Check ----

check: ## Quick compilation check (no tests)
	cargo check --workspace

# ---- Docker ----

# Pre-create the bind-mount directory as the host user; otherwise Docker creates it owned by
# root on Linux, preventing the non-root container user from writing to it.
docker: ## Build Docker image
	mkdir -p $(ARCHIVE_CACHE_HOST_DIR) $(DERIVED_CACHE_HOST_DIR)
	docker compose build

docker-up: ## Start containers (requires .env)
	@if [ ! -f .env ]; then \
		echo "Error: .env file not found. Copy .env.example to .env and set FIRMS_API_KEY."; \
		exit 1; \
	fi
	mkdir -p $(ARCHIVE_CACHE_HOST_DIR) $(DERIVED_CACHE_HOST_DIR)
	docker compose up -d
	@if [ -z "$$(ls -A "$(ARCHIVE_CACHE_HOST_DIR)" 2>/dev/null)" ]; then \
		echo 'Historical archive not downloaded. Live endpoints work; run `make data` for /api/calendar and the archive endpoints.'; \
	fi || true

docker-down: ## Stop containers
	docker compose down

docker-logs: ## Show container logs
	docker compose logs -f pyrosync

docker-shell: ## Open a shell inside the running container
	docker compose exec pyrosync /bin/sh

# ---- Data ----

data: ## Download the full FIRMS country archive (~41 GB, opt-in, no API key needed)
	@echo "Downloading the full FIRMS country archive (~9 GB download, ~41 GB unpacked on disk)."
	@echo "This can take a while depending on your connection. No FIRMS API key is required."
	@echo "Safe to interrupt with Ctrl+C at any time; re-run 'make data' to resume where it left off."
	mkdir -p $(ARCHIVE_CACHE_HOST_DIR)
	docker compose --profile seed run --rm --build seed

docker-seed: data ## Alias for 'make data' (prefer 'make data')

# Fastest path for developers with a Rust toolchain — Docker-free alternative to 'make data'.
seed: ## Download the FIRMS country archive locally without Docker (alternative to 'make data', no API key)
	mkdir -p $(ARCHIVE_CACHE_HOST_DIR)
	ARCHIVE_CACHE_DIR=$(ARCHIVE_CACHE_HOST_DIR) cargo run --release -p pyrosync-firms --bin archive_seed

precompute: ## Precompute /api/series disk cache for all archive regions via Docker
	mkdir -p $(ARCHIVE_CACHE_HOST_DIR) $(DERIVED_CACHE_HOST_DIR)
	docker compose run --rm --build pyrosync precompute

precompute-local: ## Precompute /api/series disk cache locally without Docker
	mkdir -p $(DERIVED_CACHE_HOST_DIR)
	ARCHIVE_CACHE_DIR=$(ARCHIVE_CACHE_HOST_DIR) DERIVED_CACHE_DIR=$(DERIVED_CACHE_HOST_DIR) cargo run --release -p pyrosync-server -- precompute

bundle-derived: ## Copy precomputed /api/series cache into data/derived/series for git sharing
	@if [ ! -d "$(DERIVED_CACHE_HOST_DIR)/series" ]; then \
		echo "Error: $(DERIVED_CACHE_HOST_DIR)/series not found. Run 'make precompute' or 'make precompute-local' first."; \
		exit 1; \
	fi
	mkdir -p data/derived/series
	@count=0; \
	for f in "$(DERIVED_CACHE_HOST_DIR)/series"/*; do \
		[ -f "$$f" ] || continue; \
		case "$$(basename "$$f")" in .*.tmp.*) continue ;; esac; \
		cp "$$f" data/derived/series/; \
		count=$$((count + 1)); \
	done; \
	echo "Copied $$count file(s) to data/derived/series/."; \
	echo "Commit data/derived to share precomputed results with the team."

fetch-sample: ## Fetch sample data from FIRMS API (requires FIRMS_API_KEY)
	@if [ -z "$$FIRMS_API_KEY" ]; then \
		echo "Error: FIRMS_API_KEY must be set in the environment (e.g. from .env)."; \
		exit 1; \
	fi
	cargo run -p pyrosync-firms --example nrt_check -- --save-samples

verify-data: ## Verify the local archive cache against data/archive-manifest.sha256
	ARCHIVE_CACHE_DIR=$(ARCHIVE_CACHE_HOST_DIR) sh tools/verify-archive.sh verify

manifest: ## Regenerate data/archive-manifest.sha256 from the local cache
	ARCHIVE_CACHE_DIR=$(ARCHIVE_CACHE_HOST_DIR) sh tools/verify-archive.sh generate

smoke: ## Run end-to-end smoke checks against a running server (default http://localhost:3001)
	bash tools/smoke_test.sh $(SMOKE_URL)

# ---- Clean ----

# Preserve .cache/ because re-downloading ~41 GB of FIRMS archives is slow
clean: ## Clean build artifacts (preserves .cache/)
	cargo clean
	rm -rf web/node_modules web/dist

clean-cache: ## Remove local FIRMS archive cache ($(ARCHIVE_CACHE_HOST_DIR))
	rm -rf $(ARCHIVE_CACHE_HOST_DIR)

clean-derived: ## Remove derived /api/series cache ($(DERIVED_CACHE_HOST_DIR))
	rm -rf $(DERIVED_CACHE_HOST_DIR)

# ---- Help ----

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
