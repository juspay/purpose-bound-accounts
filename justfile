# pba-service — Purpose-Bound Account Service

set dotenv-load

# Default: show available targets
default:
    @just --list

# ── Build & Run ──────────────────────────────────────────────

# Build the project (requires Zig 0.14 via nix develop)
build:
    cargo build

# Build in release mode
build-release:
    cargo build --release

# Run the service (requires Postgres + TigerBeetle running)
run:
    @echo "Admin dashboard will be at http://localhost:${PORT:-3030}/admin"
    cargo run -p pba-service

# Run with file watching (restarts on changes)
watch:
    cargo watch -x 'run -p pba-service'

# ── Testing & CI ─────────────────────────────────────────────

# Run unit tests
test:
    cargo test

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Format check
fmt-check:
    cargo fmt -- --check

# Format code
fmt:
    cargo fmt

# Local CI: format check + lint + build + test
local-ci: fmt-check lint build test
    @echo "local-ci passed"

# ── E2E Tests (Cucumber + Smithy SDK) ───────────────────────

# Test infrastructure ports and database
E2E_TB_PORT := "3001"
E2E_PG_DB := "pba_service_test"
E2E_APP_PORT := "3031"
E2E_DATABASE_URL := "postgresql://localhost:5432/" + E2E_PG_DB + "?host=/tmp"

# Start test TigerBeetle instance (separate data file and port)
tb-start-test:
    @if [ ! -f .tb_data/test/0_0.tigerbeetle ]; then \
        mkdir -p .tb_data/test; \
        tigerbeetle format --cluster=0 --replica=0 --replica-count=1 .tb_data/test/0_0.tigerbeetle; \
        echo "Test TigerBeetle data file created"; \
    fi
    @echo "Starting test TigerBeetle on port {{E2E_TB_PORT}}..."
    tigerbeetle start --addresses={{E2E_TB_PORT}} .tb_data/test/0_0.tigerbeetle &

# Stop test TigerBeetle instance (by port, won't affect dev instance)
tb-stop-test:
    @kill $(lsof -ti :{{E2E_TB_PORT}} -sTCP:LISTEN) 2>/dev/null || echo "Test TigerBeetle not running"

# Reset test database for E2E tests
e2e-reset-db:
    @psql -h /tmp -p 5432 -d postgres -c "DROP DATABASE IF EXISTS {{E2E_PG_DB}};" > /dev/null
    @psql -h /tmp -p 5432 -d postgres -c "CREATE DATABASE {{E2E_PG_DB}};" > /dev/null
    @echo "Test database reset ({{E2E_PG_DB}})"

# Start the service for E2E tests (uses test DB + test TB, runs in background)
e2e-start: e2e-reset-db tb-start-test
    @pkill -f "target/debug/pba-service.*{{E2E_APP_PORT}}" 2>/dev/null || true
    @sleep 1
    @echo "Starting pba-service for E2E tests on port {{E2E_APP_PORT}}..."
    @DATABASE_URL="{{E2E_DATABASE_URL}}" TIGERBEETLE_ADDRESSES={{E2E_TB_PORT}} PORT={{E2E_APP_PORT}} cargo run -p pba-service &
    @echo "Waiting for service to be ready..."
    @for i in $(seq 1 30); do \
        if curl -sf http://127.0.0.1:{{E2E_APP_PORT}}/purpose-types > /dev/null 2>&1; then \
            echo "Service ready on port {{E2E_APP_PORT}}"; \
            exit 0; \
        fi; \
        sleep 1; \
    done; \
    echo "ERROR: Service did not start in time"; exit 1

# Stop the E2E test service and test TigerBeetle
e2e-stop: tb-stop-test
    @kill $(lsof -ti :{{E2E_APP_PORT}} -sTCP:LISTEN) 2>/dev/null || echo "Test service not running"

# Run Cucumber E2E tests (service must be running)
e2e-run:
    PBA_SERVICE_URL="http://127.0.0.1:{{E2E_APP_PORT}}" cargo test -p pba-service --test e2e

# Full E2E cycle: reset DB, start service, run tests, stop service
e2e: e2e-start e2e-run e2e-stop
    @echo "E2E tests complete"

# Run browser UI tests (full cycle: start services, run tests, stop)
ui-e2e: e2e-start ui-e2e-run e2e-stop
    @echo "UI E2E tests complete"

# Run browser UI tests only (service must be running)
ui-e2e-run:
    PBA_SERVICE_URL="http://127.0.0.1:{{E2E_APP_PORT}}" cargo test -p pba-service --test ui_e2e

