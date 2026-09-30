# Contribution Return Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an admin-initiated `POST /pb-accounts/{id}/contribution-returns` operation that debits the PB others-pool and routes money back to each original contribution's source (sponsor normal account for `trust`, `THIRD_PARTY_FUNDING_SOURCE_TB_ID` for `third_party`), with two-phase (pending → post/void) support and FIFO allocation across multiple originals.

**Architecture:** New `pb_contribution_return_service` sibling to the existing PB services. Return rows are `TransactionType::Withdrawal` with `pool='others'` and `reverses_transaction_id` set — no new transaction type. The reservation model mirrors PR #42's refund: `sum_returns_of_in_tx` counts pending + settled, `find_returnable_originals_for_update` uses `SELECT ... FOR UPDATE`, `resolve_contribution_return` is transactional with same-direction idempotency.

**Tech Stack:** Rust + Axum + SQLx (PostgreSQL) + TigerBeetle. Cucumber-rs for BDD (API + browser via chromiumoxide). Smithy-generated `pba_client` SDK. Askama templates.

## Global Constraints

- File-per-module Rust style — no `mod.rs` directories.
- Conventional Commit titles on every commit.
- All amounts in paisa (`u64` in Rust, `BIGINT` in Postgres).
- `rustfmt` clean (`just fmt-check`) and `clippy` clean (`just lint`) on every commit.
- New pending rows populate `timeout_seconds`; defaults come from `PbContributionReturnService::default_pending_timeout_seconds`.
- No new TigerBeetle sentinels — reuse existing `THIRD_PARTY_FUNDING_SOURCE_TB_ID`.
- No schema migrations. All required columns and indexes already exist.
- Spec source of truth: `docs/superpowers/specs/2026-07-01-contribution-return-design.md`.

---

## Phase 1 — Foundation

### Task 1: Rename `sum_refunds_of` / `find_refunds_of` → `sum_returns_of` / `find_returns_of`

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs`
- Modify: `crates/pba_service/src/service/pb_payment_service.rs`
- Modify: `crates/pba_service/src/admin/handlers.rs`

**Interfaces:**
- Consumes: existing `sum_refunds_of`, `sum_refunds_of_in_tx`, `find_refunds_of` on `TransactionRepo`.
- Produces:
  - `pub async fn sum_returns_of(&self, original_row_id: Uuid) -> Result<u64, AppError>` — same behaviour, new name.
  - `pub async fn sum_returns_of_in_tx(&self, tx: &mut Transaction<'_, Postgres>, original_row_id: Uuid) -> Result<u64, AppError>` — same behaviour, new name.
  - `pub async fn find_returns_of(&self, original_row_id: Uuid) -> Result<Vec<TransactionRecord>, AppError>` — same behaviour, new name.

- [ ] **Step 1: Rename in `transaction_repo.rs`**

Rename the three functions in place. The SQL bodies stay identical.

```bash
grep -n "fn sum_refunds_of\|fn find_refunds_of" crates/pba_service/src/repository/transaction_repo.rs
```

Change:
- `pub async fn sum_refunds_of` → `pub async fn sum_returns_of`
- `pub async fn sum_refunds_of_in_tx` → `pub async fn sum_returns_of_in_tx`
- `pub async fn find_refunds_of` → `pub async fn find_returns_of`

Update any doc comments on these methods to say "return rows" instead of "refund rows".

- [ ] **Step 2: Update call-sites in `pb_payment_service.rs`**

Run `grep -n "sum_refunds_of\|find_refunds_of" crates/pba_service/src/service/pb_payment_service.rs` and replace each occurrence with the new name. Do NOT change any other logic.

- [ ] **Step 3: Update call-sites in `admin/handlers.rs`**

Run `grep -n "sum_refunds_of\|find_refunds_of" crates/pba_service/src/admin/handlers.rs` and replace each occurrence.

- [ ] **Step 4: Verify nothing else calls the old names**

```bash
grep -rn "sum_refunds_of\|find_refunds_of" crates/pba_service/src crates/pba_service/tests
```

Expected: no matches remain. If any test-side references exist (from step bindings referring to the repo directly — unlikely), update them too.

- [ ] **Step 5: Compile + run existing refund e2e**

```bash
cargo check -p pba-service
PBA_SERVICE_URL=http://127.0.0.1:3031 cargo test -p pba-service --test e2e -- payment_refund
```

Expected: refund scenarios pass. The rename is behaviour-preserving.

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/src/repository/transaction_repo.rs \
        crates/pba_service/src/service/pb_payment_service.rs \
        crates/pba_service/src/admin/handlers.rs
git commit -m "refactor(repo): rename sum_refunds_of/find_refunds_of to sum_returns_of/find_returns_of"
```

---

### Task 2: Add new repo reads (`find_returnable_originals_for_update`, `sum_others_contributions`, `sum_others_returns`)

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs`

**Interfaces:**
- Consumes: existing `TransactionRecord` shape, `TransactionRow` deserializer, `self.pool` (pgpool).
- Produces:
  - `pub async fn find_returnable_originals_for_update(&self, tx: &mut Transaction<'_, Postgres>, pb_account_id: Uuid, funding_type: &str) -> Result<Vec<TransactionRecord>, AppError>` — FIFO-ordered inbound others-pool contributions with row lock held.
  - `pub async fn sum_others_contributions(&self, pb_account_id: Uuid, funding_type: &str) -> Result<u64, AppError>` — aggregate of active inbound others-pool contributions of a given funding_type.
  - `pub async fn sum_others_returns(&self, pb_account_id: Uuid, funding_type: &str) -> Result<u64, AppError>` — aggregate of pending + settled withdrawal rows against those contributions.

- [ ] **Step 1: Add `find_returnable_originals_for_update`**

After `sum_returns_of_in_tx`, add:

```rust
/// Fetch inbound others-pool contribution rows of the given funding_type
/// that are candidates for return. FIFO-ordered by created_at. Holds a row
/// lock inside `tx` via SELECT ... FOR UPDATE.
pub async fn find_returnable_originals_for_update(
    &self,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<Vec<TransactionRecord>, AppError> {
    let rows = sqlx::query_as::<_, TransactionRow>(
        r#"
        SELECT id, account_id, account_kind, type, status, amount, pool, direction,
               source_ifsc, source_account, gateway_ref, timeout_seconds,
               merchant_id, merchant_mcc, description, funding_type,
               tb_transfer_id::text as tb_transfer_id, idempotency_key,
               correlation_id, reverses_transaction_id, created_at, updated_at
        FROM transactions
        WHERE account_id = $1
          AND account_kind = 'pb'
          AND pool = 'others'
          AND funding_type = $2
          AND direction = 'inbound'
          AND status IN ('posted', 'settled')
          AND reverses_transaction_id IS NULL
        ORDER BY created_at ASC
        FOR UPDATE
        "#,
    )
    .bind(pb_account_id)
    .bind(funding_type)
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows.into_iter().map(|r| r.into_domain()).collect())
}
```

- [ ] **Step 2: Add `sum_others_contributions`**

```rust
/// Sum of active inbound others-pool contributions of a given funding_type.
/// Excludes voided originals and rows that are themselves returns.
pub async fn sum_others_contributions(
    &self,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<u64, AppError> {
    let row: (Option<i64>,) = sqlx::query_as(
        r#"
        SELECT COALESCE(SUM(amount), 0)::bigint
        FROM transactions
        WHERE account_id = $1
          AND account_kind = 'pb'
          AND pool = 'others'
          AND funding_type = $2
          AND direction = 'inbound'
          AND status IN ('posted', 'settled')
          AND reverses_transaction_id IS NULL
        "#,
    )
    .bind(pb_account_id)
    .bind(funding_type)
    .fetch_one(&self.pool)
    .await?;

    Ok(row.0.unwrap_or(0) as u64)
}
```

- [ ] **Step 3: Add `sum_others_returns`**

```rust
/// Sum of pending + settled Withdrawal rows in the others-pool of a given
/// funding_type. Counts reservations against the contribution budget.
pub async fn sum_others_returns(
    &self,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<u64, AppError> {
    let row: (Option<i64>,) = sqlx::query_as(
        r#"
        SELECT COALESCE(SUM(amount), 0)::bigint
        FROM transactions
        WHERE account_id = $1
          AND account_kind = 'pb'
          AND pool = 'others'
          AND funding_type = $2
          AND type = 'withdrawal'
          AND status IN ('pending', 'settled')
        "#,
    )
    .bind(pb_account_id)
    .bind(funding_type)
    .fetch_one(&self.pool)
    .await?;

    Ok(row.0.unwrap_or(0) as u64)
}
```

- [ ] **Step 4: Compile**

```bash
cargo check -p pba-service
```

Expected: clean. `#[allow(dead_code)]` warnings may appear on the three new methods since no service uses them yet — accept them; Tasks 5/7 will wire them in.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/repository/transaction_repo.rs
git commit -m "feat(repo): FIFO returnable originals + others-pool aggregates"
```

---

### Task 3: Ledger helpers `create_contribution_return` + `create_pending_contribution_return`

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs`

**Interfaces:**
- Produces:
  - `pub const CONTRIBUTION_RETURN_CODE: u16 = 310;`
  - `pub async fn create_contribution_return(&self, debit_pb_others_tb_id: u128, credit_destination_tb_id: u128, amount: u64) -> Result<(), AppError>` — immediate TB transfer.
  - `pub async fn create_pending_contribution_return(&self, debit_pb_others_tb_id: u128, credit_destination_tb_id: u128, amount: u64, timeout_seconds: u32) -> Result<u128, AppError>` — pending TB transfer, returns tb_transfer_id.

- [ ] **Step 1: Add constant + helpers**

After the existing pending-payment-refund helpers in `ledger_repo.rs`:

```rust
pub const CONTRIBUTION_RETURN_CODE: u16 = 310;

/// Immediate contribution-return TB transfer. Debits the PB others-pool
/// and credits the destination (a sponsor normal account for `trust`,
/// or `THIRD_PARTY_FUNDING_SOURCE_TB_ID` for `third_party`).
pub async fn create_contribution_return(
    &self,
    debit_pb_others_tb_id: u128,
    credit_destination_tb_id: u128,
    amount: u64,
) -> Result<(), AppError> {
    self.create_transfer(
        debit_pb_others_tb_id,
        credit_destination_tb_id,
        amount,
        CONTRIBUTION_RETURN_CODE,
    )
    .await
}

/// Pending contribution-return TB transfer. Returns the TB transfer id for
/// the service to persist on the DB row so later post/void can resolve it.
pub async fn create_pending_contribution_return(
    &self,
    debit_pb_others_tb_id: u128,
    credit_destination_tb_id: u128,
    amount: u64,
    timeout_seconds: u32,
) -> Result<u128, AppError> {
    self.create_pending_transfer(
        debit_pb_others_tb_id,
        credit_destination_tb_id,
        amount,
        CONTRIBUTION_RETURN_CODE,
        timeout_seconds,
    )
    .await
}
```

- [ ] **Step 2: Compile**

```bash
cargo check -p pba-service
```

Expected: clean. Dead-code warnings on the new helpers are fine — Task 5 uses them.

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/src/repository/ledger_repo.rs
git commit -m "feat(ledger): contribution return TB helpers (code 310)"
```

---

### Task 4: Add error variants `ContributionAmountInvalid`, `ContributionFullyReturned`

**Files:**
- Modify: `crates/pba_service/src/error.rs`

**Interfaces:**
- Produces:
  - `AppError::ContributionAmountInvalid { requested: u64, remaining: u64 }` → 400, kind `"ContributionAmountInvalid"`.
  - `AppError::ContributionFullyReturned(String)` → 409, kind `"ContributionFullyReturned"`. The `String` is the PB account id for the log message.

- [ ] **Step 1: Add variants**

Locate the `pub enum AppError` block. After the `RefundAmountInvalid` and `PaymentFullyRefunded` variants, add:

```rust
ContributionAmountInvalid {
    requested: u64,
    remaining: u64,
},
ContributionFullyReturned(String),
```

- [ ] **Step 2: Add `Display` impl arms**

In the `impl std::fmt::Display for AppError` block, add:

```rust
AppError::ContributionAmountInvalid { requested, remaining } => {
    write!(
        f,
        "contribution return amount invalid: requested {requested}, remaining {remaining}"
    )
}
AppError::ContributionFullyReturned(account_id) => {
    write!(
        f,
        "contribution fully returned for pb_account {account_id}"
    )
}
```

- [ ] **Step 3: Add HTTP status + kind mapping in `IntoResponse`**

In the `match self` block of `IntoResponse for AppError` (near where `RefundAmountInvalid` and `PaymentFullyRefunded` are mapped), add:

```rust
AppError::ContributionAmountInvalid { .. } => {
    (StatusCode::BAD_REQUEST, "ContributionAmountInvalid")
}
AppError::ContributionFullyReturned(_) => {
    (StatusCode::CONFLICT, "ContributionFullyReturned")
}
```

- [ ] **Step 4: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/error.rs
git commit -m "feat(error): ContributionAmountInvalid and ContributionFullyReturned variants"
```

---

## Phase 2 — Service

### Task 5: `pb_contribution_return_service::return_contribution`

**Files:**
- Create: `crates/pba_service/src/service/pb_contribution_return_service.rs`
- Modify: `crates/pba_service/src/service.rs` (module declaration)

**Interfaces:**
- Consumes:
  - `TransactionRepo::find_returnable_originals_for_update` (Task 2)
  - `TransactionRepo::sum_returns_of_in_tx` (Task 1 renamed)
  - `TransactionRepo::find_by_correlation_id` (existing)
  - `TransactionRepo::find_by_idempotency_key` (existing)
  - `TransactionRepo::insert_in_tx` (existing)
  - `PbAccountRepo::get_account` (existing)
  - `NormalAccountRepo::get_account` (existing)
  - `LedgerRepo::create_contribution_return` and `create_pending_contribution_return` (Task 3)
  - `AppError::ContributionAmountInvalid`, `AppError::ContributionFullyReturned` (Task 4)
- Produces:
  - Struct `PbContributionReturnService`.
  - `pub async fn return_contribution(&self, pb_account_id, amount, funding_type, pending, timeout_seconds, gateway_ref, description, idempotency_key) -> Result<ContributionReturnResult, AppError>`.
  - Struct `ContributionReturnResult { return_id, original_payment_id, account_id, funding_type, amount, allocations, original_amount, remaining_returnable_after, status, correlation_id, created_at }`.
  - Struct `AllocationEntry { original_transaction_id: Uuid, amount: u64 }`.

- [ ] **Step 1: Create the file with the service struct + `new`**

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, THIRD_PARTY_FUNDING_SOURCE_TB_ID};
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

