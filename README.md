# Purpose Bound Account Service

Purpose-Bound Account Service — dual-pool accounts with purpose-based MCC validation, powered by TigerBeetle.

## What is this?

A Rust service that manages purpose-restricted financial accounts (e.g., health, education, food, transport). Each account has two internal pools:

- **Self-contribution** — funds deposited from the account holder's own bank account. Can be used for payments and withdrawals.
- **Others-contribution** — funds deposited from third parties (employer, family, etc.). Can only be used for purpose-restricted payments, never withdrawn.

Payments are validated against allowed Merchant Category Codes (MCCs) for the account's purpose, and funds are drawn from the others-pool first before falling back to the self-pool.

## Architecture

```
Axum HTTP handlers
    |
Service layer (business logic, MCC validation, pool splitting)
    |
Repository layer
    |
PostgreSQL (account metadata)  +  TigerBeetle (ledger / balances)
```

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/purpose-types` | List all purpose types with allowed MCCs |
| `GET` | `/purpose-types/{code}` | Get a specific purpose type |
| `POST` | `/accounts` | Create a purpose-bound account |
| `GET` | `/accounts/{id}` | Get account details |
| `GET` | `/accounts/{id}/balance` | Get pool balances |
| `PATCH` | `/accounts/{id}/status` | Freeze/reactivate an account |
| `POST` | `/accounts/{id}/deposits` | Deposit funds (auto-routed to self/others pool) |
| `POST` | `/accounts/{id}/payments` | Pay a merchant (MCC-validated, others-first splitting) |
| `POST` | `/accounts/{id}/withdrawals` | Withdraw from self-pool only |

The API is defined using [Smithy](https://smithy.io/) models under `model/`. A generated Rust client SDK lives at `crates/pba_client/`.

## Dev Setup

### Prerequisites

The easiest way to get all dependencies is via [Nix](https://nixos.org/):

```bash
nix develop
```

This provides Rust, PostgreSQL 16, TigerBeetle, Zig 0.14, just, sqlx-cli, cargo-watch, and Smithy CLI.

Alternatively, install manually via Homebrew:

```bash
just install-deps
```

### Development workflow

**Start services (once, leave running):**

```bash
just services-start    # Postgres on :5432, TigerBeetle on :3000
```

**Edit, build, run (inner loop):**

```bash
just run               # or: just watch (auto-restarts on changes)
```

The service starts on `http://localhost:3030`.

**Stop everything:**

```bash
just services-stop     # stop Postgres + TigerBeetle
just stop-all          # stop the app + all services
```

**All-in-one (starts services, runs app, cleans up on Ctrl+C):**

```bash
just run-all
```

### Running tests

**Unit tests:**

```bash
just test
```

**E2E tests (Cucumber BDD):**

```bash
just e2e
```

E2E tests use isolated infrastructure — a separate Postgres database (`pba_service_test`), TigerBeetle instance (port 3001), and app port (3031) — so they never touch dev data.

**Full local CI (format + lint + build + test):**

```bash
just local-ci
```

### Database migrations

```bash
just migrate                  # run pending migrations
just migrate-new add_field    # create a new migration
```

### Smithy SDK

```bash
just smithy-validate    # validate the model
just smithy-build       # regenerate the Rust client SDK
```

## Configuration

Environment variables (loaded from `.env`):

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgresql://localhost:5432/pba_service?host=/tmp` | Postgres connection |
| `TIGERBEETLE_ADDRESSES` | `3000` | TigerBeetle address(es) |
| `TIGERBEETLE_CLUSTER_ID` | `0` | TigerBeetle cluster ID |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `3030` | HTTP port |
| `RUST_LOG` | `pba_service=debug` | Log level |

## Available just targets

Run `just` to see all targets:

```
just services-start   # Start Postgres + TigerBeetle
just services-stop    # Stop Postgres + TigerBeetle
just run              # Run the service
just run-all          # Start services + run (auto-cleanup on exit)
just stop-all         # Stop app + all services
just build            # Build the project
just test             # Unit tests
just e2e              # E2E tests (isolated infra)
just local-ci         # Format + lint + build + test
just migrate          # Run database migrations
just smithy-build     # Regenerate Smithy SDK
```
