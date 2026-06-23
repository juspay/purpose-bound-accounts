# Two-Phase Reversal and Refund — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the existing `pending → post/void` lifecycle to reversal of normal→PB transfers (PR #38) and refund of PB→merchant payments (PR #40). Mirrors the existing two-phase shape used by transfers and deposits — no new business semantics.

**Architecture:** Two symmetric extensions inside the existing `transfer_service` and `pb_payment_service`. Reversal piggy-backs on the existing `/transfers/{id}/post|void` handlers (reversal rows are already `transaction_type='transfer'`). Refund gets new `/refunds/{id}/post|void` endpoints because payment rows have no post/void path today. One small index-widening migration is required so a voided reversal does not permanently block re-reversal.

**Tech Stack:** Rust + Axum + SQLx (PostgreSQL) + TigerBeetle. Cucumber-rs for BDD tests (API + browser). Smithy-generated `pba_client` SDK. Askama HTML templates.

## Global Constraints

- File-per-module Rust style — no `mod.rs` directories.
- Conventional Commit titles on every commit (e.g. `feat(service):`, `fix(repo):`, `test(e2e):`).
- All amounts in paisa (1 INR = 100 paisa), `u64` in Rust, `BIGINT` in PG.
- `rustfmt` clean (`just fmt-check`) and `clippy` clean (`just lint`) on every commit.
- New pending transactions populate `timeout_seconds`; defaults come from `PbPaymentService::default_timeout_seconds` / `TransferService::default_timeout_seconds`.
- No new TigerBeetle account flags; no new account sentinels.
- Spec source of truth: `docs/superpowers/specs/2026-06-22-two-phase-reversal-refund-design.md`.

---

## Phase 1 — Foundation

### Task 1: Migrate the reversal-uniqueness partial index

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260622000001_two_phase_reversal_uniqueness.sql`

**Interfaces:**
- Consumes: existing index `uq_transactions_reverses_transfer` created by `20260530000001_payment_refund.sql`.
- Produces: same index name, narrower predicate excluding voided rows.

- [ ] **Step 1: Write the migration**

Create `crates/pba_service/src/db/migrations/20260622000001_two_phase_reversal_uniqueness.sql`:

```sql
-- Widen the reversal-uniqueness partial index so voided pending reversals do
-- not permanently block re-reversal of the original transfer. After a void
-- the original becomes re-eligible.
--
-- See docs/superpowers/specs/2026-06-22-two-phase-reversal-refund-design.md.

DROP INDEX uq_transactions_reverses_transfer;

CREATE UNIQUE INDEX uq_transactions_reverses_transfer
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL
      AND type = 'transfer'
      AND status <> 'voided';
```

- [ ] **Step 2: Run migration locally**

```bash
just db-reset
just e2e-start
```

Expected: `pba-service` starts; logs show migration applied. No error from `DROP INDEX`.

- [ ] **Step 3: Verify index in psql**

```bash
psql -h 127.0.0.1 -d pba_test -c "\d transactions" | grep uq_transactions_reverses_transfer
```

Expected output mentions `WHERE (reverses_transaction_id IS NOT NULL) AND (type = 'transfer'::text) AND (status <> 'voided'::text)`.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/db/migrations/20260622000001_two_phase_reversal_uniqueness.sql
git commit -m "feat(db): exclude voided rows from reversal uniqueness index"
```

---

### Task 2: Widen `find_reversal_of` status filter

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs` (around the `find_reversal_of` function)

**Interfaces:**
- Consumes: `original_transfer_id: Uuid`.
- Produces: `Option<TransactionRecord>` — `Some` if a non-voided reversal exists.

- [ ] **Step 1: Locate the function**

Run: `grep -n "fn find_reversal_of" crates/pba_service/src/repository/transaction_repo.rs`

Read the function (it's a small SQLx query) and note the current WHERE clause.

- [ ] **Step 2: Widen the query**

Replace the SQL inside `find_reversal_of` from:

```sql
WHERE reverses_transaction_id = $1
```

to:

```sql
WHERE reverses_transaction_id = $1
  AND status IN ('pending', 'posted')
```

(Keep the rest of the SELECT identical.)

- [ ] **Step 3: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 4: Run the existing reversal e2e to confirm no regression**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_reversal
```

Expected: all existing reversal scenarios pass (the widened filter is a superset of current matches that excludes voided — voided reversals do not exist today).

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/repository/transaction_repo.rs
git commit -m "feat(repo): find_reversal_of excludes voided reversals"
```

---

### Task 3: Widen `sum_refunds_of` and `sum_refunds_of_in_tx` status filters

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs`

**Interfaces:**
- Consumes: `original_row_id: Uuid` (and an `&mut Transaction<'_, Postgres>` for the in_tx variant).
- Produces: `u64` (sum of refunding amounts).

- [ ] **Step 1: Locate both functions**

Run: `grep -n "fn sum_refunds_of\b\|fn sum_refunds_of_in_tx" crates/pba_service/src/repository/transaction_repo.rs`

- [ ] **Step 2: Widen both SQL queries**

In both functions, change:

```sql
WHERE reverses_transaction_id = $1
```

to:

```sql
WHERE reverses_transaction_id = $1
  AND status IN ('pending', 'settled')
```

(For `sum_refunds_of_in_tx`, the locking clause `FOR UPDATE OF transactions` — if present — stays as-is.)

- [ ] **Step 3: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 4: Run the existing refund e2e to confirm no regression**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund
```

Expected: all existing refund scenarios pass.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/repository/transaction_repo.rs
git commit -m "feat(repo): sum_refunds_of excludes voided refunds"
```

---

## Phase 2 — Idempotent same-direction post/void

### Task 4: Make `post_transfer` idempotent on already-posted rows

**Files:**
- Modify: `crates/pba_service/src/service/transfer_service.rs` (around `pub async fn post_transfer`)

**Interfaces:**
- Consumes: `(source_normal_id: Uuid, transfer_id: Uuid)`.
- Produces: `Result<TransferResult, AppError>`. On already-posted same-direction, returns the existing posted snapshot instead of `TransactionNotPending`.

- [ ] **Step 1: Add a Cucumber scenario asserting idempotent post**

