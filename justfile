# pba-service — Purpose-Bound Account Service

set dotenv-load

# Default: show available targets
default:
    @just --list

# ── Variables ───────────────────────────────────────────────

# Dev environment
DEV_TB_PORT := "3000"
DEV_TB_DATA := ".tb_data/dev"
DEV_APP_PORT := "3030"
DEV_DB := "pba_service"
DEV_DATABASE_URL := "postgresql://localhost:5432/" + DEV_DB + "?host=/tmp"

# Test environment
TEST_TB_PORT := "3001"
TEST_TB_DATA := ".tb_data/test"
TEST_APP_PORT := "3031"
TEST_DB := "pba_service_test"
TEST_DATABASE_URL := "postgresql://localhost:5432/" + TEST_DB + "?host=/tmp"

# ── Parameterized Primitives ───────────────────────────────

# Start TigerBeetle on a given port with a given data directory
[private]
tb-start port data_dir:
    @if lsof -ti :{{port}} -sTCP:LISTEN > /dev/null 2>&1; then \
        echo "TigerBeetle already running on port {{port}}"; \
    else \
        if [ ! -f {{data_dir}}/0_0.tigerbeetle ]; then \
            mkdir -p {{data_dir}}; \
            tigerbeetle format --cluster=0 --replica=0 --replica-count=1 {{data_dir}}/0_0.tigerbeetle; \
            echo "TigerBeetle data file created at {{data_dir}}"; \
        fi; \
        echo "Starting TigerBeetle on port {{port}}..."; \
        tigerbeetle start --addresses={{port}} {{data_dir}}/0_0.tigerbeetle & \
    fi

# Stop TigerBeetle on a given port
[private]
tb-stop port:
    @kill $(lsof -ti :{{port}} -sTCP:LISTEN) 2>/dev/null || echo "TigerBeetle not running on port {{port}}"

# Start pba-service in the background with given config, wait for health check
[private]
service-start port db_url tb_port:
    @echo "Starting pba-service on port {{port}}..."
    @if [ -n "$CI" ]; then \
        DATABASE_URL="{{db_url}}" TIGERBEETLE_ADDRESSES={{tb_port}} PORT={{port}} target/debug/pba-service & \
    else \
        DATABASE_URL="{{db_url}}" TIGERBEETLE_ADDRESSES={{tb_port}} PORT={{port}} cargo run -p pba-service & \
    fi
    @echo "Waiting for service to be ready..."
    @for i in $(seq 1 30); do \
        if curl -sf http://127.0.0.1:{{port}}/purpose-types > /dev/null 2>&1; then \
            echo "Service ready on port {{port}}"; \
            exit 0; \
        fi; \
        sleep 1; \
    done; \
    echo "ERROR: Service did not start in time"; exit 1

# Stop pba-service on a given port
[private]
service-stop port:
    @kill $(lsof -ti :{{port}} -sTCP:LISTEN) 2>/dev/null || echo "pba-service not running on port {{port}}"

# Reset a database (drop + recreate)
[private]
reset-db db:
    @psql -h /tmp -p 5432 -d postgres -c "DROP DATABASE IF EXISTS {{db}};" > /dev/null
    @psql -h /tmp -p 5432 -d postgres -c "CREATE DATABASE {{db}};" > /dev/null
    @echo "Database reset ({{db}})"

# ── Build ──────────────────────────────────────────────────

# Build the project (requires Zig 0.14 via nix develop)
build:
    cargo build

# Build in release mode
build-release:
    cargo build --release

# ── Dev Lifecycle ──────────────────────────────────────────

# Start infrastructure (Postgres + TigerBeetle)
infra-start: pg-start (tb-start DEV_TB_PORT DEV_TB_DATA)
    @echo "Infrastructure ready (Postgres on :5432, TigerBeetle on :{{DEV_TB_PORT}})"

# Stop infrastructure
infra-stop: (tb-stop DEV_TB_PORT) pg-stop
    @echo "Infrastructure stopped"

