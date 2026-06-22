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
| `POST` | `/normal-accounts/{id}/transfers/{id}/reverse` | Reverse a posted transfer (admin) |
| `POST` | `/pb-accounts/{id}/payments/{id}/refund` | Refund a settled PB payment in whole or in part; multiple partials allowed (admin) |

The API is defined using [Smithy](https://smithy.io/) models under `model/`. A generated Rust client SDK lives at `crates/pba_client/`.

## Dev Setup

### First-time setup (TL;DR)

```bash
git clone <repo-url> && cd purpose-bound-accounts

nix develop            # 1. enter the dev shell (Rust, Postgres, TigerBeetle, just, ...)
cp .env.example .env   # 2. create your local config from the template
just run               # 3. start Postgres + TigerBeetle + the app
```

Then open the **Admin Dashboard** at <http://localhost:3030/admin>.

The default `.env` runs with auth disabled (`AUTH_ENABLED=false`), so no
Keycloak/OIDC provider is needed to get started. The sections below explain
each piece in more detail.

### Prerequisites

The easiest way to get all dependencies is via [Nix](https://nixos.org/):

```bash
nix develop
```

This provides Rust 1.94, PostgreSQL 16, TigerBeetle, Zig 0.14, just, sqlx-cli,
cargo-watch, cocogitto, process-compose, and the Smithy CLI — no other system
setup required.

If you use [direnv](https://direnv.net/), run `direnv allow` once; the dev
shell and your `.env` then load automatically whenever you `cd` into the repo.

Alternatively, install manually via Homebrew (macOS):

```bash
just install-deps
```

> **Docker is _not_ required** for the default dev flow. It is only needed if
> you opt in to running Keycloak — see [Authentication](#authentication).

### Configure your environment

The service reads configuration from a `.env` file. It is git-ignored, so each
clone needs its own — create one from the template:

```bash
cp .env.example .env
```

The defaults work out of the box for local development. The full list of
variables is in [Configuration](#configuration); the key one for first-time
setup is `AUTH_ENABLED=false`, which lets you run without an OIDC provider.

### Development workflow

**Start everything (Postgres + TigerBeetle + app):**

```bash
just run               # starts all services via process-compose, Ctrl+C stops everything
```

The service starts on `http://localhost:3030`. Open the **Admin Dashboard** at:

```
http://localhost:3030/admin
```

From there you can browse accounts, create new ones, make deposits/payments/withdrawals, and view purpose types with their allowed MCCs.

**Start in background:**

```bash
just run-bg            # starts detached; use 'just logs' to attach, 'just stop' to stop
```

### Running tests

**Unit tests:**

```bash
just test
```

**E2E tests (Cucumber BDD):**

```bash
just api-e2e       # API tests only (via Smithy SDK client)
just ui-e2e        # Browser UI tests only (headless Chrome)
just e2e-all       # Both API + browser tests
```

To run browser tests with visible Chrome (useful for debugging):

```bash
just e2e-start
just ui-e2e-watch  # opens Chrome so you can watch the tests
just e2e-stop
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

## Authentication

**Local dev runs with auth disabled.** The `.env.example` template sets
`AUTH_ENABLED=false`, and the dev stack does **not** start an OIDC provider, so
`just run` leaves the Admin UI and API open — just navigate to
`http://localhost:3030/admin`.

### Enabling Keycloak (optional)

Keycloak is **not** bundled in the dev stack. To test the real OIDC login flow
locally, run it standalone (requires Docker), then point the service at it:

1. Start Keycloak with the bundled realm:

   ```bash
   docker run --rm --name pba-keycloak \
     -p 8180:8080 -m 2g \
     -v "$(pwd)/keycloak/realm-export.json:/opt/keycloak/data/import/realm-export.json:ro" \
     -e KC_BOOTSTRAP_ADMIN_USERNAME=admin \
     -e KC_BOOTSTRAP_ADMIN_PASSWORD=admin \
     -e JAVA_OPTS_KC_HEAP="-XX:MaxRAMPercentage=50 -Xms256m -Xmx1024m -XX:UseSVE=0" \
     quay.io/keycloak/keycloak:26.0 \
     start-dev --import-realm
   ```

   > `-XX:UseSVE=0` works around a JVM crash on Apple Silicon (M-series); the
   > heap flags and `-m 2g` keep Keycloak from starving the rest of the stack.

2. Set `AUTH_ENABLED=true` in your `.env`.
3. `just run` (or `just run-service`) — the service now uses Keycloak for auth.

**Admin UI:** Navigate to `http://localhost:3030/admin` — you'll be redirected
to Keycloak to log in. Default credentials: `admin@pba.local` / `admin`

**API access:** Use the `Authorization` header with a base64-encoded
`client_id:client_secret`:

```bash
API_KEY=$(echo -n "pba-api:pba-api-secret" | base64)
curl -H "Authorization: ApiKey $API_KEY" http://localhost:3030/purpose-types
```

## Configuration

Environment variables (loaded from `.env`):

| Variable | Default | Description |
|----------|---------|-------------|
| `DB_HOST` | `localhost` | Postgres host (use a path like `/tmp` for Unix socket) |
| `DB_PORT` | `5432` | Postgres port |
| `DB_NAME` | `pba_service` | Postgres database name |
| `DB_USER` | `$USER` (Unix socket) | Postgres user (required for TCP connections) |
| `DB_PASSWORD` | _(empty)_ | Postgres password (supports encrypted values via `SECRETS_PROVIDER`) |
| `TIGERBEETLE_ADDRESSES` | `3000` | TigerBeetle address(es) |
| `TIGERBEETLE_CLUSTER_ID` | `0` | TigerBeetle cluster ID |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `3030` | HTTP port |
| `RUST_LOG` | `pba_service=debug,tower_http=info` | Log level (`tower_http=info` enables per-request access logs) |
| `OIDC_ISSUER_URL` | `http://localhost:8180/realms/pba` | OIDC provider issuer URL (only used when `AUTH_ENABLED=true`) |
| `OIDC_CLIENT_ID` | `pba-admin` | OIDC client ID for admin UI login flow (only used when `AUTH_ENABLED=true`) |
| `COOKIE_SECRET` | _(dev default)_ | 32+ byte secret for session cookie signing |
| `AUTH_ENABLED` | `true` (code) / `false` (dev `.env`) | Set to `false` to disable auth. `.env.example` ships with `false` for local dev |
| `PATH_PREFIX` | _(empty)_ | URL prefix for reverse proxy / ingress (e.g., `/pba`) |

## Available just targets

Run `just` to see all targets:

```
just run              # Start everything via process-compose (Ctrl+C stops all)
just run-bg           # Start everything in the background (detached)
just logs             # Attach to running process-compose instance
just stop             # Stop all services
just build            # Build the project
just test             # Unit tests
just api-e2e          # API E2E tests (isolated infra)
just ui-e2e           # Browser UI E2E tests (headless Chrome)
just ui-e2e-watch     # Browser UI tests with visible Chrome
just e2e-all          # All E2E tests (API + browser)
just local-ci         # Format + lint + build + test
just migrate          # Run database migrations
just smithy-build     # Regenerate Smithy SDK
```