# Run browser UI tests with visible Chrome (service must be running)
ui-e2e-watch:
    UI_HEAD=1 PBA_SERVICE_URL="http://127.0.0.1:{{E2E_APP_PORT}}" cargo test -p pba-service --test ui_e2e

# Run all E2E tests: API + browser (resets DB between suites)
e2e-all:
    just e2e-start
    just e2e-run
    @echo "Resetting DB for UI tests..."
    @kill $(lsof -ti :{{E2E_APP_PORT}} -sTCP:LISTEN) 2>/dev/null || true
    @sleep 1
    just e2e-reset-db
    @echo "Starting pba-service for UI tests on port {{E2E_APP_PORT}}..."
    @DATABASE_URL="{{E2E_DATABASE_URL}}" TIGERBEETLE_ADDRESSES={{E2E_TB_PORT}} PORT={{E2E_APP_PORT}} cargo run -p pba-service &
    @for i in $(seq 1 30); do \
        if curl -sf http://127.0.0.1:{{E2E_APP_PORT}}/purpose-types > /dev/null 2>&1; then \
            echo "Service ready on port {{E2E_APP_PORT}}"; \
            break; \
        fi; \
        sleep 1; \
    done
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

# ── Local Infrastructure ─────────────────────────────────────

# Initialize local Postgres data directory
pg-init:
    @if [ ! -d "${PG_DATA:-.pg_data}" ]; then \
        initdb -D "${PG_DATA:-.pg_data}" --auth=trust; \
        echo "Postgres data directory created at ${PG_DATA:-.pg_data}"; \
    else \
        echo "Postgres data directory already exists at ${PG_DATA:-.pg_data}"; \
    fi

# Start local Postgres, create database, and verify it is healthy
pg-start: pg-init
    @pg_ctl -D "${PG_DATA:-.pg_data}" -l .pg.log -o "-p 5432 -k /tmp" status > /dev/null 2>&1 \
        && echo "Postgres already running" \
        || (pg_ctl -D "${PG_DATA:-.pg_data}" -l .pg.log -o "-p 5432 -k /tmp" start \
            && sleep 1 \
            && echo "Postgres started on port 5432")
    @createdb -h /tmp -p 5432 pba_service 2>/dev/null || true
    @if ! psql -h /tmp -p 5432 -d pba_service -c "SELECT 1" > /dev/null 2>&1; then \
        echo "Database 'pba_service' is invalid, recreating..."; \
        dropdb -h /tmp -p 5432 pba_service 2>/dev/null || true; \
        createdb -h /tmp -p 5432 pba_service; \
        echo "Database recreated"; \
    fi

# Stop local Postgres
pg-stop:
    @pg_ctl -D "${PG_DATA:-.pg_data}" stop 2>/dev/null || echo "Postgres not running"

# Start local TigerBeetle (creates data file if needed)
tb-start:
    @if [ ! -f .tb_data/dev/0_0.tigerbeetle ]; then \
        mkdir -p .tb_data/dev; \
        tigerbeetle format --cluster=0 --replica=0 --replica-count=1 .tb_data/dev/0_0.tigerbeetle; \
        echo "TigerBeetle data file created"; \
    fi
    @echo "Starting TigerBeetle on port 3000..."
    tigerbeetle start --addresses=3000 .tb_data/dev/0_0.tigerbeetle &

# Stop local TigerBeetle (by port, won't affect test instance)
tb-stop:
    @kill $(lsof -ti :3000 -sTCP:LISTEN) 2>/dev/null || echo "TigerBeetle not running"

# Start all dependent services (Postgres + TigerBeetle)
services-start: pg-start tb-start
    @echo "Services ready (Postgres on :5432, TigerBeetle on :3000)"

# Stop all dependent services
services-stop: tb-stop pg-stop
    @echo "Services stopped"

# Stop pba-service only (leaves Postgres + TigerBeetle running)
stop:
    @pkill -f "target/debug/pba-service" 2>/dev/null \
        || pkill -f "target/release/pba-service" 2>/dev/null \
        || echo "pba-service not running"

# Stop pba-service and all dependent services
stop-all: services-stop stop

# Start services, run the application, and stop services on exit/Ctrl+C
run-all: services-start
    @echo "Admin dashboard will be at http://localhost:${PORT:-3030}/admin"
    @trap 'echo ""; just stop-all' EXIT; cargo run -p pba-service


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
    @echo "Dependencies installed. Run 'just services-start' to start Postgres + TigerBeetle."

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

# Clean generated SDK output
smithy-clean:
    rm -rf sdk/output