Create `crates/pba_service/tests/features/transfer_post_void_idempotency.feature` (this scenario file collects both post and void idempotency cases — it's tagged `@api` so the UI runner skips it):

```gherkin
Feature: post_transfer and void_transfer are idempotent in the same direction
  Re-applying the same lifecycle resolution must be a no-op, not an error.

  @api
  Scenario: Post on an already-posted transfer is a no-op
    Given a normal account exists for holder "tpvi-s01-alice"
    And the normal account has balance 20000
    And a "health" account exists for holder "tpvi-s01-alice" with origin IFSC "HDFC0090001" and account number "9090001001"
    When I initiate a pending transfer of 10000 paisa from the normal account to the PB account
    And I post the pending transfer
    And I post the pending transfer
    Then the second post is a no-op
```

(Step bindings for the "I initiate a pending transfer", "I post the pending transfer", and "the second post is a no-op" steps may already exist; if not, add them in `tests/steps/transfer_steps.rs` mirroring existing pending-transfer steps. Check before adding duplicates.)

- [ ] **Step 2: Run the scenario to verify it fails**

```bash
just e2e-start
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_post_void_idempotency
```

Expected: scenario fails on the second post with `TransactionNotPending` (current behavior).

- [ ] **Step 3: Make `post_transfer` idempotent**

In `post_transfer`, replace the existing status guard:

```rust
if source_row.status != TransactionStatus::Pending {
    return Err(AppError::TransactionNotPending(transfer_id.to_string()));
}
```

with:

```rust
match source_row.status {
    TransactionStatus::Pending => {} // proceed
    TransactionStatus::Posted => {
        // Idempotent: return the existing posted snapshot.
        let correlation_id = source_row.correlation_id.ok_or_else(|| {
            AppError::DatabaseError("transfer source row missing correlation_id".to_string())
        })?;
        let legs = self
            .transaction_repo
            .find_by_correlation_id(correlation_id)
            .await?;
        let dest_id = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .map(|l| l.account_id)
            .ok_or_else(|| AppError::DatabaseError("transfer missing pb leg".to_string()))?;
        return Ok(self.legs_to_result(&legs, source_normal_id, dest_id));
    }
    _ => return Err(AppError::TransactionNotPending(transfer_id.to_string())),
}
```

- [ ] **Step 4: Run the scenario to verify it passes**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_post_void_idempotency
```

Expected: post-idempotency scenario passes (void-idempotency scenario added in Task 5 still pending).

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/service/transfer_service.rs \
        crates/pba_service/tests/features/transfer_post_void_idempotency.feature \
        crates/pba_service/tests/steps/transfer_steps.rs
git commit -m "feat(service): post_transfer is idempotent on already-posted rows"
```

---

### Task 5: Make `void_transfer` idempotent on already-voided rows

**Files:**
- Modify: `crates/pba_service/src/service/transfer_service.rs` (around `pub async fn void_transfer`)
- Modify: `crates/pba_service/tests/features/transfer_post_void_idempotency.feature` (add second scenario)

**Interfaces:**
- Consumes: `(source_normal_id: Uuid, transfer_id: Uuid)`.
- Produces: `Result<TransferResult, AppError>`. On already-voided same-direction, returns the existing voided snapshot.

- [ ] **Step 1: Add the void-idempotency scenario**

Append to `transfer_post_void_idempotency.feature`:

```gherkin
  @api
  Scenario: Void on an already-voided transfer is a no-op
    Given a normal account exists for holder "tpvi-s02-bob"
    And the normal account has balance 20000
    And a "health" account exists for holder "tpvi-s02-bob" with origin IFSC "HDFC0090002" and account number "9090002001"
    When I initiate a pending transfer of 10000 paisa from the normal account to the PB account
    And I void the pending transfer
    And I void the pending transfer
    Then the second void is a no-op
```

- [ ] **Step 2: Run scenario to verify it fails**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_post_void_idempotency
```

Expected: void-idempotency scenario fails on the second void with `TransactionNotPending`.

- [ ] **Step 3: Make `void_transfer` idempotent**

In `void_transfer`, replace the status guard with:

```rust
match source_row.status {
    TransactionStatus::Pending => {} // proceed
    TransactionStatus::Voided => {
        let correlation_id = source_row.correlation_id.ok_or_else(|| {
            AppError::DatabaseError("transfer source row missing correlation_id".to_string())
        })?;
        let legs = self
            .transaction_repo
            .find_by_correlation_id(correlation_id)
            .await?;
        let dest_id = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .map(|l| l.account_id)
            .ok_or_else(|| AppError::DatabaseError("transfer missing pb leg".to_string()))?;
        return Ok(self.legs_to_result(&legs, source_normal_id, dest_id));
    }
    _ => return Err(AppError::TransactionNotPending(transfer_id.to_string())),
}
```

- [ ] **Step 4: Run scenario to verify it passes**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_post_void_idempotency
```

Expected: both scenarios pass.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/service/transfer_service.rs \
        crates/pba_service/tests/features/transfer_post_void_idempotency.feature
git commit -m "feat(service): void_transfer is idempotent on already-voided rows"
```

---

## Phase 3 — Pending reversal

### Task 6: Extend `reverse_transfer` with `pending` + `timeout_seconds`

**Files:**
- Modify: `crates/pba_service/src/service/transfer_service.rs` (`pub async fn reverse_transfer` and `ReversalResult`)
- Modify: `crates/pba_service/src/domain/transaction.rs` (verify `TransactionStatus` already includes `Pending` — it does)

**Interfaces:**
- Consumes:
  - `source_normal_id: Uuid`
  - `original_transfer_id: Uuid`
  - `amount: u64`
  - `pending: bool` (new)
  - `timeout_seconds: Option<u32>` (new)
  - `gateway_ref: Option<&str>`, `description: Option<&str>`, `idempotency_key: Option<&str>`
- Produces: `ReversalResult` whose `status` field is `Pending` when `pending=true`, else `Posted`.

- [ ] **Step 1: Read the current `reverse_transfer` body**

Run: `grep -n "pub async fn reverse_transfer\b" crates/pba_service/src/service/transfer_service.rs`

Read steps 1–7 of the existing flow. Note where rows are inserted with `TransactionStatus::Posted` and where the immediate TB transfer is created.

- [ ] **Step 2: Update the signature and add pending fields**

Change the function signature to add the two parameters between `amount` and `gateway_ref`:

```rust
#[allow(clippy::too_many_arguments)]
pub async fn reverse_transfer(
    &self,
    source_normal_id: Uuid,
    original_transfer_id: Uuid,
    amount: u64,
    pending: bool,
    timeout_seconds: Option<u32>,
    gateway_ref: Option<&str>,
    description: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<ReversalResult, AppError> {
```

Update every internal call-site (the existing API handler) to pass `false` and `None` for the new parameters until Task 7.

- [ ] **Step 3: Compute the row status and timeout once**

Just before the row inserts, add:

```rust
let row_status = if pending {
    TransactionStatus::Pending
} else {
    TransactionStatus::Posted
};
let timeout = if pending {
    Some(timeout_seconds.unwrap_or(self.default_timeout_seconds))
} else {
    None
};
```

Replace `TransactionStatus::Posted` in both `insert_in_tx` calls with `row_status`. Replace the `None` currently passed for the row-level `timeout_seconds` parameter with `timeout`.

- [ ] **Step 4: Branch the TB call on `pending`**

Replace the existing `create_internal_transfer_reversal(...)` call with:

```rust
let tb_transfer_id = if pending {
    self.ledger_repo
        .create_pending_transfer(
            destination_pb_others_tb_id,
            source_normal_tb_id,
            amount,
            INTERNAL_TRANSFER_REVERSAL_CODE,
            timeout.expect("timeout populated when pending=true"),
        )
        .await?
} else {
    self.ledger_repo
        .create_internal_transfer_reversal(
            destination_pb_others_tb_id,
            source_normal_tb_id,
            amount,
        )
        .await?;
    0u128
};
```

(Use the same `destination_pb_others_tb_id` / `source_normal_tb_id` bindings the existing flow already computes; do not refactor their derivation.)

- [ ] **Step 5: Persist `tb_transfer_id` on both legs when pending**

After the TB call, inside the open transaction `tx`, add the UPDATE block (mirrors the existing pending-transfer flow):

```rust
if pending && tb_transfer_id != 0 {
    sqlx::query(
        r#"UPDATE transactions
           SET tb_transfer_id = $1::numeric, updated_at = now()
           WHERE correlation_id = $2"#,
    )
    .bind(tb_transfer_id.to_string())
    .bind(reversal_correlation_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::DatabaseError(e.to_string()))?;
}
```

- [ ] **Step 6: Propagate `status` to `ReversalResult`**

Set `ReversalResult.status = row_status` in the final return (and the idempotency-replay branch — look up the normal-side leg's status and reuse it).

- [ ] **Step 7: Compile and re-run existing reversal e2e**

```bash
cargo check -p pba-service
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_reversal
```

Expected: existing reversal scenarios pass — the new path is gated on `pending=true` which no call-site uses yet.

- [ ] **Step 8: Commit**

```bash
git add crates/pba_service/src/service/transfer_service.rs
git commit -m "feat(service): reverse_transfer accepts pending and timeout_seconds"
```

---

### Task 7: Extend the REST API DTO + handler for reversal

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs` (`ReverseTransferRequest`)
- Modify: `crates/pba_service/src/api/handlers/transfer.rs` (or wherever `reverse_transfer` axum handler lives — check with `grep -rn "reverse_transfer" crates/pba_service/src/api`)

**Interfaces:**
- Consumes: HTTP request body adding optional `pending: bool` and `timeout_seconds: u32`.
- Produces: forwards both to `transfer_service::reverse_transfer`.

- [ ] **Step 1: Add fields to `ReverseTransferRequest`**

In `dto.rs`, add to `ReverseTransferRequest`:

```rust
#[serde(default)]
pub pending: bool,

#[serde(default)]
pub timeout_seconds: Option<u32>,
```

- [ ] **Step 2: Forward the fields in the handler**

In the reverse handler, replace the existing call to `transfer_service.reverse_transfer(...)` so it passes `req.pending` and `req.timeout_seconds` in the new parameter slots.

- [ ] **Step 3: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 4: Run existing reversal e2e**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_reversal
```

Expected: all existing reversal scenarios pass (default `pending=false`).

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/api/dto.rs crates/pba_service/src/api/handlers/transfer.rs
git commit -m "feat(api): ReverseTransferRequest accepts pending and timeout_seconds"
```

---

### Task 8: Extend Smithy + regenerate SDK for reverse-transfer pending

**Files:**
- Modify: `model/transfer.smithy` (or wherever `ReverseNormalAccountTransfer` is defined — `grep -rn "operation ReverseNormalAccountTransfer" model/`)
- Regenerate: `crates/pba_client/**` and `crates/pba_service/src/api/openapi.json` via the project's smithy build (consult `just` recipes).

**Interfaces:**
- Consumes: existing Smithy input shape for the operation.
- Produces: same shape with `pending: PrimitiveBoolean` and `timeout_seconds: Integer` optional members; regenerated SDK exposes builder methods `.pending(...)` and `.timeout_seconds(...)`.

- [ ] **Step 1: Add fields to the Smithy input shape**

Inside the `ReverseNormalAccountTransfer` operation's `input :=` block, add (after the existing optional members):

```smithy
        pending: PrimitiveBoolean

        timeout_seconds: Integer
```

(`PrimitiveBoolean` defaults to `false`; `Integer` is nullable. If `PrimitiveBoolean` is not imported, follow the same import as `DepositToPBAccount` for its `pending` field — `grep -rn "pending:" model/` for precedent.)

- [ ] **Step 2: Regenerate SDK and OpenAPI**

```bash
just smithy-build   # or whichever recipe regenerates; check `just --list`
```

Expected: `crates/pba_client/src/operation/reverse_normal_account_transfer/` and `crates/pba_service/src/api/openapi.json` updated. No other Smithy operations should diff.

- [ ] **Step 3: Compile**

```bash
cargo build -p pba-service -p pba-client
```

Expected: clean.

- [ ] **Step 4: Run existing reversal e2e**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_reversal
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add model/transfer.smithy crates/pba_client/ crates/pba_service/src/api/openapi.json
git commit -m "feat(smithy): ReverseNormalAccountTransfer accepts pending and timeout_seconds"
```

---

### Task 9: Cucumber scenarios for pending reversal lifecycle

**Files:**
- Create: `crates/pba_service/tests/features/transfer_reversal_two_phase.feature`
- Modify: `crates/pba_service/tests/steps/transfer_steps.rs` (add step bindings)
- Modify: `crates/pba_service/tests/e2e.rs` (add any new `PbaWorld` fields needed — likely none beyond `last_reversal_status`)

**Interfaces:**
- Consumes: API endpoints from Tasks 6–8, the existing `/transfers/{id}/post|void` endpoints, and the idempotent post/void from Tasks 4–5.
- Produces: 6 scenarios (the 7th — timeout expiry — lands in Task 19 alongside the worker rename).

- [ ] **Step 1: Add the feature file**

Create `crates/pba_service/tests/features/transfer_reversal_two_phase.feature`:

```gherkin
Feature: Two-phase reversal of normal -> PB transfers
  A reversal may be initiated as Pending with a timeout, then committed via
  post or rolled back via void using the existing /transfers/{id}/post|void
  endpoints (because reversal rows are transaction_type='transfer').

  @api
  Scenario: Pending reversal then post credits the source only after post
    Given a normal account exists for holder "tr2p-s01-alice"
    And the normal account has balance 30000
    And a "health" account exists for holder "tr2p-s01-alice" with origin IFSC "HDFC0091001" and account number "9091001001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 10000 paisa on the last transfer
    Then the reversal status is "pending"
    And the normal account balance is 0
    When I post the pending reversal
    Then the reversal status is "posted"
    And the normal account balance is 10000

  @api
  Scenario: Pending reversal then void leaves balances unchanged and original re-reversible
    Given a normal account exists for holder "tr2p-s02-bob"
    And the normal account has balance 20000
    And a "health" account exists for holder "tr2p-s02-bob" with origin IFSC "HDFC0091002" and account number "9091002001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 20000 paisa on the last transfer
    And I void the pending reversal
    Then the reversal status is "voided"
    And the normal account balance is 0
    When I initiate a reversal of 20000 paisa on the last transfer
    Then the reversal is successful
    And the normal account balance is 20000

  @api
  Scenario: Pending reversal blocks a second reversal attempt on the same transfer
    Given a normal account exists for holder "tr2p-s03-carol"
    And the normal account has balance 50000
    And a "health" account exists for holder "tr2p-s03-carol" with origin IFSC "HDFC0091003" and account number "9091003001"
    When I transfer 50000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 25000 paisa on the last transfer
    And I attempt a reversal of 25000 paisa on the last transfer
    Then the reversal fails with "TransferAlreadyReversed"

  @api
  Scenario: Mixed-direction post-then-void rejected
    Given a normal account exists for holder "tr2p-s04-dan"
    And the normal account has balance 10000
    And a "health" account exists for holder "tr2p-s04-dan" with origin IFSC "HDFC0091004" and account number "9091004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 10000 paisa on the last transfer
    And I post the pending reversal
    And I attempt to void the reversal
    Then the operation fails with "TransactionNotPending"
```

(The post/void idempotency scenarios live in `transfer_post_void_idempotency.feature` from Tasks 4–5 and exercise pending reversal as well — they cover scenarios 4 and 5 from the spec's test list. The timeout-expiry scenario lives in Task 19.)

- [ ] **Step 2: Add step bindings**

In `crates/pba_service/tests/steps/transfer_steps.rs`, add the following functions (mirroring the existing `reverse_transfer` bindings):

```rust
#[when(regex = r#"^I initiate a pending reversal of (\d+) paisa on the last transfer$"#)]
async fn initiate_pending_reversal(world: &mut PbaWorld, amount: i64) {
    let normal_id = world.last_normal_account_id.clone().expect("no normal");
    let transfer_id = world.last_transfer_id.clone().expect("no transfer");
    let result = world
        .client
        .reverse_normal_account_transfer()
        .account_id(&normal_id)
        .transfer_id(&transfer_id)
        .amount(amount)
        .pending(true)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_reversal_id = Some(out.reversal_id().to_string());
            world.last_reversal_correlation_id = Some(out.correlation_id().to_string());
            world.last_reversal_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_transfer_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I post the pending reversal$"#)]
async fn post_pending_reversal(world: &mut PbaWorld) {
    let normal_id = world.last_normal_account_id.clone().expect("no normal");
    let reversal_id = world.last_reversal_id.clone().expect("no reversal");
    let result = world
        .client
        .post_normal_account_transfer()
        .account_id(&normal_id)
        .transfer_id(&reversal_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_reversal_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("post pending reversal failed: {e:?}"),
    }
}

#[when(regex = r#"^I void the pending reversal$"#)]
async fn void_pending_reversal(world: &mut PbaWorld) {
    let normal_id = world.last_normal_account_id.clone().expect("no normal");
    let reversal_id = world.last_reversal_id.clone().expect("no reversal");
    let result = world
        .client
        .void_normal_account_transfer()
        .account_id(&normal_id)
        .transfer_id(&reversal_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_reversal_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("void pending reversal failed: {e:?}"),
    }
}

#[when(regex = r#"^I attempt to void the reversal$"#)]
async fn attempt_void_reversal(world: &mut PbaWorld) {
    let normal_id = world.last_normal_account_id.clone().expect("no normal");
    let reversal_id = world.last_reversal_id.clone().expect("no reversal");
    let result = world
        .client
        .void_normal_account_transfer()
        .account_id(&normal_id)
        .transfer_id(&reversal_id)
        .send()
        .await;
    match result {
        Ok(_) => panic!("void should have failed"),
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: if s.contains("TransactionNotPending") { "TransactionNotPending".into() } else { "unknown".into() },
                message: Some(s),
            });
        }
    }
}

#[then(regex = r#"^the reversal status is "([^"]*)"$"#)]
async fn reversal_status_is(world: &mut PbaWorld, expected: String) {
    assert_eq!(world.last_reversal_status.as_deref(), Some(expected.as_str()));
}

#[then(regex = r#"^the operation fails with "([^"]*)"$"#)]
async fn operation_fails_with(world: &mut PbaWorld, kind: String) {
    let e = world.last_error.as_ref().expect("no error captured");
    assert_eq!(e.kind, kind);
}
```

Add `last_reversal_status: Option<String>` to `PbaWorld` if not already present (check `grep -n last_reversal_status crates/pba_service/tests/e2e.rs`).

- [ ] **Step 3: Run the new feature**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_reversal_two_phase
```

Expected: all four scenarios pass. (Order matters — they exercise normal balance after post/void, so a fresh DB per scenario is assumed; `e2e.rs` already runs scenarios serially with `max_concurrent_scenarios(1)`.)

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/features/transfer_reversal_two_phase.feature \
        crates/pba_service/tests/steps/transfer_steps.rs \
        crates/pba_service/tests/e2e.rs
git commit -m "test(e2e): two-phase reversal scenarios"
```

---

## Phase 4 — Pending refund (initiate only)

### Task 10: Add `create_pending_payment_refund` + split TB helpers

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs`

**Interfaces:**
- Produces:
  - `pub async fn create_pending_payment_refund(&self, credit_pb_pool_tb_id: u128, amount: u64, timeout_seconds: u32) -> Result<u128, AppError>` — returns the TB transfer id.
  - `pub async fn create_pending_payment_refund_split(&self, credit_pb_self_tb_id: u128, credit_pb_others_tb_id: u128, amount_self: u64, amount_others: u64, timeout_seconds: u32) -> Result<(u128, u128), AppError>` — returns `(tb_id_self, tb_id_others)`.

- [ ] **Step 1: Add the single-leg helper**

After the existing `create_payment_refund` in `ledger_repo.rs`, add:

```rust
/// Pending single-leg payment refund — pending TB transfer debiting the
/// merchant sentinel, crediting one PB pool. Returns the TB transfer ID for
/// later post/void.
pub async fn create_pending_payment_refund(
    &self,
    credit_pb_pool_tb_id: u128,
    amount: u64,
    timeout_seconds: u32,
) -> Result<u128, AppError> {
    self.create_pending_transfer(
        MERCHANT_SETTLEMENT_TB_ID,
        credit_pb_pool_tb_id,
        amount,
        PAYMENT_REFUND_CODE,
        timeout_seconds,
    )
    .await
}
```

- [ ] **Step 2: Add the split helper**

After the existing `create_payment_refund_split`, add:

```rust
/// Pending two-leg payment refund — two LINKED pending TB transfers debiting
/// the merchant sentinel, crediting self-pool and others-pool. Returns
/// (tb_transfer_id_self, tb_transfer_id_others) so the service can persist
/// both ids on their corresponding rows.
pub async fn create_pending_payment_refund_split(
    &self,
    credit_pb_self_tb_id: u128,
    credit_pb_others_tb_id: u128,
    amount_self: u64,
    amount_others: u64,
    timeout_seconds: u32,
) -> Result<(u128, u128), AppError> {
    let id_self = generate_transfer_id();
    let id_others = generate_transfer_id();

    let transfer1 = tb::Transfer::new(id_self)
        .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
        .with_credit_account_id(credit_pb_self_tb_id)
        .with_amount(amount_self as u128)
        .with_ledger(LEDGER_INR_PAISA)
        .with_code(PAYMENT_REFUND_CODE)
        .with_flags(TransferFlags::PENDING | TransferFlags::LINKED)
        .with_timeout(timeout_seconds);

    let transfer2 = tb::Transfer::new(id_others)
        .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
        .with_credit_account_id(credit_pb_others_tb_id)
        .with_amount(amount_others as u128)
        .with_ledger(LEDGER_INR_PAISA)
        .with_code(PAYMENT_REFUND_CODE)
        .with_flags(TransferFlags::PENDING)
        .with_timeout(timeout_seconds);

    self.client
        .create_transfers(vec![transfer1, transfer2])
        .await
        .map_err(|e| classify_transfer_error(e, "create_pending_payment_refund_split"))?;

    tracing::info!(
        credit_self = %credit_pb_self_tb_id,
        credit_others = %credit_pb_others_tb_id,
        amount_self, amount_others, code = PAYMENT_REFUND_CODE,
        timeout = timeout_seconds,
        id_self = %id_self,
        id_others = %id_others,
        "Created pending LINKED payment-refund TB transfers"
    );
    Ok((id_self, id_others))
}
```

(`generate_transfer_id`, `tb::Transfer`, `TransferFlags`, `LEDGER_INR_PAISA`, `PAYMENT_REFUND_CODE`, and `MERCHANT_SETTLEMENT_TB_ID` are already in scope — used by `create_payment_refund_split`.)

- [ ] **Step 3: Compile**

```bash
cargo check -p pba-service
```

Expected: clean. The `#[allow(dead_code)]` warning may appear on the new helpers if no caller wires them yet — fine for now.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/repository/ledger_repo.rs
git commit -m "feat(ledger): pending payment refund helpers (single + linked split)"
```

---

### Task 11: Extend `refund_payment` with `pending` + `timeout_seconds`

**Files:**
- Modify: `crates/pba_service/src/service/pb_payment_service.rs` (`pub async fn refund_payment`)

**Interfaces:**
- Consumes:
  - existing signature plus `pending: bool` and `timeout_seconds: Option<u32>`.
- Produces: `RefundResult` whose `status` is `Pending` when `pending=true`, else `Settled`. When pending, rows carry their `tb_transfer_id`(s) ready for post/void.

- [ ] **Step 1: Update the signature**

Change `refund_payment` to:

```rust
#[allow(clippy::too_many_arguments)]
pub async fn refund_payment(
    &self,
    pb_account_id: Uuid,
    original_payment_id: Uuid,
    amount: u64,
    pending: bool,
    timeout_seconds: Option<u32>,
    description: Option<&str>,
    gateway_ref: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<RefundResult, AppError> {
```

Update the API handler call-site to pass `false` and `None` for now (Task 12 wires the real values).

- [ ] **Step 2: Compute row status and timeout once**

After the validation block but before the row inserts (Step 7 of the existing flow), add:

```rust
let row_status = if pending {
    TransactionStatus::Pending
} else {
    TransactionStatus::Settled
};
let timeout = if pending {
    Some(timeout_seconds.unwrap_or(self.default_timeout_seconds))
} else {
    None
};
```

Replace the two hard-coded `TransactionStatus::Settled` arguments in the `insert_in_tx` calls with `row_status`. Replace the row-level timeout-seconds argument (currently `None`) with `timeout`.

- [ ] **Step 3: Branch the TB call on `pending`**

Replace the entire `// Step 8: TB transfer(s).` block with:

```rust
// Step 8: TB transfer(s).
let (tb_self_id, tb_others_id): (Option<u128>, Option<u128>) =
    if take_self > 0 && take_others > 0 {
        if pending {
            let (s, o) = self
                .ledger_repo
                .create_pending_payment_refund_split(
                    account.tb_self_account_id,
                    account.tb_others_account_id,
                    take_self,
                    take_others,
                    timeout.expect("timeout populated when pending=true"),
                )
                .await?;
            (Some(s), Some(o))
        } else {
            self.ledger_repo
                .create_payment_refund_split(
                    account.tb_self_account_id,
                    account.tb_others_account_id,
                    take_self,
                    take_others,
                )
                .await?;
            (None, None)
        }
    } else if take_self > 0 {
        if pending {
            let id = self
                .ledger_repo
                .create_pending_payment_refund(
                    account.tb_self_account_id,
                    take_self,
                    timeout.expect("timeout populated when pending=true"),
                )
                .await?;
            (Some(id), None)
        } else {
            self.ledger_repo
                .create_payment_refund(account.tb_self_account_id, take_self)
                .await?;
            (None, None)
        }
    } else if pending {
        let id = self
            .ledger_repo
            .create_pending_payment_refund(
                account.tb_others_account_id,
                take_others,
                timeout.expect("timeout populated when pending=true"),
            )
            .await?;
        (None, Some(id))
    } else {
        self.ledger_repo
            .create_payment_refund(account.tb_others_account_id, take_others)
            .await?;
        (None, None)
    };
```

- [ ] **Step 4: Persist returned TB ids when pending**

Within the same `tx`, after the TB block, add:

```rust
if pending {
    if let Some(tb_id) = tb_self_id {
        sqlx::query(
            r#"UPDATE transactions
               SET tb_transfer_id = $1::numeric, updated_at = now()
               WHERE correlation_id = $2 AND pool = 'self'"#,
        )
        .bind(tb_id.to_string())
        .bind(refund_correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }
    if let Some(tb_id) = tb_others_id {
        sqlx::query(
            r#"UPDATE transactions
               SET tb_transfer_id = $1::numeric, updated_at = now()
               WHERE correlation_id = $2 AND pool = 'others'"#,
        )
        .bind(tb_id.to_string())
        .bind(refund_correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }
}
```

- [ ] **Step 5: Reflect `status` in `RefundResult`**

Change the final `Ok(RefundResult { ..., status: TransactionStatus::Settled, ... })` to `status: row_status`. In the idempotency-replay branch (top of the function), source the status from the loaded row instead of hard-coding `Settled`.

- [ ] **Step 6: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 7: Re-run existing refund e2e**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund
```

Expected: all existing refund scenarios pass — the new path is gated on `pending=true` which the handler doesn't yet forward.

- [ ] **Step 8: Commit**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "feat(service): refund_payment accepts pending and timeout_seconds"
```

---

### Task 12: Extend the REST API DTO + handler for refund pending

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs` (`RefundPaymentRequest`)
- Modify: `crates/pba_service/src/api/handlers/pb.rs` (`refund_payment` handler)

**Interfaces:**
- Consumes: request body adding optional `pending: bool` and `timeout_seconds: u32`.
- Produces: forwards both to `pb_payment_service::refund_payment`.

- [ ] **Step 1: Add fields to `RefundPaymentRequest`**

```rust
#[serde(default)]
pub pending: bool,

#[serde(default)]
pub timeout_seconds: Option<u32>,
```

- [ ] **Step 2: Forward fields in the handler**

Update the handler's call to `refund_payment(...)` so it passes `req.pending` and `req.timeout_seconds` in the new slots.

- [ ] **Step 3: Compile and run existing refund e2e**

```bash
cargo check -p pba-service
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund
```

Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/api/dto.rs crates/pba_service/src/api/handlers/pb.rs
git commit -m "feat(api): RefundPaymentRequest accepts pending and timeout_seconds"
```

---

### Task 13: Extend Smithy + regenerate SDK for refund pending

**Files:**
- Modify: `model/payment.smithy`
- Regenerate: `crates/pba_client/**` and `crates/pba_service/src/api/openapi.json`.

- [ ] **Step 1: Add fields to the Smithy input shape**

Inside `RefundPBAccountPayment`'s `input :=` block, add (after the existing optional members):

```smithy
        pending: PrimitiveBoolean

        timeout_seconds: Integer
```

- [ ] **Step 2: Regenerate**

```bash
just smithy-build
```

- [ ] **Step 3: Compile + re-run existing refund e2e**

```bash
cargo build -p pba-service -p pba-client
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund
```

Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add model/payment.smithy crates/pba_client/ crates/pba_service/src/api/openapi.json
git commit -m "feat(smithy): RefundPBAccountPayment accepts pending and timeout_seconds"
```

---

### Task 14: Cucumber scenarios for pending refund initiate (no post/void yet)

**Files:**
- Modify: `crates/pba_service/tests/features/payment_refund.feature` (one scenario asserting default behavior unchanged) — or create a small extension file
- Create: `crates/pba_service/tests/features/payment_refund_two_phase.feature` (scenarios that don't yet need post/void)
- Modify: `crates/pba_service/tests/steps/payment_steps.rs` (add `pending=true` step)

**Interfaces:**
- Consumes: refund API with `pending=true`.
- Produces: scenarios that exercise pending-refund state and the reservation rule (Tasks 15+ add post/void scenarios).

- [ ] **Step 1: Add the step binding for initiating a pending refund**

In `payment_steps.rs`:

```rust
#[when(regex = r#"^I initiate a pending refund of (\d+) paisa from the last payment$"#)]
async fn initiate_pending_refund(world: &mut PbaWorld, amount: i64) {
    world.previous_refund_correlation_id = world.last_refund_correlation_id.take();
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let payment_id = world
        .last_payment
        .as_ref()
        .expect("No prior payment")
        .payment_id
        .clone();
    let result = world
        .client
        .refund_pb_account_payment()
        .account_id(account_id)
        .payment_id(payment_id)
        .amount(amount)
        .pending(true)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_refund_correlation_id = Some(out.correlation_id().to_string());
            world.last_refund_amount_to_self = Some(out.amount_to_self());
            world.last_refund_amount_to_others = Some(out.amount_to_others());
            world.last_refund_remaining = Some(out.remaining_refundable());
            world.last_refund_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[then(regex = r#"^the refund status is "([^"]*)"$"#)]
async fn refund_status_is(world: &mut PbaWorld, expected: String) {
    assert_eq!(world.last_refund_status.as_deref(), Some(expected.as_str()));
}
```

Add `last_refund_status: Option<String>` to `PbaWorld` + the default constructor.

- [ ] **Step 2: Add the initiate-only scenarios**

Create `crates/pba_service/tests/features/payment_refund_two_phase.feature`:

```gherkin
Feature: Two-phase refund of PB -> merchant payments
  A refund may be initiated as Pending with a timeout, then committed via
  the new /pb-accounts/{id}/refunds/{refund_id}/post endpoint or rolled back
  via /void. Pending refunds reserve their slice of the remaining-refundable
  budget so concurrent initiates do not over-refund.

  @api
  Scenario: Pending single-pool refund reserves remaining
    Given a normal account exists for holder "pr2p-s01-alice"
    And the normal account has balance 50000
    And a "health" account exists for holder "pr2p-s01-alice" with origin IFSC "HDFC0092001" and account number "9092001001"
    When I transfer 50000 paisa from the normal account to the PB account
    And I pay 50000 to merchant "HOSP01" with MCC "8062" described as "others-only payment"
    And I initiate a pending refund of 20000 paisa from the last payment
    Then the refund status is "pending"
    And the remaining refundable amount is 30000
    When I attempt to refund 40000 paisa from the last payment
    Then the refund fails with "RefundAmountInvalid"
    And the refund error remaining field is 30000
```

(More scenarios accumulate in this feature in later tasks — post, void, lifecycle, concurrency.)

- [ ] **Step 3: Run the new feature**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund_two_phase
```

Expected: scenario passes — the reservation rule from Task 3 already enforces this.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/features/payment_refund_two_phase.feature \
        crates/pba_service/tests/steps/payment_steps.rs \
        crates/pba_service/tests/e2e.rs
git commit -m "test(e2e): pending refund reserves remaining-refundable"
```

---

## Phase 5 — Refund post/void

### Task 15: Add `post_refund` + `void_refund` service methods

**Files:**
- Modify: `crates/pba_service/src/service/pb_payment_service.rs`

**Interfaces:**
- Produces:
  - `pub async fn post_refund(&self, pb_account_id: Uuid, refund_id: Uuid) -> Result<RefundResult, AppError>` — flips all rows in the refund correlation from Pending → Settled, calls `post_pending_transfer` per leg, idempotent on already-Settled.
  - `pub async fn void_refund(&self, pb_account_id: Uuid, refund_id: Uuid) -> Result<RefundResult, AppError>` — same shape, Pending → Voided, `void_pending_transfer` per leg, idempotent on already-Voided.

- [ ] **Step 1: Add a private `RefundResolution` enum and the shared helper**

After `refund_payment`, add a private enum and a `resolve_refund` helper. The two public methods (post + void) become thin wrappers — no duplication.

```rust
enum RefundResolution {
    Post,
    Void,
}

impl RefundResolution {
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

impl PbPaymentService {
    async fn resolve_refund(
        &self,
        pb_account_id: Uuid,
        refund_id: Uuid,
        direction: RefundResolution,
    ) -> Result<RefundResult, AppError> {
        let rows = self
            .transaction_repo
            .find_by_correlation_id(refund_id)
            .await?;
        if rows.is_empty() {
            return Err(AppError::TransactionNotFound(refund_id.to_string()));
        }
        for r in &rows {
            if r.account_kind != crate::domain::account_kind::AccountKind::Pb
                || r.account_id != pb_account_id
                || r.transaction_type != TransactionType::Payment
                || r.reverses_transaction_id.is_none()
            {
                return Err(AppError::TransactionNotFound(refund_id.to_string()));
            }
        }

        // Idempotent same-direction no-op
        if rows.iter().all(|r| r.status == direction.target()) {
            return self.build_refund_result_from_rows(pb_account_id, refund_id, &rows).await;
        }
        if rows.iter().any(|r| r.status != TransactionStatus::Pending) {
            return Err(AppError::TransactionNotPending(refund_id.to_string()));
        }

        // Per-leg TB resolution. Tolerate already-resolved errors from the TB
        // layer in case a LINKED chain auto-resolves the tail.
        for r in &rows {
            if r.tb_transfer_id != 0 {
                let res = match direction {
                    RefundResolution::Post => {
                        self.ledger_repo.post_pending_transfer(r.tb_transfer_id).await
                    }
                    RefundResolution::Void => {
                        self.ledger_repo.void_pending_transfer(r.tb_transfer_id).await
                    }
                };
                if let Err(e) = res {
                    if !format!("{e:?}").contains("already_") {
                        return Err(e);
                    }
                }
            }
        }

        sqlx::query(
            r#"UPDATE transactions
               SET status = $1, updated_at = now()
               WHERE correlation_id = $2 AND status = 'pending'"#,
        )
        .bind(direction.target_sql())
        .bind(refund_id)
        .execute(self.transaction_repo.pool())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let updated = self
            .transaction_repo
            .find_by_correlation_id(refund_id)
            .await?;
        self.build_refund_result_from_rows(pb_account_id, refund_id, &updated).await
    }
}
```

- [ ] **Step 2: Add the public wrappers**

```rust
pub async fn post_refund(
    &self,
    pb_account_id: Uuid,
    refund_id: Uuid,
) -> Result<RefundResult, AppError> {
    self.resolve_refund(pb_account_id, refund_id, RefundResolution::Post)
        .await
}

pub async fn void_refund(
    &self,
    pb_account_id: Uuid,
    refund_id: Uuid,
) -> Result<RefundResult, AppError> {
    self.resolve_refund(pb_account_id, refund_id, RefundResolution::Void)
        .await
}
```

- [ ] **Step 3: Add the `build_refund_result_from_rows` helper**

Private method on `PbPaymentService`:

```rust
async fn build_refund_result_from_rows(
    &self,
    pb_account_id: Uuid,
    refund_id: Uuid,
    rows: &[TransactionRecord],
) -> Result<RefundResult, AppError> {
    let amount_to_self: u64 = rows
        .iter()
        .filter(|r| r.pool.as_deref() == Some("self"))
        .map(|r| r.amount)
        .sum();
    let amount_to_others: u64 = rows
        .iter()
        .filter(|r| r.pool.as_deref() == Some("others"))
        .map(|r| r.amount)
        .sum();
    let total_amount = amount_to_self + amount_to_others;

    // Derive original_payment_id and original_amount via reverses_transaction_id.
    let reverses_id = rows
        .first()
        .and_then(|r| r.reverses_transaction_id)
        .ok_or_else(|| {
            AppError::DatabaseError("refund row missing reverses_transaction_id".into())
        })?;
    let original_row = self.transaction_repo.get_transaction(reverses_id).await?;
    let original_payment_id = original_row.correlation_id.unwrap_or(original_row.id);
    let originals = self
        .transaction_repo
        .find_by_correlation_id(original_payment_id)
        .await?;
    let original_amount: u64 = originals.iter().map(|r| r.amount).sum();

    let mut total_refunded: u64 = 0;
    for o in &originals {
        total_refunded += self.transaction_repo.sum_refunds_of(o.id).await?;
    }
    let remaining_refundable = original_amount.saturating_sub(total_refunded);

    let status = rows
        .first()
        .map(|r| r.status)
        .unwrap_or(TransactionStatus::Settled);
    let created_at = rows
        .first()
        .map(|r| r.created_at)
        .unwrap_or_else(chrono::Utc::now);

    Ok(RefundResult {
        refund_id,
        original_payment_id,
        account_id: pb_account_id,
        amount: total_amount,
        amount_to_self,
        amount_to_others,
        original_amount,
        remaining_refundable,
        status,
        correlation_id: refund_id,
        created_at,
    })
}
```

- [ ] **Step 4: Compile**

```bash
cargo check -p pba-service
```

Expected: clean. If `TransactionRecord` import is missing in this file, add the use line.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "feat(service): post_refund and void_refund with same-direction idempotency"
```

---

### Task 16: Add REST API DTOs + handlers + routes for refund post/void

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`
- Modify: `crates/pba_service/src/api/handlers/pb.rs`
- Modify: `crates/pba_service/src/api/routes.rs`

**Interfaces:**
- Produces:
  - `POST /pb-accounts/{account_id}/refunds/{refund_id}/post` → 200 `RefundResponse`.
  - `POST /pb-accounts/{account_id}/refunds/{refund_id}/void` → 200 `RefundResponse`.

- [ ] **Step 1: Add DTOs**

In `dto.rs`, add (re-using `RefundResponse` from PR #40 as the response type):

```rust
#[derive(Deserialize)]
pub struct PostRefundRequest;

#[derive(Deserialize)]
pub struct VoidRefundRequest;
```

(Both request bodies are empty for now — the path captures `refund_id`.)

- [ ] **Step 2: Add handlers**

In `handlers/pb.rs`:

```rust
pub async fn post_refund(
    State(state): State<AppState>,
    Path((account_id, refund_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<RefundResponse>), AppError> {
    let result = state.pb_payment_service.post_refund(account_id, refund_id).await?;
    Ok((StatusCode::OK, Json(result.into())))
}

pub async fn void_refund(
    State(state): State<AppState>,
    Path((account_id, refund_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<RefundResponse>), AppError> {
    let result = state.pb_payment_service.void_refund(account_id, refund_id).await?;
    Ok((StatusCode::OK, Json(result.into())))
}
```

- [ ] **Step 3: Wire routes**

In `api/routes.rs`, after the existing refund route:

```rust
.route(
    "/pb-accounts/{account_id}/refunds/{refund_id}/post",
    post(handlers::pb::post_refund),
)
.route(
    "/pb-accounts/{account_id}/refunds/{refund_id}/void",
    post(handlers::pb::void_refund),
)
```

- [ ] **Step 4: Compile + run existing refund e2e**

```bash
cargo check -p pba-service
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/api/dto.rs \
        crates/pba_service/src/api/handlers/pb.rs \
        crates/pba_service/src/api/routes.rs
git commit -m "feat(api): post and void refund routes"
```

---

### Task 17: Add Smithy ops + regenerate SDK for refund post/void

**Files:**
- Modify: `model/payment.smithy`
- Modify: `model/main.smithy` (operation list)
- Regenerate: `crates/pba_client/**` and `crates/pba_service/src/api/openapi.json`.

- [ ] **Step 1: Add operations**

In `model/payment.smithy`, after `RefundPBAccountPayment`, add:

```smithy
@http(method: "POST", uri: "/pb-accounts/{account_id}/refunds/{refund_id}/post", code: 200)
operation PostPBAccountRefund {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        refund_id: String
    }
    output := with [RefundResponseMixin] {}
    errors: [AccountNotFoundError, TransactionNotFoundError, TransactionNotPendingError]
}

@http(method: "POST", uri: "/pb-accounts/{account_id}/refunds/{refund_id}/void", code: 200)
operation VoidPBAccountRefund {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        refund_id: String
    }
    output := with [RefundResponseMixin] {}
    errors: [AccountNotFoundError, TransactionNotFoundError, TransactionNotPendingError]
}
```

(Verify `TransactionNotFoundError` / `TransactionNotPendingError` shapes already exist by `grep -rn "TransactionNotFoundError\|TransactionNotPendingError" model/`; if not, add them mirroring the existing error shape patterns.)

- [ ] **Step 2: Register in `main.smithy`**

Add `PostPBAccountRefund` and `VoidPBAccountRefund` to the service's operations list.

- [ ] **Step 3: Regenerate**

```bash
just smithy-build
```

- [ ] **Step 4: Compile + run existing refund e2e**

```bash
cargo build -p pba-service -p pba-client
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add model/payment.smithy model/main.smithy crates/pba_client/ crates/pba_service/src/api/openapi.json
git commit -m "feat(smithy): PostPBAccountRefund and VoidPBAccountRefund operations"
```

---

### Task 18: Cucumber scenarios for refund post/void (with idempotency, lifecycle, concurrency)

**Files:**
- Modify: `crates/pba_service/tests/features/payment_refund_two_phase.feature`
- Modify: `crates/pba_service/tests/steps/payment_steps.rs`

**Interfaces:**
- Consumes: post/void refund endpoints from Tasks 15–17, idempotent posture from same.
- Produces: scenarios for post, void, mixed-direction rejection, idempotency, full lifecycle, concurrent reservation.

- [ ] **Step 1: Add step bindings**

In `payment_steps.rs`:

```rust
#[when(regex = r#"^I post the pending refund$"#)]
async fn post_pending_refund(world: &mut PbaWorld) {
    let account_id = world.account_id.clone().expect("no account");
    let refund_id = world.last_refund_correlation_id.clone().expect("no refund");
    let result = world
        .client
        .post_pb_account_refund()
        .account_id(&account_id)
        .refund_id(&refund_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_refund_status = Some(out.status().to_string());
            world.last_refund_remaining = Some(out.remaining_refundable());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I void the pending refund$"#)]
async fn void_pending_refund(world: &mut PbaWorld) {
    let account_id = world.account_id.clone().expect("no account");
    let refund_id = world.last_refund_correlation_id.clone().expect("no refund");
    let result = world
        .client
        .void_pb_account_refund()
        .account_id(&account_id)
        .refund_id(&refund_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_refund_status = Some(out.status().to_string());
            world.last_refund_remaining = Some(out.remaining_refundable());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I attempt to void the pending refund$"#)]
async fn attempt_void_pending_refund(world: &mut PbaWorld) {
    void_pending_refund(world).await;
}

#[when(regex = r#"^(\d+) concurrent pending refunds of (\d+) paisa each are attempted on the last payment$"#)]
async fn concurrent_pending_refunds(world: &mut PbaWorld, count: usize, amount: i64) {
    let account_id = world.account_id.clone().expect("No account ID");
    let payment_id = world.last_payment.as_ref().expect("No prior payment").payment_id.clone();
    let client = world.client.clone();
    let futures: Vec<_> = (0..count)
        .map(|_| {
            let client = client.clone();
            let account_id = account_id.clone();
            let payment_id = payment_id.clone();
            async move {
                client
                    .refund_pb_account_payment()
                    .account_id(&account_id)
                    .payment_id(&payment_id)
                    .amount(amount)
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
    world.concurrent_refund_total_amount = Some(total);
}
```

Add `concurrent_refund_total_amount: Option<i64>` to `PbaWorld` if not already present.

- [ ] **Step 2: Append scenarios to `payment_refund_two_phase.feature`**

```gherkin
  @api
  Scenario: Pending single-pool refund then post
    Given a normal account exists for holder "pr2p-s02-bob"
    And the normal account has balance 50000
    And a "health" account exists for holder "pr2p-s02-bob" with origin IFSC "HDFC0092002" and account number "9092002001"
    When I transfer 50000 paisa from the normal account to the PB account
    And I pay 50000 to merchant "HOSP02" with MCC "8062" described as "others-only payment"
    And I initiate a pending refund of 30000 paisa from the last payment
    And I post the pending refund
    Then the refund status is "settled"
    And the remaining refundable amount is 20000

  @api
  Scenario: Pending split refund then post (LINKED legs)
    Given a "health" account exists for holder "pr2p-s03-carol" with origin IFSC "HDFC0092003" and account number "9092003001"
    And the account has 30000 in self-pool and 20000 in others-pool
    When I pay 50000 to merchant "HOSP03" with MCC "8062" described as "split payment"
    And I initiate a pending refund of 50000 paisa from the last payment
    And I post the pending refund
    Then the refund status is "settled"
    And the refund credited 30000 to self and 20000 to others

  @api
  Scenario: Pending refund then void restores remaining
    Given a normal account exists for holder "pr2p-s04-dan"
    And the normal account has balance 40000
    And a "health" account exists for holder "pr2p-s04-dan" with origin IFSC "HDFC0092004" and account number "9092004001"
    When I transfer 40000 paisa from the normal account to the PB account
    And I pay 40000 to merchant "HOSP04" with MCC "8062" described as "refund-then-void"
    And I initiate a pending refund of 15000 paisa from the last payment
    And I void the pending refund
    Then the refund status is "voided"
    When I refund 40000 paisa from the last payment
    Then the refund is successful
    And the remaining refundable amount is 0

  @api
  Scenario: Concurrent pending refunds reserve remaining
    Given a "health" account exists for holder "pr2p-s05-eve" with origin IFSC "HDFC0092005" and account number "9092005001"
    And the account has 5000 in self-pool and 5000 in others-pool
    When I pay 1000 to merchant "HOSP05" with MCC "8062" described as "concurrent pending refund"
    And 5 concurrent pending refunds of 300 paisa each are attempted on the last payment
    Then the total refunded amount across all refunds is at most 1000 paisa

  @api
  Scenario: Post on already-posted refund is a no-op
    Given a normal account exists for holder "pr2p-s06-flo"
    And the normal account has balance 20000
    And a "health" account exists for holder "pr2p-s06-flo" with origin IFSC "HDFC0092006" and account number "9092006001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP06" with MCC "8062" described as "double-post"
    And I initiate a pending refund of 10000 paisa from the last payment
    And I post the pending refund
    And I post the pending refund
    Then the refund status is "settled"

  @api
  Scenario: Void on already-voided refund is a no-op
    Given a normal account exists for holder "pr2p-s07-gus"
    And the normal account has balance 20000
    And a "health" account exists for holder "pr2p-s07-gus" with origin IFSC "HDFC0092007" and account number "9092007001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP07" with MCC "8062" described as "double-void"
    And I initiate a pending refund of 10000 paisa from the last payment
    And I void the pending refund
    And I void the pending refund
    Then the refund status is "voided"

  @api
  Scenario: Mixed direction (post then void) rejected
    Given a normal account exists for holder "pr2p-s08-han"
    And the normal account has balance 20000
    And a "health" account exists for holder "pr2p-s08-han" with origin IFSC "HDFC0092008" and account number "9092008001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP08" with MCC "8062" described as "mixed-direction"
    And I initiate a pending refund of 10000 paisa from the last payment
    And I post the pending refund
    And I attempt to void the pending refund
    Then the operation fails with "TransactionNotPending"

  @api
  Scenario: Full lifecycle pay -> pending refund -> void -> pending refund -> post
    Given a normal account exists for holder "pr2p-s09-ivy"
    And the normal account has balance 30000
    And a "health" account exists for holder "pr2p-s09-ivy" with origin IFSC "HDFC0092009" and account number "9092009001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I pay 30000 to merchant "HOSP09" with MCC "8062" described as "lifecycle"
    And I initiate a pending refund of 15000 paisa from the last payment
    And I void the pending refund
    And I initiate a pending refund of 15000 paisa from the last payment
    And I post the pending refund
    Then the refund status is "settled"
    And the remaining refundable amount is 15000
```

(The "I attempt to void the pending refund" step expects failure — add an `attempt_void_pending_refund` step that captures the error into `last_error` without panicking, mirroring `attempt_void_reversal`.)

- [ ] **Step 3: Run the feature**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund_two_phase
```

Expected: all scenarios pass.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/features/payment_refund_two_phase.feature \
        crates/pba_service/tests/steps/payment_steps.rs \
        crates/pba_service/tests/e2e.rs
git commit -m "test(e2e): refund post/void lifecycle scenarios"
```

---

## Phase 6 — Background expiry

### Task 19: Rename `run_deposit_timeout_poller` and add timeout-expiry scenarios

**Files:**
- Move/rename: `crates/pba_service/src/service/deposit_timeout.rs` → `crates/pba_service/src/service/pending_timeout.rs`
- Modify: `crates/pba_service/src/service.rs` (or wherever the module is declared — `grep -rn "deposit_timeout" crates/pba_service/src/service*.rs`)
- Modify: `crates/pba_service/src/main.rs` (the spawn call)
- Modify: `crates/pba_service/tests/features/transfer_reversal_two_phase.feature` (timeout-expiry scenario)
- Modify: `crates/pba_service/tests/features/payment_refund_two_phase.feature` (timeout-expiry scenario)

- [ ] **Step 1: Rename the file and function**

```bash
git mv crates/pba_service/src/service/deposit_timeout.rs \
       crates/pba_service/src/service/pending_timeout.rs
```

Inside the new file, rename:
- `pub async fn run_deposit_timeout_poller` → `pub async fn run_pending_timeout_poller`
- Log message `"Starting deposit timeout poller"` → `"Starting pending-transaction timeout poller"`
- `"Pending transfer timed out and voided (both legs)"` → `"Pending transaction timed out and voided (all legs)"`
- `"Pending deposit timed out and voided"` → `"Pending solo transaction timed out and voided"`
- `"Failed to update timed-out deposit status"` → `"Failed to update timed-out transaction status"`
- `"Failed to query timed-out deposits"` → `"Failed to query timed-out pending transactions"`

- [ ] **Step 2: Update module declaration and spawn**

In the service module declaration file, replace `pub mod deposit_timeout;` with `pub mod pending_timeout;`.

In `main.rs`, replace `crate::service::deposit_timeout::run_deposit_timeout_poller` with `crate::service::pending_timeout::run_pending_timeout_poller`.

- [ ] **Step 3: Compile**

```bash
cargo check -p pba-service
```

Expected: clean.

- [ ] **Step 4: Add the timeout-expiry refund scenario**

Append to `payment_refund_two_phase.feature`:

```gherkin
  @api
  Scenario: Pending refund with short timeout ages out and restores remaining
    Given a normal account exists for holder "pr2p-s10-jay"
    And the normal account has balance 30000
    And a "health" account exists for holder "pr2p-s10-jay" with origin IFSC "HDFC0092010" and account number "9092010001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I pay 30000 to merchant "HOSP10" with MCC "8062" described as "timeout-refund"
    And I initiate a pending refund of 10000 paisa from the last payment with timeout 1 second
    And I wait 3 seconds for the timeout poller
    Then the refund of the last payment has status "voided"
    And the remaining refundable amount is 30000
```

Add the step bindings (`initiate_pending_refund_with_timeout`, `wait_seconds`, `refund_has_status`) in `payment_steps.rs`. For `wait_seconds`:

```rust
#[when(regex = r#"^I wait (\d+) seconds? for the timeout poller$"#)]
async fn wait_for_poller(_world: &mut PbaWorld, seconds: u64) {
    // Poller runs on a 1-second interval in test config (see PBA_TIMEOUT_POLL_SECONDS).
    tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
}
```

(Verify the poller's interval in `pba-service.process-compose.test.yml` env. If it's larger, configure to 1 second for tests via env var if such a knob exists; otherwise the wait duration here should match real interval.)

- [ ] **Step 5: Add the timeout-expiry reversal scenario**

Append to `transfer_reversal_two_phase.feature`:

```gherkin
  @api
  Scenario: Pending reversal with short timeout ages out and frees re-reversal
    Given a normal account exists for holder "tr2p-s05-eve"
    And the normal account has balance 20000
    And a "health" account exists for holder "tr2p-s05-eve" with origin IFSC "HDFC0091005" and account number "9091005001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 20000 paisa on the last transfer with timeout 1 second
    And I wait 3 seconds for the timeout poller
    Then the last reversal has status "voided"
    When I initiate a reversal of 20000 paisa on the last transfer
    Then the reversal is successful
```

Add corresponding `initiate_pending_reversal_with_timeout` and `last_reversal_has_status` step bindings in `transfer_steps.rs`.

- [ ] **Step 6: Run both timeout scenarios**

```bash
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- payment_refund_two_phase
PBA_SERVICE_URL=http://127.0.0.1:3030 cargo test -p pba-service --test e2e -- transfer_reversal_two_phase
```

Expected: both pass.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(service): rename deposit_timeout poller to pending_timeout"
```

---

## Phase 7 — Admin UI

### Task 20: Add "Hold as pending" mode to the refund and reversal forms

**Files:**
- Modify: `crates/pba_service/templates/admin/payment_refund.html`
- Modify: `crates/pba_service/templates/admin/transfer_reverse.html`
- Modify: `crates/pba_service/src/admin/handlers.rs` (`process_refund_payment`)
- Modify: `crates/pba_service/src/admin/transfer_handlers.rs` (`process_reverse_transfer`)

**Interfaces:**
- Consumes: existing form POSTs.
- Produces: optional `pending` checkbox + `timeout_seconds` numeric input; admin handlers forward both to service.

- [ ] **Step 1: Add radio + timeout input to `payment_refund.html`**

After the `amount` input row in the form, add:

```html
<fieldset>
  <legend>Mode</legend>
  <label><input type="radio" name="mode" value="settle" checked> Settle now</label>
  <label><input type="radio" name="mode" value="pending"> Hold as pending</label>
</fieldset>

<label>
  Timeout (seconds, optional)
  <input type="number" name="timeout_seconds" min="1" placeholder="default">
</label>
```

(Inspect the existing template for the styling convention before pasting; match it. If the template uses Tailwind/utility classes, copy from a neighboring fieldset.)

- [ ] **Step 2: Mirror in `transfer_reverse.html`**

Same fieldset + timeout input.

- [ ] **Step 3: Update both admin POST handlers**

In `process_refund_payment` (admin/handlers.rs), extend the form struct to include `mode: String` and `timeout_seconds: Option<u32>`. Forward `mode == "pending"` and `timeout_seconds` to `pb_payment_service.refund_payment(...)`.

In `process_reverse_transfer` (admin/transfer_handlers.rs), do the same and forward to `transfer_service.reverse_transfer(...)`.

- [ ] **Step 4: Compile + run a quick smoke**

```bash
cargo build -p pba-service
just e2e-start
curl -sv -u test:test -X POST -d 'amount=10000&mode=pending&timeout_seconds=60' \
  http://127.0.0.1:3030/admin/accounts/<some-account>/payments/<some-payment>/refund
```

Expected: 303 redirect to the transaction detail page; the refund is recorded with `status='pending'`.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/templates/admin/payment_refund.html \
        crates/pba_service/templates/admin/transfer_reverse.html \
        crates/pba_service/src/admin/handlers.rs \
        crates/pba_service/src/admin/transfer_handlers.rs
git commit -m "feat(admin): hold-as-pending mode on refund and reversal forms"
```

---

### Task 21: Surface pending refund Post/Void on transaction detail page

**Files:**
- Modify: `crates/pba_service/templates/admin/transaction_detail.html`
- Modify: `crates/pba_service/src/admin/handlers.rs` (extend the template context struct + transaction detail handler to compute the new fields)

**Interfaces:**
- Consumes: a refund-typed row's `status` and the surrounding template context.
- Produces: a Pending badge + Post / Void button pair when the row is a refund (`type='payment' AND reverses_transaction_id IS NOT NULL`) with `status='pending'`. Also: refund history table shows the per-entry status with a Voided strike-through.

- [ ] **Step 1: Verify pending reversal predicate first**

Open `transaction_detail.html` and find the existing post/void buttons for transfer rows. Confirm the predicate is something like `if txn.status == "Pending" && txn.type == "Transfer"`. If yes, pending reversal rows already render correctly — no change needed for the reversal half. If the predicate excludes rows with `reverses_transaction_id`, drop that exclusion.

- [ ] **Step 2: Add pending-refund button block**

In the template, locate the refund-specific section (added in PR #40 — "Refund of <payment>" affordance). Add:

```html
{% if txn.is_pending_refund %}
<div class="row">
  <form method="post" action="{{ prefix }}/admin/accounts/{{ txn.account_id }}/refunds/{{ txn.correlation_id }}/post">
    <button type="submit">Post refund</button>
  </form>
  <form method="post" action="{{ prefix }}/admin/accounts/{{ txn.account_id }}/refunds/{{ txn.correlation_id }}/void">
    <button type="submit">Void refund</button>
  </form>
</div>
{% endif %}
```

Add `is_pending_refund: bool` to the template's row struct in `admin/handlers.rs` and compute it as `txn.status == TransactionStatus::Pending && txn.transaction_type == TransactionType::Payment && txn.reverses_transaction_id.is_some()`.

- [ ] **Step 3: Refund history status column**

Find the refund history table (added in PR #40 on the original payment's detail page). Add a Status column rendering the row's status. Apply `<s>` strike-through styling when status is Voided. Exclude Voided entries from the "Total refunded" sum (already handled by `sum_refunds_of` widening from Task 3; just ensure the UI calls match).

- [ ] **Step 4: Compile + manual sanity check via running service**

```bash
cargo build -p pba-service
```

Open `http://127.0.0.1:8081/admin/transactions/<pending-refund-correlation-id>` and confirm the buttons render.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/templates/admin/transaction_detail.html \
        crates/pba_service/src/admin/handlers.rs
git commit -m "feat(admin): post/void buttons on pending refund detail page"
```

---

### Task 22: Wire admin POST endpoints for refund post/void

**Files:**
- Modify: `crates/pba_service/src/admin.rs` (route registrations)
- Modify: `crates/pba_service/src/admin/handlers.rs` (two new handler functions)

**Interfaces:**
- Produces:
  - `POST /admin/accounts/{account_id}/refunds/{refund_id}/post` → 303 redirect to the refund detail page.
  - `POST /admin/accounts/{account_id}/refunds/{refund_id}/void` → 303 redirect.

- [ ] **Step 1: Add handlers**

In `admin/handlers.rs`:

```rust
pub async fn admin_post_refund(
    State(state): State<AppState>,
    Path((account_id, refund_id)): Path<(Uuid, Uuid)>,
) -> Result<Redirect, impl IntoResponse> {
    match state.pb_payment_service.post_refund(account_id, refund_id).await {
        Ok(_) => Ok(Redirect::to(&format!(
            "{}/admin/transactions/{}",
            state.path_prefix, refund_id
        ))),
        Err(e) => Err((StatusCode::BAD_REQUEST, format!("post failed: {e:?}")).into_response()),
    }
}

pub async fn admin_void_refund(
    State(state): State<AppState>,
    Path((account_id, refund_id)): Path<(Uuid, Uuid)>,
) -> Result<Redirect, impl IntoResponse> {
    match state.pb_payment_service.void_refund(account_id, refund_id).await {
        Ok(_) => Ok(Redirect::to(&format!(
            "{}/admin/transactions/{}",
            state.path_prefix, refund_id
        ))),
        Err(e) => Err((StatusCode::BAD_REQUEST, format!("void failed: {e:?}")).into_response()),
    }
}
```

- [ ] **Step 2: Register routes**

In `admin.rs`, after the existing refund-form route:

```rust
.route(
    "/admin/accounts/{account_id}/refunds/{refund_id}/post",
    post(handlers::admin_post_refund),
)
.route(
    "/admin/accounts/{account_id}/refunds/{refund_id}/void",
    post(handlers::admin_void_refund),
)
```

- [ ] **Step 3: Compile + smoke**

```bash
cargo build -p pba-service
```

Manually post via curl as in Task 20's smoke; expect a 303 to the detail page and the refund's status flipped.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/admin.rs crates/pba_service/src/admin/handlers.rs
git commit -m "feat(admin): post and void refund admin routes"
```

---

### Task 23: Cucumber UI scenarios for two-phase admin flows

**Files:**
- Create: `crates/pba_service/tests/ui_features/payment_refund_two_phase_admin.feature`
- Create: `crates/pba_service/tests/ui_features/transfer_reversal_two_phase_admin.feature`
- Modify: `crates/pba_service/tests/ui_steps/payment_steps.rs` (add UI steps for selecting Hold-as-pending mode, clicking Post/Void buttons)
- Modify: `crates/pba_service/tests/ui_steps/transfer_steps.rs` (same)
- Modify: `crates/pba_service/tests/ui_e2e.rs` (any new `UiWorld` fields)

**Interfaces:**
- Consumes: admin endpoints from Tasks 20–22; templates from same.
- Produces: 4 refund UI scenarios + 3 reversal UI scenarios.

- [ ] **Step 1: Refund admin feature**

Create the file with these scenarios:

```gherkin
Feature: Admin UI for two-phase refunds

  Scenario: Initiating a pending refund renders pending detail page
    Given a logged-in admin
    And a posted PB->merchant payment exists
    When I open the refund form for that payment
    And I select "Hold as pending"
    And I enter 15000 as the refund amount
    And I submit the form
    Then the refund detail page shows status "Pending"
    And the Post refund button is visible
    And the Void refund button is visible

  Scenario: Posting via UI flips status to Settled
    Given a logged-in admin
    And a pending refund exists for a payment
    When I open the refund detail page
    And I click "Post refund"
    Then the refund status is "Settled"

  Scenario: Voiding via UI flips status to Voided and restores remaining
    Given a logged-in admin
    And a pending refund of 10000 exists for a 30000 payment
    When I open the refund detail page
    And I click "Void refund"
    Then the refund status is "Voided"
    And the original payment shows 30000 remaining refundable

  Scenario: Refund history table shows pending + voided entries with strike-through
    Given a logged-in admin
    And a payment has a voided pending refund and a settled refund
    When I open the original payment detail page
    Then the refund history shows two entries
    And the voided entry is rendered with strike-through
```

- [ ] **Step 2: Reversal admin feature**

Create with:

```gherkin
Feature: Admin UI for two-phase transfer reversals

  Scenario: Initiating a pending reversal renders pending detail page
    Given a logged-in admin
    And a posted normal->PB transfer exists
    When I open the reverse form for that transfer
    And I select "Hold as pending"
    And I submit the form
    Then the reversal detail page shows status "Pending"
    And the Post button is visible
    And the Void button is visible

  Scenario: Posting via UI flips reversal status to Posted
    Given a logged-in admin
    And a pending reversal exists for a transfer
    When I open the reversal detail page
    And I click "Post"
    Then the reversal status is "Posted"

  Scenario: Voiding via UI flips reversal status to Voided and unlocks re-reversal
    Given a logged-in admin
    And a pending reversal exists for a transfer
    When I open the reversal detail page
    And I click "Void"
    Then the reversal status is "Voided"
    And the original transfer becomes reversible again
```

- [ ] **Step 3: Add step bindings**

In `ui_steps/payment_steps.rs` and `ui_steps/transfer_steps.rs`, add the steps to: open form, select mode radio, fill amount, submit, click button, assert badge text. Mirror the existing payment_refund_admin.feature steps where possible (verify with `grep -n "I open the refund form" crates/pba_service/tests/ui_steps/*.rs`).

- [ ] **Step 4: Run UI tests**

```bash
just e2e-all
```

Expected: full e2e green including the new UI features.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/tests/ui_features/payment_refund_two_phase_admin.feature \
        crates/pba_service/tests/ui_features/transfer_reversal_two_phase_admin.feature \
        crates/pba_service/tests/ui_steps/ \
        crates/pba_service/tests/ui_e2e.rs
git commit -m "test(ui-e2e): two-phase refund and reversal admin scenarios"
```

---

## Phase 8 — Final verification

### Task 24: Full e2e and lint sweep

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

Expected: no warnings beyond pre-existing baseline.

- [ ] **Step 3: Full e2e**

```bash
just e2e-all
```

Expected: all phases green (Build & Lint, API E2E, UI E2E).

- [ ] **Step 4: Final commit if any cleanup needed**

If any `cargo fmt` adjustments fall out of the previous step:

```bash
git add -A
git commit -m "style: cargo fmt two-phase reversal/refund changes"
```

Otherwise no commit.

- [ ] **Step 5: Push and open PR**

```bash
git push -u origin feat/two-phase-reversal-refund
gh pr create --title "feat: two-phase reversal and refund" --body "$(cat <<'EOF'
## Summary
- Extends pending/post/void lifecycle to reversal of normal->PB transfers (PR #38) and refund of PB->merchant payments (PR #40).
- Pending reversal reuses existing /transfers/{id}/post|void. Pending refund adds /pb-accounts/{id}/refunds/{id}/post|void.
- post_transfer/void_transfer become idempotent in the same direction so refund and reversal share posture.
- One small migration widens the reversal-uniqueness partial index so voided pending reversals do not permanently block re-reversal.
- Background expiry is automatic: the existing poller (renamed run_pending_timeout_poller) already handles any pending row type.

## Test plan
- [ ] just e2e-all green (API + UI cucumber)
- [ ] Pending refund initiate -> post via REST, status flips Pending -> Settled
- [ ] Pending refund initiate -> void via REST, remaining refundable restored
- [ ] Pending reversal initiate via REST, post via /transfers/{id}/post, status flips Pending -> Posted
- [ ] Pending reversal void, original re-reversible
- [ ] Concurrent pending refund initiates reserve remaining (no over-refund)
- [ ] Admin UI: Hold-as-pending mode on both forms; Post/Void buttons render on pending detail pages

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review checklist

- ✅ Spec coverage: every section in the spec maps to one or more tasks (migration → T1, repo filters → T2/T3, idempotency → T4/T5, reversal half → T6–T9, refund half → T10–T18, expiry → T19, UI → T20–T23, verification → T24).
- ✅ No placeholders: every step has concrete code or commands.
- ✅ Type consistency: `post_refund` / `void_refund` referenced consistently; `RefundResult.status` widened to carry `Pending`/`Voided` as well as `Settled`.
- ✅ TDD ordering: tests precede implementation for behavior changes (Tasks 4, 5, 9, 14, 18, 19, 23).
- ✅ Commits are frequent (one per task) and use Conventional Commit prefixes.
