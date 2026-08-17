# Synthia full-stack Makefile
#
# Unified entry point for development, build, test, and deployment
# of the synthia-server (Rust) and synthia-web (React/Vite) pair.

SHELL := /bin/bash
.DEFAULT_GOAL := help

# ---- Configuration ----
SERVER_PORT       ?= 8080
WEB_PORT          ?= 5173
SERVER_CRATE      := synthia-server
WEB_DIR           := synthia-web
DOCKER_COMPOSE    := docker compose
COMPOSE_FILE_DEV  := docker-compose.yml
COMPOSE_FILE_PROD := docker-compose.prod.yml

# ---- Help ----

.PHONY: help
help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "; printf "Usage:\n  make \033[36m<target>\033[0m\n"} \
	  /^[a-zA-Z_][a-zA-Z0-9_-]*:.*## / {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ---- Development ----

.PHONY: dev dev-server dev-web dev-stop

dev: ## Start backend (:8080) and frontend (:5173) in parallel
	@echo "Starting synthia-server on :$(SERVER_PORT) and synthia-web on :$(WEB_PORT)"
	@$(MAKE) -j2 dev-server dev-web

dev-server: ## Start backend only (cargo run with hot reload on file changes)
	cargo run -p $(SERVER_CRATE) -- --config config.yaml

dev-web: ## Start frontend only (vite dev server)
	cd $(WEB_DIR) && npm run dev -- --port $(WEB_PORT)

dev-stop: ## Stop background dev processes (best-effort)
	@pkill -f "cargo run -p $(SERVER_CRATE)" || true
	@pkill -f "vite" || true

# ---- Build ----

.PHONY: build build-server build-web build-release

build: build-server build-web ## Build both server and web (debug)

build-server: ## Build backend binary (debug)
	cargo build -p $(SERVER_CRATE)

build-web: ## Build frontend assets
	cd $(WEB_DIR) && npm ci && npm run build

build-release: ## Build release binaries
	cargo build --release -p $(SERVER_CRATE)
	cd $(WEB_DIR) && npm ci && npm run build

# ---- Test ----

.PHONY: test test-rust test-web test-e2e test-unit test-wire

test: test-rust test-web ## Run all tests (Rust + frontend unit)

test-rust: ## Run all Rust tests in the workspace
	cargo test --workspace

test-unit: ## Run only Rust library unit tests
	cargo test --workspace --lib

test-wire: ## Run synthia-core tests under --features http,axum (wire contract gate)
	cargo test -p synthia-core --features http,axum

test-web: ## Run frontend unit tests
	cd $(WEB_DIR) && npm test

# --- Contract closure (双侧契约闭环) ---
# contract-scan  : 把后端 router.rs 与前端 fetch 调用扫描成 contract.yaml
# contract-check : 校验 contract.yaml 中无 frontend-only / backend-only diff；退出码非 0 = 不一致
# contract-report: 把 contract.yaml 渲染为人类可读 contract.md
# contract-coverage: 把 contract.yaml 列出的接口路径与 Playwright 契约集核对
# test-contract-closure: 跑 contract-closure 自身的单元测试 + Playwright 契约集
contract-scan: ## Scan backend router + frontend fetch calls into contract.yaml
	cd contract-closure && npm install --silent && npm run scan

contract-check: ## Check contract.yaml for frontend/backend dangling endpoints
	cd contract-closure && npm install --silent && npm run check

contract-report: ## Render contract.yaml into human-readable contract.md
	cd contract-closure && npm install --silent && npm run report

contract-coverage: contract-scan ## Verify Playwright contract set covers every entry in contract.yaml
	cd contract-closure && npm install --silent && npm run coverage

test-contract-closure: ## Run contract-closure scanner unit tests
	cd contract-closure && npm install --silent && npm test

test-contract-closure-playwright: ## Run Playwright sub-suite (assumes synthia-server reachable on :8080)
	cd synthia-web && npx playwright test --config=playwright.contract.config.ts

# --- end Contract closure ---

test-e2e: ## Run E2E tests via Playwright (auto-installs browsers if missing)
	@if ! command -v pacman >/dev/null 2>&1 && ! command -v apt-get >/dev/null 2>&1; then \
	  echo "Unsupported distro: install Playwright deps manually, see https://playwright.dev/docs/browsers#linux"; \
	  exit 1; \
	fi
	@if command -v pacman >/dev/null 2>&1; then \
	  echo "Detected Arch Linux."; \
	  echo "If Playwright system libs are missing, run: make install-e2e-deps"; \
	else \
	  echo "Detected Debian/Ubuntu."; \
	  echo "If Playwright system libs are missing, run: make install-e2e-deps"; \
	fi
	cd $(WEB_DIR) && npx playwright install chromium
	cd $(WEB_DIR) && npx playwright test

install-e2e-deps: ## Install Playwright system deps via sudo (interactive)
	@if command -v pacman >/dev/null 2>&1; then \
	  sudo pacman -S --needed --noconfirm \
	    nss libxcomposite libxdamage libxfixes libxrandr \
	    libxkbcommon alsa-lib atk at-spi2-atk cups gtk3 pango cairo; \
	elif command -v apt-get >/dev/null 2>&1; then \
	  sudo apt-get install -y libnss3 libnspr4 libatk1.0-0 libatk-bridge2.0-0 \
	    libcups2 libdrm2 libdbus-1-3 libxkbcommon0 libxcomposite1 libxdamage1 \
	    libxfixes3 libxrandr2 libgbm1 libxcb1 libpango-1.0-0 libcairo2 libasound2; \
	else \
	  echo "Unsupported distro: install Playwright deps manually, see https://playwright.dev/docs/browsers#linux"; \
	  exit 1; \
	fi

test-e2e-ui: ## Run E2E tests in UI mode (Playwright Inspector)
	cd $(WEB_DIR) && npx playwright test --ui

test-e2e-headed: ## Run E2E tests with browser visible
	cd $(WEB_DIR) && npx playwright test --headed

test-e2e-report: ## Open the last Playwright HTML report
	cd $(WEB_DIR) && npx playwright show-report

# ---- Code quality ----

.PHONY: fmt fmt-rust fmt-web lint lint-rust lint-web

fmt: fmt-rust fmt-web ## Format all code

fmt-rust: ## Format Rust code
	cargo +nightly fmt --all

fmt-web: ## Format frontend code
	cd $(WEB_DIR) && npx prettier --write "src/**/*.{ts,tsx,css,md}"

lint: lint-rust lint-web ## Lint all code

lint-rust: ## Lint Rust code (clippy)
	cargo clippy --all-targets --all-features --tests --all -- -D warnings

lint-web: ## Lint frontend TypeScript
	cd $(WEB_DIR) && npx tsc --noEmit

# ---- Docker ----

.PHONY: docker docker-up docker-down docker-build docker-prod-up docker-prod-down

docker: docker-build ## Build Docker images

docker-build: ## Build all Docker images (dev target)
	$(DOCKER_COMPOSE) -f $(COMPOSE_FILE_DEV) build

docker-up: ## Start Docker Compose (dev)
	$(DOCKER_COMPOSE) -f $(COMPOSE_FILE_DEV) up -d

docker-down: ## Stop Docker Compose (dev)
	$(DOCKER_COMPOSE) -f $(COMPOSE_FILE_DEV) down

docker-prod-up: ## Start Docker Compose (production)
	$(DOCKER_COMPOSE) -f $(COMPOSE_FILE_PROD) up -d

docker-prod-down: ## Stop Docker Compose (production)
	$(DOCKER_COMPOSE) -f $(COMPOSE_FILE_PROD) down

# ---- Deploy ----

.PHONY: deploy deploy-local deploy-prod

deploy: build ## Build all artifacts (alias for `build`)
	@echo "Build complete. Run 'make deploy-local' to start the server."

deploy-local: build-server ## Run server locally with production build
	./target/debug/$(SERVER_CRATE)

deploy-prod: build-release docker-prod-up ## Build release and start production Docker

# ---- Cleanup ----

.PHONY: clean clean-rust clean-web clean-docker

clean: clean-rust clean-web ## Clean all build artifacts

clean-rust: ## Clean Rust build artifacts
	cargo clean

clean-web: ## Clean frontend build artifacts
	cd $(WEB_DIR) && rm -rf dist node_modules

clean-docker: ## Remove Docker containers and images
	$(DOCKER_COMPOSE) -f $(COMPOSE_FILE_DEV) down --rmi all -v
	$(DOCKER_COMPOSE) -f $(COMPOSE_FILE_PROD) down --rmi all -v

# ---- Health ----

.PHONY: health

health: ## Check server health endpoint
	@curl -sS http://localhost:$(SERVER_PORT)/health || echo "Server not responding"