# Run the service in the foreground (requires infra running)
run:
    @echo "Admin dashboard will be at http://localhost:${PORT:-{{DEV_APP_PORT}}}/admin"
    cargo run -p pba-service

# Run with file watching (restarts on changes)
watch:
    cargo watch -x 'run -p pba-service'

# Stop pba-service only (leaves infrastructure running)
stop: (service-stop DEV_APP_PORT)

# Stop pba-service and all infrastructure
stop-all: stop infra-stop

# Start infrastructure, run the service, clean up on exit/Ctrl+C
run-all: infra-start
    @echo "Admin dashboard will be at http://localhost:{{DEV_APP_PORT}}/admin"
    @trap 'echo ""; just stop-all' EXIT; cargo run -p pba-service

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

# Local CI: format check + lint + build + test
local-ci: fmt-check lint build test
    @echo "local-ci passed"

# ── E2E Tests (Cucumber + Smithy SDK) ───────────────────────

# Start test infrastructure and service for E2E tests
e2e-start: pg-start (reset-db TEST_DB) (tb-start TEST_TB_PORT TEST_TB_DATA)
    just service-stop {{TEST_APP_PORT}}
    @sleep 1
    just service-start {{TEST_APP_PORT}} {{TEST_DATABASE_URL}} {{TEST_TB_PORT}}

# Stop test service and test TigerBeetle
e2e-stop: (service-stop TEST_APP_PORT) (tb-stop TEST_TB_PORT)

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
    just service-stop {{TEST_APP_PORT}}
    @sleep 1
    just reset-db {{TEST_DB}}
    just service-start {{TEST_APP_PORT}} {{TEST_DATABASE_URL}} {{TEST_TB_PORT}}
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

# ── Postgres ─────────────────────────────────────────────────

# Initialize local Postgres data directory
[private]
pg-init:
    @if [ ! -d "${PG_DATA:-.pg_data}" ]; then \
        initdb -D "${PG_DATA:-.pg_data}" --auth=trust; \
        echo "Postgres data directory created at ${PG_DATA:-.pg_data}"; \
    else \
        echo "Postgres data directory already exists at ${PG_DATA:-.pg_data}"; \
    fi

# Start local Postgres, create dev database, and verify health
pg-start: pg-init
    @pg_ctl -D "${PG_DATA:-.pg_data}" -l .pg.log -o "-p 5432 -k /tmp" status > /dev/null 2>&1 \
        && echo "Postgres already running" \
        || (pg_ctl -D "${PG_DATA:-.pg_data}" -l .pg.log -o "-p 5432 -k /tmp" start \
            && sleep 1 \
            && echo "Postgres started on port 5432")
    @createdb -h /tmp -p 5432 {{DEV_DB}} 2>/dev/null || true
    @if ! psql -h /tmp -p 5432 -d {{DEV_DB}} -c "SELECT 1" > /dev/null 2>&1; then \
        echo "Database '{{DEV_DB}}' is invalid, recreating..."; \
        dropdb -h /tmp -p 5432 {{DEV_DB}} 2>/dev/null || true; \
        createdb -h /tmp -p 5432 {{DEV_DB}}; \
        echo "Database recreated"; \
    fi

# Stop local Postgres
pg-stop:
    @pg_ctl -D "${PG_DATA:-.pg_data}" stop 2>/dev/null || echo "Postgres not running"

# ── Install ──────────────────────────────────────────────────

# Install system dependencies (macOS via Homebrew, non-Nix)
install-deps:
    @echo "Installing dependencies via Homebrew..."
    brew install rustup postgresql@16 just
    rustup-init -y --default-toolchain stable
    cargo install sqlx-cli --no-default-features --features postgres
    cargo install cargo-watch
    rustup component add clippy rustfmt
    @echo ""
    @echo "Dependencies installed. Run 'just infra-start' to start Postgres + TigerBeetle."

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
