# pba-service — Purpose-Bound Account Service

set dotenv-load

# Default: show available targets
default:
    @just --list

# ── Build & Run ──────────────────────────────────────────────

# Build the project (default: TigerBeetle enabled, requires Zig 0.14 via nix develop)
build:
    cargo build

# Build with in-memory ledger (no TigerBeetle dependency)
build-inmem:
    cargo build --no-default-features --features inmem

# Build in release mode
build-release:
    cargo build --release

# Run the service (default: TigerBeetle, requires Postgres + TigerBeetle running)
run:
    cargo run

# Run with in-memory ledger (requires Postgres only)
run-inmem:
    cargo run --no-default-features --features inmem

# Run with file watching (restarts on changes)
watch:
    cargo watch -x run

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

# Reset database for E2E tests
e2e-reset-db:
    @psql -h /tmp -p 5432 -d postgres -c "DROP DATABASE IF EXISTS pba_service;" > /dev/null
    @psql -h /tmp -p 5432 -d postgres -c "CREATE DATABASE pba_service;" > /dev/null
    @echo "Database reset for E2E tests"

# Start the service for E2E tests (runs in background)
e2e-start: e2e-reset-db
    @pkill -f "target/debug/pba-service" 2>/dev/null || true
    @sleep 1
    @echo "Starting pba-service..."
    @cargo run &
    @echo "Waiting for service to be ready..."
    @for i in $(seq 1 30); do \
        if curl -sf http://127.0.0.1:${PORT:-3030}/purpose-types > /dev/null 2>&1; then \
            echo "Service ready on port ${PORT:-3030}"; \
            exit 0; \
        fi; \
        sleep 1; \
    done; \
    echo "ERROR: Service did not start in time"; exit 1

# Stop the E2E test service
e2e-stop:
    @pkill -f "target/debug/pba-service" 2>/dev/null || echo "Service not running"

# Run Cucumber E2E tests (service must be running)
e2e-run:
    cargo test --test e2e

# Full E2E cycle: reset DB, start service, run tests, stop service
e2e: e2e-start e2e-run e2e-stop
    @echo "E2E tests complete"

# ── Database ─────────────────────────────────────────────────

# Run sqlx migrations
migrate:
    sqlx migrate run --source src/db/migrations

# Create a new migration
migrate-new name:
    sqlx migrate add --source src/db/migrations {{name}}

# ── Local Infrastructure ─────────────────────────────────────

# Initialize local Postgres data directory
pg-init:
    @if [ ! -d "${PG_DATA:-.pg_data}" ]; then \
        initdb -D "${PG_DATA:-.pg_data}" --auth=trust; \
        echo "Postgres data directory created at ${PG_DATA:-.pg_data}"; \
    else \
        echo "Postgres data directory already exists at ${PG_DATA:-.pg_data}"; \
    fi

# Start local Postgres and create database
pg-start: pg-init
    @pg_ctl -D "${PG_DATA:-.pg_data}" -l .pg.log -o "-p 5432 -k /tmp" status > /dev/null 2>&1 \
        && echo "Postgres already running" \
        || (pg_ctl -D "${PG_DATA:-.pg_data}" -l .pg.log -o "-p 5432 -k /tmp" start \
            && sleep 1 \
            && createdb -h /tmp -p 5432 pba_service 2>/dev/null || true; \
            echo "Postgres started on port 5432")

# Stop local Postgres
pg-stop:
    @pg_ctl -D "${PG_DATA:-.pg_data}" stop 2>/dev/null || echo "Postgres not running"

# Start local TigerBeetle (creates data file if needed)
tb-start:
    @if [ ! -f .tb_data/0_0.tigerbeetle ]; then \
        mkdir -p .tb_data; \
        tigerbeetle format --cluster=0 --replica=0 --replica-count=1 .tb_data/0_0.tigerbeetle; \
        echo "TigerBeetle data file created"; \
    fi
    @echo "Starting TigerBeetle on port 3000..."
    tigerbeetle start --addresses=3000 .tb_data/0_0.tigerbeetle &

# Stop local TigerBeetle
tb-stop:
    @pkill tigerbeetle 2>/dev/null || echo "TigerBeetle not running"

# Setup with TigerBeetle (default): start Postgres + TigerBeetle
setup-tb: pg-start tb-start
    @echo "Local infrastructure ready (Postgres on :5432, TigerBeetle on :3000)"
    @echo "Run 'just run' to start the service"

# Setup with in-memory ledger: start Postgres only (no TigerBeetle)
setup-inmem: pg-start
    @echo "Local infrastructure ready (Postgres on :5432, in-memory ledger)"
    @echo "Run 'just run-inmem' to start the service"

# Stop all local services
teardown: tb-stop pg-stop
    @echo "Local infrastructure stopped"

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
    @echo "NOTE: TigerBeetle is optional (use 'just setup-inmem' + 'just run-inmem' without it)."
    @echo "  See https://docs.tigerbeetle.com/operating/hardware/"
    @echo ""
    @echo "Dependencies installed. Run 'just setup-tb' or 'just setup-inmem' to start local services."

# Install pba-service binary to ~/.cargo/bin
install:
    cargo install --path .

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
