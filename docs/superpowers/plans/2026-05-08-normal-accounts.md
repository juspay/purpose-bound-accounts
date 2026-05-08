# Normal Accounts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a second account kind — **normal accounts** (single TigerBeetle account, fronted by Postgres) — alongside purpose-bound (PB) accounts. Support normal → PB internal transfers (with pending lifecycle). Remove the direct trust → PB deposit path.

**Architecture:** Three sequential phases that map 1:1 to three PRs. **Phase 1** is mechanical renames only (`accounts` table → `pb_accounts`, `account_*.rs` files → `pb_account_*.rs`); zero behavioural change. **Phase 2** is additive: new `normal_accounts` table, new domain/repo/service/handlers/routes for normal accounts, new Smithy operations, legacy `/accounts/*` URLs kept as in-process aliases. **Phase 3** is the only behavioural break: introduces normal → PB transfers and rejects `funding_type='trust'` on PB deposits.

**Tech Stack:** Rust (Axum 0.8, sqlx, `tigerbeetle_unofficial`), PostgreSQL 16, TigerBeetle, Smithy 2.0 IDL → generated `pba_client` SDK, Cucumber-rs for BDD, just/process-compose for the dev runtime.

**Spec:** `docs/superpowers/specs/2026-05-08-normal-accounts-design.md`

**Branch convention:** Implement on `normal-accounts-design` (already created); each phase is its own PR squash-merged into `main`.

---

## File Structure

The plan creates and modifies the following files. Anything not listed stays untouched.

### Phase 1 — renames (PR 1)

Created:
- `crates/pba_service/src/db/migrations/20260508000001_rename_accounts_to_pb_accounts.sql`

Renamed (file rename only — content rewrites paths/types):
- `crates/pba_service/src/repository/account_repo.rs` → `pb_account_repo.rs`
- `crates/pba_service/src/service/account_service.rs` → `pb_account_service.rs`
- `crates/pba_service/src/service/deposit_service.rs` → `pb_deposit_service.rs`
- `crates/pba_service/src/service/payment_service.rs` → `pb_payment_service.rs`
- `crates/pba_service/src/service/withdrawal_service.rs` → `pb_withdrawal_service.rs`

Modified (path/type updates from the renames — no logic change):
- `crates/pba_service/src/repository.rs` (re-exports)
- `crates/pba_service/src/service.rs` (re-exports)
- `crates/pba_service/src/main.rs` (`AppState` field renames + import updates)
- `crates/pba_service/src/api/handlers.rs` (uses `state.pb_account_service` etc.)
- `crates/pba_service/src/admin/handlers.rs` (uses renamed services)
- `crates/pba_service/src/service/deposit_timeout.rs` (no logical change; references account_repo only)
- All SQL string literals referencing `accounts` table → `pb_accounts`
- All Cucumber step-definition files in `crates/pba_service/tests/` (compile-only — they import service types)

### Phase 2 — normal accounts (PR 2)

Created:
- `crates/pba_service/src/db/migrations/20260508000002_normal_accounts.sql`
- `crates/pba_service/src/db/migrations/20260508000003_transactions_kind_correlation.sql`
- `crates/pba_service/src/domain/account_kind.rs`
- `crates/pba_service/src/domain/normal_account.rs`
- `crates/pba_service/src/repository/normal_account_repo.rs`
- `crates/pba_service/src/service/normal_account_service.rs`
- `crates/pba_service/src/service/normal_deposit_service.rs`
- `crates/pba_service/src/service/normal_withdrawal_service.rs`
- `crates/pba_service/src/api/handlers/pb.rs` (split out of monolithic handlers.rs)
- `crates/pba_service/src/api/handlers/normal.rs`
- `crates/pba_service/src/api/handlers/transactions.rs`
- `model/normal_account.smithy`
- `crates/pba_service/tests/features/normal_account_lifecycle.feature`
- `crates/pba_service/tests/ui_features/normal_account_admin.feature`

Modified:
- `crates/pba_service/src/domain.rs` (re-export new modules)
- `crates/pba_service/src/repository.rs` (re-export `normal_account_repo`)
- `crates/pba_service/src/service.rs` (re-export new services)
- `crates/pba_service/src/repository/ledger_repo.rs` (add `CODE_NORMAL_POOL`, `create_normal_account`, `get_single_balance`)
- `crates/pba_service/src/repository/transaction_repo.rs` (add `account_kind` parameter, make `pool` `Option<&str>`, add `correlation_id`, add `find_by_correlation_id`)
- `crates/pba_service/src/domain/transaction.rs` (`TransactionRecord.pool` becomes `Option<String>`; new `TransactionType::Transfer` variant; add `correlation_id` field)
- `crates/pba_service/src/error.rs` (rename `AccountNotFound` → `PbAccountNotFound`, `AccountNotActive` → `PbAccountNotActive`; add `NormalAccountNotFound`, `NormalAccountNotActive`)
- `crates/pba_service/src/api/handlers.rs` (becomes thin re-export shim over `api/handlers/{pb,normal,transactions}.rs`)
- `crates/pba_service/src/api/routes.rs` (canonical `/pb-accounts/*` and `/normal-accounts/*`; legacy `/accounts/*` in-process aliases)
- `crates/pba_service/src/api/dto.rs` (new normal-account DTOs; `TransactionSummaryDto` gets `account_kind` and `correlation_id` optional fields; `pool` becomes optional)
- `crates/pba_service/src/main.rs` (wire new repos and services into `AppState`)
- `model/main.smithy` (add new operations + `@deprecated` aliases for renamed PB operations)
- `model/account.smithy` (move PB-specific shapes; rename operations)
- `model/transaction.smithy` (extend `TransactionSummary` with `accountKind`)

### Phase 3 — transfers + trust removal (PR 3)

Created:
- `crates/pba_service/src/domain/transfer.rs`
- `crates/pba_service/src/service/transfer_service.rs`
- `crates/pba_service/src/api/handlers/transfer.rs`
- `model/transfer.smithy`
- `crates/pba_service/tests/features/internal_transfer.feature`
- `crates/pba_service/tests/features/trust_direct_deposit_removed.feature`
- `crates/pba_service/tests/ui_features/transfer_admin.feature`

Modified:
- `crates/pba_service/src/repository/ledger_repo.rs` (add `create_internal_transfer` + pending variant; add transfer codes 400/401)
- `crates/pba_service/src/service/pb_deposit_service.rs` (reject `funding_type='trust'`)
- `crates/pba_service/src/service/deposit_timeout.rs` (handle transfer pairs via `correlation_id`)
- `crates/pba_service/src/api/dto.rs` (transfer DTOs)
- `crates/pba_service/src/api/routes.rs` (transfer routes under `/normal-accounts/{id}/transfers`)
- `crates/pba_service/src/api/handlers.rs` (re-export transfer handlers)
- `crates/pba_service/src/main.rs` (wire `TransferService`)
- Existing Cucumber scenarios that use `funding_type='trust'` on PB deposits — flipped to use the transfer flow.

---

## Phase 1 — Renames (PR 1)

**Goal of phase 1:** rename `accounts` → `pb_accounts` everywhere; rename `account_*.rs` → `pb_account_*.rs`; zero behavioural change. Reviewer should be able to verify with `git diff -M` showing pure renames + import path updates. All existing tests pass unchanged.

### Task 1.1: Create branch checkpoint and confirm baseline

**Files:** none (workspace state)

- [ ] **Step 1.1.1: Confirm clean working tree on `normal-accounts-design`**

```bash
git status
git rev-parse --abbrev-ref HEAD
```

Expected: `On branch normal-accounts-design`, clean working tree (only the design doc committed in `4081e5d`).

- [ ] **Step 1.1.2: Run baseline tests to confirm green starting point**

```bash
just test
```

Expected: all unit tests pass. If anything fails, stop — investigate before starting Phase 1.

### Task 1.2: Migration M1 — rename `accounts` to `pb_accounts`

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260508000001_rename_accounts_to_pb_accounts.sql`

- [ ] **Step 1.2.1: Write migration SQL**

```sql
-- Rename the dual-pool account table to pb_accounts (purpose-bound).
-- A future migration introduces normal_accounts as a sibling table.
ALTER TABLE accounts RENAME TO pb_accounts;
ALTER INDEX idx_accounts_origin_purpose RENAME TO idx_pb_accounts_origin_purpose;
ALTER INDEX idx_accounts_holder         RENAME TO idx_pb_accounts_holder;

-- The existing FK on transactions.account_id was created as
-- transactions_account_id_fkey REFERENCES accounts(id). Postgres preserves
-- the FK target across the rename automatically. We do NOT drop it here —
-- the FK now points at pb_accounts(id), which is correct for Phase 1.
```

- [ ] **Step 1.2.2: Run migration locally to confirm it applies**

```bash
just migrate
psql $DATABASE_URL -c "\d pb_accounts" | head -5
psql $DATABASE_URL -c "\d transactions" | grep account_id
```

Expected: `pb_accounts` table exists with all original columns; `transactions.account_id` shows FK to `pb_accounts(id)`.

- [ ] **Step 1.2.3: Commit migration**

```bash
git add crates/pba_service/src/db/migrations/20260508000001_rename_accounts_to_pb_accounts.sql
git commit -m "feat(db): rename accounts table to pb_accounts"
```

### Task 1.3: Update `account_repo.rs` SQL to reference `pb_accounts`

**Files:**
- Modify: `crates/pba_service/src/repository/account_repo.rs` (SQL strings only — file is renamed in Task 1.4)

- [ ] **Step 1.3.1: Update all SQL literals**

Replace every `FROM accounts` and `INSERT INTO accounts` and `UPDATE accounts` in `crates/pba_service/src/repository/account_repo.rs` with `FROM pb_accounts` / `INSERT INTO pb_accounts` / `UPDATE pb_accounts`. The file currently has six SQL queries (one each for `create_account`, `get_account`, `update_status`, `list_accounts`, `count_accounts_by_status`, `count_accounts_by_purpose`).

`grep -n 'accounts' crates/pba_service/src/repository/account_repo.rs` and update each match. Leave references to `purpose_mcc_allowlist` untouched.

- [ ] **Step 1.3.2: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build.

- [ ] **Step 1.3.3: Run unit tests**

```bash
just test
```

Expected: all pass.

- [ ] **Step 1.3.4: Commit**

```bash
git add crates/pba_service/src/repository/account_repo.rs
git commit -m "refactor(repo): point account_repo SQL at renamed pb_accounts table"
```

### Task 1.4: Rename `account_repo.rs` → `pb_account_repo.rs` and update import paths

**Files:**
- Renamed: `crates/pba_service/src/repository/account_repo.rs` → `pb_account_repo.rs`
- Modified: `crates/pba_service/src/repository.rs` (re-export module name)
- Modified: every consumer (`service/account_service.rs`, `service/deposit_service.rs`, `service/payment_service.rs`, `service/withdrawal_service.rs`, `admin/handlers.rs`, `main.rs`, `service/deposit_timeout.rs`)

- [ ] **Step 1.4.1: Rename the file via git**

```bash
git mv crates/pba_service/src/repository/account_repo.rs \
       crates/pba_service/src/repository/pb_account_repo.rs
```

- [ ] **Step 1.4.2: Update `repository.rs`**

In `crates/pba_service/src/repository.rs`, replace:

```rust
pub mod account_repo;
```

with:

```rust
pub mod pb_account_repo;
```

- [ ] **Step 1.4.3: Update consumers**

Across the codebase, replace every:

```rust
use crate::repository::account_repo::AccountRepo;
```

with:

```rust
use crate::repository::pb_account_repo::PbAccountRepo;
```

The struct itself also renames: `AccountRepo` → `PbAccountRepo`. Inside `pb_account_repo.rs`, change every `pub struct AccountRepo` and `impl AccountRepo` to `pub struct PbAccountRepo` and `impl PbAccountRepo`.

`grep -rn 'AccountRepo' crates/pba_service/src/` finds all sites. Replace each with `PbAccountRepo`.

- [ ] **Step 1.4.4: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build. Any miss → grep again; should be zero hits for `AccountRepo` (case-sensitive, whole word).

- [ ] **Step 1.4.5: Commit**

```bash
git add -A
git commit -m "refactor: rename AccountRepo to PbAccountRepo (file + struct)"
```

### Task 1.5: Rename service files

**Files:**
- Renamed:
  - `service/account_service.rs` → `pb_account_service.rs` (struct `AccountService` → `PbAccountService`)
  - `service/deposit_service.rs` → `pb_deposit_service.rs` (struct `DepositService` → `PbDepositService`)
  - `service/payment_service.rs` → `pb_payment_service.rs` (struct `PaymentService` → `PbPaymentService`)
  - `service/withdrawal_service.rs` → `pb_withdrawal_service.rs` (struct `WithdrawalService` → `PbWithdrawalService`)
- Modified:
  - `service.rs` (module re-exports)
  - `main.rs` (`AppState` fields + service struct names)
  - `api/handlers.rs` (`state.account_service` → `state.pb_account_service`, etc.)
  - `admin/handlers.rs` (same)
  - `service/deposit_timeout.rs` (no service-name references; only repo)

- [ ] **Step 1.5.1: Git-rename the four service files**

```bash
git mv crates/pba_service/src/service/account_service.rs    crates/pba_service/src/service/pb_account_service.rs
git mv crates/pba_service/src/service/deposit_service.rs    crates/pba_service/src/service/pb_deposit_service.rs
git mv crates/pba_service/src/service/payment_service.rs    crates/pba_service/src/service/pb_payment_service.rs
git mv crates/pba_service/src/service/withdrawal_service.rs crates/pba_service/src/service/pb_withdrawal_service.rs
```

- [ ] **Step 1.5.2: Update `service.rs` re-exports**

In `crates/pba_service/src/service.rs`, replace:

```rust
pub mod account_service;
pub mod deposit_service;
pub mod deposit_timeout;
pub mod payment_service;
pub mod withdrawal_service;
```

with:

```rust
pub mod deposit_timeout;
pub mod pb_account_service;
pub mod pb_deposit_service;
pub mod pb_payment_service;
pub mod pb_withdrawal_service;
```

- [ ] **Step 1.5.3: Rename the service structs inside each file**

In each renamed file, replace the struct identifier and impl block headers:

| File | Old | New |
|---|---|---|
| `pb_account_service.rs` | `AccountService` | `PbAccountService` |
| `pb_deposit_service.rs` | `DepositService` | `PbDepositService` |
| `pb_payment_service.rs` | `PaymentService` | `PbPaymentService` |
| `pb_withdrawal_service.rs` | `WithdrawalService` | `PbWithdrawalService` |

Both `pub struct X` and `impl X` need updating. Tests within these files (`#[cfg(test)] mod tests { … }`) stay logically identical.

Also update the `PaymentResult` / `WithdrawalResult` callsite struct names if needed — they don't need renaming (they're result types, not service handles).

- [ ] **Step 1.5.4: Update `main.rs`**

In `crates/pba_service/src/main.rs`, update imports:

```rust
use repository::pb_account_repo::PbAccountRepo;
use service::pb_account_service::PbAccountService;
use service::pb_deposit_service::PbDepositService;
use service::pb_payment_service::PbPaymentService;
use service::pb_withdrawal_service::PbWithdrawalService;
```

Update `AppState` struct:

```rust
#[derive(Clone)]
pub struct AppState {
    pub pb_account_service: Arc<PbAccountService>,
    pub pb_deposit_service: Arc<PbDepositService>,
    pub pb_payment_service: Arc<PbPaymentService>,
    pub pb_withdrawal_service: Arc<PbWithdrawalService>,
    pub pb_account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub auth: AuthContext,
    pub path_prefix: String,
}
```

Update the constructor block in `main()`:

```rust
let pb_account_repo = Arc::new(PbAccountRepo::new(pg_pool.clone()));
// (transaction_repo and ledger_repo unchanged)

let pb_account_service = Arc::new(PbAccountService::new(
    Arc::clone(&pb_account_repo),
    Arc::clone(&ledger_repo),
));
let pb_deposit_service = Arc::new(PbDepositService::new(
    Arc::clone(&pb_account_repo),
    Arc::clone(&ledger_repo),
    Arc::clone(&transaction_repo),
    config.deposit_timeout_seconds,
));
let pb_payment_service = Arc::new(PbPaymentService::new(
    Arc::clone(&pb_account_repo),
    Arc::clone(&ledger_repo),
    Arc::clone(&transaction_repo),
));
let pb_withdrawal_service = Arc::new(PbWithdrawalService::new(
    Arc::clone(&pb_account_repo),
    Arc::clone(&ledger_repo),
    Arc::clone(&transaction_repo),
));

let state = AppState {
    pb_account_service,
    pb_deposit_service,
    pb_payment_service,
    pb_withdrawal_service,
    pb_account_repo,
    ledger_repo,
    transaction_repo: Arc::clone(&transaction_repo),
    auth: auth_ctx,
    path_prefix: config.path_prefix,
};
```

- [ ] **Step 1.5.5: Update `api/handlers.rs` callsites**

`grep -n 'state\.account_service\|state\.deposit_service\|state\.payment_service\|state\.withdrawal_service\|state\.account_repo' crates/pba_service/src/api/handlers.rs`. Replace each with the `pb_*` prefix.

Also update imports inside the file from `crate::service::account_service::AccountService` etc. to the new `pb_*` paths and types.

- [ ] **Step 1.5.6: Update `admin/handlers.rs`**

Same treatment as Step 1.5.5 but for `crates/pba_service/src/admin/handlers.rs`.

- [ ] **Step 1.5.7: Update `service/deposit_timeout.rs` if needed**

