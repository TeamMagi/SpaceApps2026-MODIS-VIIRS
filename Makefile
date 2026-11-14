.PHONY: dev dev-backend dev-frontend build build-backend build-frontend test test-backend test-frontend lint lint-backend check docker docker-up docker-down docker-logs docker-shell fetch-sample clean clean-cache help

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

docker: ## Build Docker image
	docker compose build

docker-up: ## Start containers (requires .env)
	@if [ ! -f .env ]; then \
		echo "Error: .env file not found. Copy .env.example to .env and set FIRMS_API_KEY."; \
		exit 1; \
	fi
	docker compose up -d

docker-down: ## Stop containers
	docker compose down

docker-logs: ## Show container logs
	docker compose logs -f pyrosync

docker-shell: ## Open a shell inside the running container
	docker compose exec pyrosync /bin/sh

# ---- Data ----

fetch-sample: ## Fetch sample data from FIRMS API (requires FIRMS_API_KEY)
	@if [ -z "$$FIRMS_API_KEY" ]; then \
		echo "Error: FIRMS_API_KEY must be set in the environment (e.g. from .env)."; \
		exit 1; \
	fi
	cargo run -p pyrosync-firms --example nrt_check -- --save-samples

# ---- Clean ----

# Preserve .cache/ because re-downloading ~2.4 GB of FIRMS archives is slow
clean: ## Clean build artifacts (preserves .cache/)
	cargo clean
	rm -rf web/node_modules web/dist

clean-cache: ## Remove local FIRMS archive cache (.cache/)
	rm -rf .cache

# ---- Help ----

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