pub struct PbContributionReturnService {
    pub pb_account_repo: Arc<PbAccountRepo>,
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub default_pending_timeout_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct AllocationEntry {
    pub original_transaction_id: Uuid,
    pub amount: u64,
}

#[derive(Debug, Clone)]
pub struct ContributionReturnResult {
    pub return_id: Uuid,
    pub account_id: Uuid,
    pub funding_type: String,
    pub amount: u64,
    pub allocations: Vec<AllocationEntry>,
    pub remaining_returnable_after: u64,
    pub status: TransactionStatus,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl PbContributionReturnService {
    pub fn new(
        pb_account_repo: Arc<PbAccountRepo>,
        normal_account_repo: Arc<NormalAccountRepo>,
        transaction_repo: Arc<TransactionRepo>,
        ledger_repo: Arc<LedgerRepo>,
        default_pending_timeout_seconds: u32,
    ) -> Self {
        Self {
            pb_account_repo,
            normal_account_repo,
            transaction_repo,
            ledger_repo,
            default_pending_timeout_seconds,
        }
    }
}
```

- [ ] **Step 2: Add module declaration**

In `crates/pba_service/src/service.rs`, add the line (order alphabetically):

```rust
pub mod pb_contribution_return_service;
```

- [ ] **Step 3: Add `return_contribution` (idempotency replay path)**

Inside `impl PbContributionReturnService`, add:

```rust
#[allow(clippy::too_many_arguments)]
pub async fn return_contribution(
    &self,
    pb_account_id: Uuid,
    amount: u64,
    funding_type: &str,
    pending: bool,
    timeout_seconds: Option<u32>,
    gateway_ref: Option<&str>,
    description: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<ContributionReturnResult, AppError> {
    // Step 1: idempotency replay.
    if let Some(key) = idempotency_key {
        if let Some(existing) = self
            .transaction_repo
            .find_by_idempotency_key(AccountKind::Pb, pb_account_id, key)
            .await?
        {
            let correlation_id = existing.correlation_id.unwrap_or(existing.id);
            let rows = self
                .transaction_repo
                .find_by_correlation_id(correlation_id)
                .await?;
            let total_amount: u64 = rows.iter().map(|r| r.amount).sum();
            let allocations = rows
                .iter()
                .map(|r| AllocationEntry {
                    original_transaction_id: r
                        .reverses_transaction_id
                        .expect("return row missing reverses_transaction_id"),
                    amount: r.amount,
                })
                .collect();
            let remaining_returnable_after = self.compute_remaining(pb_account_id, funding_type).await?;
            return Ok(ContributionReturnResult {
                return_id: correlation_id,
                account_id: pb_account_id,
                funding_type: funding_type.to_string(),
                amount: total_amount,
                allocations,
                remaining_returnable_after,
                status: existing.status,
                correlation_id,
                created_at: existing.created_at,
            });
        }
    }

    // Continued in Step 4 below.
    self.execute_return(
        pb_account_id,
        amount,
        funding_type,
        pending,
        timeout_seconds,
        gateway_ref,
        description,
        idempotency_key,
    )
    .await
}

async fn compute_remaining(
    &self,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<u64, AppError> {
    let contributed = self
        .transaction_repo
        .sum_others_contributions(pb_account_id, funding_type)
        .await?;
    let returned = self
        .transaction_repo
        .sum_others_returns(pb_account_id, funding_type)
        .await?;
    Ok(contributed.saturating_sub(returned))
}
```

- [ ] **Step 4: Add `execute_return` (main path with FIFO + TB routing)**

```rust
#[allow(clippy::too_many_arguments)]
async fn execute_return(
    &self,
    pb_account_id: Uuid,
    amount: u64,
    funding_type: &str,
    pending: bool,
    timeout_seconds: Option<u32>,
    gateway_ref: Option<&str>,
    description: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<ContributionReturnResult, AppError> {
    if funding_type != "trust" && funding_type != "third_party" {
        return Err(AppError::Validation(format!(
            "funding_type must be 'trust' or 'third_party', got {funding_type:?}"
        )));
    }

    // Step 2: begin DB tx.
    let mut tx = self.transaction_repo.pool().begin().await?;

    // Step 3: find candidate originals under a row lock.
    let originals = self
        .transaction_repo
        .find_returnable_originals_for_update(&mut tx, pb_account_id, funding_type)
        .await?;

    // Step 4: compute per-original remaining.
    let mut candidates: Vec<(TransactionRecord, u64)> = Vec::with_capacity(originals.len());
    for o in originals.into_iter() {
        let already_returned = self
            .transaction_repo
            .sum_returns_of_in_tx(&mut tx, o.id)
            .await?;
        let remaining = o.amount.saturating_sub(already_returned);
        if remaining > 0 {
            candidates.push((o, remaining));
        }
    }
    let total_available: u64 = candidates.iter().map(|(_, r)| *r).sum();

    // Step 5: validate.
    if total_available == 0 {
        return Err(AppError::ContributionFullyReturned(pb_account_id.to_string()));
    }
    if amount == 0 || amount > total_available {
        return Err(AppError::ContributionAmountInvalid {
            requested: amount,
            remaining: total_available,
        });
    }

    // Step 6: PB account active check.
    let pb_account = self.pb_account_repo.get_account(pb_account_id).await?;
    if !pb_account.status.is_active() {
        return Err(AppError::PbAccountNotActive(pb_account_id.to_string()));
    }

    // Step 7: FIFO allocation.
    let mut allocations_raw: Vec<(TransactionRecord, u64)> = Vec::new();
    let mut amount_left = amount;
    for (original, remaining) in candidates.into_iter() {
        if amount_left == 0 {
            break;
        }
        let take = amount_left.min(remaining);
        allocations_raw.push((original, take));
        amount_left -= take;
    }
    debug_assert_eq!(amount_left, 0);

    let row_status = if pending {
        TransactionStatus::Pending
    } else {
        TransactionStatus::Settled
    };
    let timeout = if pending {
        Some(timeout_seconds.unwrap_or(self.default_pending_timeout_seconds))
    } else {
        None
    };

    let return_correlation_id = Uuid::now_v7();

    // Step 8: insert one Withdrawal row per allocation.
    let mut row_ids: Vec<Uuid> = Vec::with_capacity(allocations_raw.len());
    for (idx, (original, take)) in allocations_raw.iter().enumerate() {
        // First row's id == correlation_id (mirrors make_payment / refund pattern).
        let row_id = if idx == 0 {
            return_correlation_id
        } else {
            Uuid::now_v7()
        };
        row_ids.push(row_id);
        let idem = if idx == 0 { idempotency_key } else { None };
        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                row_id,
                pb_account_id,
                AccountKind::Pb,
                TransactionType::Withdrawal,
                row_status,
                *take,
                Some("others"),
                TransactionDirection::Outbound,
                None,
                None,
                gateway_ref,
                timeout,
                None,
                None,
                description,
                Some(funding_type),
                0,
                idem,
                Some(return_correlation_id),
                Some(original.id),
            )
            .await?;
    }

    // Step 9: TB transfers, one per allocation. Persist returned tb_transfer_id when pending.
    for (idx, (original, take)) in allocations_raw.iter().enumerate() {
        let credit_destination_tb_id = self
            .resolve_credit_destination(original, funding_type)
            .await?;
        if pending {
            let tb_id = self
                .ledger_repo
                .create_pending_contribution_return(
                    pb_account.tb_others_account_id,
                    credit_destination_tb_id,
                    *take,
                    timeout.expect("timeout populated when pending=true"),
                )
                .await?;
            let row_id = row_ids[idx];
            sqlx::query(
                r#"UPDATE transactions
                   SET tb_transfer_id = $1::numeric, updated_at = now()
                   WHERE id = $2"#,
            )
            .bind(tb_id.to_string())
            .bind(row_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
        } else {
            self.ledger_repo
                .create_contribution_return(
                    pb_account.tb_others_account_id,
                    credit_destination_tb_id,
                    *take,
                )
                .await?;
        }
    }

    // Step 10: commit.
    tx.commit().await?;

    // Step 11: build result.
    let allocations = allocations_raw
        .iter()
        .map(|(o, take)| AllocationEntry {
            original_transaction_id: o.id,
            amount: *take,
        })
        .collect();
    let remaining_returnable_after = total_available - amount;

    Ok(ContributionReturnResult {
        return_id: return_correlation_id,
        account_id: pb_account_id,
        funding_type: funding_type.to_string(),
        amount,
        allocations,
        remaining_returnable_after,
        status: row_status,
        correlation_id: return_correlation_id,
        created_at: chrono::Utc::now(),
    })
}

async fn resolve_credit_destination(
    &self,
    original: &TransactionRecord,
    funding_type: &str,
) -> Result<u128, AppError> {
    if funding_type == "third_party" {
        return Ok(THIRD_PARTY_FUNDING_SOURCE_TB_ID);
    }
    // 'trust': look up the normal-side leg of the original transfer.
    let correlation_id = original.correlation_id.ok_or_else(|| {
        AppError::DatabaseError(
            "trust contribution original missing correlation_id".to_string(),
        )
    })?;
    let legs = self
        .transaction_repo
        .find_by_correlation_id(correlation_id)
        .await?;
    let normal_leg = legs
        .iter()
        .find(|l| l.account_kind == AccountKind::Normal)
        .ok_or_else(|| {
            AppError::DatabaseError(
                "trust contribution original missing normal leg".to_string(),
            )
        })?;
    let normal_account = self
        .normal_account_repo
        .get_account(normal_leg.account_id)
        .await?;
    Ok(normal_account.tb_account_id)
}
```

- [ ] **Step 5: Compile**

```bash
cargo check -p pba-service
```

Expected: clean. If `TransactionRepo::pool()` isn't public, expose it (`pub` on that method) — the refund service already uses this pattern.

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/src/service/pb_contribution_return_service.rs \
        crates/pba_service/src/service.rs
git commit -m "feat(service): return_contribution with FIFO allocation and pending path"
```

---

### Task 6: `resolve_contribution_return` + public `post_contribution_return` / `void_contribution_return`

**Files:**
- Modify: `crates/pba_service/src/service/pb_contribution_return_service.rs`

**Interfaces:**
- Consumes:
  - `TransactionRepo::find_by_correlation_id_for_update` (existing from PR #42)
  - `TransactionRepo::find_by_correlation_id` (existing)
  - `LedgerRepo::post_pending_transfer`, `void_pending_transfer` (existing)
  - `AppError::TbPendingAlreadyResolved`, `AppError::TransactionNotFound`, `AppError::TransactionNotPending` (existing)
- Produces:
  - `pub async fn post_contribution_return(&self, pb_account_id, return_id) -> Result<ContributionReturnResult, AppError>`.
  - `pub async fn void_contribution_return(&self, pb_account_id, return_id) -> Result<ContributionReturnResult, AppError>`.

- [ ] **Step 1: Add `ContributionReturnResolution` enum and helpers**

In `pb_contribution_return_service.rs` above `impl PbContributionReturnService`:

```rust
enum ContributionReturnResolution {
    Post,
    Void,
}

impl ContributionReturnResolution {
    fn target(&self) -> TransactionStatus {
        match self {
            Self::Post => TransactionStatus::Settled,
            Self::Void => TransactionStatus::Voided,
        }
    }

    fn target_sql(&self) -> &'static str {
        match self {
            Self::Post => "settled",
            Self::Void => "voided",
        }
    }
}
```

- [ ] **Step 2: Add `resolve_contribution_return` inside `impl PbContributionReturnService`**

```rust
async fn resolve_contribution_return(
    &self,
    pb_account_id: Uuid,
    return_id: Uuid,
    direction: ContributionReturnResolution,
) -> Result<ContributionReturnResult, AppError> {
    let mut tx = self.transaction_repo.pool().begin().await?;
    let rows = self
        .transaction_repo
        .find_by_correlation_id_for_update(&mut tx, return_id)
        .await?;
    if rows.is_empty() {
        return Err(AppError::TransactionNotFound(return_id.to_string()));
    }
    for r in &rows {
        if r.account_kind != AccountKind::Pb
            || r.account_id != pb_account_id
            || r.transaction_type != TransactionType::Withdrawal
            || r.pool.as_deref() != Some("others")
            || r.reverses_transaction_id.is_none()
        {
            return Err(AppError::TransactionNotFound(return_id.to_string()));
        }
    }

    // Idempotent same-direction no-op
    if rows.iter().all(|r| r.status == direction.target()) {
        tx.commit().await?;
        let updated = self
            .transaction_repo
            .find_by_correlation_id(return_id)
            .await?;
        return self.build_result_from_rows(pb_account_id, return_id, &updated).await;
    }
    if rows.iter().any(|r| r.status != TransactionStatus::Pending) {
        return Err(AppError::TransactionNotPending(return_id.to_string()));
    }

    for r in &rows {
        if r.tb_transfer_id != 0 {
            let res = match direction {
                ContributionReturnResolution::Post => {
                    self.ledger_repo.post_pending_transfer(r.tb_transfer_id).await
                }
                ContributionReturnResolution::Void => {
                    self.ledger_repo.void_pending_transfer(r.tb_transfer_id).await
                }
            };
            match res {
                Ok(()) => {}
                Err(AppError::TbPendingAlreadyResolved) => {}
                Err(e) => return Err(e),
            }
        }
    }

    sqlx::query(
        r#"UPDATE transactions
           SET status = $1, updated_at = now()
           WHERE correlation_id = $2 AND status = 'pending'"#,
    )
    .bind(direction.target_sql())
    .bind(return_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    tx.commit().await?;

    let updated = self
        .transaction_repo
        .find_by_correlation_id(return_id)
        .await?;
    self.build_result_from_rows(pb_account_id, return_id, &updated).await
}

async fn build_result_from_rows(
    &self,
    pb_account_id: Uuid,
    return_id: Uuid,
    rows: &[TransactionRecord],
) -> Result<ContributionReturnResult, AppError> {
    let total: u64 = rows.iter().map(|r| r.amount).sum();
    let allocations = rows
        .iter()
        .map(|r| AllocationEntry {
            original_transaction_id: r
                .reverses_transaction_id
                .expect("return row missing reverses_transaction_id"),
            amount: r.amount,
        })
        .collect();
    let funding_type = rows
        .first()
        .and_then(|r| r.funding_type.clone())
        .unwrap_or_else(|| "trust".to_string());
    let remaining_returnable_after = self.compute_remaining(pb_account_id, &funding_type).await?;
    let status = rows
        .first()
        .map(|r| r.status)
        .unwrap_or(TransactionStatus::Settled);
    let created_at = rows
        .first()
        .map(|r| r.created_at)
        .unwrap_or_else(chrono::Utc::now);
    Ok(ContributionReturnResult {
        return_id,
        account_id: pb_account_id,
        funding_type,
        amount: total,
        allocations,
        remaining_returnable_after,
        status,
        correlation_id: return_id,
        created_at,
    })
}
```

- [ ] **Step 3: Add public wrappers**

```rust
pub async fn post_contribution_return(
    &self,
    pb_account_id: Uuid,
    return_id: Uuid,
) -> Result<ContributionReturnResult, AppError> {
    self.resolve_contribution_return(pb_account_id, return_id, ContributionReturnResolution::Post)
        .await
}

pub async fn void_contribution_return(
    &self,
    pb_account_id: Uuid,
    return_id: Uuid,
) -> Result<ContributionReturnResult, AppError> {
    self.resolve_contribution_return(pb_account_id, return_id, ContributionReturnResolution::Void)
        .await
}
```

- [ ] **Step 4: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/service/pb_contribution_return_service.rs
git commit -m "feat(service): post_contribution_return + void_contribution_return"
```

---

### Task 7: `summary` service method + supporting DTO struct

**Files:**
- Modify: `crates/pba_service/src/service/pb_contribution_return_service.rs`

**Interfaces:**
- Consumes: `TransactionRepo::sum_others_contributions` and `sum_others_returns` (Task 2).
- Produces:
  - Struct `FundingTypeSummary { total_contributed: u64, total_returned: u64, remaining_returnable: u64 }`.
  - Struct `ContributionSummary { trust: FundingTypeSummary, third_party: FundingTypeSummary }`.
  - `pub async fn summary(&self, pb_account_id: Uuid) -> Result<ContributionSummary, AppError>`.

- [ ] **Step 1: Add DTO structs at the top of the service file**

```rust
#[derive(Debug, Clone)]
pub struct FundingTypeSummary {
    pub total_contributed: u64,
    pub total_returned: u64,
    pub remaining_returnable: u64,
}

#[derive(Debug, Clone)]
pub struct ContributionSummary {
    pub trust: FundingTypeSummary,
    pub third_party: FundingTypeSummary,
}
```

- [ ] **Step 2: Add `summary` inside `impl PbContributionReturnService`**

```rust
pub async fn summary(
    &self,
    pb_account_id: Uuid,
) -> Result<ContributionSummary, AppError> {
    let trust = self.summary_for(pb_account_id, "trust").await?;
    let third_party = self.summary_for(pb_account_id, "third_party").await?;
    Ok(ContributionSummary { trust, third_party })
}

async fn summary_for(
    &self,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<FundingTypeSummary, AppError> {
    let total_contributed = self
        .transaction_repo
        .sum_others_contributions(pb_account_id, funding_type)
        .await?;
    let total_returned = self
        .transaction_repo
        .sum_others_returns(pb_account_id, funding_type)
        .await?;
    let remaining_returnable = total_contributed.saturating_sub(total_returned);
    Ok(FundingTypeSummary {
        total_contributed,
        total_returned,
        remaining_returnable,
    })
}
```

- [ ] **Step 3: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/service/pb_contribution_return_service.rs
git commit -m "feat(service): contribution summary read"
```

---

## Phase 3 — API layer

### Task 8: REST DTOs

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`

**Interfaces:**
- Produces:
  - `pub struct ContributionReturnRequest { amount, funding_type, pending, timeout_seconds, gateway_ref, description, idempotency_key }`.
  - `pub struct AllocationEntryDto { original_transaction_id, amount }` with `From<AllocationEntry>` conversion.
  - `pub struct ContributionReturnResponse { return_id, correlation_id, account_id, funding_type, amount, allocations, remaining_returnable_after, status, created_at }` with `From<ContributionReturnResult>` conversion.
  - `pub struct FundingTypeSummaryDto` and `pub struct ContributionSummaryResponse` with conversions from the service types.

- [ ] **Step 1: Add request/response DTOs**

Near the existing `RefundPaymentRequest` block:

```rust
#[derive(Debug, Deserialize)]
pub struct ContributionReturnRequest {
    pub amount: u64,
    pub funding_type: String,
    #[serde(default)]
    pub pending: bool,
    pub timeout_seconds: Option<u32>,
    pub gateway_ref: Option<String>,
    pub description: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AllocationEntryDto {
    pub original_transaction_id: Uuid,
    pub amount: u64,
}

impl From<crate::service::pb_contribution_return_service::AllocationEntry> for AllocationEntryDto {
    fn from(a: crate::service::pb_contribution_return_service::AllocationEntry) -> Self {
        Self {
            original_transaction_id: a.original_transaction_id,
            amount: a.amount,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContributionReturnResponse {
    pub return_id: Uuid,
    pub correlation_id: Uuid,
    pub account_id: Uuid,
    pub funding_type: String,
    pub amount: u64,
    pub allocations: Vec<AllocationEntryDto>,
    pub remaining_returnable_after: u64,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::pb_contribution_return_service::ContributionReturnResult>
    for ContributionReturnResponse
{
    fn from(r: crate::service::pb_contribution_return_service::ContributionReturnResult) -> Self {
        Self {
            return_id: r.return_id,
            correlation_id: r.correlation_id,
            account_id: r.account_id,
            funding_type: r.funding_type,
            amount: r.amount,
            allocations: r.allocations.into_iter().map(Into::into).collect(),
            remaining_returnable_after: r.remaining_returnable_after,
            status: r.status.as_str().to_string(),
            created_at: r.created_at,
        }
    }
}
```

- [ ] **Step 2: Add summary DTOs**

```rust
#[derive(Debug, Serialize)]
pub struct FundingTypeSummaryDto {
    pub total_contributed: u64,
    pub total_returned: u64,
    pub remaining_returnable: u64,
}

impl From<crate::service::pb_contribution_return_service::FundingTypeSummary>
    for FundingTypeSummaryDto
{
    fn from(s: crate::service::pb_contribution_return_service::FundingTypeSummary) -> Self {
        Self {
            total_contributed: s.total_contributed,
            total_returned: s.total_returned,
            remaining_returnable: s.remaining_returnable,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContributionSummaryResponse {
    pub trust: FundingTypeSummaryDto,
    pub third_party: FundingTypeSummaryDto,
}

impl From<crate::service::pb_contribution_return_service::ContributionSummary>
    for ContributionSummaryResponse
{
    fn from(s: crate::service::pb_contribution_return_service::ContributionSummary) -> Self {
        Self {
            trust: s.trust.into(),
            third_party: s.third_party.into(),
        }
    }
}
```

- [ ] **Step 3: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/api/dto.rs
git commit -m "feat(api): contribution return + summary DTOs"
```

---

### Task 9: REST handlers + routes

**Files:**
- Modify: `crates/pba_service/src/api/handlers/pb.rs`
- Modify: `crates/pba_service/src/api/routes.rs`
- Modify: `crates/pba_service/src/main.rs` (AppState wiring + service construction)
- Modify: `crates/pba_service/src/lib.rs` OR wherever AppState is defined

**Interfaces:**
- Produces:
  - `pub async fn return_contribution(...) -> Result<(StatusCode, Json<ContributionReturnResponse>), AppError>` handler.
  - `pub async fn post_contribution_return(...) -> Result<(StatusCode, Json<ContributionReturnResponse>), AppError>` handler.
  - `pub async fn void_contribution_return(...) -> Result<(StatusCode, Json<ContributionReturnResponse>), AppError>` handler.
  - `pub async fn get_contribution_summary(...) -> Result<(StatusCode, Json<ContributionSummaryResponse>), AppError>` handler.
  - Routes:
    - `POST /pb-accounts/{account_id}/contribution-returns`
    - `POST /pb-accounts/{account_id}/contribution-returns/{return_id}/post`
    - `POST /pb-accounts/{account_id}/contribution-returns/{return_id}/void`
    - `GET /pb-accounts/{account_id}/contributions/summary`

- [ ] **Step 1: Wire the service into AppState**

Find the `AppState` struct (search with `grep -n "pub struct AppState\|pb_payment_service:" crates/pba_service/src/`). Add:

```rust
pub pb_contribution_return_service: Arc<PbContributionReturnService>,
```

Add the import at the top:
```rust
use crate::service::pb_contribution_return_service::PbContributionReturnService;
```

In `main.rs` where other PB services are constructed (search for `PbPaymentService::new`), add:

```rust
let pb_contribution_return_service = Arc::new(PbContributionReturnService::new(
    Arc::clone(&pb_account_repo),
    Arc::clone(&normal_account_repo),
    Arc::clone(&transaction_repo),
    Arc::clone(&ledger_repo),
    config.default_pending_timeout_seconds,
));
```

Include it in the AppState struct literal.

- [ ] **Step 2: Add handlers to `api/handlers/pb.rs`**

```rust
pub async fn return_contribution(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<ContributionReturnRequest>,
) -> Result<(StatusCode, Json<ContributionReturnResponse>), AppError> {
    let result = state
        .pb_contribution_return_service
        .return_contribution(
            account_id,
            req.amount,
            &req.funding_type,
            req.pending,
            req.timeout_seconds,
            req.gateway_ref.as_deref(),
            req.description.as_deref(),
            req.idempotency_key.as_deref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(result.into())))
}

pub async fn post_contribution_return(
    State(state): State<AppState>,
    Path((account_id, return_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<ContributionReturnResponse>), AppError> {
    let result = state
        .pb_contribution_return_service
        .post_contribution_return(account_id, return_id)
        .await?;
    Ok((StatusCode::OK, Json(result.into())))
}

pub async fn void_contribution_return(
    State(state): State<AppState>,
    Path((account_id, return_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<ContributionReturnResponse>), AppError> {
    let result = state
        .pb_contribution_return_service
        .void_contribution_return(account_id, return_id)
        .await?;
    Ok((StatusCode::OK, Json(result.into())))
}

pub async fn get_contribution_summary(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Result<(StatusCode, Json<ContributionSummaryResponse>), AppError> {
    let result = state
        .pb_contribution_return_service
        .summary(account_id)
        .await?;
    Ok((StatusCode::OK, Json(result.into())))
}
```

- [ ] **Step 3: Register routes in `api/routes.rs`**

After the existing `/refunds/{refund_id}/post|void` entries:

```rust
.route(
    "/pb-accounts/{account_id}/contribution-returns",
    post(handlers::pb::return_contribution),
)
.route(
    "/pb-accounts/{account_id}/contribution-returns/{return_id}/post",
    post(handlers::pb::post_contribution_return),
)
.route(
    "/pb-accounts/{account_id}/contribution-returns/{return_id}/void",
    post(handlers::pb::void_contribution_return),
)
.route(
    "/pb-accounts/{account_id}/contributions/summary",
    get(handlers::pb::get_contribution_summary),
)
```

- [ ] **Step 4: Compile + run existing refund + reversal e2e as regression checks**

```bash
cargo build -p pba-service
process-compose process restart pba-service
PBA_SERVICE_URL=http://127.0.0.1:3031 cargo test -p pba-service --test e2e -- payment_refund transfer_reversal
```

Expected: all existing scenarios pass. The new endpoints exist but aren't exercised yet.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/api/handlers/pb.rs \
        crates/pba_service/src/api/routes.rs \
        crates/pba_service/src/main.rs
git commit -m "feat(api): contribution return routes + handlers"
```

---

## Phase 4 — SDK

### Task 10: Smithy operations + SDK regen

**Files:**
- Modify: `model/pb_account.smithy` (or wherever PB operations are declared — check with `grep -rn "operation RefundPBAccountPayment" model/`)
- Modify: `model/main.smithy` (register the new operations)
- Regenerate: `crates/pba_client/**` and `crates/pba_service/src/api/openapi.json` via `just smithy-build`

**Interfaces:**
- Produces four Smithy operations:
  - `ReturnPBAccountContribution` — POST `/pb-accounts/{account_id}/contribution-returns`, 201.
  - `PostPBAccountContributionReturn` — POST `/pb-accounts/{account_id}/contribution-returns/{return_id}/post`, 200.
  - `VoidPBAccountContributionReturn` — POST `/pb-accounts/{account_id}/contribution-returns/{return_id}/void`, 200.
  - `GetPBAccountContributionSummary` — GET `/pb-accounts/{account_id}/contributions/summary`, 200.

- [ ] **Step 1: Add response mixin + operations**

In `model/pb_account.smithy` (near `RefundResponseMixin`):

```smithy
structure AllocationEntry {
    @required
    original_transaction_id: String

    @required
    amount: Money
}

@mixin
structure ContributionReturnResponseMixin {
    @required
    return_id: String

    @required
    correlation_id: String

    @required
    account_id: String

    @required
    funding_type: FundingType

    @required
    amount: Money

    @required
    allocations: AllocationEntries

    @required
    remaining_returnable_after: Money

    @required
    status: TransactionStatus

    @required
    created_at: Timestamp
}

list AllocationEntries {
    member: AllocationEntry
}

structure FundingTypeSummary {
    @required
    total_contributed: Money

    @required
    total_returned: Money

    @required
    remaining_returnable: Money
}

@http(method: "POST", uri: "/pb-accounts/{account_id}/contribution-returns", code: 201)
operation ReturnPBAccountContribution {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money

        @required
        funding_type: FundingType

        pending: Boolean

        timeout_seconds: Integer

        gateway_ref: String

        description: String

        idempotency_key: String
    }
    output := with [ContributionReturnResponseMixin] {}
    errors: [
        AccountNotFoundError,
        AccountNotActiveError,
        ContributionAmountInvalidError,
        ContributionFullyReturnedError,
    ]
}

@http(method: "POST", uri: "/pb-accounts/{account_id}/contribution-returns/{return_id}/post", code: 200)
operation PostPBAccountContributionReturn {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        return_id: String
    }
    output := with [ContributionReturnResponseMixin] {}
    errors: [TransactionNotFoundError, TransactionNotPendingError]
}

@http(method: "POST", uri: "/pb-accounts/{account_id}/contribution-returns/{return_id}/void", code: 200)
operation VoidPBAccountContributionReturn {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        return_id: String
    }
    output := with [ContributionReturnResponseMixin] {}
    errors: [TransactionNotFoundError, TransactionNotPendingError]
}

@http(method: "GET", uri: "/pb-accounts/{account_id}/contributions/summary", code: 200)
operation GetPBAccountContributionSummary {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := {
        @required
        trust: FundingTypeSummary

        @required
        third_party: FundingTypeSummary
    }
    errors: [AccountNotFoundError]
}
```

- [ ] **Step 2: Add error shapes if missing**

Check with:
```bash
grep -rn "ContributionAmountInvalidError\|ContributionFullyReturnedError" model/
```

If neither exists, add them mirroring `AccountNotFoundError`:

```smithy
@error("client")
@httpError(400)
structure ContributionAmountInvalidError {
    @required
    message: String

    @required
    requested: Money

    @required
    remaining: Money
}

@error("client")
@httpError(409)
structure ContributionFullyReturnedError {
    @required
    message: String

    @required
    pb_account_id: String
}
```

- [ ] **Step 3: Register the four operations in `model/main.smithy`**

Search for `operations: [` in the service block and add the four new op names.

- [ ] **Step 4: Regenerate**

```bash
just smithy-build
```

Expected: SDK files under `crates/pba_client/src/operation/return_pb_account_contribution/`, etc. `crates/pba_service/src/api/openapi.json` updated with the new paths.

- [ ] **Step 5: Compile everything**

```bash
cargo build -p pba-service -p pba-client
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add model/ crates/pba_client/ crates/pba_service/src/api/openapi.json
git commit -m "feat(smithy): contribution return operations + SDK regen"
```

---

## Phase 5 — API tests

### Task 11: `PbaWorld` fields + step bindings for contribution return

**Files:**
- Modify: `crates/pba_service/tests/e2e.rs` (`PbaWorld` struct + default)
- Modify: `crates/pba_service/tests/steps/payment_steps.rs` (add step bindings — same file as refund steps, near-symmetric)

**Interfaces:**
- Produces new `PbaWorld` fields:
  - `last_return_correlation_id: Option<String>`
  - `last_return_status: Option<String>`
  - `last_return_amount: Option<i64>`
  - `last_return_remaining_after: Option<i64>`
  - `last_return_allocations_count: Option<usize>`
  - `last_return_allocation_totals: Option<Vec<(String, i64)>>` — for FIFO assertions.
  - `contribution_summary_trust_remaining: Option<i64>`
  - `contribution_summary_third_party_remaining: Option<i64>`
- Produces step bindings listed in Step 3 below.

- [ ] **Step 1: Extend `PbaWorld` in `e2e.rs`**

Add fields at the end of the struct (before the closing `}`):

```rust
/// Last contribution-return correlation_id
last_return_correlation_id: Option<String>,
/// Last contribution-return status
last_return_status: Option<String>,
/// Last contribution-return total amount
last_return_amount: Option<i64>,
/// Last contribution-return remaining_returnable_after
last_return_remaining_after: Option<i64>,
/// Last contribution-return allocations count
last_return_allocations_count: Option<usize>,
/// Last contribution-return per-allocation (original_transaction_id, amount)
last_return_allocations: Option<Vec<(String, i64)>>,
/// Summary read: trust remaining_returnable
contribution_summary_trust_remaining: Option<i64>,
/// Summary read: third_party remaining_returnable
contribution_summary_third_party_remaining: Option<i64>,
```

Add matching `None` initialisers in the `Default::default()` impl.

- [ ] **Step 2: Create a new step-bindings file for contribution return**

Create `crates/pba_service/tests/steps/contribution_return_steps.rs`. Register it in `tests/steps.rs` (or wherever step modules are declared — check `crates/pba_service/tests/steps.rs`).

The file skeleton:

```rust
use cucumber::{then, when};

use crate::PbaWorld;

fn classify_return_error(err_str: &str) -> &'static str {
    if err_str.contains("ContributionAmountInvalid") {
        "ContributionAmountInvalid"
    } else if err_str.contains("ContributionFullyReturned") {
        "ContributionFullyReturned"
    } else if err_str.contains("PbAccountNotActive") {
        "PbAccountNotActive"
    } else if err_str.contains("TransactionNotPending") {
        "TransactionNotPending"
    } else if err_str.contains("TransactionNotFound") {
        "TransactionNotFound"
    } else {
        "unknown"
    }
}
```

- [ ] **Step 3: Add the step bindings**

Add these functions to `contribution_return_steps.rs`. Match the imports the existing `payment_steps.rs` uses for SDK calls.

```rust
#[when(regex = r#"^I return (\d+) paisa of "([^"]+)" contributions$"#)]
async fn return_contribution(world: &mut PbaWorld, amount: i64, funding_type: String) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let result = world
        .client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type(&funding_type)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_correlation_id = Some(out.correlation_id().to_string());
            world.last_return_status = Some(out.status().to_string());
            world.last_return_amount = Some(out.amount());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            let allocations: Vec<(String, i64)> = out
                .allocations()
                .iter()
                .map(|a| (a.original_transaction_id().to_string(), a.amount()))
                .collect();
            world.last_return_allocations_count = Some(allocations.len());
            world.last_return_allocations = Some(allocations);
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(
    regex = r#"^I initiate a pending return of (\d+) paisa of "([^"]+)" contributions$"#
)]
async fn initiate_pending_return(world: &mut PbaWorld, amount: i64, funding_type: String) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let result = world
        .client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type(&funding_type)
        .pending(true)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_correlation_id = Some(out.correlation_id().to_string());
            world.last_return_status = Some(out.status().to_string());
            world.last_return_amount = Some(out.amount());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I attempt to return (\d+) paisa of "([^"]+)" contributions$"#)]
async fn attempt_return(world: &mut PbaWorld, amount: i64, funding_type: String) {
    return_contribution(world, amount, funding_type).await;
}

#[when("I post the pending return")]
async fn post_pending_return(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let return_id = world
        .last_return_correlation_id
        .as_ref()
        .expect("no return id")
        .clone();
    let result = world
        .client
        .post_pb_account_contribution_return()
        .account_id(&account_id)
        .return_id(&return_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_status = Some(out.status().to_string());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when("I void the pending return")]
async fn void_pending_return(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let return_id = world
        .last_return_correlation_id
        .as_ref()
        .expect("no return id")
        .clone();
    let result = world
        .client
        .void_pb_account_contribution_return()
        .account_id(&account_id)
        .return_id(&return_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_status = Some(out.status().to_string());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when("I attempt to void the pending return")]
async fn attempt_void_pending_return(world: &mut PbaWorld) {
    void_pending_return(world).await;
}

#[when(regex = r#"^I fetch the contribution summary$"#)]
async fn fetch_summary(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let out = world
        .client
        .get_pb_account_contribution_summary()
        .account_id(&account_id)
        .send()
        .await
        .expect("summary fetch failed");
    let trust = out.trust();
    world.contribution_summary_trust_remaining = Some(trust.remaining_returnable());
    let third_party = out.third_party();
    world.contribution_summary_third_party_remaining =
        Some(third_party.remaining_returnable());
}

#[then("the return is successful")]
async fn return_success(world: &mut PbaWorld) {
    assert!(world.last_error.is_none(), "unexpected error: {:?}", world.last_error);
    assert!(world.last_return_correlation_id.is_some(), "no return correlation_id");
}

#[then(regex = r#"^the return status is "([^"]+)"$"#)]
async fn return_status_is(world: &mut PbaWorld, expected: String) {
    assert_eq!(world.last_return_status.as_deref(), Some(expected.as_str()));
}

#[then(regex = r#"^the return remaining_returnable_after is (\d+)$"#)]
async fn return_remaining_is(world: &mut PbaWorld, expected: i64) {
    assert_eq!(world.last_return_remaining_after, Some(expected));
}

#[then(regex = r#"^the return has (\d+) allocation(?:s)?$"#)]
async fn return_allocations_count(world: &mut PbaWorld, expected: usize) {
    assert_eq!(world.last_return_allocations_count, Some(expected));
}

#[then(regex = r#"^allocation (\d+) is for (\d+) paisa$"#)]
async fn allocation_n_amount(world: &mut PbaWorld, index_1based: usize, amount: i64) {
    let allocations = world.last_return_allocations.as_ref().expect("no allocations");
    let entry = allocations
        .get(index_1based - 1)
        .expect("allocation index out of range");
    assert_eq!(entry.1, amount);
}

#[then(regex = r#"^the return fails with "([^"]+)"$"#)]
async fn return_fails_with(world: &mut PbaWorld, kind: String) {
    let e = world.last_error.as_ref().expect("no error captured");
    assert_eq!(e.kind, kind);
}

#[then(regex = r#"^the trust remaining_returnable is (\d+)$"#)]
async fn trust_remaining_is(world: &mut PbaWorld, expected: i64) {
    assert_eq!(world.contribution_summary_trust_remaining, Some(expected));
}

#[then(regex = r#"^the third_party remaining_returnable is (\d+)$"#)]
async fn third_party_remaining_is(world: &mut PbaWorld, expected: i64) {
    assert_eq!(
        world.contribution_summary_third_party_remaining,
        Some(expected)
    );
}

#[when(regex = r#"^(\d+) concurrent pending returns of (\d+) paisa each of "([^"]+)" contributions are attempted$"#)]
async fn concurrent_pending_returns(
    world: &mut PbaWorld,
    count: usize,
    amount: i64,
    funding_type: String,
) {
    let account_id = world.account_id.clone().expect("no account id");
    let client = world.client.clone();
    let futures: Vec<_> = (0..count)
        .map(|_| {
            let client = client.clone();
            let account_id = account_id.clone();
            let ft = funding_type.clone();
            async move {
                client
                    .return_pb_account_contribution()
                    .account_id(&account_id)
                    .amount(amount)
                    .funding_type(&ft)
                    .pending(true)
                    .send()
                    .await
            }
        })
        .collect();
    let results = futures::future::join_all(futures).await;
    let mut successes = 0usize;
    let mut total = 0i64;
    for r in &results {
        if let Ok(out) = r {
            successes += 1;
            total += out.amount();
        }
    }
    world.concurrent_successes = Some(successes);
    world.concurrent_failures = Some(results.len() - successes);
    // Reuse the existing concurrent_refund_total_amount field for the sum;
    // the field name is a legacy from PR #42 but the semantic (total success
    // amount) applies here too.
    world.concurrent_refund_total_amount = Some(total);
}

#[then(regex = r#"^the total returned amount across all returns is at most (\d+) paisa$"#)]
async fn total_returned_at_most(world: &mut PbaWorld, max: i64) {
    let t = world
        .concurrent_refund_total_amount
        .expect("no total returned value");
    assert!(t <= max, "expected total returned <= {max}, got {t}");
}
```

- [ ] **Step 4: Compile the tests**

```bash
cargo check -p pba-service --tests
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/tests/e2e.rs \
        crates/pba_service/tests/steps/contribution_return_steps.rs \
        crates/pba_service/tests/steps.rs
git commit -m "test(e2e): PbaWorld + step bindings for contribution return"
```

---

### Task 12: `contribution_return.feature` (11 scenarios)

**Files:**
- Create: `crates/pba_service/tests/features/contribution_return.feature`

**Interfaces:**
- Consumes: step bindings from Task 11.
- Produces: 11 scenarios covering full/partial return, FIFO, error cases, idempotency, concurrency, funding-type isolation, frozen account.

- [ ] **Step 1: Create the feature file**

```gherkin
Feature: Contribution return
  Admin returns others-pool contributions (trust or third_party) to their
  contributors. Return rows are TransactionType::Withdrawal in the others
  pool, linked via reverses_transaction_id to specific originals. Multiple
  partial returns per original are allowed; FIFO across originals when
  a single call draws from more than one.

  @api
  Scenario: Full return of a single trust contribution
    Given a normal account exists for holder "cr-s01-alice"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-s01-alice" with origin IFSC "HDFC0080001" and account number "8080001001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I return 20000 paisa of "trust" contributions
    Then the return is successful
    And the return status is "settled"
    And the return has 1 allocation
    And allocation 1 is for 20000 paisa
    And the return remaining_returnable_after is 0
    And the normal account balance is 20000

  @api
  Scenario: Full return of a single third-party contribution
    Given a "health" account exists for holder "cr-s02-bob" with origin IFSC "HDFC0080002" and account number "8080002001"
    And the PB account receives 15000 paisa via a third-party deposit
    When I return 15000 paisa of "third_party" contributions
    Then the return is successful
    And the return status is "settled"
    And the return has 1 allocation
    And allocation 1 is for 15000 paisa
    And the PB account others-pool balance is 0

  @api
  Scenario: Partial return of a single trust contribution
    Given a normal account exists for holder "cr-s03-carol"
    And the normal account has balance 30000
    And a "health" account exists for holder "cr-s03-carol" with origin IFSC "HDFC0080003" and account number "8080003001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I return 10000 paisa of "trust" contributions
    Then the return is successful
    And the return remaining_returnable_after is 20000
    And the normal account balance is 10000

  @api
  Scenario: FIFO across two trust contributions
    Given a normal account exists for holder "cr-s04-dan"
    And the normal account has balance 30000
    And a "health" account exists for holder "cr-s04-dan" with origin IFSC "HDFC0080004" and account number "8080004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I transfer 15000 paisa from the normal account to the PB account
    And I return 20000 paisa of "trust" contributions
    Then the return is successful
    And the return has 2 allocations
    And allocation 1 is for 10000 paisa
    And allocation 2 is for 10000 paisa
    And the return remaining_returnable_after is 5000

  @api
  Scenario: Return amount exceeding remaining is rejected
    Given a normal account exists for holder "cr-s05-eve"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr-s05-eve" with origin IFSC "HDFC0080005" and account number "8080005001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I attempt to return 15000 paisa of "trust" contributions
    Then the return fails with "ContributionAmountInvalid"

  @api
  Scenario: Return of zero is rejected
    Given a normal account exists for holder "cr-s06-flo"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr-s06-flo" with origin IFSC "HDFC0080006" and account number "8080006001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I attempt to return 0 paisa of "trust" contributions
    Then the return fails with "ContributionAmountInvalid"

  @api
  Scenario: Return on account with no matching originals is rejected
    Given a "health" account exists for holder "cr-s07-gus" with origin IFSC "HDFC0080007" and account number "8080007001"
    When I attempt to return 5000 paisa of "trust" contributions
    Then the return fails with "ContributionFullyReturned"

  @api
  Scenario: Trust and third-party pools are independent
    Given a normal account exists for holder "cr-s08-han"
    And the normal account has balance 15000
    And a "health" account exists for holder "cr-s08-han" with origin IFSC "HDFC0080008" and account number "8080008001"
    And the PB account receives 12000 paisa via a third-party deposit
    When I transfer 15000 paisa from the normal account to the PB account
    And I return 15000 paisa of "trust" contributions
    Then the return is successful
    When I fetch the contribution summary
    Then the trust remaining_returnable is 0
    And the third_party remaining_returnable is 12000

  @api
  Scenario: Frozen PB account rejects return; reactivation allows it
    Given a normal account exists for holder "cr-s09-ivy"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-s09-ivy" with origin IFSC "HDFC0080009" and account number "8080009001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I freeze the account
    And I attempt to return 10000 paisa of "trust" contributions
    Then the return fails with "PbAccountNotActive"
    When I reactivate the account
    And I return 10000 paisa of "trust" contributions
    Then the return is successful

  @api
  Scenario: Idempotency replay returns the same correlation_id
    Given a normal account exists for holder "cr-s10-jay"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-s10-jay" with origin IFSC "HDFC0080010" and account number "8080010001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I return 10000 paisa of "trust" contributions with idempotency key "cr-idem-jay-1"
    Then the return is successful
    When I return 10000 paisa of "trust" contributions with idempotency key "cr-idem-jay-1"
    Then the return is successful
    And both returns share the same correlation_id

  @api
  Scenario: Concurrent pending returns reserve remaining
    Given a "health" account exists for holder "cr-s11-lyn" with origin IFSC "HDFC0080011" and account number "8080011001"
    And the PB account receives 5000 paisa via a third-party deposit
    When 5 concurrent pending returns of 300 paisa each of "third_party" contributions are attempted
    Then the total returned amount across all returns is at most 5000 paisa
```

- [ ] **Step 2: Add helper step bindings for scenarios that use them**

Several scenarios reference steps that may not yet exist:

- `the PB account receives N paisa via a third-party deposit` — locate this or add it in `contribution_return_steps.rs`. Reuse the existing pattern from `payment_steps.rs` for admin third-party deposits (find via `grep -n "third_party" crates/pba_service/tests/steps/`). If missing, add:

```rust
#[given(regex = r#"^the PB account receives (\d+) paisa via a third-party deposit$"#)]
async fn pb_receives_third_party_deposit(world: &mut PbaWorld, amount: i64) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    world
        .client
        .deposit_to_pb_account()
        .account_id(&account_id)
        .amount(amount)
        .source_ifsc("HDFC0009999")
        .source_account_number("9999999999")
        .funding_type("third_party")
        .send()
        .await
        .expect("third-party deposit failed");
}
```

- `I return N paisa of "..." contributions with idempotency key "..."` and `both returns share the same correlation_id`. Add:

```rust
#[when(regex = r#"^I return (\d+) paisa of "([^"]+)" contributions with idempotency key "([^"]+)"$"#)]
async fn return_with_idem(world: &mut PbaWorld, amount: i64, funding_type: String, key: String) {
    world.previous_return_correlation_id = world.last_return_correlation_id.take();
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let result = world
        .client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type(&funding_type)
        .idempotency_key(&key)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_correlation_id = Some(out.correlation_id().to_string());
            world.last_return_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[then("both returns share the same correlation_id")]
async fn both_returns_same_correlation(world: &mut PbaWorld) {
    let prev = world.previous_return_correlation_id.as_ref().expect("no previous return id");
    let now = world.last_return_correlation_id.as_ref().expect("no current return id");
    assert_eq!(prev, now);
}
```

Add the `previous_return_correlation_id: Option<String>` field on `PbaWorld` in `e2e.rs` and initialise to `None`.

- [ ] **Step 3: Ensure `PbaWorld.account_id` gets set from the "health" account step**

In existing feature files (`payment_refund.feature`, `transfer_reversal.feature`), the pattern `Given a "health" account exists for holder ...` already sets `world.account_id`. Verify with `grep -n "account_id = Some" crates/pba_service/tests/steps/normal_account_steps.rs` — the reversal e2e depends on this. If missing, extend the existing "health account exists" step to set it (same fix as PR #40's scenario-ordering solution).

- [ ] **Step 4: Compile + run the new feature**

```bash
cargo build -p pba-service
process-compose process restart pba-service
PBA_SERVICE_URL=http://127.0.0.1:3031 cargo test -p pba-service --test e2e -- contribution_return
```

Expected: all 11 scenarios pass.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/tests/features/contribution_return.feature \
        crates/pba_service/tests/steps/contribution_return_steps.rs \
        crates/pba_service/tests/e2e.rs
git commit -m "test(e2e): contribution_return.feature with 11 scenarios"
```

---

### Task 13: `contribution_return_two_phase.feature` (7 scenarios)

**Files:**
- Create: `crates/pba_service/tests/features/contribution_return_two_phase.feature`
- Modify: `crates/pba_service/tests/steps/contribution_return_steps.rs` (add any missing step bindings for status refresh / timeout wait)

**Interfaces:**
- Consumes: `post_pending_return`, `void_pending_return`, `initiate_pending_return`, `attempt_void_pending_return` from Task 11; the `pending_timeout` poller from PR #42.
- Produces: 7 scenarios exercising pending lifecycle, idempotency, mixed direction, timeout expiry.

- [ ] **Step 1: Create the feature file**

```gherkin
Feature: Two-phase contribution return
  A contribution return may be initiated as Pending with a timeout, then
  posted (commits) or voided (rolls back). Pending returns reserve their
  slice of the remaining_returnable so concurrent initiates don't
  over-return.

  @api
  Scenario: Pending return then post credits the source only after post
    Given a normal account exists for holder "cr2p-s01-alice"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr2p-s01-alice" with origin IFSC "HDFC0081001" and account number "8081001001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending return of 15000 paisa of "trust" contributions
    Then the return status is "pending"
    And the normal account balance is 0
    When I post the pending return
    Then the return status is "settled"
    And the normal account balance is 15000

  @api
  Scenario: Pending return then void restores remaining
    Given a normal account exists for holder "cr2p-s02-bob"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr2p-s02-bob" with origin IFSC "HDFC0081002" and account number "8081002001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending return of 15000 paisa of "trust" contributions
    And I void the pending return
    Then the return status is "voided"
    When I fetch the contribution summary
    Then the trust remaining_returnable is 20000

  @api
  Scenario: Pending return blocks a second return that would exceed reserved
    Given a normal account exists for holder "cr2p-s03-carol"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s03-carol" with origin IFSC "HDFC0081003" and account number "8081003001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 6000 paisa of "trust" contributions
    And I attempt to return 5000 paisa of "trust" contributions
    Then the return fails with "ContributionAmountInvalid"

  @api
  Scenario: Post on already-posted return is a no-op
    Given a normal account exists for holder "cr2p-s04-dan"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s04-dan" with origin IFSC "HDFC0081004" and account number "8081004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions
    And I post the pending return
    And I post the pending return
    Then the return status is "settled"

  @api
  Scenario: Void on already-voided return is a no-op
    Given a normal account exists for holder "cr2p-s05-eve"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s05-eve" with origin IFSC "HDFC0081005" and account number "8081005001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions
    And I void the pending return
    And I void the pending return
    Then the return status is "voided"

  @api
  Scenario: Mixed-direction post-then-void rejected
    Given a normal account exists for holder "cr2p-s06-flo"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s06-flo" with origin IFSC "HDFC0081006" and account number "8081006001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions
    And I post the pending return
    And I attempt to void the pending return
    Then the return fails with "TransactionNotPending"

  @api
  Scenario: Pending return with short timeout ages out via pending_timeout poller
    Given a normal account exists for holder "cr2p-s07-gus"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s07-gus" with origin IFSC "HDFC0081007" and account number "8081007001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions with timeout 1 second
    And I wait 3 seconds for the timeout poller
    When I fetch the contribution summary
    Then the trust remaining_returnable is 10000
```

- [ ] **Step 2: Add step bindings for the timeout scenario**

In `contribution_return_steps.rs`:

```rust
#[when(regex = r#"^I initiate a pending return of (\d+) paisa of "([^"]+)" contributions with timeout (\d+) seconds?$"#)]
async fn initiate_pending_return_with_timeout(
    world: &mut PbaWorld,
    amount: i64,
    funding_type: String,
    timeout: i32,
) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let result = world
        .client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type(&funding_type)
        .pending(true)
        .timeout_seconds(timeout)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_correlation_id = Some(out.correlation_id().to_string());
            world.last_return_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("initiate pending return with timeout failed: {e:?}"),
    }
}
```

The `I wait N seconds for the timeout poller` step already exists (from PR #42, `payment_steps.rs`). Verify with:

```bash
grep -n "wait_for_poller\|for the timeout poller" crates/pba_service/tests/steps/payment_steps.rs
```

If the step lives in `payment_steps.rs`, it'll be linked from `contribution_return_two_phase.feature` — cucumber-rs finds registered steps globally within the same `PbaWorld`.

- [ ] **Step 3: Compile + run**

```bash
cargo build -p pba-service
process-compose process restart pba-service
PBA_SERVICE_URL=http://127.0.0.1:3031 cargo test -p pba-service --test e2e -- contribution_return_two_phase
```

Expected: 7 scenarios pass.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/features/contribution_return_two_phase.feature \
        crates/pba_service/tests/steps/contribution_return_steps.rs
git commit -m "test(e2e): two-phase contribution return scenarios"
```

---

## Phase 6 — Admin UI

### Task 14: Contributions panel on PB account detail

**Files:**
- Modify: `crates/pba_service/templates/admin/pb_account_detail.html`
- Modify: `crates/pba_service/src/admin/handlers.rs` (extend the PB account detail handler + template context)

**Interfaces:**
- Consumes: `PbContributionReturnService::summary` (Task 7).
- Produces: A "Contributions" card on the PB account detail page listing per-funding-type contributed / returned / returnable + `[Return...]` links.

- [ ] **Step 1: Locate the existing PB account detail handler**

```bash
grep -n "pb_account_detail\|PbAccountDetailTemplate" crates/pba_service/src/admin/handlers.rs
```

Find the handler and its template context struct. Look at where it currently builds the response.

- [ ] **Step 2: Add contribution fields to the template context**

Add these fields to the PB account detail template struct:

```rust
show_contributions_panel: bool,
trust_contributed_display: String,
trust_returned_display: String,
trust_returnable_paisa: u64,
trust_returnable_display: String,
third_party_contributed_display: String,
third_party_returned_display: String,
third_party_returnable_paisa: u64,
third_party_returnable_display: String,
```

- [ ] **Step 3: Populate them in the handler**

In the PB account detail handler, add before the template render:

```rust
let summary = state
    .pb_contribution_return_service
    .summary(account_id)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(account_id = %account_id, error = %e, "Contribution summary failed; rendering zeros");
        crate::service::pb_contribution_return_service::ContributionSummary {
            trust: crate::service::pb_contribution_return_service::FundingTypeSummary {
                total_contributed: 0,
                total_returned: 0,
                remaining_returnable: 0,
            },
            third_party: crate::service::pb_contribution_return_service::FundingTypeSummary {
                total_contributed: 0,
                total_returned: 0,
                remaining_returnable: 0,
            },
        }
    });

let fmt = |a: u64| format!("{}.{:02}", a / 100, a % 100);
let show_contributions_panel = summary.trust.total_contributed > 0
    || summary.third_party.total_contributed > 0;
```

Include the eight fields in the template struct literal below:

```rust
show_contributions_panel,
trust_contributed_display: fmt(summary.trust.total_contributed),
trust_returned_display: fmt(summary.trust.total_returned),
trust_returnable_paisa: summary.trust.remaining_returnable,
trust_returnable_display: fmt(summary.trust.remaining_returnable),
third_party_contributed_display: fmt(summary.third_party.total_contributed),
third_party_returned_display: fmt(summary.third_party.total_returned),
third_party_returnable_paisa: summary.third_party.remaining_returnable,
third_party_returnable_display: fmt(summary.third_party.remaining_returnable),
```

- [ ] **Step 4: Add the panel HTML to `pb_account_detail.html`**

Insert above the transaction list block:

```html
{% if show_contributions_panel %}
<section class="card">
    <h2>Contributions</h2>
    <table>
        <thead>
            <tr>
                <th>Source</th>
                <th>Contributed</th>
                <th>Returned</th>
                <th>Returnable</th>
                <th></th>
            </tr>
        </thead>
        <tbody>
            <tr>
                <td>Trust (sponsor)</td>
                <td>{{ trust_contributed_display }}</td>
                <td>{{ trust_returned_display }}</td>
                <td>{{ trust_returnable_display }}</td>
                <td>
                    {% if trust_returnable_paisa > 0 %}
                    <a href="{{ prefix }}/admin/accounts/{{ account_id }}/contribution-returns/new?funding_type=trust">Return...</a>
                    {% endif %}
                </td>
            </tr>
            <tr>
                <td>Third-party</td>
                <td>{{ third_party_contributed_display }}</td>
                <td>{{ third_party_returned_display }}</td>
                <td>{{ third_party_returnable_display }}</td>
                <td>
                    {% if third_party_returnable_paisa > 0 %}
                    <a href="{{ prefix }}/admin/accounts/{{ account_id }}/contribution-returns/new?funding_type=third_party">Return...</a>
                    {% endif %}
                </td>
            </tr>
        </tbody>
    </table>
</section>
{% endif %}
```

- [ ] **Step 5: Compile**

```bash
cargo check -p pba-service
```

Expected: clean. The templates are askama-checked at compile time; missing fields fail here.

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/templates/admin/pb_account_detail.html \
        crates/pba_service/src/admin/handlers.rs
git commit -m "feat(admin): contributions panel on PB account detail"
```

---

### Task 15: Return form template + admin handler + route

**Files:**
- Create: `crates/pba_service/templates/admin/contribution_return.html`
- Modify: `crates/pba_service/src/admin/handlers.rs`
- Modify: `crates/pba_service/src/admin.rs`

**Interfaces:**
- Produces:
  - `pub async fn contribution_return_form(State<AppState>, Path<Uuid>, Query<...>) -> Response` handler for `GET`.
  - `pub async fn process_contribution_return(State<AppState>, Path<Uuid>, Form<ContributionReturnForm>) -> Response` handler for `POST`.
  - Route `/admin/accounts/{account_id}/contribution-returns/new` (GET) — form page.
  - Route `/admin/accounts/{account_id}/contribution-returns` (POST) — form submit.
  - `ContributionReturnTemplate` askama struct.
  - `ContributionReturnForm` deserialiser.

- [ ] **Step 1: Create the template**

```html
{% extends "admin/base.html" %}
{% block title %}Return contribution{% endblock %}
{% block content %}
<section class="card">
    <h2>Return contribution</h2>
    {% if let Some(err) = error %}
    <div class="error-banner">{{ err }}</div>
    {% endif %}
    <p><strong>Account:</strong> {{ account_id_short }} ({{ account_id }})</p>
    <p><strong>Funding type:</strong> {{ funding_type }}</p>
    <p><strong>Returnable:</strong> {{ remaining_returnable_display }}</p>

    <form method="post" action="{{ prefix }}/admin/accounts/{{ account_id }}/contribution-returns">
        <input type="hidden" name="funding_type" value="{{ funding_type }}">

        <label>
            Amount (paisa)
            <input type="number" name="amount_paisa" min="1" max="{{ remaining_returnable_paisa }}" required>
        </label>

        <fieldset>
            <legend>Mode</legend>
            <label><input type="radio" name="mode" value="settle" checked> Settle now</label>
            <label><input type="radio" name="mode" value="pending"> Hold as pending</label>
        </fieldset>

        <label>
            Timeout (seconds, optional)
            <input type="number" name="timeout_seconds" min="1" placeholder="default">
        </label>

        <label>
            Description
            <input type="text" name="description">
        </label>

        <label>
            Gateway ref
            <input type="text" name="gateway_ref">
        </label>

        <button type="submit">Submit return</button>
        <a href="{{ prefix }}/admin/accounts/{{ account_id }}">Cancel</a>
    </form>
</section>
{% endblock %}
```

- [ ] **Step 2: Add template struct + form struct + handlers in `admin/handlers.rs`**

Near the existing `PaymentRefundTemplate` and `RefundPaymentForm`:

```rust
#[derive(Template)]
#[template(path = "admin/contribution_return.html")]
struct ContributionReturnTemplate {
    prefix: String,
    account_id: String,
    account_id_short: String,
    funding_type: String,
    remaining_returnable_paisa: u64,
    remaining_returnable_display: String,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct ContributionReturnFormQuery {
    pub funding_type: Option<String>,
}

#[derive(Deserialize)]
pub struct ContributionReturnForm {
    pub amount_paisa: u64,
    pub funding_type: String,
    #[serde(default)]
    pub mode: Option<String>,
    // Option<String> not Option<u32> — empty submit ("timeout_seconds=") would fail
    // integer parsing. Matches the pattern from PR #42.
    #[serde(default)]
    pub timeout_seconds: Option<String>,
    pub description: Option<String>,
    pub gateway_ref: Option<String>,
}

pub async fn contribution_return_form(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Query(q): Query<ContributionReturnFormQuery>,
) -> Response {
    let funding_type = q.funding_type.unwrap_or_else(|| "trust".to_string());
    if funding_type != "trust" && funding_type != "third_party" {
        return (
            StatusCode::BAD_REQUEST,
            "funding_type must be 'trust' or 'third_party'",
        )
            .into_response();
    }
    let summary = match state
        .pb_contribution_return_service
        .summary(account_id)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response();
        }
    };
    let (contributed, remaining) = if funding_type == "trust" {
        (summary.trust.total_contributed, summary.trust.remaining_returnable)
    } else {
        (
            summary.third_party.total_contributed,
            summary.third_party.remaining_returnable,
        )
    };
    if contributed == 0 {
        return (
            StatusCode::NOT_FOUND,
            "No contributions of this funding_type",
        )
            .into_response();
    }
    let fmt = |a: u64| format!("{}.{:02}", a / 100, a % 100);
    let account_id_str = account_id.to_string();
    let account_id_short: String = account_id_str.chars().take(8).collect();
    render(ContributionReturnTemplate {
        prefix: state.path_prefix.clone(),
        account_id: account_id_str,
        account_id_short,
        funding_type,
        remaining_returnable_paisa: remaining,
        remaining_returnable_display: fmt(remaining),
        error: None,
    })
}

pub async fn process_contribution_return(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<ContributionReturnForm>,
) -> Response {
    let is_pending = form.mode.as_deref() == Some("pending");
    let timeout_seconds: Option<u32> = form
        .timeout_seconds
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok());
    match state
        .pb_contribution_return_service
        .return_contribution(
            account_id,
            form.amount_paisa,
            &form.funding_type,
            is_pending,
            timeout_seconds,
            form.gateway_ref.as_deref(),
            form.description.as_deref(),
            None,
        )
        .await
    {
        Ok(r) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transactions/{}", r.return_id),
        ))
        .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("Return failed: {e}")).into_response(),
    }
}
```

- [ ] **Step 3: Register the routes in `admin.rs`**

Near the existing `/refund` and `/reverse` admin routes:

```rust
.route(
    "/admin/accounts/{account_id}/contribution-returns/new",
    get(handlers::contribution_return_form),
)
.route(
    "/admin/accounts/{account_id}/contribution-returns",
    post(handlers::process_contribution_return),
)
```

- [ ] **Step 4: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/templates/admin/contribution_return.html \
        crates/pba_service/src/admin/handlers.rs \
        crates/pba_service/src/admin.rs
git commit -m "feat(admin): contribution return form + POST handler"
```

---

### Task 16: Admin post/void routes + Post/Void buttons on transaction_detail.html

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs`
- Modify: `crates/pba_service/src/admin.rs`
- Modify: `crates/pba_service/templates/admin/transaction_detail.html`

**Interfaces:**
- Produces:
  - `pub async fn admin_post_contribution_return(...)` handler.
  - `pub async fn admin_void_contribution_return(...)` handler.
  - Routes `POST /admin/accounts/{account_id}/contribution-returns/{return_id}/post|void`.
  - `is_pending_contribution_return: bool` in `TransactionDetailTemplate`.

- [ ] **Step 1: Add the two admin handlers**

Near `admin_post_refund` / `admin_void_refund`:

```rust
pub async fn admin_post_contribution_return(
    State(state): State<AppState>,
    Path((account_id, return_id)): Path<(Uuid, Uuid)>,
) -> Response {
    match state
        .pb_contribution_return_service
        .post_contribution_return(account_id, return_id)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transactions/{return_id}"),
        ))
        .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("post failed: {e}")).into_response(),
    }
}

pub async fn admin_void_contribution_return(
    State(state): State<AppState>,
    Path((account_id, return_id)): Path<(Uuid, Uuid)>,
) -> Response {
    match state
        .pb_contribution_return_service
        .void_contribution_return(account_id, return_id)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transactions/{return_id}"),
        ))
        .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, format!("void failed: {e}")).into_response(),
    }
}
```

- [ ] **Step 2: Register routes**

In `admin.rs`:

```rust
.route(
    "/admin/accounts/{account_id}/contribution-returns/{return_id}/post",
    post(handlers::admin_post_contribution_return),
)
.route(
    "/admin/accounts/{account_id}/contribution-returns/{return_id}/void",
    post(handlers::admin_void_contribution_return),
)
```

- [ ] **Step 3: Add `is_pending_contribution_return` to the template struct**

Find `TransactionDetailTemplate` in `handlers.rs`. Add the field:

```rust
is_pending_contribution_return: bool,
```

Compute it in the handler where the template context is built, alongside `is_pending_refund`:

```rust
let is_pending_contribution_return = txn.status == TransactionStatus::Pending
    && txn.transaction_type == TransactionType::Withdrawal
    && txn.pool.as_deref() == Some("others")
    && txn.reverses_transaction_id.is_some();
```

Include it in every constructor of `TransactionDetailTemplate` (main handler + all test fixtures — the fixtures were flagged during PR #42 review; keep them in sync).

- [ ] **Step 4: Add the button block to `transaction_detail.html`**

Near the `{% if is_pending_refund %}` block:

```html
{% if is_pending_contribution_return %}
<div class="row">
    <form method="post" action="{{ prefix }}/admin/accounts/{{ txn.account_id }}/contribution-returns/{{ txn.correlation_id }}/post">
        <button type="submit">Post return</button>
    </form>
    <form method="post" action="{{ prefix }}/admin/accounts/{{ txn.account_id }}/contribution-returns/{{ txn.correlation_id }}/void">
        <button type="submit">Void return</button>
    </form>
</div>
{% endif %}
```

- [ ] **Step 5: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/src/admin/handlers.rs \
        crates/pba_service/src/admin.rs \
        crates/pba_service/templates/admin/transaction_detail.html
git commit -m "feat(admin): post/void contribution return routes + detail-page buttons"
```

---

### Task 17: "Returned by" affordance on transfer detail + extend existing features

**Files:**
- Modify: `crates/pba_service/templates/admin/transaction_detail.html` (generic — sponsor transfers and third-party deposits both use this page)
- Modify: `crates/pba_service/src/admin/handlers.rs` (populate returns list on the template context)
- Modify: `crates/pba_service/tests/features/deposits.feature`
- Modify: `crates/pba_service/tests/features/transfer_reversal.feature`

**Interfaces:**
- Consumes: `TransactionRepo::find_returns_of` (Task 1 renamed).
- Produces: A "Returns" list on the transaction detail page for any row that has one or more return rows pointing at it.

- [ ] **Step 1: Add `returns_of_this_row: Vec<ReturnHistoryRow>` to `TransactionDetailTemplate`**

Define a helper struct:

```rust
#[derive(Clone)]
struct ReturnHistoryRow {
    return_id: String,
    return_id_short: String,
    created_at: String,
    amount_display: String,
    status: String,
    is_voided: bool,
}
```

Add the field to `TransactionDetailTemplate`:

```rust
returns_of_this_row: Vec<ReturnHistoryRow>,
```

In the handler, compute it after the existing refund history loop:

```rust
let return_rows = state
    .transaction_repo
    .find_returns_of(txn.id)
    .await
    .unwrap_or_default();

let returns_of_this_row: Vec<ReturnHistoryRow> = return_rows
    .iter()
    // Only surface Withdrawal-typed returns here (that's the contribution
    // return shape). Refund rows (payment-typed) are surfaced via the
    // refund history table on payment detail.
    .filter(|r| r.transaction_type == TransactionType::Withdrawal
        && r.pool.as_deref() == Some("others"))
    .map(|r| {
        let id_str = r.correlation_id.unwrap_or(r.id).to_string();
        let id_short = id_str.chars().take(8).collect::<String>();
        let amount_display = format!("{}.{:02}", r.amount / 100, r.amount % 100);
        let status = r.status.as_str().to_string();
        let is_voided = matches!(r.status, TransactionStatus::Voided);
        ReturnHistoryRow {
            return_id: id_str,
            return_id_short: id_short,
            created_at: r.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            amount_display,
            status,
            is_voided,
        }
    })
    .collect();
```

Include the vec in every constructor of `TransactionDetailTemplate` (fixtures use empty vec `vec![]`).

- [ ] **Step 2: Add the HTML block in `transaction_detail.html`**

Below the existing refund history section:

```html
{% if !returns_of_this_row.is_empty() %}
<section class="card">
    <h2>Returned in part</h2>
    <table>
        <thead>
            <tr>
                <th>Date</th>
                <th>Amount</th>
                <th>Status</th>
                <th></th>
            </tr>
        </thead>
        <tbody>
            {% for r in returns_of_this_row %}
            <tr>
                <td>{{ r.created_at }}</td>
                <td>{% if r.is_voided %}<s>{{ r.amount_display }}</s>{% else %}{{ r.amount_display }}{% endif %}</td>
                <td>{{ r.status }}</td>
                <td><a href="{{ prefix }}/admin/transactions/{{ r.return_id }}">{{ r.return_id_short }}</a></td>
            </tr>
            {% endfor %}
        </tbody>
    </table>
</section>
{% endif %}
```

- [ ] **Step 3: Add regression scenarios in existing feature files**

Append to `crates/pba_service/tests/features/transfer_reversal.feature`:

```gherkin
  @api
  Scenario: Returned-by affordance surfaces contribution returns on the transfer detail
    Given a normal account exists for holder "rby-sponsor"
    And the normal account has balance 20000
    And a "health" account exists for holder "rby-sponsor" with origin IFSC "HDFC0079999" and account number "7079999001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I return 8000 paisa of "trust" contributions
    And I fetch the transfer detail page returns list
    Then the transfer detail returns list has 1 entry
    And the transfer detail returns entry 1 amount is "80.00"
```

Append to `crates/pba_service/tests/features/deposits.feature`:

```gherkin
  @api
  Scenario: Returned-by affordance surfaces contribution returns on the third-party deposit detail
    Given a "health" account exists for holder "rby-third-party" with origin IFSC "HDFC0079998" and account number "7079998001"
    And the PB account receives 12000 paisa via a third-party deposit
    When I return 5000 paisa of "third_party" contributions
    And I fetch the transfer detail page returns list
    Then the transfer detail returns list has 1 entry
    And the transfer detail returns entry 1 amount is "50.00"
```

- [ ] **Step 4: Add the fetch-returns-list step bindings**

The step `I fetch the transfer detail page returns list` needs to hit `GET /admin/transactions/{id}` and parse the HTML, or (simpler) use a new small REST endpoint. To avoid adding an endpoint just for tests, add a repo-level helper accessible via a lightweight test-only route, OR reuse `find_returns_of` via a hidden JSON endpoint. Cleanest path: expose `GET /admin/transactions/{id}/returns.json` returning a minimal JSON list, gated behind the admin auth.

For v1 keep it simple: add the JSON endpoint under admin:

```rust
#[derive(Serialize)]
struct ReturnListItem {
    return_id: String,
    amount_display: String,
    status: String,
}

#[derive(Serialize)]
struct ReturnListResponse {
    items: Vec<ReturnListItem>,
}

pub async fn admin_returns_list_json(
    State(state): State<AppState>,
    Path(txn_id): Path<Uuid>,
) -> Response {
    let rows = match state.transaction_repo.find_returns_of(txn_id).await {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response(),
    };
    let items: Vec<ReturnListItem> = rows
        .iter()
        .filter(|r| r.transaction_type == TransactionType::Withdrawal
            && r.pool.as_deref() == Some("others"))
        .map(|r| ReturnListItem {
            return_id: r.correlation_id.unwrap_or(r.id).to_string(),
            amount_display: format!("{}.{:02}", r.amount / 100, r.amount % 100),
            status: r.status.as_str().to_string(),
        })
        .collect();
    Json(ReturnListResponse { items }).into_response()
}
```

Register in `admin.rs`:
```rust
.route(
    "/admin/transactions/{txn_id}/returns.json",
    get(handlers::admin_returns_list_json),
)
```

Step binding in `contribution_return_steps.rs`:

```rust
#[when("I fetch the transfer detail page returns list")]
async fn fetch_returns_list(world: &mut PbaWorld) {
    let txn_id = world
        .last_transfer_id
        .as_ref()
        .or(world.last_deposit_id.as_ref())
        .expect("no transfer or deposit id")
        .clone();
    let base = std::env::var("PBA_SERVICE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:3030".to_string());
    let url = format!("{base}/admin/transactions/{txn_id}/returns.json");
    let resp = reqwest::Client::new()
        .get(&url)
        .basic_auth("test", Some("test"))
        .send()
        .await
        .expect("returns.json request failed");
    let body: serde_json::Value = resp.json().await.expect("returns.json parse failed");
    let items = body.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    world.last_transfer_returns_list = Some(
        items
            .iter()
            .map(|it| {
                (
                    it.get("return_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    it.get("amount_display").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    it.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                )
            })
            .collect(),
    );
}

#[then(regex = r#"^the transfer detail returns list has (\d+) entry|entries$"#)]
async fn returns_list_count(world: &mut PbaWorld, expected: usize) {
    let list = world
        .last_transfer_returns_list
        .as_ref()
        .expect("no returns list");
    assert_eq!(list.len(), expected);
}

#[then(regex = r#"^the transfer detail returns entry (\d+) amount is "([^"]+)"$"#)]
async fn returns_list_entry_amount(world: &mut PbaWorld, index: usize, expected: String) {
    let list = world
        .last_transfer_returns_list
        .as_ref()
        .expect("no returns list");
    let entry = list.get(index - 1).expect("entry index out of range");
    assert_eq!(entry.1, expected);
}
```

Add `last_transfer_returns_list: Option<Vec<(String, String, String)>>` on `PbaWorld` in `e2e.rs`.

- [ ] **Step 5: Compile + run existing + extended features**

```bash
cargo build -p pba-service
process-compose process restart pba-service
PBA_SERVICE_URL=http://127.0.0.1:3031 cargo test -p pba-service --test e2e -- transfer_reversal deposits
```

Expected: all scenarios pass including the new "Returned-by" ones.

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/templates/admin/transaction_detail.html \
        crates/pba_service/src/admin/handlers.rs \
        crates/pba_service/src/admin.rs \
        crates/pba_service/tests/features/transfer_reversal.feature \
        crates/pba_service/tests/features/deposits.feature \
        crates/pba_service/tests/steps/contribution_return_steps.rs \
        crates/pba_service/tests/e2e.rs
git commit -m "feat(admin): returned-by affordance on transfer/deposit detail"
```

---

## Phase 7 — UI tests

### Task 18: `contribution_return_admin.feature` UI scenarios

**Files:**
- Create: `crates/pba_service/tests/ui_features/contribution_return_admin.feature`
- Modify: `crates/pba_service/tests/ui_steps/payment_steps.rs` (add contribution-return UI step bindings — same file as refund UI steps)
- Modify: `crates/pba_service/tests/ui_e2e.rs` (`UiWorld` field additions)

**Interfaces:**
- Consumes: admin endpoints from Tasks 14–17; templates from same.
- Produces: 5 UI scenarios.

- [ ] **Step 1: Extend `UiWorld` if needed**

Search for existing refund UI world fields (`grep -n "last_refund" crates/pba_service/tests/ui_e2e.rs`). Add analogous fields:

```rust
last_return_id: Option<String>,
last_return_status: Option<String>,
```

- [ ] **Step 2: Create the feature file**

```gherkin
Feature: Admin UI for contribution returns

  Scenario: Contributions panel renders correct totals
    Given a logged-in admin
    And a PB account with 20000 paisa of trust contributions
    When I open the PB account detail page
    Then the contributions panel shows trust contributed as "200.00"
    And the contributions panel shows trust returnable as "200.00"

  Scenario: Return form pre-selects funding_type from panel button
    Given a logged-in admin
    And a PB account with 20000 paisa of trust contributions
    When I open the PB account detail page
    And I click "Return..." for trust
    Then the return form shows funding_type "trust"

  Scenario: Full trust return via UI credits sponsor and updates panel
    Given a logged-in admin
    And a PB account with 15000 paisa of trust contributions from sponsor "ui-alice"
    When I open the return form for trust
    And I enter 15000 as the return amount
    And I submit the return form
    Then the return detail page shows status "Settled"
    When I open the PB account detail page
    Then the contributions panel shows trust returnable as "0.00"

  Scenario: Pending return via UI renders Post/Void buttons
    Given a logged-in admin
    And a PB account with 10000 paisa of trust contributions
    When I open the return form for trust
    And I enter 5000 as the return amount
    And I select "Hold as pending"
    And I submit the return form
    Then the return detail page shows status "Pending"
    And the Post return button is visible
    And the Void return button is visible

  Scenario: Post via UI flips return status to Settled
    Given a logged-in admin
    And a pending return of 5000 paisa exists on a PB account
    When I open the return detail page
    And I click "Post return"
    Then the return detail page shows status "Settled"
```

- [ ] **Step 3: Add UI step bindings**

Add to `ui_steps/payment_steps.rs` (mirroring the existing refund UI steps). The specifics depend on the existing UI infrastructure; look at `when_click_refund_and_submit`, `when_submit_refund_form`, and their setup blocks. Copy the shape, changing:
- URLs to `/admin/accounts/{id}/contribution-returns/*`.
- Button text to "Post return" / "Void return".
- Setup steps to create trust contributions via transfer, third-party via deposit.

Key steps to add:

```rust
#[given(regex = r#"^a PB account with (\d+) paisa of trust contributions$"#)]
async fn ui_setup_trust_contrib(world: &mut UiWorld, amount: i64) {
    // Reuse the existing UI setup pattern: create a normal account,
    // fund it, create a PB account, transfer. Look at existing "a PB
    // account with N paisa deposited" setup for the pattern.
    ui_create_normal_account_with_balance(world, amount).await;
    ui_create_health_pb_account(world).await;
    ui_transfer_from_normal_to_pb(world, amount).await;
}

#[given(regex = r#"^a PB account with (\d+) paisa of trust contributions from sponsor "([^"]+)"$"#)]
async fn ui_setup_trust_contrib_named_sponsor(world: &mut UiWorld, amount: i64, _sponsor: String) {
    ui_setup_trust_contrib(world, amount).await;
}

#[given(regex = r#"^a pending return of (\d+) paisa exists on a PB account$"#)]
async fn ui_setup_pending_return(world: &mut UiWorld, amount: i64) {
    ui_setup_trust_contrib(world, amount * 2).await;
    // Then initiate a pending return via REST to keep the setup fast.
    let account_id = world.pb_account_id.as_ref().expect("no PB account id").clone();
    let client = world.rest_client();
    let response = client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type("trust")
        .pending(true)
        .send()
        .await
        .expect("pending return initiate failed");
    world.last_return_id = Some(response.return_id().to_string());
}

#[when("I open the PB account detail page")]
async fn ui_open_pb_account_detail(world: &mut UiWorld) {
    let account_id = world.pb_account_id.as_ref().expect("no PB account id").clone();
    let base = world.admin_base_url();
    let url = format!("{base}/admin/accounts/{account_id}");
    let page = world.ensure_page().await;
    page.goto(url).await.expect("navigate to PB account detail failed");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
}

#[when(regex = r#"^I click "Return\.\.\." for (\w+)$"#)]
async fn ui_click_return_link(world: &mut UiWorld, funding_type: String) {
    let page = world.ensure_page().await;
    let selector = format!(
        "a[href*='funding_type={funding_type}']"
    );
    let link = page.find_element(&selector).await.expect("Return link not found");
    link.click().await.expect("click failed");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
}

#[when(regex = r#"^I open the return form for (\w+)$"#)]
async fn ui_open_return_form(world: &mut UiWorld, funding_type: String) {
    let account_id = world.pb_account_id.as_ref().expect("no PB account id").clone();
    let base = world.admin_base_url();
    let url = format!(
        "{base}/admin/accounts/{account_id}/contribution-returns/new?funding_type={funding_type}"
    );
    let page = world.ensure_page().await;
    page.goto(url).await.expect("navigate to return form failed");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
}

#[when(regex = r#"^I enter (\d+) as the return amount$"#)]
async fn ui_enter_return_amount(world: &mut UiWorld, amount: i64) {
    let page = world.ensure_page().await;
    let input = page
        .find_element("input[name='amount_paisa']")
        .await
        .expect("amount_paisa input not found");
    input.click().await.ok();
    input
        .type_str(&amount.to_string())
        .await
        .expect("failed to type amount");
}

#[when(r#"I select "Hold as pending""#)]
async fn ui_select_pending(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let radio = page
        .find_element("input[type='radio'][value='pending']")
        .await
        .expect("pending radio not found");
    radio.click().await.expect("click pending radio failed");
}

#[when("I submit the return form")]
async fn ui_submit_return_form(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let button = page
        .find_element("button[type='submit']")
        .await
        .expect("submit button not found");
    button.click().await.expect("submit click failed");
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    // Capture return id from URL if we landed on the detail page.
    let url = page.url().await.ok().flatten().unwrap_or_default();
    if let Some(id_start) = url.rfind("/transactions/") {
        let id = url[id_start + "/transactions/".len()..].split('?').next().unwrap_or("");
        world.last_return_id = Some(id.to_string());
    }
}

#[when("I open the return detail page")]
async fn ui_open_return_detail(world: &mut UiWorld) {
    let return_id = world.last_return_id.as_ref().expect("no return id").clone();
    let base = world.admin_base_url();
    let url = format!("{base}/admin/transactions/{return_id}");
    let page = world.ensure_page().await;
    page.goto(url).await.expect("navigate to return detail failed");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
}

#[when(r#"I click "Post return""#)]
async fn ui_click_post_return(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let button = page
        .find_element("button:has-text('Post return')")
        .await
        .expect("Post return button not found");
    button.click().await.expect("click Post return failed");
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
}

#[then(regex = r#"^the return detail page shows status "([^"]+)"$"#)]
async fn ui_return_status_is(world: &mut UiWorld, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("page content read failed");
    assert!(
        content.contains(&expected),
        "expected status '{expected}' on page, got: {}",
        content.chars().take(500).collect::<String>()
    );
}

#[then(regex = r#"^the contributions panel shows (\w+) (contributed|returnable) as "([^"]+)"$"#)]
async fn ui_panel_field_is(world: &mut UiWorld, funding_type: String, field: String, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("page content read failed");
    let label = if funding_type == "trust" { "Trust (sponsor)" } else { "Third-party" };
    // Simple sanity check: assert both the label and the expected value appear.
    assert!(content.contains(label), "panel label '{label}' not on page");
    assert!(
        content.contains(&expected),
        "expected {field} value '{expected}' for {funding_type} not on page"
    );
}

#[then(regex = r#"^the return form shows funding_type "([^"]+)"$"#)]
async fn ui_form_funding_type(world: &mut UiWorld, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("page content read failed");
    assert!(
        content.contains(&expected),
        "expected funding_type '{expected}' on return form"
    );
}

#[then("the Post return button is visible")]
async fn ui_post_return_button_visible(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("page content read failed");
    assert!(content.contains("Post return"), "Post return button not on page");
}

#[then("the Void return button is visible")]
async fn ui_void_return_button_visible(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("page content read failed");
    assert!(content.contains("Void return"), "Void return button not on page");
}
```

Note: `ui_create_normal_account_with_balance`, `ui_create_health_pb_account`, `ui_transfer_from_normal_to_pb`, `world.rest_client()`, `world.pb_account_id`, `world.admin_base_url()` are UI-side helpers assumed to exist (they're used by the payment_refund_admin UI feature from PR #40). Grep for their real names and adapt if slightly different.

- [ ] **Step 3: Compile + run UI e2e**

```bash
cargo build -p pba-service
process-compose process restart pba-service
just ui-e2e-run
```

Expected: 5 new UI scenarios pass, no regression on existing UI features.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/ui_features/contribution_return_admin.feature \
        crates/pba_service/tests/ui_steps/payment_steps.rs \
        crates/pba_service/tests/ui_e2e.rs
git commit -m "test(ui-e2e): admin UI scenarios for contribution return"
```

---

## Phase 8 — Final verification

### Task 19: Full sweep — fmt, lint, e2e-all

**Files:** none modified.

- [ ] **Step 1: Format check**

```bash
just fmt-check
```

Expected: clean.

- [ ] **Step 2: Clippy**

```bash
just lint
```

Expected: clean.

- [ ] **Step 3: Full e2e**

```bash
just e2e-all
```

Expected: all phases green (Build & Lint, API E2E, UI E2E).

- [ ] **Step 4: Any cleanup commits**

If `just fmt` picked up any residuals:
```bash
git add -A
git commit -m "style: cargo fmt contribution return"
```

Otherwise no commit.

- [ ] **Step 5: Push + open PR**

```bash
git push -u origin feat/contribution-return
gh pr create --title "feat: contribution return" --body "$(cat <<'EOF'
## Summary
- Admin-initiated return of others-pool contributions back to their source.
- funding_type discriminator (trust | third_party) mirrors deposit. Sponsor returns route to the originating normal account; third-party returns route to THIRD_PARTY_FUNDING_SOURCE_TB_ID.
- Two-phase support (pending → post/void) from day one; FIFO allocation across multiple originals when the return amount spans them.
- Contribution summary read at GET /pb-accounts/{id}/contributions/summary; admin Contributions panel + return form + Post/Void buttons; "Returned by" affordance on transfer/deposit detail.
- Repo rename sum_refunds_of/find_refunds_of → sum_returns_of/find_returns_of so the names reflect the type-agnostic contract.

## Test plan
- [ ] just fmt-check clean
- [ ] just lint clean
- [ ] just e2e-all green
- [ ] Full trust return credits sponsor's normal account
- [ ] Full third-party return credits THIRD_PARTY_FUNDING_SOURCE_TB_ID
- [ ] FIFO allocation across two contributions produces two allocation entries
- [ ] Trust and third-party pools are independent
- [ ] Concurrent pending returns reserve remaining (no over-return)
- [ ] Pending return → void restores remaining
- [ ] Pending return with short timeout ages out via pending_timeout poller
- [ ] Admin UI: Contributions panel + Return form + Post/Void detail buttons + Returned-by affordance

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review checklist

- ✅ **Spec coverage**: every section of the spec maps to one or more tasks.
  - Rename (Task 1) — covers `Repository / ledger layer § renames`.
  - New repo reads (Task 2) — covers the FIFO fetch and summary aggregates.
  - Ledger helpers (Task 3) — covers `CONTRIBUTION_RETURN_CODE` and both TB helpers.
  - Error variants (Task 4) — covers `ContributionAmountInvalid` and `ContributionFullyReturned`.
  - Service (Tasks 5–7) — covers `return_contribution`, `resolve_contribution_return`, wrappers, `summary`.
  - API layer (Tasks 8–9) — covers DTOs, handlers, routes, AppState wiring.
  - Smithy + SDK (Task 10) — covers all four operations + regen.
  - API tests (Tasks 11–13) — cover both new feature files (18 scenarios total) + step bindings.
  - Admin UI (Tasks 14–17) — cover Contributions panel, return form, post/void buttons, Returned-by affordance.
  - UI tests (Task 18) — cover 5 UI scenarios.
  - Verification (Task 19) — final gate.

- ✅ **No placeholders**: every step has concrete code or shell commands.
- ✅ **Type consistency**: `ContributionReturnResult`, `AllocationEntry`, `ContributionSummary`, `FundingTypeSummary` are consistent from service (Tasks 5–7) through DTOs (Task 8) to handlers (Task 9). `PbContributionReturnService` name is stable. `sum_returns_of` / `find_returns_of` renames applied consistently.
- ✅ **TDD ordering**: for the feature-test tasks (12, 13, 17, 18), the tests-first cycle is bundled into the task (write + run + verify). For pure code tasks (1–10, 14–16), cargo check is the verification since the service isn't exercised until the corresponding test task lands.