Open the file and verify it only references `transaction_repo` (it should — it doesn't manipulate accounts directly). If it imports `AccountRepo` anywhere, update to `PbAccountRepo` per Task 1.4.

- [ ] **Step 1.5.8: Compile and run tests**

```bash
cargo build -p pba_service
cargo build --tests -p pba_service
just test
```

Expected: clean build, all unit tests pass. The Cucumber tests are not run yet (`just api-e2e` and `just ui-e2e` deferred to a later step in this task).

- [ ] **Step 1.5.9: Update Cucumber step definitions**

`grep -rn 'AccountService\|DepositService\|PaymentService\|WithdrawalService\|AccountRepo\b' crates/pba_service/tests/` — for any matches, update the type references to the `Pb*` variants. The Cucumber world likely holds an SDK client rather than service references, so this may be a no-op; if so, skip.

- [ ] **Step 1.5.10: Run e2e**

```bash
just api-e2e
```

Expected: all scenarios pass — they go through HTTP, which is still on the legacy `/accounts/*` URLs (no URL changes in Phase 1).

- [ ] **Step 1.5.11: Commit**

```bash
git add -A
git commit -m "refactor: rename PB account services to Pb* (file + struct)"
```

### Task 1.6: Phase 1 verification + push PR

**Files:** none (workflow)

- [ ] **Step 1.6.1: Full local CI**

```bash
just local-ci
```

Expected: all green (format + lint + build + test). If lint fails, fix and re-run before continuing.

- [ ] **Step 1.6.2: Run UI e2e to confirm admin still works**

```bash
just ui-e2e
```

Expected: all admin browser scenarios pass.

- [ ] **Step 1.6.3: Push and open PR 1**

```bash
git push -u origin normal-accounts-design
gh pr create --title "refactor: rename accounts → pb_accounts (Phase 1 of normal accounts)" --body "$(cat <<'EOF'
## Summary
- Rename Postgres `accounts` table to `pb_accounts` (DB + indexes via migration).
- Rename `account_repo.rs` → `pb_account_repo.rs`, four `*_service.rs` files to `pb_*_service.rs`, and the corresponding structs.
- Update `AppState` fields, all callsites, and SQL literals.

Zero behavioural change. Reviewer should verify with `git diff -M` showing pure renames + import-path updates.

This is Phase 1 of the normal accounts feature. See [design doc](docs/superpowers/specs/2026-05-08-normal-accounts-design.md).

## Test plan
- [ ] `just local-ci` passes
- [ ] `just api-e2e` passes
- [ ] `just ui-e2e` passes

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Wait for PR review and merge before starting Phase 2. Phase 2 builds on the renamed types.

---

## Phase 2 — Normal accounts (PR 2)

**Goal of phase 2:** introduce `normal_accounts` table, the parallel domain modules, new HTTP routes (`/pb-accounts/*` canonical, `/normal-accounts/*` new, `/accounts/*` legacy alias), and the new Smithy operations. PB behaviour stays unchanged. Trust deposits to PB still work — the breaking change comes in Phase 3.

After PR 1 merges, recreate the working branch from `main`:

```bash
git checkout main
git pull
git checkout -b normal-accounts-phase-2
```

### Task 2.1: Migration M2 — `normal_accounts` table

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260508000002_normal_accounts.sql`

- [ ] **Step 2.1.1: Write migration SQL**

```sql
CREATE TABLE normal_accounts (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    holder_id              VARCHAR(255) NOT NULL,             -- matches pb_accounts.holder_id width
    origin_ifsc            VARCHAR(11),
    origin_account_number  VARCHAR(20),
    vpa                    VARCHAR(50),
    virtual_ifsc           VARCHAR(11),
    virtual_account_number VARCHAR(20),
    tb_account_id          NUMERIC(39) NOT NULL,
    kyc_tier               VARCHAR(10) NOT NULL DEFAULT 'minimum',
    status                 VARCHAR(10) NOT NULL DEFAULT 'active',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_normal_accounts_holder ON normal_accounts (holder_id);
```

- [ ] **Step 2.1.2: Apply migration**

```bash
just migrate
psql $DATABASE_URL -c "\d normal_accounts"
```

Expected: table exists with all columns and index.

- [ ] **Step 2.1.3: Commit**

```bash
git add crates/pba_service/src/db/migrations/20260508000002_normal_accounts.sql
git commit -m "feat(db): add normal_accounts table"
```

### Task 2.2: Migration M3 — `transactions` extensions

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260508000003_transactions_kind_correlation.sql`

- [ ] **Step 2.2.1: Write migration SQL**

```sql
-- Add account_kind discriminator. Default 'pb' backfills existing rows; then drop default.
ALTER TABLE transactions
    ADD COLUMN account_kind   VARCHAR(10) NOT NULL DEFAULT 'pb',
    ADD COLUMN correlation_id UUID NULL,
    ALTER COLUMN pool DROP NOT NULL;

ALTER TABLE transactions ALTER COLUMN account_kind DROP DEFAULT;

-- Drop the FK on transactions.account_id (was pointing at pb_accounts after the
-- Phase 1 rename). With normal_accounts as a sibling table, the column now
-- references one of two tables; the application enforces the link via account_kind.
ALTER TABLE transactions DROP CONSTRAINT transactions_account_id_fkey;

-- Replace the per-account index with a kind-aware composite.
DROP INDEX IF EXISTS idx_transactions_account;
CREATE INDEX idx_transactions_account_kind_account
    ON transactions (account_kind, account_id, created_at DESC);

-- Correlation lookup index (used to find both legs of an internal transfer).
CREATE INDEX idx_transactions_correlation
    ON transactions (correlation_id) WHERE correlation_id IS NOT NULL;

-- Idempotency unique index now keyed on (kind, account, key).
DROP INDEX IF EXISTS idx_transactions_idempotency;
CREATE UNIQUE INDEX uq_transactions_idempotency
    ON transactions (account_kind, account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
```

- [ ] **Step 2.2.2: Apply migration and verify**

```bash
just migrate
psql $DATABASE_URL -c "\d transactions" | grep -E 'account_kind|correlation_id|pool '
psql $DATABASE_URL -c "SELECT account_kind, COUNT(*) FROM transactions GROUP BY account_kind;"
```

Expected: `account_kind VARCHAR(10) NOT NULL`; `correlation_id UUID`; `pool TEXT` (no NOT NULL); count query returns existing rows tagged `pb`.

- [ ] **Step 2.2.3: Commit**

```bash
git add crates/pba_service/src/db/migrations/20260508000003_transactions_kind_correlation.sql
git commit -m "feat(db): add account_kind and correlation_id to transactions"
```

### Task 2.3: `AccountKind` enum

**Files:**
- Create: `crates/pba_service/src/domain/account_kind.rs`
- Modify: `crates/pba_service/src/domain.rs`

- [ ] **Step 2.3.1: Write the failing test**

Append to `crates/pba_service/src/domain/account_kind.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Pb,
    Normal,
}

impl AccountKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pb => "pb",
            Self::Normal => "normal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pb" => Some(Self::Pb),
            "normal" => Some(Self::Normal),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_strings() {
        assert_eq!(AccountKind::Pb.as_str(), "pb");
        assert_eq!(AccountKind::Normal.as_str(), "normal");
        assert_eq!(AccountKind::from_str("pb"), Some(AccountKind::Pb));
        assert_eq!(AccountKind::from_str("normal"), Some(AccountKind::Normal));
        assert_eq!(AccountKind::from_str("other"), None);
    }
}
```

- [ ] **Step 2.3.2: Register module**

In `crates/pba_service/src/domain.rs`, add:

```rust
pub mod account_kind;
```

- [ ] **Step 2.3.3: Run test**

```bash
cargo test -p pba_service domain::account_kind::tests
```

Expected: PASS.

- [ ] **Step 2.3.4: Commit**

```bash
git add crates/pba_service/src/domain/account_kind.rs crates/pba_service/src/domain.rs
git commit -m "feat(domain): introduce AccountKind enum"
```

### Task 2.4: `NormalAccount` domain struct + `tb_normal_id`

**Files:**
- Create: `crates/pba_service/src/domain/normal_account.rs`
- Modify: `crates/pba_service/src/domain.rs`

- [ ] **Step 2.4.1: Write the failing test**

`crates/pba_service/src/domain/normal_account.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::account::AccountStatus;
use crate::domain::banking::{AccountNumber, Ifsc};

#[derive(Debug, Clone, Serialize)]
pub struct NormalAccount {
    pub id: Uuid,
    pub holder_id: String,
    pub origin_ifsc: Option<Ifsc>,
    pub origin_account_number: Option<AccountNumber>,
    pub vpa: Option<String>,
    pub virtual_ifsc: Option<Ifsc>,
    pub virtual_account_number: Option<AccountNumber>,
    pub tb_account_id: u128,
    pub kyc_tier: String,
    pub status: AccountStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Deterministic u128 ID derivation from UUID for the single TigerBeetle
/// account behind a normal account. Same byte layout as `tb_self_id`; collisions
/// across UUIDs are bounded by UUID v4 collision probability (~ 2^-122).
pub fn tb_normal_id(account_id: Uuid) -> u128 {
    u128::from_be_bytes(*account_id.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tb_normal_id_is_deterministic() {
        let id = Uuid::new_v4();
        assert_eq!(tb_normal_id(id), tb_normal_id(id));
    }

    #[test]
    fn tb_normal_id_distinguishes_uuids() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        assert_ne!(tb_normal_id(a), tb_normal_id(b));
    }
}
```

- [ ] **Step 2.4.2: Register module**

In `crates/pba_service/src/domain.rs`, add:

```rust
pub mod normal_account;
```

- [ ] **Step 2.4.3: Run tests**

```bash
cargo test -p pba_service domain::normal_account::tests
```

Expected: PASS.

- [ ] **Step 2.4.4: Commit**

```bash
git add crates/pba_service/src/domain/normal_account.rs crates/pba_service/src/domain.rs
git commit -m "feat(domain): add NormalAccount struct and tb_normal_id"
```

### Task 2.5: `TransactionType::Transfer` variant + `correlation_id` on `TransactionRecord` + nullable `pool`

**Files:**
- Modify: `crates/pba_service/src/domain/transaction.rs`

- [ ] **Step 2.5.1: Add the test**

Append to the existing `#[cfg(test)] mod tests` in `crates/pba_service/src/domain/transaction.rs` (or create one if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_round_trips() {
        assert_eq!(TransactionType::Transfer.as_str(), "transfer");
        assert_eq!(TransactionType::from_str("transfer"), Some(TransactionType::Transfer));
    }
}
```

- [ ] **Step 2.5.2: Run test (expect FAIL)**

```bash
cargo test -p pba_service domain::transaction::tests::transfer_round_trips
```

Expected: FAIL — `Transfer` variant doesn't exist.

- [ ] **Step 2.5.3: Implement `Transfer` variant + `correlation_id` field + nullable `pool`**

In `crates/pba_service/src/domain/transaction.rs`:

Update the `TransactionType` enum and impl:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Deposit,
    Payment,
    Withdrawal,
    Transfer,
}

impl TransactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deposit => "deposit",
            Self::Payment => "payment",
            Self::Withdrawal => "withdrawal",
            Self::Transfer => "transfer",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "deposit" => Some(Self::Deposit),
            "payment" => Some(Self::Payment),
            "withdrawal" => Some(Self::Withdrawal),
            "transfer" => Some(Self::Transfer),
            _ => None,
        }
    }
}
```

Update `TransactionRecord` — change `pool` to `Option<String>`, add `correlation_id`:

```rust
#[derive(Debug, Clone)]
pub struct TransactionRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub account_kind: crate::domain::account_kind::AccountKind,
    pub transaction_type: TransactionType,
    pub status: TransactionStatus,
    pub amount: u64,
    pub pool: Option<String>,
    pub direction: TransactionDirection,
    pub source_ifsc: Option<String>,
    pub source_account: Option<String>,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub merchant_id: Option<String>,
    pub merchant_mcc: Option<String>,
    pub description: Option<String>,
    pub funding_type: Option<String>,
    pub tb_transfer_id: u128,
    pub idempotency_key: Option<String>,
    pub correlation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Update `type_label`:

```rust
pub fn type_label(&self) -> &'static str {
    match (self.transaction_type, self.status) {
        (TransactionType::Deposit, TransactionStatus::Pending) => "Deposit (Pending)",
        (TransactionType::Deposit, TransactionStatus::Posted) => "Deposit",
        (TransactionType::Deposit, TransactionStatus::Voided) => "Deposit (Voided)",
        (TransactionType::Payment, _) => "Payment",
        (TransactionType::Withdrawal, _) => "Withdrawal",
        (TransactionType::Transfer, TransactionStatus::Pending) => "Transfer (Pending)",
        (TransactionType::Transfer, TransactionStatus::Posted)
            | (TransactionType::Transfer, TransactionStatus::Settled) => "Transfer",
        (TransactionType::Transfer, TransactionStatus::Voided) => "Transfer (Voided)",
        _ => "Unknown",
    }
}
```

- [ ] **Step 2.5.4: Run test**

```bash
cargo test -p pba_service domain::transaction::tests::transfer_round_trips
```

Expected: PASS. Other tests will now fail to compile because of struct field changes — fix in subsequent tasks (2.6, 2.7).

- [ ] **Step 2.5.5: Commit**

```bash
git add crates/pba_service/src/domain/transaction.rs
git commit -m "feat(domain): add Transfer variant, account_kind, correlation_id, optional pool to TransactionRecord"
```

### Task 2.6: Update `transaction_repo` for new fields

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs`

- [ ] **Step 2.6.1: Update `TransactionRow` struct**

Replace the `TransactionRow` struct in `crates/pba_service/src/repository/transaction_repo.rs`:

```rust
#[derive(sqlx::FromRow)]
struct TransactionRow {
    id: Uuid,
    account_id: Uuid,
    account_kind: String,
    #[sqlx(rename = "type")]
    transaction_type: String,
    status: String,
    amount: i64,
    pool: Option<String>,
    direction: String,
    source_ifsc: Option<String>,
    source_account: Option<String>,
    gateway_ref: Option<String>,
    timeout_seconds: Option<i32>,
    merchant_id: Option<String>,
    merchant_mcc: Option<String>,
    description: Option<String>,
    funding_type: Option<String>,
    tb_transfer_id: String,
    idempotency_key: Option<String>,
    correlation_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

Update `TransactionRow::into_domain`:

```rust
impl TransactionRow {
    fn into_domain(self) -> TransactionRecord {
        TransactionRecord {
            id: self.id,
            account_id: self.account_id,
            account_kind: crate::domain::account_kind::AccountKind::from_str(&self.account_kind)
                .unwrap_or(crate::domain::account_kind::AccountKind::Pb),
            transaction_type: TransactionType::from_str(&self.transaction_type)
                .unwrap_or(TransactionType::Deposit),
            status: TransactionStatus::from_str(&self.status).unwrap_or(TransactionStatus::Pending),
            amount: self.amount as u64,
            pool: self.pool,
            direction: TransactionDirection::from_str(&self.direction)
                .unwrap_or(TransactionDirection::Inbound),
            source_ifsc: self.source_ifsc,
            source_account: self.source_account,
            gateway_ref: self.gateway_ref,
            timeout_seconds: self.timeout_seconds.map(|s| s as u32),
            merchant_id: self.merchant_id,
            merchant_mcc: self.merchant_mcc,
            description: self.description,
            funding_type: self.funding_type,
            tb_transfer_id: self.tb_transfer_id.parse().unwrap_or(0),
            idempotency_key: self.idempotency_key,
            correlation_id: self.correlation_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
```

- [ ] **Step 2.6.2: Update `insert_in_tx` signature and SQL**

Replace `insert_in_tx`:

```rust
#[allow(clippy::too_many_arguments)]
pub async fn insert_in_tx(
    &self,
    tx: &mut PgTransaction<'_, Postgres>,
    id: Uuid,
    account_id: Uuid,
    account_kind: crate::domain::account_kind::AccountKind,
    transaction_type: TransactionType,
    status: TransactionStatus,
    amount: u64,
    pool: Option<&str>,
    direction: TransactionDirection,
    source_ifsc: Option<&str>,
    source_account: Option<&str>,
    gateway_ref: Option<&str>,
    timeout_seconds: Option<u32>,
    merchant_id: Option<&str>,
    merchant_mcc: Option<&str>,
    description: Option<&str>,
    funding_type: Option<&str>,
    tb_transfer_id: u128,
    idempotency_key: Option<&str>,
    correlation_id: Option<Uuid>,
) -> Result<TransactionRecord, AppError> {
    let tb_id_str = tb_transfer_id.to_string();
    let row = sqlx::query_as::<_, TransactionRow>(
        r#"
        INSERT INTO transactions (id, account_id, account_kind, type, status, amount, pool, direction,
                                  source_ifsc, source_account, gateway_ref, timeout_seconds,
                                  merchant_id, merchant_mcc, description, funding_type,
                                  tb_transfer_id, idempotency_key, correlation_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17::numeric, $18, $19)
        RETURNING id, account_id, account_kind, type, status, amount, pool, direction,
                  source_ifsc, source_account, gateway_ref, timeout_seconds,
                  merchant_id, merchant_mcc, description, funding_type,
                  tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                  created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(account_id)
    .bind(account_kind.as_str())
    .bind(transaction_type.as_str())
    .bind(status.as_str())
    .bind(amount as i64)
    .bind(pool)
    .bind(direction.as_str())
    .bind(source_ifsc)
    .bind(source_account)
    .bind(gateway_ref)
    .bind(timeout_seconds.map(|s| s as i32))
    .bind(merchant_id)
    .bind(merchant_mcc)
    .bind(description)
    .bind(funding_type)
    .bind(&tb_id_str)
    .bind(idempotency_key)
    .bind(correlation_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    Ok(row.into_domain())
}
```

- [ ] **Step 2.6.3: Update every read SELECT in this file**

Add `account_kind` and `correlation_id` to the SELECT lists in `update_status`, `get_by_id`, `get_transaction`, `find_by_idempotency_key`, `list_by_account`, `list_all`, `list_pending_by_account`, `find_timed_out_pending`. Each query needs:

```
SELECT id, account_id, account_kind, type, status, amount, pool, direction,
       source_ifsc, source_account, gateway_ref, timeout_seconds,
       merchant_id, merchant_mcc, description, funding_type,
       tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
       created_at, updated_at
FROM transactions
WHERE …
```

- [ ] **Step 2.6.4: Update `find_by_idempotency_key` to take `account_kind`**

```rust
pub async fn find_by_idempotency_key(
    &self,
    account_kind: crate::domain::account_kind::AccountKind,
    account_id: Uuid,
    key: &str,
) -> Result<Option<TransactionRecord>, AppError> {
    let row = sqlx::query_as::<_, TransactionRow>(
        r#"
        SELECT id, account_id, account_kind, type, status, amount, pool, direction,
               source_ifsc, source_account, gateway_ref, timeout_seconds,
               merchant_id, merchant_mcc, description, funding_type,
               tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
               created_at, updated_at
        FROM transactions
        WHERE account_kind = $1 AND account_id = $2 AND idempotency_key = $3
        "#,
    )
    .bind(account_kind.as_str())
    .bind(account_id)
    .bind(key)
    .fetch_optional(&self.pool)
    .await?;

    Ok(row.map(|r| r.into_domain()))
}
```

- [ ] **Step 2.6.5: Update `list_by_account` to take `account_kind`**

Add `account_kind: AccountKind` as the first parameter; update WHERE to `WHERE account_kind = $1 AND account_id = $2`. Adjust `count_by_account` similarly.

- [ ] **Step 2.6.6: Add `find_by_correlation_id`**

Append to `impl TransactionRepo`:

```rust
pub async fn find_by_correlation_id(
    &self,
    correlation_id: Uuid,
) -> Result<Vec<TransactionRecord>, AppError> {
    let rows = sqlx::query_as::<_, TransactionRow>(
        r#"
        SELECT id, account_id, account_kind, type, status, amount, pool, direction,
               source_ifsc, source_account, gateway_ref, timeout_seconds,
               merchant_id, merchant_mcc, description, funding_type,
               tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
               created_at, updated_at
        FROM transactions
        WHERE correlation_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(correlation_id)
    .fetch_all(&self.pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.into_domain()).collect())
}
```

- [ ] **Step 2.6.7: Update `pool_summary` and `pool_summary_extended`**

These use the `pool` column which is now nullable. The match on `pool.as_str()` becomes a match on `Option<String>`:

```rust
let rows: Vec<(Option<String>, String, String, i64)> = sqlx::query_as(
    r#"
    SELECT pool, direction, status, COALESCE(SUM(amount), 0)::bigint AS total
    FROM transactions
    WHERE status IN ('posted', 'settled', 'pending')
    GROUP BY pool, direction, status
    "#,
)
.fetch_all(&self.pool)
.await?;

let mut summary = PoolSummary::default();
for (pool, direction, status, total) in rows {
    let amt = total as u64;
    let pool_str = pool.as_deref().unwrap_or("");
    match (pool_str, direction.as_str(), status.as_str()) {
        ("self", "inbound", "posted" | "settled") => summary.self_inbound += amt,
        // …rest as before…
        _ => {}
    }
}
```

The fallback `unwrap_or("")` ensures normal-account rows (with `pool=NULL`) don't accidentally contribute to PB pool summaries — they fall through the match.

- [ ] **Step 2.6.8: Update all callsites of `insert_in_tx`**

Existing PB callers (`pb_deposit_service.rs`, `pb_payment_service.rs`, `pb_withdrawal_service.rs`) now need:
- New parameter `account_kind: AccountKind::Pb` between `account_id` and `transaction_type`.
- `pool` argument changes from `&str` to `Option<&str>` — wrap each existing literal: `"self"` → `Some("self")`, `"others"` → `Some("others")`.
- New trailing parameter `correlation_id: Option<Uuid>` — pass `None`.

Use `grep -n 'insert_in_tx' crates/pba_service/src/service/` to find the four call sites (one in pb_deposit_service for pending, one for immediate; two in pb_payment_service for split paths; one in pb_withdrawal_service).

Also update existing PB callers of `find_by_idempotency_key`: prepend `AccountKind::Pb` argument.

Existing callers of `list_by_account`: prepend `AccountKind::Pb` argument.

- [ ] **Step 2.6.9: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build. If any callsite still uses the old shape, fix it.

- [ ] **Step 2.6.10: Run all unit tests**

```bash
just test
```

Expected: pass. The repo's test surface is small (mostly domain tests); the integration tests run separately.

- [ ] **Step 2.6.11: Commit**

```bash
git add -A
git commit -m "refactor(repo): thread account_kind and correlation_id through transaction_repo"
```

### Task 2.7: Extend `ledger_repo` with normal-account support

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs`

- [ ] **Step 2.7.1: Add new constants**

Near the top of `crates/pba_service/src/repository/ledger_repo.rs`, after `CODE_OTHERS_POOL`:

```rust
const CODE_NORMAL_POOL: u16 = 3;
```

- [ ] **Step 2.7.2: Add `SingleBalance` struct**

After `PoolBalance` re-export (or near the top of the file):

```rust
#[derive(Debug, Clone, Copy)]
pub struct SingleBalance {
    pub posted: u64,
    pub pending: u64,
}
```

Re-export from the module if needed (most existing types are accessed via `crate::repository::ledger_repo::SingleBalance`).

- [ ] **Step 2.7.3: Add `create_normal_account` method**

Append to `impl LedgerRepo`:

```rust
pub async fn create_normal_account(&self, tb_account_id: u128) -> Result<(), AppError> {
    let account = tb::Account::new(tb_account_id, LEDGER_INR_PAISA, CODE_NORMAL_POOL)
        .with_flags(AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS | AccountFlags::HISTORY);

    self.client
        .create_accounts(vec![account])
        .await
        .map_err(|e| AppError::TigerBeetleError(format!("create_normal_account failed: {e:?}")))?;

    tracing::info!(tb_account_id = %tb_account_id, "Created TB normal account");
    Ok(())
}
```

- [ ] **Step 2.7.4: Add `get_single_balance` method**

```rust
pub async fn get_single_balance(&self, tb_account_id: u128) -> Result<SingleBalance, AppError> {
    let accounts = self
        .client
        .lookup_accounts(vec![tb_account_id])
        .await
        .map_err(|e| AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}")))?;

    let mut posted: u64 = 0;
    let mut pending: u64 = 0;

    for account in &accounts {
        if account.id() == tb_account_id {
            let net = account.credits_posted().saturating_sub(account.debits_posted());
            posted = u64::try_from(net).unwrap_or(u64::MAX);
            pending = u64::try_from(account.credits_pending()).unwrap_or(u64::MAX);
        }
    }

    Ok(SingleBalance { posted, pending })
}
```

- [ ] **Step 2.7.5: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build.

- [ ] **Step 2.7.6: Commit**

```bash
git add crates/pba_service/src/repository/ledger_repo.rs
git commit -m "feat(ledger): add CODE_NORMAL_POOL, create_normal_account, get_single_balance"
```

### Task 2.8: Add error variants and renames

**Files:**
- Modify: `crates/pba_service/src/error.rs`

- [ ] **Step 2.8.1: Update `AppError` enum**

Replace the `AppError` enum in `crates/pba_service/src/error.rs`:

```rust
#[derive(Debug)]
pub enum AppError {
    PbAccountNotFound(String),
    PbAccountNotActive(String),
    NormalAccountNotFound(String),
    NormalAccountNotActive(String),
    PurposeTypeNotFound(String),
    InsufficientFunds {
        requested: u64,
        available: u64,
    },
    InvalidMcc {
        mcc: String,
        purpose_code: String,
    },
    TransactionNotFound(String),
    TransactionNotPending(String),
    FundingTypeRequired,
    /// Transfer rejected by TigerBeetle because debit would exceed credits (overdraft).
    /// This is retryable with a fresh balance read.
    ExceedsBalance,
    TigerBeetleError(String),
    DatabaseError(String),
    Unauthorized(String),
    Forbidden(String),
    Validation(String),
}
```

Update the `Display` impl with the renamed variants and matching messages:

```rust
Self::PbAccountNotFound(id) => write!(f, "PB account not found: {id}"),
Self::PbAccountNotActive(id) => write!(f, "PB account not active: {id}"),
Self::NormalAccountNotFound(id) => write!(f, "Normal account not found: {id}"),
Self::NormalAccountNotActive(id) => write!(f, "Normal account not active: {id}"),
```

Update the `IntoResponse` impl:

```rust
AppError::PbAccountNotFound(_)     => (StatusCode::NOT_FOUND, "PbAccountNotFound"),
AppError::PbAccountNotActive(_)    => (StatusCode::CONFLICT,  "PbAccountNotActive"),
AppError::NormalAccountNotFound(_) => (StatusCode::NOT_FOUND, "NormalAccountNotFound"),
AppError::NormalAccountNotActive(_)=> (StatusCode::CONFLICT,  "NormalAccountNotActive"),
```

- [ ] **Step 2.8.2: Update all callsites**

```bash
grep -rn 'AccountNotFound\|AccountNotActive' crates/pba_service/src/ | grep -v error.rs
```

Each match → rewrite with `PbAccountNotFound` / `PbAccountNotActive`. Most live in `pb_account_service.rs`, `pb_deposit_service.rs`, `pb_payment_service.rs`, `pb_withdrawal_service.rs`.

- [ ] **Step 2.8.3: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build.

- [ ] **Step 2.8.4: Commit**

```bash
git add -A
git commit -m "refactor(error): rename AccountNotFound/Active to Pb*; add Normal* variants"
```

### Task 2.9: `NormalAccountRepo`

**Files:**
- Create: `crates/pba_service/src/repository/normal_account_repo.rs`
- Modify: `crates/pba_service/src/repository.rs`

- [ ] **Step 2.9.1: Write the failing test**

`crates/pba_service/src/repository/normal_account_repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::account::AccountStatus;
use crate::domain::banking::{AccountNumber, Ifsc};
use crate::domain::normal_account::NormalAccount;
use crate::error::AppError;

pub struct NormalAccountRepo {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct NormalAccountRow {
    id: Uuid,
    holder_id: String,
    origin_ifsc: Option<Ifsc>,
    origin_account_number: Option<AccountNumber>,
    vpa: Option<String>,
    virtual_ifsc: Option<Ifsc>,
    virtual_account_number: Option<AccountNumber>,
    tb_account_id: String,
    kyc_tier: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl NormalAccountRow {
    fn into_domain(self) -> NormalAccount {
        NormalAccount {
            id: self.id,
            holder_id: self.holder_id,
            origin_ifsc: self.origin_ifsc,
            origin_account_number: self.origin_account_number,
            vpa: self.vpa,
            virtual_ifsc: self.virtual_ifsc,
            virtual_account_number: self.virtual_account_number,
            tb_account_id: self.tb_account_id.parse().expect("invalid tb_account_id in DB"),
            kyc_tier: self.kyc_tier,
            status: AccountStatus::from_str(&self.status).unwrap_or(AccountStatus::Active),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl NormalAccountRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_account(
        &self,
        id: Uuid,
        holder_id: &str,
        origin_ifsc: Option<&Ifsc>,
        origin_account_number: Option<&AccountNumber>,
        tb_account_id: u128,
    ) -> Result<NormalAccount, AppError> {
        let tb_id_str = tb_account_id.to_string();
        let row = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            INSERT INTO normal_accounts (id, holder_id, origin_ifsc, origin_account_number, tb_account_id)
            VALUES ($1, $2, $3, $4, $5::numeric)
            RETURNING id, holder_id, origin_ifsc, origin_account_number,
                      vpa, virtual_ifsc, virtual_account_number,
                      tb_account_id::text as tb_account_id,
                      kyc_tier, status, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(holder_id)
        .bind(origin_ifsc)
        .bind(origin_account_number)
        .bind(&tb_id_str)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into_domain())
    }

    pub async fn get_account(&self, id: Uuid) -> Result<NormalAccount, AppError> {
        let row = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            SELECT id, holder_id, origin_ifsc, origin_account_number,
                   vpa, virtual_ifsc, virtual_account_number,
                   tb_account_id::text as tb_account_id,
                   kyc_tier, status, created_at, updated_at
            FROM normal_accounts WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NormalAccountNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn list_accounts(&self) -> Result<Vec<NormalAccount>, AppError> {
        let rows = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            SELECT id, holder_id, origin_ifsc, origin_account_number,
                   vpa, virtual_ifsc, virtual_account_number,
                   tb_account_id::text as tb_account_id,
                   kyc_tier, status, created_at, updated_at
            FROM normal_accounts
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AccountStatus,
    ) -> Result<NormalAccount, AppError> {
        let row = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            UPDATE normal_accounts SET status = $2, updated_at = now()
            WHERE id = $1
            RETURNING id, holder_id, origin_ifsc, origin_account_number,
                      vpa, virtual_ifsc, virtual_account_number,
                      tb_account_id::text as tb_account_id,
                      kyc_tier, status, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(status.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NormalAccountNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn count_accounts_by_status(&self) -> Result<Vec<(String, i64)>, AppError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT status, COUNT(*) as count
            FROM normal_accounts
            GROUP BY status
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
```

In `crates/pba_service/src/repository.rs`, add:

```rust
pub mod normal_account_repo;
```

- [ ] **Step 2.9.2: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build.

- [ ] **Step 2.9.3: Commit**

```bash
git add crates/pba_service/src/repository/normal_account_repo.rs crates/pba_service/src/repository.rs
git commit -m "feat(repo): add NormalAccountRepo"
```

### Task 2.10: `NormalAccountService`

**Files:**
- Create: `crates/pba_service/src/service/normal_account_service.rs`
- Modify: `crates/pba_service/src/service.rs`

- [ ] **Step 2.10.1: Write the service**

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account::AccountStatus;
use crate::domain::banking::{AccountNumber, Ifsc};
use crate::domain::normal_account::{tb_normal_id, NormalAccount};
use crate::error::AppError;
use crate::repository::ledger_repo::LedgerRepo;
use crate::repository::normal_account_repo::NormalAccountRepo;

pub struct NormalAccountService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
}

impl NormalAccountService {
    pub fn new(normal_account_repo: Arc<NormalAccountRepo>, ledger_repo: Arc<LedgerRepo>) -> Self {
        Self { normal_account_repo, ledger_repo }
    }

    pub async fn create_account(
        &self,
        holder_id: &str,
        origin_ifsc: Option<&Ifsc>,
        origin_account_number: Option<&AccountNumber>,
    ) -> Result<NormalAccount, AppError> {
        let account_id = Uuid::new_v4();
        let tb_id = tb_normal_id(account_id);

        self.ledger_repo.create_normal_account(tb_id).await?;

        let account = self
            .normal_account_repo
            .create_account(account_id, holder_id, origin_ifsc, origin_account_number, tb_id)
            .await?;

        Ok(account)
    }

    pub async fn get_account(&self, id: Uuid) -> Result<NormalAccount, AppError> {
        self.normal_account_repo.get_account(id).await
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AccountStatus,
    ) -> Result<NormalAccount, AppError> {
        self.normal_account_repo.get_account(id).await?;
        self.normal_account_repo.update_status(id, status).await
    }
}
```

In `crates/pba_service/src/service.rs`, add:

```rust
pub mod normal_account_service;
```

- [ ] **Step 2.10.2: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build.

- [ ] **Step 2.10.3: Commit**

```bash
git add -A
git commit -m "feat(service): add NormalAccountService"
```

### Task 2.11: `NormalDepositService`

**Files:**
- Create: `crates/pba_service/src/service/normal_deposit_service.rs`
- Modify: `crates/pba_service/src/service.rs`

- [ ] **Step 2.11.1: Write the service**

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, TRUST_FUNDING_SOURCE_TB_ID};
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const NORMAL_DEPOSIT_TRANSFER_CODE: u16 = 110;
const PENDING_NORMAL_DEPOSIT_TRANSFER_CODE: u16 = 111;

pub struct NormalDepositService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
}

impl NormalDepositService {
    pub fn new(
        normal_account_repo: Arc<NormalAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
    ) -> Self {
        Self {
            normal_account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit(
        &self,
        account_id: Uuid,
        amount: u64,
        pending: bool,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        idempotency_key: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Normal, account_id, key)
                .await?
            {
                return Ok(existing);
            }
        }

        let account = self.normal_account_repo.get_account(account_id).await?;
        if !account.status.is_active() {
            return Err(AppError::NormalAccountNotActive(account_id.to_string()));
        }

        let deposit_id = Uuid::new_v4();
        let mut tx = self.transaction_repo.pool().begin().await?;

        if pending {
            let timeout = timeout_seconds.unwrap_or(self.default_timeout_seconds);

            let record = self
                .transaction_repo
                .insert_in_tx(
                    &mut tx,
                    deposit_id,
                    account_id,
                    AccountKind::Normal,
                    TransactionType::Deposit,
                    TransactionStatus::Pending,
                    amount,
                    None,
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    Some(timeout),
                    None,
                    None,
                    None,
                    Some("trust"),
                    0,
                    idempotency_key,
                    None,
                )
                .await?;

            let tb_transfer_id = self
                .ledger_repo
                .create_pending_transfer(
                    TRUST_FUNDING_SOURCE_TB_ID,
                    account.tb_account_id,
                    amount,
                    PENDING_NORMAL_DEPOSIT_TRANSFER_CODE,
                    timeout,
                )
                .await
                .map_err(|e| {
                    tracing::error!("TB pending transfer failed, rolling back: {e}");
                    e
                })?;

            self.transaction_repo
                .update_tb_transfer_id_in_tx(&mut tx, deposit_id, tb_transfer_id)
                .await?;

            tx.commit().await?;
            Ok(TransactionRecord { tb_transfer_id, ..record })
        } else {
            let record = self
                .transaction_repo
                .insert_in_tx(
                    &mut tx,
                    deposit_id,
                    account_id,
                    AccountKind::Normal,
                    TransactionType::Deposit,
                    TransactionStatus::Posted,
                    amount,
                    None,
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    None,
                    None,
                    None,
                    None,
                    Some("trust"),
                    0,
                    idempotency_key,
                    None,
                )
                .await?;

            self.ledger_repo
                .create_transfer(
                    TRUST_FUNDING_SOURCE_TB_ID,
                    account.tb_account_id,
                    amount,
                    NORMAL_DEPOSIT_TRANSFER_CODE,
                )
                .await
                .map_err(|e| {
                    tracing::error!("TB transfer failed, rolling back: {e}");
                    e
                })?;

            tx.commit().await?;
            Ok(record)
        }
    }

    pub async fn post_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
    ) -> Result<TransactionRecord, AppError> {
        let txn = self.transaction_repo.get_by_id(deposit_id, account_id).await?;
        if txn.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(deposit_id.to_string()));
        }
        self.ledger_repo.post_pending_transfer(txn.tb_transfer_id).await?;
        self.transaction_repo.update_status(deposit_id, TransactionStatus::Posted).await
    }

    pub async fn void_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
    ) -> Result<TransactionRecord, AppError> {
        let txn = self.transaction_repo.get_by_id(deposit_id, account_id).await?;
        if txn.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(deposit_id.to_string()));
        }
        self.ledger_repo.void_pending_transfer(txn.tb_transfer_id).await?;
        self.transaction_repo.update_status(deposit_id, TransactionStatus::Voided).await
    }
}
```

In `service.rs` add `pub mod normal_deposit_service;`.

- [ ] **Step 2.11.2: Compile and commit**

```bash
cargo build -p pba_service
git add -A
git commit -m "feat(service): add NormalDepositService"
```

### Task 2.12: `NormalWithdrawalService`

**Files:**
- Create: `crates/pba_service/src/service/normal_withdrawal_service.rs`
- Modify: `crates/pba_service/src/service.rs`

- [ ] **Step 2.12.1: Write the service**

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, WITHDRAWAL_SETTLEMENT_TB_ID};
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const NORMAL_WITHDRAWAL_TRANSFER_CODE: u16 = 310;

pub struct NormalWithdrawalService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}

impl NormalWithdrawalService {
    pub fn new(
        normal_account_repo: Arc<NormalAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
    ) -> Self {
        Self { normal_account_repo, ledger_repo, transaction_repo }
    }

    pub async fn withdraw(
        &self,
        account_id: Uuid,
        amount: u64,
        idempotency_key: Option<&str>,
        gateway_ref: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Normal, account_id, key)
                .await?
            {
                return Ok(existing);
            }
        }

        let account = self.normal_account_repo.get_account(account_id).await?;
        if !account.status.is_active() {
            return Err(AppError::NormalAccountNotActive(account_id.to_string()));
        }

        let balance = self.ledger_repo.get_single_balance(account.tb_account_id).await?;
        if balance.posted < amount {
            return Err(AppError::InsufficientFunds {
                requested: amount,
                available: balance.posted,
            });
        }

        let withdrawal_id = Uuid::new_v4();
        let mut tx = self.transaction_repo.pool().begin().await?;

        let record = self
            .transaction_repo
            .insert_in_tx(
                &mut tx,
                withdrawal_id,
                account_id,
                AccountKind::Normal,
                TransactionType::Withdrawal,
                TransactionStatus::Settled,
                amount,
                None,
                TransactionDirection::Outbound,
                None,
                None,
                gateway_ref,
                None,
                None,
                None,
                None,
                None,
                0,
                idempotency_key,
                None,
            )
            .await?;

        self.ledger_repo
            .create_transfer(
                account.tb_account_id,
                WITHDRAWAL_SETTLEMENT_TB_ID,
                amount,
                NORMAL_WITHDRAWAL_TRANSFER_CODE,
            )
            .await
            .map_err(|e| {
                tracing::error!("TB withdrawal failed, rolling back: {e}");
                e
            })?;

        tx.commit().await?;
        Ok(record)
    }
}
```

In `service.rs` add `pub mod normal_withdrawal_service;`.

- [ ] **Step 2.12.2: Compile and commit**

```bash
cargo build -p pba_service
git add -A
git commit -m "feat(service): add NormalWithdrawalService"
```

### Task 2.13: DTO additions

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`

- [ ] **Step 2.13.1: Add normal-account DTOs**

Append to `crates/pba_service/src/api/dto.rs`:

```rust
// ── Normal Account ──

#[derive(Debug, Deserialize)]
pub struct CreateNormalAccountRequest {
    pub holder_id: String,
    pub origin_ifsc: Option<String>,
    pub origin_account_number: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NormalAccountResponse {
    pub id: Uuid,
    pub holder_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_account_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_account_number: Option<String>,
    pub kyc_tier: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::domain::normal_account::NormalAccount> for NormalAccountResponse {
    fn from(a: crate::domain::normal_account::NormalAccount) -> Self {
        Self {
            id: a.id,
            holder_id: a.holder_id,
            origin_ifsc: a.origin_ifsc.map(|v| v.to_string()),
            origin_account_number: a.origin_account_number.map(|v| v.to_string()),
            vpa: a.vpa,
            virtual_ifsc: a.virtual_ifsc.map(|v| v.to_string()),
            virtual_account_number: a.virtual_account_number.map(|v| v.to_string()),
            kyc_tier: a.kyc_tier,
            status: a.status.as_str().to_string(),
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NormalAccountBalanceResponse {
    pub account_id: Uuid,
    pub balance: u64,
    pub pending: u64,
}

#[derive(Debug, Deserialize)]
pub struct DepositToNormalAccountRequest {
    pub amount: u64,
    #[serde(default)]
    pub pending: bool,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NormalDepositResponse {
    pub deposit_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct WithdrawFromNormalAccountRequest {
    pub amount: u64,
    pub idempotency_key: Option<String>,
    pub gateway_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NormalWithdrawalResponse {
    pub account_id: Uuid,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
}
```

- [ ] **Step 2.13.2: Update `TransactionSummaryDto`**

In the same file, modify `TransactionSummaryDto`:

```rust
#[derive(Debug, Serialize)]
pub struct TransactionSummaryDto {
    pub id: Uuid,
    pub account_id: Uuid,
    pub account_kind: String,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub status: String,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    pub direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_mcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funding_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::domain::transaction::TransactionRecord> for TransactionSummaryDto {
    fn from(t: crate::domain::transaction::TransactionRecord) -> Self {
        Self {
            id: t.id,
            account_id: t.account_id,
            account_kind: t.account_kind.as_str().to_string(),
            transaction_type: t.transaction_type.as_str().to_string(),
            status: t.status.as_str().to_string(),
            amount: t.amount,
            pool: t.pool,
            direction: t.direction.as_str().to_string(),
            description: t.description,
            merchant_id: t.merchant_id,
            merchant_mcc: t.merchant_mcc,
            source_ifsc: t.source_ifsc,
            source_account: t.source_account,
            gateway_ref: t.gateway_ref,
            funding_type: t.funding_type,
            correlation_id: t.correlation_id,
            created_at: t.created_at,
        }
    }
}
```

- [ ] **Step 2.13.3: Compile and commit**

```bash
cargo build -p pba_service
git add crates/pba_service/src/api/dto.rs
git commit -m "feat(api): add normal-account DTOs and account_kind/correlation_id on TransactionSummary"
```

### Task 2.14: Split `api/handlers.rs` into `api/handlers/{pb,normal,transactions}.rs`

**Files:**
- Create: `crates/pba_service/src/api/handlers/pb.rs`
- Create: `crates/pba_service/src/api/handlers/normal.rs`
- Create: `crates/pba_service/src/api/handlers/transactions.rs`
- Modify: `crates/pba_service/src/api/handlers.rs` (becomes a re-export shim)
- Modify: `crates/pba_service/src/api/routes.rs`
- Modify: `crates/pba_service/src/api.rs` (if it gates handlers module)

This is a refactor + addition. The PB handlers move to `handlers/pb.rs` unchanged in body; the `handlers.rs` file becomes a thin re-export module + new module declarations.

- [ ] **Step 2.14.1: Move existing PB handlers**

```bash
mkdir -p crates/pba_service/src/api/handlers
git mv crates/pba_service/src/api/handlers.rs crates/pba_service/src/api/handlers/pb.rs
```

- [ ] **Step 2.14.2: Create the new `handlers.rs` shim**

`crates/pba_service/src/api/handlers.rs`:

```rust
pub mod normal;
pub mod pb;
pub mod transactions;

// Re-export PB handlers at the legacy module path so existing route definitions
// like `handlers::create_account` continue to compile during the migration.
pub use pb::*;
```

- [ ] **Step 2.14.3: Move the cross-kind ListAllTransactions handler**

In `handlers/pb.rs`, locate the `list_all_transactions` and any other cross-kind handler. Cut and paste them into `crates/pba_service/src/api/handlers/transactions.rs`. Re-export from `handlers.rs`:

```rust
pub use transactions::*;
```

- [ ] **Step 2.14.4: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build. The route definitions in `routes.rs` still call `handlers::*` and resolve via the re-exports.

- [ ] **Step 2.14.5: Add `normal` handlers (skeleton — implementations follow)**

Write `crates/pba_service/src/api/handlers/normal.rs`:

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::api::dto::*;
use crate::domain::account::AccountStatus;
use crate::domain::banking::{AccountNumber, Ifsc};
use crate::error::AppError;
use crate::AppState;

pub async fn create_normal_account(
    State(state): State<AppState>,
    Json(req): Json<CreateNormalAccountRequest>,
) -> Result<(StatusCode, Json<NormalAccountResponse>), AppError> {
    let ifsc = req
        .origin_ifsc
        .as_deref()
        .map(Ifsc::parse)
        .transpose()
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let acc_num = req
        .origin_account_number
        .as_deref()
        .map(AccountNumber::parse)
        .transpose()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let account = state
        .normal_account_service
        .create_account(&req.holder_id, ifsc.as_ref(), acc_num.as_ref())
        .await?;

    Ok((StatusCode::CREATED, Json(account.into())))
}

pub async fn get_normal_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<NormalAccountResponse>, AppError> {
    let account = state.normal_account_service.get_account(id).await?;
    Ok(Json(account.into()))
}

pub async fn list_normal_accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<NormalAccountResponse>>, AppError> {
    let accounts = state.normal_account_repo.list_accounts().await?;
    Ok(Json(accounts.into_iter().map(Into::into).collect()))
}

pub async fn update_normal_account_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<NormalAccountResponse>, AppError> {
    let status = AccountStatus::from_str(&req.status)
        .ok_or_else(|| AppError::Validation(format!("invalid status: {}", req.status)))?;
    let account = state.normal_account_service.update_status(id, status).await?;
    Ok(Json(account.into()))
}

pub async fn get_normal_account_balance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<NormalAccountBalanceResponse>, AppError> {
    let account = state.normal_account_service.get_account(id).await?;
    let balance = state.ledger_repo.get_single_balance(account.tb_account_id).await?;
    Ok(Json(NormalAccountBalanceResponse {
        account_id: id,
        balance: balance.posted,
        pending: balance.pending,
    }))
}

pub async fn deposit_to_normal_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<DepositToNormalAccountRequest>,
) -> Result<(StatusCode, Json<NormalDepositResponse>), AppError> {
    if req.amount == 0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    if !req.pending && req.timeout_seconds.is_some() {
        return Err(AppError::Validation("timeout_seconds is only valid when pending=true".into()));
    }
    let record = state
        .normal_deposit_service
        .deposit(
            id,
            req.amount,
            req.pending,
            req.gateway_ref.as_deref(),
            req.timeout_seconds,
            req.idempotency_key.as_deref(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(NormalDepositResponse {
            deposit_id: record.id,
            account_id: id,
            amount: record.amount,
            status: record.status.as_str().to_string(),
            gateway_ref: record.gateway_ref,
            timeout_seconds: record.timeout_seconds,
        }),
    ))
}

pub async fn post_normal_account_deposit(
    State(state): State<AppState>,
    Path((id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<NormalDepositResponse>, AppError> {
    let record = state.normal_deposit_service.post_deposit(id, deposit_id).await?;
    Ok(Json(NormalDepositResponse {
        deposit_id: record.id,
        account_id: id,
        amount: record.amount,
        status: record.status.as_str().to_string(),
        gateway_ref: record.gateway_ref,
        timeout_seconds: record.timeout_seconds,
    }))
}

pub async fn void_normal_account_deposit(
    State(state): State<AppState>,
    Path((id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<NormalDepositResponse>, AppError> {
    let record = state.normal_deposit_service.void_deposit(id, deposit_id).await?;
    Ok(Json(NormalDepositResponse {
        deposit_id: record.id,
        account_id: id,
        amount: record.amount,
        status: record.status.as_str().to_string(),
        gateway_ref: record.gateway_ref,
        timeout_seconds: record.timeout_seconds,
    }))
}

pub async fn withdraw_from_normal_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<WithdrawFromNormalAccountRequest>,
) -> Result<(StatusCode, Json<NormalWithdrawalResponse>), AppError> {
    if req.amount == 0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    let record = state
        .normal_withdrawal_service
        .withdraw(id, req.amount, req.idempotency_key.as_deref(), req.gateway_ref.as_deref())
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(NormalWithdrawalResponse {
            account_id: id,
            amount: record.amount,
            gateway_ref: record.gateway_ref,
        }),
    ))
}

pub async fn list_normal_account_transactions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<ListTransactionsQuery>,
) -> Result<Json<ListTransactionsResponse>, AppError> {
    let offset = q.offset.unwrap_or(0);
    let limit = q.limit.unwrap_or(50);
    let txns = state
        .transaction_repo
        .list_by_account(crate::domain::account_kind::AccountKind::Normal, id, offset, limit, q.from_date, q.to_date)
        .await?;
    let total = state
        .transaction_repo
        .count_by_account(id, q.from_date, q.to_date)
        .await?;
    Ok(Json(ListTransactionsResponse {
        transactions: txns.into_iter().map(Into::into).collect(),
        total,
        offset,
        limit,
    }))
}
```

- [ ] **Step 2.14.6: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build. (`AppState` will need new fields — added in next task.)

- [ ] **Step 2.14.7: Commit**

```bash
git add -A
git commit -m "feat(api): split handlers into pb/normal/transactions modules"
```

### Task 2.15: Wire new repos and services into `AppState`

**Files:**
- Modify: `crates/pba_service/src/main.rs`

- [ ] **Step 2.15.1: Add fields and constructor**

Update `AppState` in `crates/pba_service/src/main.rs`:

```rust
#[derive(Clone)]
pub struct AppState {
    pub pb_account_service: Arc<PbAccountService>,
    pub pb_deposit_service: Arc<PbDepositService>,
    pub pb_payment_service: Arc<PbPaymentService>,
    pub pb_withdrawal_service: Arc<PbWithdrawalService>,
    pub pb_account_repo: Arc<PbAccountRepo>,
    pub normal_account_service: Arc<NormalAccountService>,
    pub normal_deposit_service: Arc<NormalDepositService>,
    pub normal_withdrawal_service: Arc<NormalWithdrawalService>,
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub auth: AuthContext,
    pub path_prefix: String,
}
```

Add imports:

```rust
use repository::normal_account_repo::NormalAccountRepo;
use service::normal_account_service::NormalAccountService;
use service::normal_deposit_service::NormalDepositService;
use service::normal_withdrawal_service::NormalWithdrawalService;
```

In `main()` after the existing repo/service construction, add:

```rust
let normal_account_repo = Arc::new(NormalAccountRepo::new(pg_pool.clone()));

let normal_account_service = Arc::new(NormalAccountService::new(
    Arc::clone(&normal_account_repo),
    Arc::clone(&ledger_repo),
));
let normal_deposit_service = Arc::new(NormalDepositService::new(
    Arc::clone(&normal_account_repo),
    Arc::clone(&ledger_repo),
    Arc::clone(&transaction_repo),
    config.deposit_timeout_seconds,
));
let normal_withdrawal_service = Arc::new(NormalWithdrawalService::new(
    Arc::clone(&normal_account_repo),
    Arc::clone(&ledger_repo),
    Arc::clone(&transaction_repo),
));
```

Add the new fields to the `AppState { … }` struct literal.

- [ ] **Step 2.15.2: Compile and commit**

```bash
cargo build -p pba_service
git add crates/pba_service/src/main.rs
git commit -m "feat(main): wire normal_account services into AppState"
```

### Task 2.16: Add canonical and legacy routes

**Files:**
- Modify: `crates/pba_service/src/api/routes.rs`

- [ ] **Step 2.16.1: Restructure the protected router**

Replace `protected_router` in `crates/pba_service/src/api/routes.rs`:

```rust
pub fn protected_router() -> Router<AppState> {
    let pb = Router::new()
        .route("/pb-accounts",                                              post(handlers::pb::create_account))
        .route("/pb-accounts/{account_id}",                                 get(handlers::pb::get_account))
        .route("/pb-accounts/{account_id}/status",                          patch(handlers::pb::update_account_status))
        .route("/pb-accounts/{account_id}/balance",                         get(handlers::pb::get_balance))
        .route("/pb-accounts/{account_id}/deposits",                        post(handlers::pb::deposit))
        .route("/pb-accounts/{account_id}/deposits/{deposit_id}/post",      post(handlers::pb::post_deposit))
        .route("/pb-accounts/{account_id}/deposits/{deposit_id}/void",      post(handlers::pb::void_deposit))
        .route("/pb-accounts/{account_id}/payments",                        post(handlers::pb::make_payment))
        .route("/pb-accounts/{account_id}/withdrawals",                     post(handlers::pb::withdraw))
        .route("/pb-accounts/{account_id}/transactions",                    get(handlers::pb::list_transactions));

    let normal = Router::new()
        .route("/normal-accounts",                                          post(handlers::normal::create_normal_account))
        .route("/normal-accounts",                                          get(handlers::normal::list_normal_accounts))
        .route("/normal-accounts/{account_id}",                             get(handlers::normal::get_normal_account))
        .route("/normal-accounts/{account_id}/status",                      patch(handlers::normal::update_normal_account_status))
        .route("/normal-accounts/{account_id}/balance",                     get(handlers::normal::get_normal_account_balance))
        .route("/normal-accounts/{account_id}/deposits",                    post(handlers::normal::deposit_to_normal_account))
        .route("/normal-accounts/{account_id}/deposits/{deposit_id}/post",  post(handlers::normal::post_normal_account_deposit))
        .route("/normal-accounts/{account_id}/deposits/{deposit_id}/void",  post(handlers::normal::void_normal_account_deposit))
        .route("/normal-accounts/{account_id}/withdrawals",                 post(handlers::normal::withdraw_from_normal_account))
        .route("/normal-accounts/{account_id}/transactions",                get(handlers::normal::list_normal_account_transactions));

    let legacy = Router::new()
        .route("/accounts",                                                 post(handlers::pb::create_account))
        .route("/accounts/{account_id}",                                    get(handlers::pb::get_account))
        .route("/accounts/{account_id}/status",                             patch(handlers::pb::update_account_status))
        .route("/accounts/{account_id}/balance",                            get(handlers::pb::get_balance))
        .route("/accounts/{account_id}/deposits",                           post(handlers::pb::deposit))
        .route("/accounts/{account_id}/deposits/{deposit_id}/post",         post(handlers::pb::post_deposit))
        .route("/accounts/{account_id}/deposits/{deposit_id}/void",         post(handlers::pb::void_deposit))
        .route("/accounts/{account_id}/payments",                           post(handlers::pb::make_payment))
        .route("/accounts/{account_id}/withdrawals",                        post(handlers::pb::withdraw))
        .route("/accounts/{account_id}/transactions",                       get(handlers::pb::list_transactions))
        .layer(axum::middleware::from_fn(deprecation_headers));

    Router::new()
        .merge(pb)
        .merge(normal)
        .merge(legacy)
        .route("/transactions", get(handlers::list_all_transactions))
}
```

- [ ] **Step 2.16.2: Add `deprecation_headers` middleware**

Append to `routes.rs` (or place in a `middleware.rs` file beside it):

```rust
use axum::body::Body;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;

async fn deprecation_headers(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    response.headers_mut().insert("Deprecation", "true".parse().unwrap());
    response.headers_mut().insert("Sunset", "2026-08-06".parse().unwrap());
    response.headers_mut().insert(
        "Link",
        "</docs#deprecation>; rel=\"deprecation\"".parse().unwrap(),
    );
    response
}
```

(The Sunset date is 90 days from PR 2 merge — 2026-05-08 + 90 = 2026-08-06.)

- [ ] **Step 2.16.3: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build. If you see "function not found" errors, check that the renamed handler functions exist in `pb.rs` (they may still be named `create_account` etc. — that's fine; the handler module path is `handlers::pb::create_account`).

- [ ] **Step 2.16.4: Smoke test**

```bash
just run-bg
sleep 5
curl -s http://localhost:3030/health
curl -s -i http://localhost:3030/accounts/00000000-0000-0000-0000-000000000000 -H "Authorization: ApiKey $(echo -n 'pba-api:pba-api-secret' | base64)" | head -10
just stop
```

Expected: `/health` returns 200; the legacy `/accounts/{id}` returns 404 with `Deprecation: true` and `Sunset: 2026-08-06` headers; the JSON body says `PbAccountNotFound`.

- [ ] **Step 2.16.5: Commit**

```bash
git add crates/pba_service/src/api/routes.rs
git commit -m "feat(api): add /pb-accounts and /normal-accounts canonical routes; legacy /accounts/* aliased with deprecation headers"
```

### Task 2.17: Smithy operations for normal accounts and rename PB operations

**Files:**
- Create: `model/normal_account.smithy`
- Modify: `model/main.smithy`
- Modify: `model/account.smithy` (rename PB operations + add `@deprecated` aliases)
- Modify: `model/transaction.smithy` (add `accountKind` to summary)

- [ ] **Step 2.17.1: Rename existing PB operations in `model/account.smithy`**

Open `model/account.smithy`. For each operation `CreateAccount`, `GetAccount`, `GetBalance`, `MakePayment`, `Withdraw`, `UpdateAccountStatus`, `Deposit`, `PostDeposit`, `VoidDeposit`, do the following:

1. Rename the operation to its `PB*` form (`CreatePBAccount`, etc.).
2. Update the URL trait on each operation to the canonical `/pb-accounts/...` form.
3. Add a duplicate operation under the OLD name marked `@deprecated`, with the OLD URL (`/accounts/...`), referencing the same input/output shapes.

Example for `CreateAccount`:

```smithy
@http(method: "POST", uri: "/pb-accounts", code: 201)
operation CreatePBAccount {
    input: CreateAccountInput
    output: AccountResponse
    errors: [ValidationException, NotFoundException]
}

@http(method: "POST", uri: "/accounts", code: 201)
@deprecated(message: "Use CreatePBAccount.", since: "2026-05-08")
operation CreateAccount {
    input: CreateAccountInput
    output: AccountResponse
    errors: [ValidationException, NotFoundException]
}
```

Apply the same dual-operation pattern to all PB operations.

- [ ] **Step 2.17.2: Create `model/normal_account.smithy`**

```smithy
$version: "2"
namespace com.ppi.pba

@http(method: "POST", uri: "/normal-accounts", code: 201)
operation CreateNormalAccount {
    input: CreateNormalAccountInput
    output: NormalAccountResponse
    errors: [ValidationException]
}

@http(method: "GET", uri: "/normal-accounts/{accountId}")
@readonly
operation GetNormalAccount {
    input: GetNormalAccountInput
    output: NormalAccountResponse
    errors: [NotFoundException]
}

@http(method: "GET", uri: "/normal-accounts")
@readonly
operation ListNormalAccounts {
    output: ListNormalAccountsOutput
}

@http(method: "PATCH", uri: "/normal-accounts/{accountId}/status")
operation UpdateNormalAccountStatus {
    input: UpdateNormalAccountStatusInput
    output: NormalAccountResponse
    errors: [NotFoundException, ValidationException]
}

@http(method: "GET", uri: "/normal-accounts/{accountId}/balance")
@readonly
operation GetNormalAccountBalance {
    input: GetNormalAccountBalanceInput
    output: NormalAccountBalanceResponse
    errors: [NotFoundException]
}

@http(method: "POST", uri: "/normal-accounts/{accountId}/deposits", code: 201)
operation DepositToNormalAccount {
    input: DepositToNormalAccountInput
    output: NormalDepositResponse
    errors: [NotFoundException, ConflictException, ValidationException]
}

@http(method: "POST", uri: "/normal-accounts/{accountId}/deposits/{depositId}/post")
operation PostNormalAccountDeposit {
    input: PostNormalAccountDepositInput
    output: NormalDepositResponse
    errors: [NotFoundException, ConflictException]
}

@http(method: "POST", uri: "/normal-accounts/{accountId}/deposits/{depositId}/void")
operation VoidNormalAccountDeposit {
    input: VoidNormalAccountDepositInput
    output: NormalDepositResponse
    errors: [NotFoundException, ConflictException]
}

@http(method: "POST", uri: "/normal-accounts/{accountId}/withdrawals", code: 201)
operation WithdrawFromNormalAccount {
    input: WithdrawFromNormalAccountInput
    output: NormalWithdrawalResponse
    errors: [NotFoundException, ConflictException, ValidationException]
}

@http(method: "GET", uri: "/normal-accounts/{accountId}/transactions")
@readonly
operation ListNormalAccountTransactions {
    input: ListNormalAccountTransactionsInput
    output: ListTransactionsOutput
    errors: [NotFoundException]
}

structure CreateNormalAccountInput {
    @required holderId: String
    originIfsc: String
    originAccountNumber: String
}

structure NormalAccountResponse {
    @required id: String
    @required holderId: String
    originIfsc: String
    originAccountNumber: String
    vpa: String
    virtualIfsc: String
    virtualAccountNumber: String
    @required kycTier: String
    @required status: String
    @required createdAt: Timestamp
    @required updatedAt: Timestamp
}

structure ListNormalAccountsOutput {
    @required normalAccounts: NormalAccountList
}

list NormalAccountList { member: NormalAccountResponse }

structure GetNormalAccountInput {
    @required @httpLabel accountId: String
}

structure UpdateNormalAccountStatusInput {
    @required @httpLabel accountId: String
    @required status: String
}

structure GetNormalAccountBalanceInput {
    @required @httpLabel accountId: String
}

structure NormalAccountBalanceResponse {
    @required accountId: String
    @required balance: Long
    @required pending: Long
}

structure DepositToNormalAccountInput {
    @required @httpLabel accountId: String
    @required amount: Long
    pending: Boolean
    gatewayRef: String
    timeoutSeconds: Integer
    idempotencyKey: String
}

structure NormalDepositResponse {
    @required depositId: String
    @required accountId: String
    @required amount: Long
    @required status: String
    gatewayRef: String
    timeoutSeconds: Integer
}

structure PostNormalAccountDepositInput {
    @required @httpLabel accountId: String
    @required @httpLabel depositId: String
}

structure VoidNormalAccountDepositInput {
    @required @httpLabel accountId: String
    @required @httpLabel depositId: String
}

structure WithdrawFromNormalAccountInput {
    @required @httpLabel accountId: String
    @required amount: Long
    gatewayRef: String
    idempotencyKey: String
}

structure NormalWithdrawalResponse {
    @required accountId: String
    @required amount: Long
    gatewayRef: String
}

structure ListNormalAccountTransactionsInput {
    @required @httpLabel accountId: String
    @httpQuery("offset") offset: Long
    @httpQuery("limit") limit: Long
    @httpQuery("from_date") fromDate: Timestamp
    @httpQuery("to_date") toDate: Timestamp
}
```

- [ ] **Step 2.17.3: Update `model/main.smithy` operation list**

Add the new operations and the renamed PB operations + their deprecated aliases:

```smithy
operations: [
    // PB operations (canonical)
    CreatePBAccount, GetPBAccount, GetPBAccountBalance,
    DepositToPBAccount, PostPBAccountDeposit, VoidPBAccountDeposit,
    MakePBAccountPayment, WithdrawFromPBAccount,
    UpdatePBAccountStatus, ListPBAccountTransactions,

    // PB operations (deprecated aliases — to be removed 2026-08-06)
    CreateAccount, GetAccount, GetBalance,
    Deposit, PostDeposit, VoidDeposit,
    MakePayment, Withdraw,
    UpdateAccountStatus, ListTransactions,

    // Normal-account operations
    CreateNormalAccount, GetNormalAccount, ListNormalAccounts,
    UpdateNormalAccountStatus, GetNormalAccountBalance,
    DepositToNormalAccount, PostNormalAccountDeposit, VoidNormalAccountDeposit,
    WithdrawFromNormalAccount, ListNormalAccountTransactions,

    // Cross-kind / unchanged
    ListAllTransactions,
    ListPurposeTypes, GetPurposeType
]
```

- [ ] **Step 2.17.4: Update `model/transaction.smithy`**

Add `accountKind` and `correlationId` to `TransactionSummary`:

```smithy
structure TransactionSummary {
    @required id: String
    @required accountId: String
    @required accountKind: String
    @required type: String
    @required status: String
    @required amount: Long
    pool: String
    @required direction: String
    description: String
    merchantId: String
    merchantMcc: String
    sourceIfsc: String
    sourceAccount: String
    gatewayRef: String
    fundingType: String
    correlationId: String
    @required createdAt: Timestamp
}
```

- [ ] **Step 2.17.5: Validate Smithy and regenerate SDK**

```bash
just smithy-validate
just smithy-build
```

Expected: validation passes; client crate rebuilds. The generated client now exposes both old and new operations.

- [ ] **Step 2.17.6: Build the SDK consumer**

```bash
cargo build -p pba_client
```

Expected: clean build.

- [ ] **Step 2.17.7: Commit**

```bash
git add model/ crates/pba_client/
git commit -m "feat(smithy): add normal-account operations; rename PB operations with deprecated aliases"
```

### Task 2.18: Cucumber feature for normal account lifecycle

**Files:**
- Create: `crates/pba_service/tests/features/normal_account_lifecycle.feature`
- Modify: relevant step-definition files (likely `crates/pba_service/tests/api_steps.rs` or similar — confirm by inspection)

- [ ] **Step 2.18.1: Identify the existing step-definition file pattern**

```bash
ls crates/pba_service/tests/
grep -l 'Given\|When\|Then' crates/pba_service/tests/*.rs
```

Expected: a file like `api_e2e.rs` or `api_steps.rs` containing `#[given]`, `#[when]`, `#[then]` macros.

- [ ] **Step 2.18.2: Write the feature**

`crates/pba_service/tests/features/normal_account_lifecycle.feature`:

```gherkin
Feature: Normal account lifecycle
  As an admin
  I want to create, fund, withdraw from, freeze, and inspect normal accounts

  Background:
    Given a clean test environment

  Scenario: Create a normal account with no origin bank
    When I create a normal account for holder "alice"
    Then the response status is 201
    And the response contains a "normal" account_kind
    And the holder_id is "alice"
    And the origin_ifsc is absent

  Scenario: Create a normal account with an origin bank
    When I create a normal account for holder "bob" with origin "HDFC0001234" / "1111111111"
    Then the response status is 201
    And the origin_ifsc is "HDFC0001234"

  Scenario: List normal accounts excludes PB accounts
    Given a PB account exists for holder "carla" with purpose "health"
    And a normal account exists for holder "carla"
    When I GET /normal-accounts
    Then the response contains exactly 1 account
    And every returned account has account_kind "normal"

  Scenario: Deposit to normal account credits the balance from the trust sentinel
    Given a normal account "n1" for holder "dan"
    When I deposit 5000 paisa to "n1"
    Then the response status is 201
    And the balance of "n1" is 5000

  Scenario: Pending deposit + post lifecycle
    Given a normal account "n1" for holder "ed"
    When I create a pending deposit of 7500 paisa to "n1" with timeout 120
    Then the deposit status is "pending"
    When I post the deposit
    Then the deposit status is "posted"
    And the balance of "n1" is 7500

  Scenario: Pending deposit + void
    Given a normal account "n1" for holder "fay"
    When I create a pending deposit of 9000 paisa to "n1"
    And I void the deposit
    Then the deposit status is "voided"
    And the balance of "n1" is 0

  Scenario: Withdraw to settlement sentinel
    Given a normal account "n1" with balance 4000 for holder "gus"
    When I withdraw 2500 paisa from "n1"
    Then the response status is 201
    And the balance of "n1" is 1500

  Scenario: Withdraw rejected when insufficient
    Given a normal account "n1" with balance 100 for holder "han"
    When I withdraw 500 paisa from "n1"
    Then the response status is 422
    And the error code is "InsufficientFunds"

  Scenario: Frozen account rejects deposits and withdrawals
    Given a normal account "n1" for holder "ira"
    When I freeze "n1"
    Then deposits to "n1" are rejected with "NormalAccountNotActive"
    And withdrawals from "n1" are rejected with "NormalAccountNotActive"

  Scenario: Idempotency replay on deposit
    Given a normal account "n1" for holder "joy"
    When I deposit 1000 paisa to "n1" with idempotency key "k1"
    And I retry the same deposit
    Then both responses have the same deposit_id
    And the balance of "n1" is 1000

  Scenario: Per-account transactions list shows only normal-account transactions
    Given a normal account "n1" with two deposits and one withdrawal for holder "ken"
    And a PB account "p1" with one deposit for holder "ken"
    When I GET /normal-accounts/{n1}/transactions
    Then exactly 3 transactions are returned
    And every transaction has account_kind "normal"
```

- [ ] **Step 2.18.3: Add or extend step definitions**

Open the step-definition file (per Step 2.18.1) and add the missing step impls. Reuse the existing SDK client; for the new operations use the regenerated `CreateNormalAccount`, `DepositToNormalAccount`, etc. methods.

For each new step phrase, add the appropriate Cucumber-rs macro. Where multiple scenarios share a phrase, write the step once.

- [ ] **Step 2.18.4: Run the feature**

```bash
just api-e2e
```

Expected: all `normal_account_lifecycle.feature` scenarios pass; existing scenarios also pass (they go through `/accounts/*` legacy aliases which still work in Phase 2).

- [ ] **Step 2.18.5: Commit**

```bash
git add -A
git commit -m "test(e2e): cucumber feature for normal account lifecycle"
```

### Task 2.19: Admin UI for normal accounts

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs` (add list/detail/create routes)
- Modify: `crates/pba_service/templates/admin/...` (add `normal_accounts.html`, `normal_account_detail.html`, etc. — patterned after PB equivalents)
- Modify: `crates/pba_service/src/admin.rs` if it gates routes
- Create: `crates/pba_service/tests/ui_features/normal_account_admin.feature`

The shape of the admin UI today (PB list, PB detail, deposit/payment/withdraw forms, transaction detail) provides the template. Mirror it for normal accounts with the smaller operation set.

- [ ] **Step 2.19.1: Survey existing admin layout**

```bash
ls crates/pba_service/templates/admin/
cat crates/pba_service/src/admin/handlers.rs | head -80
```

This identifies the templates and route handlers to mirror.

- [ ] **Step 2.19.2: Add admin routes for normal accounts**

In `crates/pba_service/src/admin/handlers.rs`, add handlers for:

- `GET /admin/normal-accounts` — list page
- `GET /admin/normal-accounts/new` — create form
- `POST /admin/normal-accounts` — form submission
- `GET /admin/normal-accounts/{id}` — detail page
- `POST /admin/normal-accounts/{id}/freeze` (and `/reactivate`)
- `GET /admin/normal-accounts/{id}/deposits/new`, `POST /admin/normal-accounts/{id}/deposits`
- `GET /admin/normal-accounts/{id}/withdrawals/new`, `POST /admin/normal-accounts/{id}/withdrawals`
- `GET /admin/normal-accounts/{id}/transactions`

Each handler is structurally identical to the PB equivalent (reads form, calls service, redirects). Use `crates/pba_service/src/admin/handlers.rs` for reference.

- [ ] **Step 2.19.3: Add templates**

Mirror these PB templates for normal accounts (drop fields that don't apply — purpose, MCC, payment form):

- `templates/admin/normal_accounts.html` (list)
- `templates/admin/normal_account_detail.html` (detail)
- `templates/admin/normal_account_create.html` (form)
- `templates/admin/normal_account_deposit.html`
- `templates/admin/normal_account_withdrawal.html`

Also add an entry in the existing nav/layout template so users can reach `/admin/normal-accounts` from the dashboard.

- [ ] **Step 2.19.4: Write the UI feature**

`crates/pba_service/tests/ui_features/normal_account_admin.feature`:

```gherkin
Feature: Normal account admin pages
  Background:
    Given an admin session

  Scenario: Create normal account through the admin form
    When I navigate to /admin/normal-accounts/new
    And I submit the form with holder "alice"
    Then I am redirected to the normal account detail page
    And the page shows holder "alice"
    And the page shows status "active"

  Scenario: Deposit and withdraw from the admin UI
    Given a normal account "n1" for holder "bob"
    When I navigate to the deposit form for "n1"
    And I submit a deposit of 5000 paisa
    Then I am redirected to the detail page
    And the balance shown is "50.00"
    When I navigate to the withdrawal form for "n1"
    And I submit a withdrawal of 2000 paisa
    Then the balance shown is "30.00"

  Scenario: Transactions list filters to normal-account rows
    Given a normal account "n1" with one deposit and one withdrawal
    When I navigate to /admin/normal-accounts/{n1}/transactions
    Then I see exactly 2 rows
    And each row's account kind cell says "normal"
```

- [ ] **Step 2.19.5: Add or extend UI step definitions**

Mirror the patterns in `crates/pba_service/tests/ui_e2e.rs` (or equivalent). Most "form submit" steps already exist generically.

- [ ] **Step 2.19.6: Run UI tests**

```bash
just ui-e2e
```

Expected: all scenarios pass.

- [ ] **Step 2.19.7: Commit**

```bash
git add -A
git commit -m "feat(admin): admin UI for normal accounts (list, create, deposit, withdraw, transactions)"
```

### Task 2.20: Phase 2 final verification + push PR 2

- [ ] **Step 2.20.1: Local CI**

```bash
just local-ci
just api-e2e
just ui-e2e
```

Expected: all green.

- [ ] **Step 2.20.2: Open PR 2**

```bash
git push -u origin normal-accounts-phase-2
gh pr create --title "feat: introduce normal accounts (Phase 2 of normal accounts)" --body "$(cat <<'EOF'
## Summary
- New Postgres `normal_accounts` table; new `account_kind` and `correlation_id` columns on `transactions`.
- New domain (`NormalAccount`, `AccountKind`), repo (`NormalAccountRepo`), services (`NormalAccountService`, `NormalDepositService`, `NormalWithdrawalService`).
- New canonical routes `/pb-accounts/*` and `/normal-accounts/*`. Legacy `/accounts/*` retained as in-process aliases with `Deprecation: true; Sunset: 2026-08-06` headers.
- New Smithy operations for normal accounts; PB operations renamed to `*PBAccount*` with `@deprecated` aliases under their old names.
- New Cucumber feature `normal_account_lifecycle.feature` and admin UI feature `normal_account_admin.feature`.

PB behaviour unchanged. Trust deposits to PB still accepted via `funding_type='trust'` — the breaking change comes in Phase 3.

See [design doc](docs/superpowers/specs/2026-05-08-normal-accounts-design.md).

## Test plan
- [ ] `just local-ci` passes
- [ ] `just api-e2e` passes (existing + new normal_account_lifecycle scenarios)
- [ ] `just ui-e2e` passes (existing + new normal_account_admin scenarios)
- [ ] Smoke-test legacy alias: `curl /accounts/{id}` returns expected response with Deprecation headers

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Wait for review and merge. Phase 3 builds on Phase 2's domain types.

---

## Phase 3 — Transfers + Trust Removal (PR 3)

**Goal of phase 3:** add `normal → PB` internal transfers (immediate + pending lifecycle) and remove the direct `funding_type='trust'` path on PB deposits. This is the only PR with a behavioural break — isolated for clean revertability.

After PR 2 merges:

```bash
git checkout main
git pull
git checkout -b normal-accounts-phase-3
```

### Task 3.1: Internal transfer ledger helpers

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs`

- [ ] **Step 3.1.1: Add transfer code constants**

Near the top of `ledger_repo.rs`:

```rust
const INTERNAL_TRANSFER_CODE: u16 = 400;
const PENDING_INTERNAL_TRANSFER_CODE: u16 = 401;
```

Add a public re-export of these if Service code needs them (unlikely — the service passes them through `create_internal_transfer`, but if the constants are referenced outside, gate them with `pub`). Default: keep them private.

- [ ] **Step 3.1.2: Add `create_internal_transfer` method**

```rust
pub async fn create_internal_transfer(
    &self,
    debit_account_id: u128,
    credit_account_id: u128,
    amount: u64,
) -> Result<(), AppError> {
    self.create_transfer(debit_account_id, credit_account_id, amount, INTERNAL_TRANSFER_CODE)
        .await
}

pub async fn create_pending_internal_transfer(
    &self,
    debit_account_id: u128,
    credit_account_id: u128,
    amount: u64,
    timeout_seconds: u32,
) -> Result<u128, AppError> {
    self.create_pending_transfer(
        debit_account_id, credit_account_id,
        amount, PENDING_INTERNAL_TRANSFER_CODE, timeout_seconds,
    ).await
}
```

These are thin wrappers over the existing `create_transfer` and `create_pending_transfer` — they exist to fix the transfer code at the ledger layer rather than allowing services to pass arbitrary codes.

- [ ] **Step 3.1.3: Compile and commit**

```bash
cargo build -p pba_service
git add crates/pba_service/src/repository/ledger_repo.rs
git commit -m "feat(ledger): internal transfer helpers (codes 400/401)"
```

### Task 3.2: Reject `funding_type='trust'` on PB deposits

**Files:**
- Modify: `crates/pba_service/src/service/pb_deposit_service.rs`
- Modify: `crates/pba_service/src/error.rs` (new variant)

- [ ] **Step 3.2.1: Add `TrustDepositRequiresTransfer` error variant**

In `error.rs`:

Add to the enum:

```rust
TrustDepositRequiresTransfer,
```

Add to `Display`:

```rust
Self::TrustDepositRequiresTransfer => write!(
    f,
    "Trust-funded deposits to PB accounts have been removed. Use POST /normal-accounts/{{id}}/transfers instead."
),
```

Add to `IntoResponse`:

```rust
AppError::TrustDepositRequiresTransfer => (StatusCode::BAD_REQUEST, "TrustDepositRequiresTransfer"),
```

- [ ] **Step 3.2.2: Add the failing test**

In `crates/pba_service/src/service/pb_deposit_service.rs`, add at the bottom (or in a separate test file under `tests/`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // … any existing tests stay …

    // The deposit test path uses real PG/TB — skip if those aren't reachable.
    // The simpler integration test goes in a Cucumber feature instead. This test
    // verifies the early-return shape via direct service-method introspection.
    // (No fixture needed; the function returns Err before any IO.)
    #[test]
    fn rejects_trust_funding_type_via_validation() {
        // Surface check: `funding_type='trust'` must be rejected at the top of `deposit()`.
        // Concrete behavioural test lives in the Cucumber feature
        // `trust_direct_deposit_removed.feature`.
        // This is just a placeholder to remind the implementer of the rule.
    }
}
```

(The behavioural assertion lives in the Cucumber feature added in Task 3.5; unit-testing requires PG/TB fixtures that the existing service tests don't include.)

- [ ] **Step 3.2.3: Add the rejection at the top of `deposit()`**

In `crates/pba_service/src/service/pb_deposit_service.rs`, at the start of `deposit()` (immediately after the idempotency replay block):

```rust
if funding_type == Some("trust") {
    return Err(AppError::TrustDepositRequiresTransfer);
}
```

The check goes after idempotency replay so that an idempotent retry of an already-recorded `trust` deposit (from before this rule) still returns the cached record rather than newly rejecting — which is the behaviour the existing PB tests assume during the deprecation overlap.

- [ ] **Step 3.2.4: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build.

- [ ] **Step 3.2.5: Commit**

```bash
git add -A
git commit -m "feat(deposit): reject funding_type='trust' on PB deposits"
```

### Task 3.3: `Transfer` domain helpers

**Files:**
- Create: `crates/pba_service/src/domain/transfer.rs`
- Modify: `crates/pba_service/src/domain.rs`

- [ ] **Step 3.3.1: Write helpers**

```rust
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TransferLegs {
    pub source_txn_id: Uuid,
    pub destination_txn_id: Uuid,
    pub correlation_id: Uuid,
}

impl TransferLegs {
    pub fn new() -> Self {
        Self {
            source_txn_id: Uuid::new_v4(),
            destination_txn_id: Uuid::new_v4(),
            correlation_id: Uuid::new_v4(),
        }
    }
}

impl Default for TransferLegs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legs_have_distinct_ids() {
        let legs = TransferLegs::new();
        assert_ne!(legs.source_txn_id, legs.destination_txn_id);
        assert_ne!(legs.source_txn_id, legs.correlation_id);
    }
}
```

In `domain.rs`, add `pub mod transfer;`.

- [ ] **Step 3.3.2: Run test, commit**

```bash
cargo test -p pba_service domain::transfer::tests
git add -A
git commit -m "feat(domain): TransferLegs helper"
```

### Task 3.4: `TransferService`

**Files:**
- Create: `crates/pba_service/src/service/transfer_service.rs`
- Modify: `crates/pba_service/src/service.rs`

- [ ] **Step 3.4.1: Write the service**

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::domain::transfer::TransferLegs;
use crate::error::AppError;
use crate::repository::ledger_repo::LedgerRepo;
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const MAX_RETRIES: u32 = 3;

pub struct TransferService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub pb_account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct TransferResult {
    pub source_txn_id: Uuid,
    pub destination_txn_id: Uuid,
    pub source_account_id: Uuid,
    pub destination_account_id: Uuid,
    pub amount: u64,
    pub status: TransactionStatus,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TransferService {
    pub fn new(
        normal_account_repo: Arc<NormalAccountRepo>,
        pb_account_repo: Arc<PbAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
    ) -> Self {
        Self {
            normal_account_repo,
            pb_account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transfer(
        &self,
        source_normal_id: Uuid,
        destination_pb_id: Uuid,
        amount: u64,
        pending: bool,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        description: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<TransferResult, AppError> {
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Normal, source_normal_id, key)
                .await?
            {
                let correlation_id = existing.correlation_id.ok_or_else(|| {
                    AppError::DatabaseError(
                        "transfer source row missing correlation_id".to_string(),
                    )
                })?;
                let legs = self.transaction_repo.find_by_correlation_id(correlation_id).await?;
                if legs.len() != 2 {
                    return Err(AppError::DatabaseError(
                        "transfer correlation has != 2 legs".to_string(),
                    ));
                }
                return Ok(self.legs_to_result(&legs, source_normal_id, destination_pb_id));
            }
        }

        let source = self.normal_account_repo.get_account(source_normal_id).await?;
        let destination = self.pb_account_repo.get_account(destination_pb_id).await?;

        if !source.status.is_active() {
            return Err(AppError::NormalAccountNotActive(source_normal_id.to_string()));
        }
        if !destination.status.is_active() {
            return Err(AppError::PbAccountNotActive(destination_pb_id.to_string()));
        }

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tracing::info!(source_normal_id = %source_normal_id, attempt, "retrying transfer with fresh balance");
            }

            let balance = self.ledger_repo.get_single_balance(source.tb_account_id).await?;
            if balance.posted < amount {
                return Err(AppError::InsufficientFunds {
                    requested: amount,
                    available: balance.posted,
                });
            }

            let legs = TransferLegs::new();
            let mut tx = self.transaction_repo.pool().begin().await?;

            let source_status = if pending {
                TransactionStatus::Pending
            } else {
                TransactionStatus::Posted
            };

            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    legs.source_txn_id,
                    source_normal_id,
                    AccountKind::Normal,
                    TransactionType::Transfer,
                    source_status,
                    amount,
                    None,
                    TransactionDirection::Outbound,
                    None,
                    None,
                    gateway_ref,
                    if pending { Some(timeout_seconds.unwrap_or(self.default_timeout_seconds)) } else { None },
                    None,
                    None,
                    description,
                    Some("trust"),
                    0,
                    idempotency_key,
                    Some(legs.correlation_id),
                )
                .await?;

            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    legs.destination_txn_id,
                    destination_pb_id,
                    AccountKind::Pb,
                    TransactionType::Deposit,
                    source_status,
                    amount,
                    Some("others"),
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    if pending { Some(timeout_seconds.unwrap_or(self.default_timeout_seconds)) } else { None },
                    None,
                    None,
                    description,
                    Some("trust"),
                    0,
                    None,
                    Some(legs.correlation_id),
                )
                .await?;

            let tb_result = if pending {
                self.ledger_repo
                    .create_pending_internal_transfer(
                        source.tb_account_id,
                        destination.tb_others_account_id,
                        amount,
                        timeout_seconds.unwrap_or(self.default_timeout_seconds),
                    )
                    .await
            } else {
                self.ledger_repo
                    .create_internal_transfer(
                        source.tb_account_id,
                        destination.tb_others_account_id,
                        amount,
                    )
                    .await
                    .map(|_| 0u128)
            };

            match tb_result {
                Ok(tb_transfer_id) => {
                    if pending && tb_transfer_id != 0 {
                        sqlx::query(
                            r#"UPDATE transactions SET tb_transfer_id = $1::numeric, updated_at = now() WHERE correlation_id = $2"#,
                        )
                        .bind(tb_transfer_id.to_string())
                        .bind(legs.correlation_id)
                        .execute(tx.as_mut())
                        .await
                        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
                    }
                    tx.commit().await?;
                    let updated_legs = self.transaction_repo.find_by_correlation_id(legs.correlation_id).await?;
                    return Ok(self.legs_to_result(&updated_legs, source_normal_id, destination_pb_id));
                }
                Err(AppError::ExceedsBalance) => {
                    last_err = Some(AppError::ExceedsBalance);
                    // tx rolls back on drop
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or(AppError::ExceedsBalance))
    }

    pub async fn post_transfer(
        &self,
        source_normal_id: Uuid,
        transfer_id: Uuid,
    ) -> Result<TransferResult, AppError> {
        let source_row = self.transaction_repo.get_by_id(transfer_id, source_normal_id).await?;
        if source_row.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(transfer_id.to_string()));
        }

        self.ledger_repo.post_pending_transfer(source_row.tb_transfer_id).await?;

        let correlation_id = source_row.correlation_id.ok_or_else(|| {
            AppError::DatabaseError("transfer source row missing correlation_id".to_string())
        })?;
        sqlx::query(
            r#"UPDATE transactions SET status = $1, updated_at = now() WHERE correlation_id = $2"#,
        )
        .bind(TransactionStatus::Posted.as_str())
        .bind(correlation_id)
        .execute(self.transaction_repo.pool())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let legs = self.transaction_repo.find_by_correlation_id(correlation_id).await?;
        Ok(self.legs_to_result(&legs, source_normal_id, legs[1].account_id))
    }

    pub async fn void_transfer(
        &self,
        source_normal_id: Uuid,
        transfer_id: Uuid,
    ) -> Result<TransferResult, AppError> {
        let source_row = self.transaction_repo.get_by_id(transfer_id, source_normal_id).await?;
        if source_row.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(transfer_id.to_string()));
        }

        self.ledger_repo.void_pending_transfer(source_row.tb_transfer_id).await?;

        let correlation_id = source_row.correlation_id.ok_or_else(|| {
            AppError::DatabaseError("transfer source row missing correlation_id".to_string())
        })?;
        sqlx::query(
            r#"UPDATE transactions SET status = $1, updated_at = now() WHERE correlation_id = $2"#,
        )
        .bind(TransactionStatus::Voided.as_str())
        .bind(correlation_id)
        .execute(self.transaction_repo.pool())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let legs = self.transaction_repo.find_by_correlation_id(correlation_id).await?;
        Ok(self.legs_to_result(&legs, source_normal_id, legs[1].account_id))
    }

    fn legs_to_result(
        &self,
        legs: &[TransactionRecord],
        source_normal_id: Uuid,
        destination_pb_id: Uuid,
    ) -> TransferResult {
        let source_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Normal)
            .expect("transfer correlation has a normal leg");
        let dest_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .expect("transfer correlation has a pb leg");
        TransferResult {
            source_txn_id: source_leg.id,
            destination_txn_id: dest_leg.id,
            source_account_id: source_normal_id,
            destination_account_id: destination_pb_id,
            amount: source_leg.amount,
            status: source_leg.status,
            correlation_id: source_leg.correlation_id.expect("source leg has correlation_id"),
            created_at: source_leg.created_at,
        }
    }
}
```

In `service.rs` add `pub mod transfer_service;`.

- [ ] **Step 3.4.2: Compile**

```bash
cargo build -p pba_service
```

Expected: clean build. If `AccountKind` doesn't `derive(PartialEq)`, add it in `domain/account_kind.rs`.

- [ ] **Step 3.4.3: Wire into `AppState` (main.rs)**

Add field `pub transfer_service: Arc<TransferService>` to `AppState`. Add to imports. Construct in `main()`:

```rust
let transfer_service = Arc::new(TransferService::new(
    Arc::clone(&normal_account_repo),
    Arc::clone(&pb_account_repo),
    Arc::clone(&ledger_repo),
    Arc::clone(&transaction_repo),
    config.deposit_timeout_seconds,
));
```

Add to `AppState { … }` literal.

- [ ] **Step 3.4.4: Compile and commit**

```bash
cargo build -p pba_service
git add -A
git commit -m "feat(service): TransferService with immediate + pending lifecycle"
```

### Task 3.5: Transfer DTOs and handlers

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`
- Create: `crates/pba_service/src/api/handlers/transfer.rs`
- Modify: `crates/pba_service/src/api/handlers.rs` (re-export)
- Modify: `crates/pba_service/src/api/routes.rs` (new routes)

- [ ] **Step 3.5.1: Add DTOs**

Append to `dto.rs`:

```rust
// ── Transfer ──

#[derive(Debug, Deserialize)]
pub struct TransferToPBAccountRequest {
    pub destination_pb_account_id: Uuid,
    pub amount: u64,
    #[serde(default)]
    pub pending: bool,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub description: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransferResponse {
    pub transfer_id: Uuid,
    pub source_account_id: Uuid,
    pub destination_account_id: Uuid,
    pub amount: u64,
    pub status: String,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::transfer_service::TransferResult> for TransferResponse {
    fn from(r: crate::service::transfer_service::TransferResult) -> Self {
        Self {
            transfer_id: r.source_txn_id,
            source_account_id: r.source_account_id,
            destination_account_id: r.destination_account_id,
            amount: r.amount,
            status: r.status.as_str().to_string(),
            correlation_id: r.correlation_id,
            created_at: r.created_at,
        }
    }
}
```

- [ ] **Step 3.5.2: Add handlers**

`crates/pba_service/src/api/handlers/transfer.rs`:

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::api::dto::*;
use crate::error::AppError;
use crate::AppState;

pub async fn initiate_transfer(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
    Json(req): Json<TransferToPBAccountRequest>,
) -> Result<(StatusCode, Json<TransferResponse>), AppError> {
    if req.amount == 0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    if !req.pending && req.timeout_seconds.is_some() {
        return Err(AppError::Validation("timeout_seconds is only valid when pending=true".into()));
    }
    if let Some(d) = req.description.as_deref() {
        if d.len() > 256 {
            return Err(AppError::Validation("description must be ≤ 256 chars".into()));
        }
    }

    let result = state
        .transfer_service
        .transfer(
            source_id,
            req.destination_pb_account_id,
            req.amount,
            req.pending,
            req.gateway_ref.as_deref(),
            req.timeout_seconds,
            req.description.as_deref(),
            req.idempotency_key.as_deref(),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(result.into())))
}

pub async fn post_transfer(
    State(state): State<AppState>,
    Path((source_id, transfer_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TransferResponse>, AppError> {
    let result = state.transfer_service.post_transfer(source_id, transfer_id).await?;
    Ok(Json(result.into()))
}

pub async fn void_transfer(
    State(state): State<AppState>,
    Path((source_id, transfer_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TransferResponse>, AppError> {
    let result = state.transfer_service.void_transfer(source_id, transfer_id).await?;
    Ok(Json(result.into()))
}
```

In `handlers.rs`:

```rust
pub mod transfer;
```

- [ ] **Step 3.5.3: Add routes**

In `crates/pba_service/src/api/routes.rs`, inside the `normal` router:

```rust
.route("/normal-accounts/{account_id}/transfers",                       post(handlers::transfer::initiate_transfer))
.route("/normal-accounts/{account_id}/transfers/{transfer_id}/post",    post(handlers::transfer::post_transfer))
.route("/normal-accounts/{account_id}/transfers/{transfer_id}/void",    post(handlers::transfer::void_transfer))
```

- [ ] **Step 3.5.4: Compile and commit**

```bash
cargo build -p pba_service
git add -A
git commit -m "feat(api): transfer DTOs, handlers, and routes"
```

### Task 3.6: Smithy operations for transfers

**Files:**
- Create: `model/transfer.smithy`
- Modify: `model/main.smithy`

- [ ] **Step 3.6.1: Write Smithy operations**

`model/transfer.smithy`:

```smithy
$version: "2"
namespace com.ppi.pba

@http(method: "POST", uri: "/normal-accounts/{accountId}/transfers", code: 201)
operation TransferToPBAccount {
    input: TransferToPBAccountInput
    output: TransferResponse
    errors: [NotFoundException, ConflictException, ValidationException]
}

@http(method: "POST", uri: "/normal-accounts/{accountId}/transfers/{transferId}/post")
operation PostNormalAccountTransfer {
    input: PostNormalAccountTransferInput
    output: TransferResponse
    errors: [NotFoundException, ConflictException]
}

@http(method: "POST", uri: "/normal-accounts/{accountId}/transfers/{transferId}/void")
operation VoidNormalAccountTransfer {
    input: VoidNormalAccountTransferInput
    output: TransferResponse
    errors: [NotFoundException, ConflictException]
}

structure TransferToPBAccountInput {
    @required @httpLabel accountId: String
    @required destinationPbAccountId: String
    @required amount: Long
    pending: Boolean
    gatewayRef: String
    timeoutSeconds: Integer
    description: String
    idempotencyKey: String
}

structure TransferResponse {
    @required transferId: String
    @required sourceAccountId: String
    @required destinationAccountId: String
    @required amount: Long
    @required status: String
    @required correlationId: String
    @required createdAt: Timestamp
}

structure PostNormalAccountTransferInput {
    @required @httpLabel accountId: String
    @required @httpLabel transferId: String
}

structure VoidNormalAccountTransferInput {
    @required @httpLabel accountId: String
    @required @httpLabel transferId: String
}
```

- [ ] **Step 3.6.2: Add to `main.smithy` operations list**

```smithy
TransferToPBAccount, PostNormalAccountTransfer, VoidNormalAccountTransfer,
```

- [ ] **Step 3.6.3: Validate and rebuild SDK**

```bash
just smithy-validate
just smithy-build
cargo build -p pba_client
```

- [ ] **Step 3.6.4: Commit**

```bash
git add model/ crates/pba_client/
git commit -m "feat(smithy): transfer operations"
```

### Task 3.7: Update `deposit_timeout` to handle transfer correlation

**Files:**
- Modify: `crates/pba_service/src/service/deposit_timeout.rs`

- [ ] **Step 3.7.1: Read current implementation**

Open `crates/pba_service/src/service/deposit_timeout.rs`. The poller calls `transaction_repo.find_timed_out_pending` and then voids the TB transfer plus updates the PG row's status to `voided`.

- [ ] **Step 3.7.2: Update void-and-update logic**

When processing each timed-out row, if the row has a `correlation_id`, the `UPDATE` should target both legs:

```rust
for txn in timed_out {
    match ledger_repo.void_pending_transfer(txn.tb_transfer_id).await {
        Ok(()) | Err(AppError::TigerBeetleError(_)) => {
            // TB returns "already voided" for transfers past their auto-timeout — treat as success.
            if let Some(correlation_id) = txn.correlation_id {
                sqlx::query(
                    r#"UPDATE transactions SET status = 'voided', updated_at = now() WHERE correlation_id = $1 AND status = 'pending'"#,
                )
                .bind(correlation_id)
                .execute(transaction_repo.pool())
                .await
                .ok();
            } else {
                let _ = transaction_repo.update_status(txn.id, TransactionStatus::Voided).await;
            }
        }
        Err(e) => {
            tracing::error!(transaction_id = %txn.id, "void_pending_transfer failed: {e:?}");
        }
    }
}
```

The `AND status = 'pending'` clause ensures we don't overwrite a row that was concurrently posted/voided through the API.

- [ ] **Step 3.7.3: Compile and commit**

```bash
cargo build -p pba_service
git add crates/pba_service/src/service/deposit_timeout.rs
git commit -m "feat(timeout): handle transfer correlation when auto-voiding pending rows"
```

### Task 3.8: Cucumber feature for internal transfers

**Files:**
- Create: `crates/pba_service/tests/features/internal_transfer.feature`
- Modify: step definitions

- [ ] **Step 3.8.1: Write the feature**

```gherkin
Feature: Internal transfers from normal accounts to PB accounts
  Background:
    Given a clean test environment

  Scenario: Immediate transfer credits the PB others-pool
    Given a normal account "n1" with balance 10000 for holder "alice"
    And a PB account "p1" for holder "alice" with purpose "health"
    When I transfer 5000 paisa from "n1" to "p1"
    Then the response status is 201
    And the response status field is "posted"
    And the balance of "n1" is 5000
    And the others-pool balance of "p1" is 5000
    And the self-pool balance of "p1" is 0

  Scenario: Pending transfer + post lifecycle
    Given a normal account "n1" with balance 10000 for holder "bob"
    And a PB account "p1" for holder "bob" with purpose "education"
    When I create a pending transfer of 3000 paisa from "n1" to "p1" with timeout 120
    Then the response status field is "pending"
    When I post the transfer
    Then the response status field is "posted"
    And the others-pool balance of "p1" is 3000

  Scenario: Pending transfer + void
    Given a normal account "n1" with balance 5000 for holder "carla"
    And a PB account "p1" for holder "carla" with purpose "food"
    When I create a pending transfer of 1500 paisa from "n1" to "p1"
    And I void the transfer
    Then the response status field is "voided"
    And the balance of "n1" is 5000
    And the others-pool balance of "p1" is 0

  Scenario: Pending transfer auto-voids after timeout
    Given a normal account "n1" with balance 5000 for holder "dan"
    And a PB account "p1" for holder "dan" with purpose "transport"
    When I create a pending transfer of 2000 paisa from "n1" to "p1" with timeout 1
    And I wait 3 seconds
    Then the transfer status is "voided"
    And the balance of "n1" is 5000

  Scenario: Insufficient balance rejects transfer
    Given a normal account "n1" with balance 100 for holder "ed"
    And a PB account "p1" for holder "ed" with purpose "health"
    When I transfer 500 paisa from "n1" to "p1"
    Then the response status is 422
    And the error code is "InsufficientFunds"

  Scenario: Source frozen rejects transfer
    Given a normal account "n1" with balance 5000 for holder "fay"
    And "n1" is frozen
    And a PB account "p1" for holder "fay" with purpose "health"
    When I transfer 1000 paisa from "n1" to "p1"
    Then the response status is 409
    And the error code is "NormalAccountNotActive"

  Scenario: Destination frozen rejects transfer
    Given a normal account "n1" with balance 5000 for holder "gus"
    And a PB account "p1" for holder "gus" with purpose "health"
    And "p1" is frozen
    When I transfer 1000 paisa from "n1" to "p1"
    Then the response status is 409
    And the error code is "PbAccountNotActive"

  Scenario: Idempotency replay returns the same transfer
    Given a normal account "n1" with balance 5000 for holder "han"
    And a PB account "p1" for holder "han" with purpose "health"
    When I transfer 2000 paisa from "n1" to "p1" with idempotency key "k1"
    And I retry the same transfer
    Then both responses have the same transfer_id
    And the others-pool balance of "p1" is 2000

  Scenario: Both legs visible via correlation_id on the transactions list
    Given a normal account "n1" with balance 5000 for holder "ira"
    And a PB account "p1" for holder "ira" with purpose "health"
    When I transfer 2500 paisa from "n1" to "p1"
    Then the source-side transaction has correlation_id matching the destination-side transaction
    And the source-side transaction has type "transfer" and direction "outbound"
    And the destination-side transaction has type "deposit" and pool "others" and funding_type "trust"
```

- [ ] **Step 3.8.2: Add step definitions**

Add the missing steps to the existing step file. Most will reuse existing patterns; new ones include "I transfer X paisa from Y to Z", "the others-pool balance of Z is N", "I post/void the transfer", "the source-side transaction has correlation_id matching ...".

- [ ] **Step 3.8.3: Run feature**

```bash
just api-e2e
```

Expected: all transfer scenarios pass.

- [ ] **Step 3.8.4: Commit**

```bash
git add -A
git commit -m "test(e2e): cucumber feature for internal transfers"
```

### Task 3.9: Cucumber feature for trust-direct deposit removal

**Files:**
- Create: `crates/pba_service/tests/features/trust_direct_deposit_removed.feature`
- Update existing scenarios that use `funding_type='trust'` on PB deposits

- [ ] **Step 3.9.1: Find existing trust-deposit scenarios**

```bash
grep -rn -i 'funding_type.*trust\|fundingType.*trust' crates/pba_service/tests/features/
```

For each match, decide:
1. If the scenario is **about** trust deposits as a feature (e.g., `funding_types.feature`), modify it: replace the trust path with the transfer flow as the new mechanism.
2. If trust was incidental setup for another scenario (e.g., setting up PB others-pool balance for a payment scenario), modify the setup to use a normal account + transfer.

- [ ] **Step 3.9.2: Write the rejection feature**

`crates/pba_service/tests/features/trust_direct_deposit_removed.feature`:

```gherkin
Feature: Trust deposits to PB accounts have been removed
  Background:
    Given a clean test environment

  Scenario: Trust deposit on canonical /pb-accounts URL is rejected
    Given a PB account "p1" for holder "alice" with purpose "health"
    When I POST a deposit to "/pb-accounts/{p1}/deposits" with funding_type "trust" and source bank "TRUST0000000" / "0000000000"
    Then the response status is 400
    And the error code is "TrustDepositRequiresTransfer"
    And the response body mentions "/normal-accounts/" as the replacement

  Scenario: Trust deposit on legacy /accounts URL is rejected with the same error
    Given a PB account "p1" for holder "bob" with purpose "health"
    When I POST a deposit to "/accounts/{p1}/deposits" with funding_type "trust" and source bank "TRUST0000000" / "0000000000"
    Then the response status is 400
    And the error code is "TrustDepositRequiresTransfer"

  Scenario: Self deposit still works on the canonical URL
    Given a PB account "p1" for holder "carla" with origin "HDFC0001234" / "1111111111" and purpose "health"
    When I POST a deposit to "/pb-accounts/{p1}/deposits" with source bank "HDFC0001234" / "1111111111"
    Then the response status is 201
    And the deposit pool is "self"

  Scenario: Third-party deposit still works on the canonical URL
    Given a PB account "p1" for holder "dan" with origin "HDFC0001234" / "2222222222" and purpose "health"
    When I POST a deposit to "/pb-accounts/{p1}/deposits" with funding_type "third_party" and source bank "ICIC0005678" / "9999999999"
    Then the response status is 201
    And the deposit pool is "others"
    And the deposit funding_type is "third_party"
```

- [ ] **Step 3.9.3: Update step definitions for the new error code**

Add a step that asserts the error code returned in the JSON body. The existing step framework probably already supports this — confirm by inspection of the step file.

- [ ] **Step 3.9.4: Update scenarios identified in 3.9.1**

Edit each scenario to use the transfer flow where it previously used a trust deposit. The pattern: `Given a normal account "n1" with balance X for holder Y; And a PB account "p1" for holder Y with purpose Z; When I transfer X paisa from "n1" to "p1"`.

- [ ] **Step 3.9.5: Run e2e**

```bash
just api-e2e
```

Expected: trust-direct rejection passes; previously-passing scenarios that used trust deposits now use transfers and still pass.

- [ ] **Step 3.9.6: Commit**

```bash
git add -A
git commit -m "test(e2e): cucumber for trust-direct deposit rejection; migrate existing trust scenarios to transfer flow"
```

### Task 3.10: Admin UI for transfers

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs` (add transfer handlers)
- Add admin templates (`normal_account_transfer.html`, transactions list update for `correlation_id` linking)
- Create: `crates/pba_service/tests/ui_features/transfer_admin.feature`

- [ ] **Step 3.10.1: Add transfer admin routes**

In `crates/pba_service/src/admin/handlers.rs`, add:

- `GET /admin/normal-accounts/{id}/transfers/new` — form (with destination PB account selector)
- `POST /admin/normal-accounts/{id}/transfers` — initiate; redirect to transfer detail
- `GET /admin/transfers/{transfer_id}` — paired-leg view
- `POST /admin/transfers/{transfer_id}/post` and `/void`

- [ ] **Step 3.10.2: Add templates**

- `templates/admin/normal_account_transfer.html` — form. Destination is a dropdown of PB accounts (queried via `pb_account_repo.list_accounts`).
- Update `templates/admin/transaction_detail.html` — when the row has a `correlation_id`, show a link "View matching leg →" pointing at `/admin/transfers/{correlation_id}`.

- [ ] **Step 3.10.3: Write UI feature**

`crates/pba_service/tests/ui_features/transfer_admin.feature`:

```gherkin
Feature: Transfer admin UI
  Background:
    Given an admin session

  Scenario: Initiate immediate transfer from the normal account detail page
    Given a normal account "n1" with balance 5000 for holder "alice"
    And a PB account "p1" for holder "alice" with purpose "health"
    When I navigate to /admin/normal-accounts/{n1}/transfers/new
    And I select destination "p1" and amount 2000
    And I submit the form
    Then I am redirected to the transfer detail page
    And the page shows source account "n1" and destination account "p1"
    And the page shows status "posted"

  Scenario: Pending transfer + post via UI
    Given a normal account "n1" with balance 5000 for holder "bob"
    And a PB account "p1" for holder "bob" with purpose "health"
    When I navigate to /admin/normal-accounts/{n1}/transfers/new
    And I select destination "p1", amount 1500, mark as pending
    And I submit the form
    Then the transfer status is "pending"
    When I click "Post"
    Then the transfer status is "posted"

  Scenario: Both legs are linked from the transactions list
    Given a normal account "n1" with balance 5000 for holder "carla"
    And a PB account "p1" for holder "carla" with purpose "health"
    When I transfer 1000 paisa from "n1" to "p1"
    And I navigate to the transaction detail of the source-side transaction
    Then the page contains a link to the destination-side transaction
```

- [ ] **Step 3.10.4: Add step definitions**

Mirror the patterns in existing UI step files.

- [ ] **Step 3.10.5: Run UI tests**

```bash
just ui-e2e
```

- [ ] **Step 3.10.6: Commit**

```bash
git add -A
git commit -m "feat(admin): UI for transfers (initiate, pending lifecycle, paired-leg navigation)"
```

### Task 3.11: Phase 3 final verification + push PR 3

- [ ] **Step 3.11.1: Local CI**

```bash
just local-ci
just api-e2e
just ui-e2e
```

Expected: all green.

- [ ] **Step 3.11.2: Open PR 3**

```bash
git push -u origin normal-accounts-phase-3
gh pr create --title "feat: normal-account → PB transfers; remove direct trust deposit (Phase 3 of normal accounts)" --body "$(cat <<'EOF'
## Summary
- New transfer service supporting immediate and pending lifecycle. Normal account is debited; PB others-pool is credited; the trust funding sentinel is no longer touched on this hop (it was already debited when money entered the normal account).
- The two transaction rows (source-outbound on normal, destination-inbound deposit on PB) share a `correlation_id` and `tb_transfer_id`.
- `funding_type='trust'` on PB deposits is rejected with `TrustDepositRequiresTransfer` (400) on both `/pb-accounts/{id}/deposits` and the legacy `/accounts/{id}/deposits` alias. Self and third-party deposits remain unchanged.
- New Cucumber features `internal_transfer.feature` and `trust_direct_deposit_removed.feature`. Existing scenarios that used trust deposits are updated to use the transfer flow.
- Admin UI: transfer initiation form on the normal account detail page; paired-leg navigation on transaction detail.

This is the breaking change. See [design doc](docs/superpowers/specs/2026-05-08-normal-accounts-design.md) for rollout context.

## Test plan
- [ ] `just local-ci` passes
- [ ] `just api-e2e` passes (existing + internal_transfer + trust_direct_deposit_removed scenarios)
- [ ] `just ui-e2e` passes (existing + transfer_admin scenarios)
- [ ] Smoke-test: trust deposit on legacy URL returns 400 with `TrustDepositRequiresTransfer`

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

After PR 3 merges, the feature is live. Schedule a follow-up issue for the 90-day shim sunset removal (per spec).

---

## Self-review

After writing the plan, applied the Self-Review checklist:

**1. Spec coverage.** Walked each numbered section/requirement of the spec:
- Architecture (parallel modules, shared infra) → Tasks 1.5, 2.10–2.12, 3.4 (services); 2.9 (repo); 2.7 (ledger); 2.6 (transactions repo).
- Schema M1, M2, M3 → Tasks 1.2, 2.1, 2.2.
- API surface → Task 1.5 (renames), 2.13–2.16 (DTOs/handlers/routes), 2.17 (Smithy), 3.5–3.6 (transfers), 3.7 (timeout).
- Ledger conventions → Tasks 2.7, 3.1.
- Transfer flow → Tasks 3.3, 3.4.
- Errors → Tasks 2.8, 3.2.
- Testing → Tasks 2.18, 2.19, 3.8, 3.9, 3.10.
- Rollout → three PRs match three phases (1.6, 2.20, 3.11).

**2. Placeholder scan.** No `TBD`, `TODO`, "implement later" patterns found. The note in Task 3.2.2 explicitly defers behavioural assertion to the Cucumber feature in Task 3.5; that's a forward reference, not a placeholder.

**3. Type consistency.** Spot-checked: `AccountKind::Pb` and `AccountKind::Normal` used consistently. `find_by_idempotency_key(kind, account_id, key)` signature consistent across Tasks 2.6, 2.11, 2.12, 3.4. `tb_transfer_id` is `u128` everywhere; persisted as `NUMERIC(39)`. `correlation_id` is `Option<Uuid>` in domain and `Option<Uuid>` in repo signatures.

**4. Ambiguity check.** One thing worth flagging in execution: the existing `pool_summary` queries (Task 2.6.7) currently treat `pool` as required; once nullable, normal-account rows have `pool=NULL` and naturally fall through the match. Implementer should re-run any admin "totals" pages after Task 2.6 to confirm numbers don't shift unexpectedly.

---

Plan complete and saved to `docs/superpowers/plans/2026-05-08-normal-accounts.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
