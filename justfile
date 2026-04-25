# pba-service — Purpose-Bound Account Service

set dotenv-load

# Default: show available targets
default:
    @just --list

# ── Variables ───────────────────────────────────────────────

TEST_APP_PORT := "3031"

# ── Build ──────────────────────────────────────────────────

# Build the project (requires Zig 0.14 via nix develop)
build:
    cargo build

# Build in release mode
build-release:
    cargo build --release

# ── Dev Lifecycle ──────────────────────────────────────────

# Start everything (Postgres + TigerBeetle + app) via process-compose
run:
    process-compose up

# Start everything in the background (detached)
run-bg:
    process-compose up -D
    @echo "Services started in background. Use 'just logs' to view, 'just stop-all' to stop."

# View process-compose logs (attach to running instance)
logs:
    process-compose attach

# Run the service in the foreground (requires infra running separately)
run-service:
    @echo "Admin dashboard will be at http://localhost:${PORT:-3030}/admin"
    cargo run -p pba-service

# Run with file watching (restarts on changes)
watch:
    cargo watch -x 'run -p pba-service'

# Stop pba-service only (leaves infrastructure running)
stop:
    process-compose process stop pba-service

# Stop all services
stop-all:
    process-compose down

# ── Conventional Commits ──────────────────────────────────────

# Verify a commit message follows conventional commit standards
cog-verify message:
    cog verify "{{message}}"

# Check all commits on the current branch against conventional commit standards
cog-check:
    cog check

# Install cocogitto git hooks (commit-msg hook for local enforcement)
cog-install-hook:
    cog install-hook commit-msg
    @echo "Conventional commit hook installed — commits will be validated automatically"

# ── Testing & CI ─────────────────────────────────────────────

# Run unit tests
test:
    cargo test

# Run clippy lints (excludes generated SDK)
lint:
    cargo clippy -p pba-service -- -D warnings

# Format check (excludes generated SDK)
fmt-check:
    cargo fmt -p pba-service -- --check

# Format code (excludes generated SDK)
fmt:
    cargo fmt -p pba-service

# Local CI: format check + lint + build + test + commit convention check
local-ci: fmt-check lint build test cog-check
    @echo "local-ci passed"

# ── E2E Tests (Cucumber + Smithy SDK) ───────────────────────

# Start test infrastructure and service for E2E tests
e2e-start:
    process-compose -f process-compose.test.yml up -D
    @echo "Waiting for test service..."
    @for i in $(seq 1 60); do \
        if curl -sf http://127.0.0.1:{{TEST_APP_PORT}}/purpose-types > /dev/null 2>&1; then \
            echo "Test service ready on port {{TEST_APP_PORT}}"; \
            exit 0; \
        fi; \
        sleep 1; \
    done; \
    echo "ERROR: Test service did not start in time"; \
    echo "--- process-compose status ---"; \
    process-compose process list 2>&1 || true; \
    echo "--- tigerbeetle logs ---"; \
    process-compose process logs tigerbeetle 2>&1 | tail -20 || true; \
    echo "--- pba-service logs ---"; \
    process-compose process logs pba-service 2>&1 | tail -20 || true; \
    echo "--- db-reset logs ---"; \
    process-compose process logs db-reset 2>&1 | tail -10 || true; \
    echo "--- postgres logs ---"; \
    process-compose process logs postgres 2>&1 | tail -10 || true; \
    exit 1

# Stop test service and infrastructure
e2e-stop:
    process-compose down

# Run API E2E tests only (service must be running)
api-e2e-run:
    PBA_SERVICE_URL="http://127.0.0.1:{{TEST_APP_PORT}}" cargo test -p pba-service --test e2e

# Full API E2E cycle: start, run tests, stop
api-e2e: e2e-start api-e2e-run e2e-stop
    @echo "API E2E tests complete"

# Run UI E2E tests only (service must be running)
ui-e2e-run:
    PBA_SERVICE_URL="http://127.0.0.1:{{TEST_APP_PORT}}" cargo test -p pba-service --test ui_e2e

# Full UI E2E cycle: start, run tests, stop
ui-e2e: e2e-start ui-e2e-run e2e-stop
    @echo "UI E2E tests complete"

# Run UI E2E tests with visible Chrome (service must be running)
ui-e2e-watch:
    UI_HEAD=1 PBA_SERVICE_URL="http://127.0.0.1:{{TEST_APP_PORT}}" cargo test -p pba-service --test ui_e2e

# Run all E2E tests: API + UI (resets DB between suites)
e2e-all:
    just e2e-start
    just api-e2e-run
    @echo "Resetting DB for UI tests..."
    just e2e-stop
    @sleep 1
    just e2e-start
    just ui-e2e-run
    just e2e-stop
    @echo "All E2E tests complete"

# ── Database ─────────────────────────────────────────────────

# Run sqlx migrations
migrate:
    sqlx migrate run --source crates/pba_service/src/db/migrations

# Create a new migration
migrate-new name:
    sqlx migrate add --source crates/pba_service/src/db/migrations {{name}}

# ── Install ──────────────────────────────────────────────────

# Install system dependencies (macOS via Homebrew, non-Nix)
install-deps:
    @echo "Installing dependencies via Homebrew..."
    brew install rustup postgresql@16 just process-compose
    rustup-init -y --default-toolchain stable
    cargo install sqlx-cli --no-default-features --features postgres
    cargo install cargo-watch
    cargo install cocogitto
    rustup component add clippy rustfmt
    @echo ""
    @echo "Dependencies installed. Run 'just run' to start everything."

# Install pba-service binary to ~/.cargo/bin
install:
    cargo install --path crates/pba_service

# ── Smithy ───────────────────────────────────────────────────

# Validate Smithy model
smithy-validate:
    smithy validate model/

# Build Smithy SDK (generates Rust client at crates/pba_client/)
smithy-build:
    SMITHY_MAVEN_REPOS="https://repo1.maven.org/maven2|https://sandbox.assets.juspay.in/smithy/m2" smithy build
    @rm -rf crates/pba_client
    @mkdir -p crates/pba_client
    @cp -r sdk/output/source/rust-client-codegen/* crates/pba_client/
    @echo "SDK generated at crates/pba_client/"
    @cp sdk/output/source/openapi/PurposeBoundAccountService.openapi.json crates/pba_service/src/api/openapi.json
    @echo "OpenAPI spec generated at crates/pba_service/src/api/openapi.json"

# Clean generated SDK output
smithy-clean:
    rm -rf sdk/output
