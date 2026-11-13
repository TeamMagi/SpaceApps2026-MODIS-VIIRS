.PHONY: dev build test lint clean docker docker-up docker-down check

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

docker-up: ## Start containers
	docker compose up -d

docker-down: ## Stop containers
	docker compose down

docker-logs: ## Show container logs
	docker compose logs -f pyrosync

# ---- Data ----

fetch-sample: ## Fetch sample data from FIRMS API
	@echo "Requires FIRMS_API_KEY in .env"
	cargo run -p pyrosync-server -- --fetch-sample

# ---- Clean ----

clean: ## Clean build artifacts
	cargo clean
	rm -rf web/node_modules web/dist

# ---- Help ----

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
