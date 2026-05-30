# Payment Refund Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add admin-initiated refund of a settled PB→merchant payment. Each refund is recorded as a new compensating transaction (1 or 2 rows mirroring the payment's pool split) plus matching TigerBeetle transfer(s) in the opposite direction (debit `MERCHANT_SETTLEMENT_TB_ID`, credit PB pool). Multiple partial refunds are allowed per payment, summing to ≤ original; refund credits self-pool first then others-pool.

**Architecture:** A single new method on the existing `PbPaymentService` (`refund_payment`) loads the original payment rows by `correlation_id`, computes per-pool remaining-unrefunded via a new repo aggregate (`sum_refunds_of`), inserts up to two refund rows under a fresh `correlation_id`, and writes one or two TB transfers with code `210`. The HTTP API gets one new route under `/pb-accounts/{account_id}/payments/{payment_id}/refund`; the admin UI gets a Refund button + refund-history block on the existing transaction detail page. The schema change is forward-compatible — the existing partial unique index on `reverses_transaction_id` is tightened to `type='transfer'`, so a future PR can relax transfer reversal to multi-partial without further migration churn beyond dropping that index.

**Tech Stack:** Rust (axum, sqlx, tokio), PostgreSQL, TigerBeetle (via `tigerbeetle_unofficial`), Smithy for the API model + generated Rust client SDK, Askama templates for the admin UI, Cucumber for BDD.

**Spec:** `docs/superpowers/specs/2026-05-30-payment-refund-design.md`.

**Branch:** `feat/payment-refund` (created from `main` at commit `ff7480b`).

---

## Pre-existing facts about the codebase to know before starting

- **Error response shape.** `AppError::into_response` writes `{ "error": "<PascalCaseVariantName>", "message": "<Display impl>" }`. New variants follow this. *Do not* invent a `snake_case` error code field.
- **`InsufficientFunds` → HTTP 422** (`UNPROCESSABLE_ENTITY`). For refunds we do **not** hit `InsufficientFunds` — the merchant sentinel has no balance constraint, and over-amount cases are caught at `RefundAmountInvalid` (HTTP 400) before TB is touched.
- **UUIDv7 ids.** Use `Uuid::now_v7()` for entity ids — project convention since commit `48268d1`. The refund's `correlation_id` is a fresh `Uuid::now_v7()`.
- **Auth.** The HTTP API router (`api/routes.rs`) is API-key authenticated and has *no* admin-role gate. The admin UI router is gated by `auth::admin_auth::require_admin_session`. We follow the same posture as `reverse_transfer`: the API endpoint is exposed to any authenticated API caller; the admin UI calls the same endpoint via the OIDC-session-gated `/admin/*` surface.
- **No `mod.rs`.** Project preference: use `foo.rs`-style modules.
- **DTO style.** Rust struct fields are `snake_case`; serde uses defaults. Do not switch to `camelCase`.
- **Migration filenames.** `YYYYMMDDhhmmss_<snake_case>.sql` under `crates/pba_service/src/db/migrations/`. Use `20260530000001_payment_refund.sql`.
- **`tests/features/*.feature`** runs via `just api-e2e` and uses the **Smithy-generated client SDK** (`world.client.make_pb_account_payment()...`). So API E2E tests for the new operation require the Smithy model to be added and the SDK regenerated *before* the test passes end-to-end.
- **`insert_in_tx` signature** already takes `reverses_transaction_id: Option<Uuid>` as the trailing parameter (added by PR #38). No signature change in this PR.
- **`MERCHANT_SETTLEMENT_TB_ID`** is a sentinel with only the `LINKED` flag (no balance constraint). Debiting it for a refund is unconstrained at the TB layer; no `ExceedsBalance` path here.
- **Existing `create_linked_transfers`** in `ledger_repo.rs` has the **payment direction** signature: two debit accounts (others, self) + one credit account. For refunds we need the inverse: one debit (merchant sentinel) + two credit accounts (self, others). Add a new sibling `create_payment_refund_split` rather than try to overload the existing helper.
- **`find_by_correlation_id`** in `transaction_repo.rs` already exists and returns rows ordered by `created_at ASC`. Use it to resolve the original payment.
- **`type_label()` precedence.** The existing match has `(TransactionType::Transfer, _) if self.reverses_transaction_id.is_some() => "Reversal"` *before* the other Transfer arms. Add the Payment-Refund arm with the same `if` guard *before* the catch-all `(TransactionType::Payment, _) => "Payment"`.

## File map

| File | Disposition | Responsibility |
|---|---|---|
| `crates/pba_service/src/db/migrations/20260530000001_payment_refund.sql` | Create | Drop existing `uq_transactions_reverses`; recreate as `uq_transactions_reverses_transfer` with `WHERE type='transfer'`. |
| `crates/pba_service/src/error.rs` | Modify | New variants: `RefundNotRefundable(String, String)`, `RefundAmountInvalid { requested: u64, remaining: u64 }`, `PaymentFullyRefunded(String)`. |
| `crates/pba_service/src/domain/transaction.rs` | Modify | One new branch in `type_label()` for `(Payment, _) if reverses_transaction_id.is_some()` → `"Refund"`. |
| `crates/pba_service/src/repository/transaction_repo.rs` | Modify | Add `find_refunds_of(original_row_id)` and `sum_refunds_of(original_row_id)`. Type-agnostic — both work for transfer reversal too. |
| `crates/pba_service/src/repository/ledger_repo.rs` | Modify | Add `PAYMENT_REFUND_CODE: u16 = 210`, `create_payment_refund` (single transfer), `create_payment_refund_split` (linked pair). |
| `crates/pba_service/src/service/pb_payment_service.rs` | Modify | Add `RefundResult` struct and `refund_payment` method. |
| `crates/pba_service/src/api/dto.rs` | Modify | Add `RefundPaymentRequest`, `RefundResponse`, `impl From<RefundResult> for RefundResponse`. |
| `crates/pba_service/src/api/handlers/pb.rs` | Modify | Add `refund_payment` handler. |
| `crates/pba_service/src/api/routes.rs` | Modify | Add the new route. |
| `model/payment.smithy` | Modify | Add `RefundPBAccountPayment` operation + `RefundResponseMixin`. |
| `model/main.smithy` | Modify | Register the new operation on the service. |
| `crates/pba_service/src/admin/handlers.rs` | Modify | Extend `TransactionDetailTemplate` with refund-related fields; populate from `find_refunds_of` + `sum_refunds_of`. |
| `crates/pba_service/src/admin/pb_handlers.rs` | Modify (or `handlers.rs` if pb_handlers does not exist — confirmed at task time) | Add `refund_payment_form` and `process_refund_payment` admin handlers. |
| `crates/pba_service/src/admin.rs` | Modify | Register the two new admin routes. |
| `crates/pba_service/templates/admin/transaction_detail.html` | Modify | Add Refund button (settled payment, not a refund, remaining > 0); add "Refund history" block; "Refund of [payment]" affordance on a refund row. |
| `crates/pba_service/templates/admin/payment_refund.html` | Create | Refund form template (amount + description). |
| `crates/pba_service/tests/features/payment_refund.feature` | Create | 13 BDD scenarios per the spec. |
| `crates/pba_service/tests/steps/payment_steps.rs` | Modify | Add refund step bindings. |
| `crates/pba_service/tests/world.rs` (or wherever `PbaWorld` lives — confirmed at task time) | Modify | Add `last_refund_id`, `last_refund_correlation_id`, `last_refund_amount_to_self`, `last_refund_amount_to_others`, `last_refund_remaining` fields. |
| `crates/pba_service/tests/ui_features/payment_refund_admin.feature` | Create | 5 UI scenarios per the spec. |
| `crates/pba_service/tests/ui_steps/payment_steps.rs` | Modify | Add UI step bindings for refund. |
| `README.md` | Modify | Add refund row to the API table. |
| `WHAT.md` | Modify | Add a "Refunding a payment" subsection. |

---

## Task 1: Migration — tighten `reverses_transaction_id` uniqueness to transfers

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260530000001_payment_refund.sql`

- [ ] **Step 1: Create the migration file.**

```sql
-- Tighten the transfer-reversal uniqueness so payment refunds can have many
-- rows pointing at the same original payment row. The transfer-reversal
-- at-most-one invariant is preserved by restricting the index to type='transfer'.
--
-- See docs/superpowers/specs/2026-05-30-payment-refund-design.md.

DROP INDEX uq_transactions_reverses;

CREATE UNIQUE INDEX uq_transactions_reverses_transfer
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL AND type = 'transfer';

-- The plain partial index idx_transactions_reverses_transaction_id from the
-- previous migration is unchanged — it supports both find_reversal_of (single
-- row, transfers) and find_refunds_of (many rows, payments).
```

- [ ] **Step 2: Verify the migration applies cleanly against a fresh DB.**

Run: `DB_NAME=pba_service_test just migrate`

Expected: migration runs without error. In psql, `\d transactions` should show `uq_transactions_reverses_transfer` (with the `type='transfer'` predicate) and no `uq_transactions_reverses`.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/db/migrations/20260530000001_payment_refund.sql
git commit -m "feat(db): tighten reverses_transaction_id uniqueness to type='transfer'

Replaces uq_transactions_reverses with uq_transactions_reverses_transfer
restricted to type='transfer', so payment refunds can have multiple rows
pointing at the same original payment row while transfer reversal's
at-most-one invariant is preserved."
```

---

## Task 2: Error variants

**Files:**
- Modify: `crates/pba_service/src/error.rs`

- [ ] **Step 1: Write the failing tests** at the bottom of the existing `#[cfg(test)] mod tests` block in `error.rs`.

```rust
#[tokio::test]
async fn refund_not_refundable_error_response() {
    let err = AppError::RefundNotRefundable("abc".into(), "is_itself_a_refund".into());
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap(),
    ).unwrap();
    assert_eq!(body["error"], "RefundNotRefundable");
    assert!(body["message"].as_str().unwrap().contains("abc"));
}

#[tokio::test]
async fn refund_amount_invalid_error_response() {
    let err = AppError::RefundAmountInvalid { requested: 1500, remaining: 1000 };
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap(),
    ).unwrap();
    assert_eq!(body["error"], "RefundAmountInvalid");
    let msg = body["message"].as_str().unwrap();
    assert!(msg.contains("1500"));
    assert!(msg.contains("1000"));
}

#[tokio::test]
async fn payment_fully_refunded_error_response() {
    let err = AppError::PaymentFullyRefunded("xyz".into());
    let resp = err.into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap(),
    ).unwrap();
    assert_eq!(body["error"], "PaymentFullyRefunded");
    assert!(body["message"].as_str().unwrap().contains("xyz"));
}
```

- [ ] **Step 2: Run the tests to verify they fail.**

Run: `cargo test -p pba-service error::tests::refund -- --nocapture`
Expected: FAIL — variants do not exist.

- [ ] **Step 3: Add the variants.**

In `crates/pba_service/src/error.rs`, add to the `AppError` enum (near the existing `TransferAlreadyReversed`, `ReversalAmountInvalid` block — keep variants grouped):

```rust
/// A payment cannot be refunded. `reason` is one of: not_settled,
/// is_itself_a_refund, wrong_type, wrong_account.
RefundNotRefundable(String, String),

/// Refund amount is invalid (0 or exceeds remaining).
RefundAmountInvalid { requested: u64, remaining: u64 },

/// Payment has already been fully refunded (sum of refunds == original).
PaymentFullyRefunded(String),
```

Add the matching `Display` arms in the `impl fmt::Display for AppError`:

```rust
Self::RefundNotRefundable(id, reason) => {
    write!(f, "Payment {id} cannot be refunded: {reason}")
}
Self::RefundAmountInvalid { requested, remaining } => write!(
    f,
    "Refund amount invalid: requested {requested}, remaining refundable {remaining}"
),
Self::PaymentFullyRefunded(id) => {
    write!(f, "Payment {id} has already been fully refunded")
}
```

Add the HTTP status mapping in `into_response`:

```rust
AppError::RefundNotRefundable(_, _) => (StatusCode::CONFLICT, "RefundNotRefundable"),
AppError::RefundAmountInvalid { .. } => (StatusCode::BAD_REQUEST, "RefundAmountInvalid"),
AppError::PaymentFullyRefunded(_) => (StatusCode::CONFLICT, "PaymentFullyRefunded"),
```

- [ ] **Step 4: Run the tests to verify they pass.**

Run: `cargo test -p pba-service error::tests::refund -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/error.rs
git commit -m "feat(error): add refund AppError variants

RefundNotRefundable (409), RefundAmountInvalid (400), and
PaymentFullyRefunded (409)."
```

---

## Task 3: Domain `type_label()` branch for Refund

**Files:**
- Modify: `crates/pba_service/src/domain/transaction.rs`

- [ ] **Step 1: Add the failing test.** Inside `#[cfg(test)] mod tests` in `transaction.rs`, after the existing `type_label_*` tests:

```rust
#[test]
fn type_label_renders_refund_for_payment_with_reverses_link() {
    let r = make(TransactionType::Payment, Some(Uuid::now_v7()));
    assert_eq!(r.type_label(), "Refund");
}

#[test]
fn type_label_renders_payment_when_no_reverses_link() {
    let r = make(TransactionType::Payment, None);
    assert_eq!(r.type_label(), "Payment");
}
```

- [ ] **Step 2: Run the tests to verify they fail.**

Run: `cargo test -p pba-service domain::transaction::tests::type_label_renders_refund -- --nocapture`
Expected: FAIL — currently returns "Payment" unconditionally.

- [ ] **Step 3: Add the branch.** In `crates/pba_service/src/domain/transaction.rs`, in the `type_label()` match, add **above** the existing `(TransactionType::Payment, _) => "Payment"` line:

```rust
(TransactionType::Payment, _) if self.reverses_transaction_id.is_some() => "Refund",
```

The full Payment block becomes:

```rust
(TransactionType::Payment, _) if self.reverses_transaction_id.is_some() => "Refund",
(TransactionType::Payment, _) => "Payment",
```

- [ ] **Step 4: Run the tests to verify they pass.**

Run: `cargo test -p pba-service domain::transaction::tests::type_label -- --nocapture`
Expected: PASS (both the new tests and the existing `type_label_*` tests).

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/domain/transaction.rs
git commit -m "feat(domain): type_label renders 'Refund' for payment rows linked to an original"
```

---

## Task 4: Repository — `find_refunds_of` + `sum_refunds_of`

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs`
- Test: same file, `#[cfg(test)]` if present, else a new integration test under `tests/repository_refunds_test.rs`. **The existing `transaction_repo` does not have a `#[cfg(test)]` module that hits Postgres directly — Cucumber covers it. So for this task we add a small `#[cfg(test)]` unit test that exercises the SQL via `sqlx::test`. Confirmed at task time whether the project already wires `sqlx::test`; if not, skip to step "promote the assertions into Cucumber feature `payment_refund.feature` Scenario 11 (per-account visibility)" and rely on that coverage. The repo helpers still get written and committed in this task; their behaviour is asserted in the service unit tests (Task 6) and the BDD scenarios.**

- [ ] **Step 1: Add the helpers.** In `crates/pba_service/src/repository/transaction_repo.rs`, after `find_reversal_of`:

```rust
/// Return every refund row (or, in general, every row whose
/// `reverses_transaction_id` matches `original_row_id`). Used by payment
/// refund history and by `sum_refunds_of` consumers that want the rows
/// themselves. Type-agnostic — works for transfer reversal too.
pub async fn find_refunds_of(
    &self,
    original_row_id: Uuid,
) -> Result<Vec<TransactionRecord>, AppError> {
    let rows = sqlx::query_as::<_, TransactionRow>(
        r#"
        SELECT id, account_id, account_kind, type, status, amount, pool, direction,
               source_ifsc, source_account, gateway_ref, timeout_seconds,
               merchant_id, merchant_mcc, description, funding_type,
               tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
               reverses_transaction_id, created_at, updated_at
        FROM transactions
        WHERE reverses_transaction_id = $1
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(original_row_id)
    .fetch_all(&self.pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.into_domain()).collect())
}

/// Sum the `amount` of every row whose `reverses_transaction_id` matches.
/// Type-agnostic. Used by `pb_payment_service::refund_payment` to compute
/// per-pool remaining-unrefunded. Returns 0 when no rows match.
pub async fn sum_refunds_of(
    &self,
    original_row_id: Uuid,
) -> Result<u64, AppError> {
    let row: (Option<i64>,) = sqlx::query_as(
        r#"SELECT COALESCE(SUM(amount), 0)::bigint
           FROM transactions
           WHERE reverses_transaction_id = $1"#,
    )
    .bind(original_row_id)
    .fetch_one(&self.pool)
    .await?;

    Ok(row.0.unwrap_or(0) as u64)
}
```

- [ ] **Step 2: Verify the crate still compiles.**

Run: `cargo check -p pba-service`
Expected: clean build.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/repository/transaction_repo.rs
git commit -m "feat(repo): find_refunds_of + sum_refunds_of

Type-agnostic helpers: find_refunds_of returns every row linked back to a
given original via reverses_transaction_id (ordered by created_at, id);
sum_refunds_of aggregates their amount. Designed to be reused when transfer
reversal moves to the multi-partial pattern."
```

---

## Task 5: Ledger — code 210 + `create_payment_refund` + `create_payment_refund_split`

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs`

- [ ] **Step 1: Add the code constant** near the other transfer codes in `ledger_repo.rs` (search for `INTERNAL_TRANSFER_REVERSAL_CODE`):

```rust
pub const PAYMENT_REFUND_CODE: u16 = 210;
```

- [ ] **Step 2: Add `create_payment_refund` (single-leg refund).** Insert near `create_internal_transfer_reversal`:

```rust
/// Single-leg payment refund — debit MERCHANT_SETTLEMENT_TB_ID, credit one
/// pool of the PB account. Used when only one of `take_self` / `take_others`
/// is non-zero.
///
/// The merchant sentinel has no balance constraint (only `LINKED` flag), so
/// debiting it is unconstrained — over-amount cases are caught upstream in
/// `pb_payment_service::refund_payment` step 4 as `RefundAmountInvalid`.
pub async fn create_payment_refund(
    &self,
    credit_pb_pool_tb_id: u128,
    amount: u64,
) -> Result<(), AppError> {
    self.create_transfer(
        MERCHANT_SETTLEMENT_TB_ID,
        credit_pb_pool_tb_id,
        amount,
        PAYMENT_REFUND_CODE,
    )
    .await
}
```

- [ ] **Step 3: Add `create_payment_refund_split` (linked pair).** Right after:

```rust
/// Linked two-leg payment refund — debit MERCHANT_SETTLEMENT_TB_ID twice,
/// credit the self-pool and the others-pool. Both transfers land atomically
/// via TB's LINKED flag. Used when both `take_self` and `take_others` are
/// non-zero.
pub async fn create_payment_refund_split(
    &self,
    credit_pb_self_tb_id: u128,
    credit_pb_others_tb_id: u128,
    amount_self: u64,
    amount_others: u64,
) -> Result<(), AppError> {
    let transfer1 = tb::Transfer::new(generate_transfer_id())
        .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
        .with_credit_account_id(credit_pb_self_tb_id)
        .with_amount(amount_self as u128)
        .with_ledger(LEDGER_INR_PAISA)
        .with_code(PAYMENT_REFUND_CODE)
        .with_flags(TransferFlags::LINKED);

    let transfer2 = tb::Transfer::new(generate_transfer_id())
        .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
        .with_credit_account_id(credit_pb_others_tb_id)
        .with_amount(amount_others as u128)
        .with_ledger(LEDGER_INR_PAISA)
        .with_code(PAYMENT_REFUND_CODE);

    self.client
        .create_transfers(vec![transfer1, transfer2])
        .await
        .map_err(|e| classify_transfer_error(e, "create_payment_refund_split"))?;

    tracing::info!(
        credit_self = %credit_pb_self_tb_id,
        credit_others = %credit_pb_others_tb_id,
        amount_self, amount_others, code = PAYMENT_REFUND_CODE,
        "Created linked payment-refund TB transfers"
    );
    Ok(())
}
```

- [ ] **Step 4: Verify the crate compiles.**

Run: `cargo check -p pba-service`
Expected: clean build.

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/repository/ledger_repo.rs
git commit -m "feat(ledger): code 210 create_payment_refund + create_payment_refund_split

Debit MERCHANT_SETTLEMENT_TB_ID, credit one or two PB pool accounts. The
split variant uses linked TB transfers for atomicity. The merchant sentinel
has no balance constraint, so over-amount is caught upstream in the service
layer."
```

---

## Task 6: Service — `pb_payment_service::refund_payment`

This task introduces the new service method. It is the largest task in the plan; it is broken into sub-steps with one failing test per behaviour, so each commit is small and reviewable.

**Files:**
- Modify: `crates/pba_service/src/service/pb_payment_service.rs`

### Task 6a: scaffold `RefundResult` and stub `refund_payment`

- [ ] **Step 1: Add the `RefundResult` struct** at the bottom of `pb_payment_service.rs`, beside `PaymentResult`:

```rust
pub struct RefundResult {
    pub refund_id: Uuid,
    pub original_payment_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub amount_to_self: u64,
    pub amount_to_others: u64,
    pub original_amount: u64,
    pub remaining_refundable: u64,
    pub status: crate::domain::transaction::TransactionStatus,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

- [ ] **Step 2: Stub `refund_payment`** as a method on `PbPaymentService` returning `unimplemented!()`. This lets later sub-tasks pin behaviour via failing tests one-by-one without breaking the build.

```rust
#[allow(clippy::too_many_arguments)]
pub async fn refund_payment(
    &self,
    _pb_account_id: Uuid,
    _original_payment_id: Uuid,
    _amount: u64,
    _description: Option<&str>,
    _gateway_ref: Option<&str>,
    _idempotency_key: Option<&str>,
) -> Result<RefundResult, AppError> {
    unimplemented!("refund_payment — implemented in task 6b–6h")
}
```

- [ ] **Step 3: Verify compile.**

Run: `cargo check -p pba-service`
Expected: clean build (the stub uses `_`-prefixed args so no unused warnings).

- [ ] **Step 4: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "feat(service): scaffold RefundResult + refund_payment stub"
```

### Task 6b: full refund of single-pool (others-only) payment — happy path

This sub-task is the **first end-to-end pass** through `refund_payment`. It implements just enough of the method to satisfy a full-refund-of-an-others-only-payment test, exercising loading the original, validating, allocating, inserting, calling TB, and committing. Later sub-tasks add the other cases (split payment, partial, multi-partial, error paths, idempotency).

- [ ] **Step 1: Write the failing unit test.** Add to `#[cfg(test)] mod tests` in `pb_payment_service.rs` (or extend the existing one if present):

```rust
#[tokio::test]
async fn refund_full_others_only_payment() {
    // Arrange: make a payment that lands entirely in the others-pool
    //   - PB account active, allowed MCC
    //   - others-pool funded with ₹1000
    //   - self-pool empty
    //   - make payment for ₹500 (all from others)
    //
    // Act: refund ₹500.
    //
    // Assert:
    //   - RefundResult: amount=500, amount_to_self=0, amount_to_others=500,
    //     original_amount=500, remaining_refundable=0, status=Settled
    //   - exactly one refund row in PG: pool='others', direction='inbound',
    //     status='settled', reverses_transaction_id=original_others_row.id,
    //     correlation_id != original.correlation_id
    //   - others-pool TB balance back to ₹1000 (credits - debits = 1000)
    //   - merchant sentinel TB balance back to 0 net
    let (state, pb_id, _holder) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let payment = state
        .pb_payment_service
        .make_payment(pb_id, 50000, "MEDS", "M1", "first", None, None)
        .await
        .expect("payment");

    let refund = state
        .pb_payment_service
        .refund_payment(pb_id, payment.payment_id, 50000, None, None, None)
        .await
        .expect("refund");

    assert_eq!(refund.amount, 50000);
    assert_eq!(refund.amount_to_self, 0);
    assert_eq!(refund.amount_to_others, 50000);
    assert_eq!(refund.original_amount, 50000);
    assert_eq!(refund.remaining_refundable, 0);

    let rows = state
        .transaction_repo
        .find_by_correlation_id(refund.correlation_id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pool.as_deref(), Some("others"));
    assert_eq!(rows[0].direction, TransactionDirection::Inbound);
    assert_eq!(rows[0].status, TransactionStatus::Settled);
    let original_rows = state
        .transaction_repo
        .find_by_correlation_id(payment.payment_id)
        .await
        .unwrap();
    assert_eq!(rows[0].reverses_transaction_id, Some(original_rows[0].id));
}
```

The `setup_pb_account_with_mcc(others_paisa, self_paisa, mcc)` helper does not yet exist; add it in the same `mod tests` (or a `tests/common/mod.rs` if the codebase already has unit-test fixtures — confirmed at task time). Sketch:

```rust
async fn setup_pb_account_with_mcc(
    others_paisa: u64,
    self_paisa: u64,
    mcc: &str,
) -> (AppState, Uuid, String) {
    // Use the same test bootstrap the existing pb_payment_service tests use.
    // If no such tests exist in this file, fall back to a Cucumber-only
    // assertion of this scenario by deleting this unit test and adding the
    // corresponding scenario to `payment_refund.feature` (Task 10).
    todo!("plumb test fixture using whatever convention the codebase uses")
}
```

**If `pb_payment_service.rs` has no existing unit tests that exercise the DB**, drop the unit test from this sub-task and rely on the Cucumber happy-path scenario in Task 10 for coverage. The intent is to TDD wherever the codebase already supports it; do not invent a test infrastructure just for this task.

- [ ] **Step 2: Run the test to verify it fails.**

Run: `cargo test -p pba-service refund_full_others_only_payment -- --nocapture`
Expected: FAIL with `unimplemented!` panic.

- [ ] **Step 3: Implement the minimal flow.** Replace the body of `refund_payment` with the steps 1, 2, 3, 5, 6, 7-(others only), 8-(single), 9, 10 from the spec; defer steps 4 (over-amount validation), self-leg-row insertion, and idempotency replay to later sub-tasks. Skeleton:

```rust
pub async fn refund_payment(
    &self,
    pb_account_id: Uuid,
    original_payment_id: Uuid,
    amount: u64,
    description: Option<&str>,
    gateway_ref: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<RefundResult, AppError> {
    // Step 1: idempotency replay — deferred to Task 6h.

    // Step 2: load original payment rows.
    let original_rows = self
        .transaction_repo
        .find_by_correlation_id(original_payment_id)
        .await?;
    if original_rows.is_empty() {
        return Err(AppError::TransactionNotFound(original_payment_id.to_string()));
    }
    for row in &original_rows {
        if row.account_id != pb_account_id {
            return Err(AppError::RefundNotRefundable(
                original_payment_id.to_string(),
                "wrong_account".into(),
            ));
        }
        if row.transaction_type != TransactionType::Payment {
            return Err(AppError::RefundNotRefundable(
                original_payment_id.to_string(),
                "wrong_type".into(),
            ));
        }
        if row.status != TransactionStatus::Settled {
            return Err(AppError::RefundNotRefundable(
                original_payment_id.to_string(),
                "not_settled".into(),
            ));
        }
        if row.reverses_transaction_id.is_some() {
            return Err(AppError::RefundNotRefundable(
                original_payment_id.to_string(),
                "is_itself_a_refund".into(),
            ));
        }
    }

    let p_self = original_rows.iter().find(|r| r.pool.as_deref() == Some("self"));
    let p_others = original_rows.iter().find(|r| r.pool.as_deref() == Some("others"));

    // Step 3: per-pool remaining.
    let self_remaining = match p_self {
        Some(r) => r.amount - self.transaction_repo.sum_refunds_of(r.id).await?,
        None => 0,
    };
    let others_remaining = match p_others {
        Some(r) => r.amount - self.transaction_repo.sum_refunds_of(r.id).await?,
        None => 0,
    };
    let total_remaining = self_remaining + others_remaining;

    // Step 4: amount validation — full impl in Task 6g. For now, just reject 0
    // and reject > total_remaining so the happy path test is well-formed.
    if amount == 0 || amount > total_remaining {
        if total_remaining == 0 {
            return Err(AppError::PaymentFullyRefunded(original_payment_id.to_string()));
        }
        return Err(AppError::RefundAmountInvalid {
            requested: amount,
            remaining: total_remaining,
        });
    }

    // Step 5: PB account active check.
    let account = self.account_repo.get_account(pb_account_id).await?;
    if !account.status.is_active() {
        return Err(AppError::PbAccountNotActive(pb_account_id.to_string()));
    }

    // Step 6: allocate self-first.
    let take_self = amount.min(self_remaining);
    let take_others = amount - take_self;

    // Step 7 + 8 + 9 + 10: insert refund rows and execute TB transfers.
    let refund_correlation_id = Uuid::now_v7();
    let mut tx = self.transaction_repo.pool().begin().await?;

    // Self-leg row (when applicable).
    let self_row_id = if take_self > 0 {
        let id = Uuid::now_v7();
        let p_self_row = p_self.expect("take_self>0 requires p_self");
        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                id,
                pb_account_id,
                crate::domain::account_kind::AccountKind::Pb,
                TransactionType::Payment,
                TransactionStatus::Settled,
                take_self,
                Some("self"),
                TransactionDirection::Inbound,
                None,
                None,
                gateway_ref,
                None,
                p_self_row.merchant_id.as_deref(),
                p_self_row.merchant_mcc.as_deref(),
                description,
                p_self_row.funding_type.as_deref(),
                0,
                idempotency_key,
                Some(refund_correlation_id),
                Some(p_self_row.id),
            )
            .await?;
        Some(id)
    } else {
        None
    };

    // Others-leg row (when applicable). idempotency_key only on the primary
    // (self-leg if present, else others-leg).
    let others_row_id = if take_others > 0 {
        let id = Uuid::now_v7();
        let p_others_row = p_others.expect("take_others>0 requires p_others");
        let idem = if self_row_id.is_none() { idempotency_key } else { None };
        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                id,
                pb_account_id,
                crate::domain::account_kind::AccountKind::Pb,
                TransactionType::Payment,
                TransactionStatus::Settled,
                take_others,
                Some("others"),
                TransactionDirection::Inbound,
                None,
                None,
                gateway_ref,
                None,
                p_others_row.merchant_id.as_deref(),
                p_others_row.merchant_mcc.as_deref(),
                description,
                p_others_row.funding_type.as_deref(),
                0,
                idem,
                Some(refund_correlation_id),
                Some(p_others_row.id),
            )
            .await?;
        Some(id)
    } else {
        None
    };

    // Step 8: TB transfer(s).
    if take_self > 0 && take_others > 0 {
        self.ledger_repo
            .create_payment_refund_split(
                account.tb_self_account_id,
                account.tb_others_account_id,
                take_self,
                take_others,
            )
            .await?;
    } else if take_self > 0 {
        self.ledger_repo
            .create_payment_refund(account.tb_self_account_id, take_self)
            .await?;
    } else {
        self.ledger_repo
            .create_payment_refund(account.tb_others_account_id, take_others)
            .await?;
    }

    // Step 9 deferred: persisting tb_transfer_id on the refund rows. The
    // existing payment service also leaves tb_transfer_id=0 on its rows
    // (TB ids are not currently surfaced from create_transfer / create_linked_transfers).
    // For symmetry we leave it 0 here too; future symmetry work can wire it
    // through both paths in one shot.

    tx.commit().await?;

    let original_amount: u64 = original_rows.iter().map(|r| r.amount).sum();
    let remaining_refundable = total_remaining - amount;

    Ok(RefundResult {
        refund_id: refund_correlation_id,
        original_payment_id,
        account_id: pb_account_id,
        amount,
        amount_to_self: take_self,
        amount_to_others: take_others,
        original_amount,
        remaining_refundable,
        status: TransactionStatus::Settled,
        correlation_id: refund_correlation_id,
        created_at: chrono::Utc::now(),
    })
}
```

(Note on `tb_transfer_id`: the existing `create_transfer` / `create_linked_transfers` helpers return `()`, not the TB-side ids. Surfacing those ids is outside this spec's scope — see the "TB transfer id propagation" item in `WHAT.md` future-work.)

- [ ] **Step 4: Run the test to verify it passes.**

Run: `cargo test -p pba-service refund_full_others_only_payment -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "feat(service): refund_payment — full refund of others-only payment

Loads the original, validates state, allocates self-first, inserts the
refund row, calls TB code 210. Partial / split / idempotency cases land in
follow-up commits within the same task."
```

### Task 6c: full refund of split (self + others) payment

- [ ] **Step 1: Write the failing test.**

```rust
#[tokio::test]
async fn refund_full_split_payment() {
    // Arrange: ₹400 self + ₹600 others available, make a ₹1000 payment
    // → others-first allocation produces P_others=600, P_self=400.
    // Refund the full ₹1000.
    let (state, pb_id, _holder) = setup_pb_account_with_mcc(60000, 40000, "MEDS").await;
    let p = state.pb_payment_service
        .make_payment(pb_id, 100000, "MEDS", "M1", "split", None, None)
        .await.expect("payment");

    let r = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 100000, None, None, None)
        .await.expect("refund");

    assert_eq!(r.amount, 100000);
    assert_eq!(r.amount_to_self, 40000);
    assert_eq!(r.amount_to_others, 60000);
    assert_eq!(r.remaining_refundable, 0);

    let rows = state.transaction_repo.find_by_correlation_id(r.correlation_id).await.unwrap();
    assert_eq!(rows.len(), 2);
    let self_row = rows.iter().find(|x| x.pool.as_deref() == Some("self")).unwrap();
    let others_row = rows.iter().find(|x| x.pool.as_deref() == Some("others")).unwrap();
    assert_eq!(self_row.amount, 40000);
    assert_eq!(others_row.amount, 60000);
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p pba-service refund_full_split_payment -- --nocapture`
Expected: PASS without changes — Task 6b's implementation already handles split.

(If it fails, the most likely cause is the self/others lookup in step 6 of `refund_payment` misclassifying the rows. Confirm `p_self` and `p_others` are resolved by `pool == "self"` / `pool == "others"` and not by `direction`.)

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "test(service): refund_payment full-refund split-payment scenario"
```

### Task 6d: partial refund — self-only

- [ ] **Step 1: Write the failing test.**

```rust
#[tokio::test]
async fn refund_partial_self_only_against_split_payment() {
    // ₹400 self + ₹600 others available, ₹1000 payment (P_others=600, P_self=400)
    // Refund ₹300 → all to self (self_remaining=400 ≥ 300).
    let (state, pb_id, _) = setup_pb_account_with_mcc(60000, 40000, "MEDS").await;
    let p = state.pb_payment_service
        .make_payment(pb_id, 100000, "MEDS", "M1", "split", None, None)
        .await.unwrap();

    let r = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 30000, None, None, None)
        .await.unwrap();

    assert_eq!(r.amount_to_self, 30000);
    assert_eq!(r.amount_to_others, 0);
    assert_eq!(r.remaining_refundable, 70000);
    let rows = state.transaction_repo.find_by_correlation_id(r.correlation_id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pool.as_deref(), Some("self"));
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p pba-service refund_partial_self_only -- --nocapture`
Expected: PASS without changes (self-first allocation already implemented).

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "test(service): refund_payment partial-self-only scenario"
```

### Task 6e: partial refund — spanning self + others

- [ ] **Step 1: Write the failing test.**

```rust
#[tokio::test]
async fn refund_partial_spans_self_then_others() {
    // ₹400 self + ₹600 others, ₹1000 payment. Refund ₹500 → ₹400 self + ₹100 others.
    let (state, pb_id, _) = setup_pb_account_with_mcc(60000, 40000, "MEDS").await;
    let p = state.pb_payment_service
        .make_payment(pb_id, 100000, "MEDS", "M1", "split", None, None)
        .await.unwrap();

    let r = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 50000, None, None, None)
        .await.unwrap();

    assert_eq!(r.amount_to_self, 40000);
    assert_eq!(r.amount_to_others, 10000);
    assert_eq!(r.remaining_refundable, 50000);
    let rows = state.transaction_repo.find_by_correlation_id(r.correlation_id).await.unwrap();
    assert_eq!(rows.len(), 2);
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p pba-service refund_partial_spans_self_then_others -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "test(service): refund_payment partial-spans-self-then-others scenario"
```

### Task 6f: sequential partial refunds totalling original

- [ ] **Step 1: Write the failing test.**

```rust
#[tokio::test]
async fn refund_sequential_partials_total_original() {
    let (state, pb_id, _) = setup_pb_account_with_mcc(60000, 40000, "MEDS").await;
    let p = state.pb_payment_service
        .make_payment(pb_id, 100000, "MEDS", "M1", "split", None, None)
        .await.unwrap();

    let r1 = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 50000, None, None, None)
        .await.unwrap();
    assert_eq!(r1.amount_to_self, 40000);
    assert_eq!(r1.amount_to_others, 10000);

    let r2 = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 50000, None, None, None)
        .await.unwrap();
    assert_eq!(r2.amount_to_self, 0);
    assert_eq!(r2.amount_to_others, 50000);
    assert_eq!(r2.remaining_refundable, 0);

    // Third attempt: PaymentFullyRefunded.
    let err = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 1, None, None, None)
        .await.unwrap_err();
    assert!(matches!(err, AppError::PaymentFullyRefunded(_)));
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p pba-service refund_sequential_partials -- --nocapture`
Expected: PASS — the existing self-first allocator naturally produces these breakdowns.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "test(service): refund_payment sequential partials, third attempt fully-refunded"
```

### Task 6g: error paths

- [ ] **Step 1: Add failing tests.**

```rust
#[tokio::test]
async fn refund_amount_zero_rejected() {
    let (state, pb_id, _) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let p = state.pb_payment_service.make_payment(pb_id, 50000, "MEDS", "M1", "x", None, None).await.unwrap();
    let err = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 0, None, None, None)
        .await.unwrap_err();
    assert!(matches!(err, AppError::RefundAmountInvalid { requested: 0, .. }));
}

#[tokio::test]
async fn refund_amount_over_total_remaining_rejected() {
    let (state, pb_id, _) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let p = state.pb_payment_service.make_payment(pb_id, 50000, "MEDS", "M1", "x", None, None).await.unwrap();
    let err = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 50001, None, None, None)
        .await.unwrap_err();
    let AppError::RefundAmountInvalid { requested, remaining } = err else {
        panic!("expected RefundAmountInvalid");
    };
    assert_eq!(requested, 50001);
    assert_eq!(remaining, 50000);
}

#[tokio::test]
async fn refund_of_refund_rejected() {
    let (state, pb_id, _) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let p = state.pb_payment_service.make_payment(pb_id, 50000, "MEDS", "M1", "x", None, None).await.unwrap();
    let r1 = state.pb_payment_service.refund_payment(pb_id, p.payment_id, 50000, None, None, None).await.unwrap();
    // Try to refund the refund.
    let err = state.pb_payment_service
        .refund_payment(pb_id, r1.refund_id, 1, None, None, None)
        .await.unwrap_err();
    assert!(matches!(err, AppError::RefundNotRefundable(_, reason) if reason == "is_itself_a_refund"));
}

#[tokio::test]
async fn refund_when_account_frozen_rejected() {
    let (state, pb_id, _) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let p = state.pb_payment_service.make_payment(pb_id, 50000, "MEDS", "M1", "x", None, None).await.unwrap();
    state.pb_account_service.update_status(pb_id, AccountStatus::Frozen).await.unwrap();
    let err = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 50000, None, None, None)
        .await.unwrap_err();
    assert!(matches!(err, AppError::PbAccountNotActive(_)));
}

#[tokio::test]
async fn refund_wrong_account_rejected() {
    let (state, pb_a, _) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let (_, pb_b, _) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let p = state.pb_payment_service.make_payment(pb_a, 50000, "MEDS", "M1", "x", None, None).await.unwrap();
    let err = state.pb_payment_service
        .refund_payment(pb_b, p.payment_id, 50000, None, None, None)
        .await.unwrap_err();
    assert!(matches!(err, AppError::RefundNotRefundable(_, reason) if reason == "wrong_account"));
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p pba-service refund_ -- --nocapture`
Expected: PASS — Task 6b's implementation already handles all of these branches.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "test(service): refund_payment error paths (amount, refund-of-refund, frozen, wrong account)"
```

### Task 6h: idempotency replay

- [ ] **Step 1: Write the failing test.**

```rust
#[tokio::test]
async fn refund_idempotency_replay() {
    let (state, pb_id, _) = setup_pb_account_with_mcc(50000, 0, "MEDS").await;
    let p = state.pb_payment_service.make_payment(pb_id, 50000, "MEDS", "M1", "x", None, None).await.unwrap();

    let r1 = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 50000, None, None, Some("idem-1"))
        .await.unwrap();
    let r2 = state.pb_payment_service
        .refund_payment(pb_id, p.payment_id, 50000, None, None, Some("idem-1"))
        .await.unwrap();

    assert_eq!(r1.correlation_id, r2.correlation_id);
    assert_eq!(r1.amount, r2.amount);
    // PG should still have just one refund row (no second insert).
    let refund_rows = state.transaction_repo.find_by_correlation_id(r1.correlation_id).await.unwrap();
    assert_eq!(refund_rows.len(), 1);
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p pba-service refund_idempotency_replay -- --nocapture`
Expected: FAIL — Task 6b deferred idempotency.

- [ ] **Step 3: Implement idempotency replay.** At the very start of `refund_payment`, before the original-rows load, add:

```rust
if let Some(key) = idempotency_key {
    if let Some(existing) = self
        .transaction_repo
        .find_by_idempotency_key(
            crate::domain::account_kind::AccountKind::Pb,
            pb_account_id,
            key,
        )
        .await?
    {
        let correlation_id = existing.correlation_id.unwrap_or(existing.id);
        let refund_rows = self
            .transaction_repo
            .find_by_correlation_id(correlation_id)
            .await?;
        let amount_to_self: u64 = refund_rows.iter()
            .filter(|r| r.pool.as_deref() == Some("self"))
            .map(|r| r.amount).sum();
        let amount_to_others: u64 = refund_rows.iter()
            .filter(|r| r.pool.as_deref() == Some("others"))
            .map(|r| r.amount).sum();
        let amount = amount_to_self + amount_to_others;

        // Recompute original_amount + remaining_refundable for the response.
        let original_payment_id_resolved = refund_rows.first()
            .and_then(|r| r.reverses_transaction_id)
            .map(|rev| self.transaction_repo.get_transaction(rev))
            .expect("refund row carries reverses_transaction_id")
            .await?;
        let originals = self
            .transaction_repo
            .find_by_correlation_id(original_payment_id_resolved.correlation_id.unwrap_or(original_payment_id_resolved.id))
            .await?;
        let original_amount: u64 = originals.iter().map(|r| r.amount).sum();
        let total_refunded: u64 = {
            let mut sum = 0u64;
            for o in &originals {
                sum += self.transaction_repo.sum_refunds_of(o.id).await?;
            }
            sum
        };
        let remaining_refundable = original_amount - total_refunded;

        return Ok(RefundResult {
            refund_id: correlation_id,
            original_payment_id: originals[0].correlation_id.unwrap_or(originals[0].id),
            account_id: pb_account_id,
            amount,
            amount_to_self,
            amount_to_others,
            original_amount,
            remaining_refundable,
            status: existing.status,
            correlation_id,
            created_at: existing.created_at,
        });
    }
}
```

- [ ] **Step 4: Run.**

Run: `cargo test -p pba-service refund_idempotency_replay -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/service/pb_payment_service.rs
git commit -m "feat(service): refund_payment idempotency replay

On idempotency_key hit: re-load refund rows by correlation_id and return the
identical RefundResult without a second TB call or PG insert."
```

---

## Task 7: DTOs

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`

- [ ] **Step 1: Add the DTOs.** Append to `api/dto.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct RefundPaymentRequest {
    pub amount: u64,
    pub description: Option<String>,
    pub gateway_ref: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RefundResponse {
    pub refund_id: Uuid,
    pub original_payment_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub amount_to_self: u64,
    pub amount_to_others: u64,
    pub original_amount: u64,
    pub remaining_refundable: u64,
    pub status: String,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::pb_payment_service::RefundResult> for RefundResponse {
    fn from(r: crate::service::pb_payment_service::RefundResult) -> Self {
        Self {
            refund_id: r.refund_id,
            original_payment_id: r.original_payment_id,
            account_id: r.account_id,
            amount: r.amount,
            amount_to_self: r.amount_to_self,
            amount_to_others: r.amount_to_others,
            original_amount: r.original_amount,
            remaining_refundable: r.remaining_refundable,
            status: r.status.as_str().to_string(),
            correlation_id: r.correlation_id,
            created_at: r.created_at,
        }
    }
}
```

- [ ] **Step 2: Verify compile.**

Run: `cargo check -p pba-service`
Expected: clean build.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/api/dto.rs
git commit -m "feat(api): refund DTOs + From<RefundResult>"
```

---

## Task 8: API handler + route

**Files:**
- Modify: `crates/pba_service/src/api/handlers/pb.rs`
- Modify: `crates/pba_service/src/api/routes.rs`

- [ ] **Step 1: Add the handler** in `crates/pba_service/src/api/handlers/pb.rs`, right after `make_payment`:

```rust
pub async fn refund_payment(
    State(state): State<AppState>,
    Path((account_id, payment_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<RefundPaymentRequest>,
) -> Result<(axum::http::StatusCode, Json<RefundResponse>), AppError> {
    let result = state
        .pb_payment_service
        .refund_payment(
            account_id,
            payment_id,
            req.amount,
            req.description.as_deref(),
            req.gateway_ref.as_deref(),
            req.idempotency_key.as_deref(),
        )
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(result.into())))
}
```

Add the necessary imports at the top of `pb.rs` (`RefundPaymentRequest`, `RefundResponse`).

- [ ] **Step 2: Register the route** in `api/routes.rs`, inside the `/pb-accounts/...` block (right after the `payments` POST):

```rust
.route(
    "/pb-accounts/{account_id}/payments/{payment_id}/refund",
    post(handlers::pb::refund_payment),
)
```

- [ ] **Step 3: Verify compile.**

Run: `cargo check -p pba-service`
Expected: clean build.

- [ ] **Step 4: Commit.**

```bash
git add crates/pba_service/src/api/handlers/pb.rs crates/pba_service/src/api/routes.rs
git commit -m "feat(api): POST /pb-accounts/{id}/payments/{id}/refund

Thin handler wrapping PbPaymentService::refund_payment."
```

---

## Task 9: Smithy operation + SDK regen

**Files:**
- Modify: `model/payment.smithy`
- Modify: `model/main.smithy`
- Run: smithy SDK regeneration (see step 3).

- [ ] **Step 1: Add the operation** to `model/payment.smithy` (append at the end):

```smithy
/// Refund a previously settled PB→merchant payment.
///
/// Records a new compensating transaction (1 or 2 rows mirroring the
/// payment's pool split) plus matching TB transfer(s) in the opposite
/// direction. Original payment rows are not mutated; each refund row
/// links back via `reverses_transaction_id`.
///
/// Multiple partial refunds are allowed per payment; the sum must not
/// exceed the original payment amount. Refund credits self-pool first up
/// to that pool's remaining-unrefunded amount, then others-pool. The PB
/// account must be Active. Refunds cannot themselves be refunded.
@http(
    method: "POST",
    uri: "/pb-accounts/{account_id}/payments/{payment_id}/refund",
    code: 201
)
operation RefundPBAccountPayment {
    input := {
        @required @httpLabel account_id: String
        @required @httpLabel payment_id: String
        @required amount: Money
        description: String
        gateway_ref: String
        idempotency_key: String
    }
    output := with [RefundResponseMixin] {}
    errors: [AccountNotFoundError]
}

@mixin
structure RefundResponseMixin {
    @required refund_id: String
    @required original_payment_id: String
    @required account_id: String
    @required amount: Money
    @required amount_to_self: Money
    @required amount_to_others: Money
    @required original_amount: Money
    @required remaining_refundable: Money
    @required status: String
    @required correlation_id: String
    @required created_at: DateTime
}
```

- [ ] **Step 2: Register the operation** in `model/main.smithy`, adding `RefundPBAccountPayment` to the service's `operations:` list (the same place `ReverseNormalAccountTransfer` was added).

- [ ] **Step 3: Regenerate the SDK.** The project conventionally regenerates the `pba_client` crate via a justfile target. Confirm at task time:

```bash
grep -E 'smithy|build-client|gen' justfile | head -10
```

Run the indicated target (e.g. `just smithy-build` or whatever the project calls it). If no target exists, follow the same procedure the `feat(smithy): ReverseNormalAccountTransfer operation + SDK regen` commit (`d9abbc9` parent area) used — typically `smithy build` from `model/` and copying the generated Rust client over `crates/pba_client/`.

- [ ] **Step 4: Verify compile.**

Run: `cargo check -p pba-service && cargo check -p pba-client`
Expected: clean build.

- [ ] **Step 5: Commit.** Two commits keep the model change reviewable from the regen diff:

```bash
git add model/payment.smithy model/main.smithy
git commit -m "feat(smithy): RefundPBAccountPayment operation"

git add crates/pba_client/
git commit -m "feat(smithy): regen client SDK for RefundPBAccountPayment"
```

---

## Task 10: Cucumber feature — `payment_refund.feature`

**Files:**
- Create: `crates/pba_service/tests/features/payment_refund.feature`
- Modify: `crates/pba_service/tests/steps/payment_steps.rs`
- Modify: the `PbaWorld` struct (`tests/world.rs` or wherever it lives — confirmed at task time)
- Modify: `crates/pba_service/tests/e2e.rs` to register the new feature file if registration is explicit (per the existing `transfer_reversal.feature` wiring).

### Task 10a: scaffold the feature file

- [ ] **Step 1: Create the feature file** at `crates/pba_service/tests/features/payment_refund.feature`:

```gherkin
@refund
Feature: Refund of PB→merchant payments
  As an admin, I can refund a settled PB payment in whole or in part
  multiple times, with each refund credited self-pool first then others-pool,
  while the original payment rows are never mutated.

  Background:
    Given a fresh test environment

  Scenario: Full refund of an others-only payment
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    Then the payment succeeds
    When I refund the last payment for 50000 paisa
    Then the refund succeeds with amount_to_self 0 and amount_to_others 50000
    And the remaining refundable amount is 0
    And the PB account others-pool balance is 50000 paisa

  Scenario: Full refund of a split (self + others) payment
    Given a PB account "acct-1" with purpose "health"
    And the account has 60000 paisa in its others-pool
    And the account has 40000 paisa in its self-pool
    When I make a 100000 paisa payment to merchant "M1" mcc "8011"
    Then the payment succeeds
    When I refund the last payment for 100000 paisa
    Then the refund succeeds with amount_to_self 40000 and amount_to_others 60000
    And the remaining refundable amount is 0

  Scenario: Partial refund — self-only
    Given a PB account "acct-1" with purpose "health"
    And the account has 60000 paisa in its others-pool
    And the account has 40000 paisa in its self-pool
    When I make a 100000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 30000 paisa
    Then the refund succeeds with amount_to_self 30000 and amount_to_others 0
    And the remaining refundable amount is 70000

  Scenario: Partial refund — spans self then others
    Given a PB account "acct-1" with purpose "health"
    And the account has 60000 paisa in its others-pool
    And the account has 40000 paisa in its self-pool
    When I make a 100000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50000 paisa
    Then the refund succeeds with amount_to_self 40000 and amount_to_others 10000
    And the remaining refundable amount is 50000

  Scenario: Sequential partial refunds totalling original
    Given a PB account "acct-1" with purpose "health"
    And the account has 60000 paisa in its others-pool
    And the account has 40000 paisa in its self-pool
    When I make a 100000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50000 paisa
    And I refund the last payment for 50000 paisa
    Then the most recent refund credited 0 to self and 50000 to others
    And the remaining refundable amount is 0
    When I refund the last payment for 1 paisa
    Then the refund fails with "PaymentFullyRefunded"

  Scenario: Reject amount over total remaining
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50001 paisa
    Then the refund fails with "RefundAmountInvalid"
    And the refund error remaining field is 50000

  Scenario: Reject amount = 0
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 0 paisa
    Then the refund fails with "RefundAmountInvalid"

  Scenario: Reject refunding a refund row
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50000 paisa
    And I refund the last refund for 1 paisa
    Then the refund fails with reason "is_itself_a_refund"

  Scenario: Reject when PB account frozen, succeed after reactivate
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I freeze the PB account
    And I refund the last payment for 50000 paisa
    Then the refund fails with "PbAccountNotActive"
    When I reactivate the PB account
    And I refund the last payment for 50000 paisa
    Then the refund succeeds with amount_to_self 0 and amount_to_others 50000

  Scenario: Idempotency replay
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50000 paisa with idempotency key "refund-1"
    And I refund the last payment for 50000 paisa with idempotency key "refund-1"
    Then both refunds share the same correlation_id
    And only one refund row exists for the payment

  Scenario: Per-account visibility
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50000 paisa
    Then GET /pb-accounts/{acct-1}/transactions returns the original payment and the refund row
    And the refund row's type_label renders as "Refund"

  Scenario: Wrong PB account in URL
    Given a PB account "acct-A" with purpose "health"
    And the account has 50000 paisa in its others-pool
    And a PB account "acct-B" with purpose "health"
    When I make a 50000 paisa payment from "acct-A" to merchant "M1" mcc "8011"
    And I attempt to refund the last payment for 50000 paisa under "acct-B"
    Then the refund fails with reason "wrong_account"

  Scenario: Linked-transfer atomicity (TB failure rolls back PG)
    # This is a probe of the spec's atomicity claim. If the test harness
    # cannot inject TB failures, retain this scenario tagged @manual for
    # documentation purposes and skip from the @refund automated run.
    @manual
    Given a PB account "acct-1" with purpose "health"
    And the account has 60000 paisa in its others-pool
    And the account has 40000 paisa in its self-pool
    When I make a 100000 paisa payment to merchant "M1" mcc "8011"
    And TigerBeetle is configured to fail the next linked-transfer chain
    And I refund the last payment for 50000 paisa
    Then the refund fails
    And no refund rows exist for the payment
    And the others-pool TB balance is unchanged
```

- [ ] **Step 2: Wire the feature file** into `tests/e2e.rs` if registration is explicit. (The existing `transfer_reversal.feature` is wired by adding an entry to the feature file list — replicate that line, swapping `transfer_reversal` for `payment_refund`.)

- [ ] **Step 3: Commit the scaffold.**

```bash
git add crates/pba_service/tests/features/payment_refund.feature crates/pba_service/tests/e2e.rs
git commit -m "test(e2e): scaffold payment_refund.feature (steps land next)"
```

### Task 10b: extend `PbaWorld` + add step bindings

- [ ] **Step 1: Extend `PbaWorld`.** Add fields:

```rust
pub last_refund_id: Option<String>,
pub last_refund_correlation_id: Option<String>,
pub last_refund_amount_to_self: Option<u64>,
pub last_refund_amount_to_others: Option<u64>,
pub last_refund_remaining: Option<u64>,
pub last_refund_error: Option<crate::PbaError>,
```

(`PbaError` is the existing world-side wrapper; if its shape needs an extra `reason` field, follow the same edit transfer reversal made — see Task 9 of the transfer-reversal plan.)

- [ ] **Step 2: Add step bindings** in `crates/pba_service/tests/steps/payment_steps.rs`. For each Gherkin step phrase in the feature, add a `#[when]` / `#[then]` binding. Sketch (one per step phrase — fill in to match the feature exactly):

```rust
#[when(expr = "I refund the last payment for {int} paisa")]
async fn refund_last_payment(world: &mut PbaWorld, amount: u64) {
    let payment_id = world.last_payment_id.clone().expect("no prior payment");
    let account_id = world.current_account_id.clone().expect("no current account");
    let resp = world.client
        .refund_pb_account_payment()
        .account_id(account_id)
        .payment_id(payment_id)
        .amount(amount as i64)
        .send().await;
    match resp {
        Ok(r) => {
            world.last_refund_id = Some(r.refund_id().to_string());
            world.last_refund_correlation_id = Some(r.correlation_id().to_string());
            world.last_refund_amount_to_self = Some(r.amount_to_self() as u64);
            world.last_refund_amount_to_others = Some(r.amount_to_others() as u64);
            world.last_refund_remaining = Some(r.remaining_refundable() as u64);
            world.last_refund_error = None;
        }
        Err(e) => {
            world.last_refund_error = Some(PbaError::from(e));
            world.last_refund_id = None;
        }
    }
}

#[when(expr = "I refund the last payment for {int} paisa with idempotency key {string}")]
async fn refund_with_idem(world: &mut PbaWorld, amount: u64, key: String) {
    // Same as above but pass .idempotency_key(key).
}

#[when(expr = "I refund the last refund for {int} paisa")]
async fn refund_last_refund(world: &mut PbaWorld, amount: u64) {
    // Use world.last_refund_correlation_id as the {payment_id} URL param.
}

#[when(expr = "I attempt to refund the last payment for {int} paisa under {string}")]
async fn refund_under_wrong_account(world: &mut PbaWorld, amount: u64, account_label: String) {
    // Look up the account_id for the labelled account from world.accounts and
    // call refund_pb_account_payment with that id (which won't match the
    // payment's account).
}

#[then(expr = "the refund succeeds with amount_to_self {int} and amount_to_others {int}")]
async fn refund_succeeds_with_amounts(world: &mut PbaWorld, to_self: u64, to_others: u64) {
    assert!(world.last_refund_error.is_none(), "expected success");
    assert_eq!(world.last_refund_amount_to_self, Some(to_self));
    assert_eq!(world.last_refund_amount_to_others, Some(to_others));
}

#[then(expr = "the remaining refundable amount is {int}")]
async fn remaining_refundable_is(world: &mut PbaWorld, remaining: u64) {
    assert_eq!(world.last_refund_remaining, Some(remaining));
}

#[then(expr = "the refund fails with {string}")]
async fn refund_fails_with(world: &mut PbaWorld, code: String) {
    let err = world.last_refund_error.as_ref().expect("expected failure");
    assert_eq!(err.error_code(), code);
}

#[then(expr = "the refund fails with reason {string}")]
async fn refund_fails_with_reason(world: &mut PbaWorld, reason: String) {
    let err = world.last_refund_error.as_ref().expect("expected failure");
    assert!(err.message().contains(&reason), "expected reason {reason} in {:?}", err.message());
}

#[then(expr = "the refund error remaining field is {int}")]
async fn refund_error_remaining(world: &mut PbaWorld, expected: u64) {
    let err = world.last_refund_error.as_ref().expect("expected failure");
    assert!(err.message().contains(&format!("remaining refundable {expected}")));
}

#[then(expr = "the most recent refund credited {int} to self and {int} to others")]
async fn most_recent_credited(world: &mut PbaWorld, to_self: u64, to_others: u64) {
    assert_eq!(world.last_refund_amount_to_self, Some(to_self));
    assert_eq!(world.last_refund_amount_to_others, Some(to_others));
}

#[then(expr = "both refunds share the same correlation_id")]
async fn both_share_correlation(world: &mut PbaWorld) {
    // The step `When ... idempotency key "refund-1"` was invoked twice; the
    // last_refund_correlation_id captured each time should match. Maintain a
    // small `world.previous_refund_correlation_id` cache by snapshotting in
    // the `#[when]` step above.
    let curr = world.last_refund_correlation_id.as_ref().expect("no current refund");
    let prev = world.previous_refund_correlation_id.as_ref().expect("only one refund recorded");
    assert_eq!(curr, prev);
}

#[then(expr = "only one refund row exists for the payment")]
async fn one_refund_row(world: &mut PbaWorld) {
    // Query GET /pb-accounts/{id}/transactions, count rows whose
    // type_label is "Refund" and assert == 1.
}

#[then(expr = "GET /pb-accounts/{string}/transactions returns the original payment and the refund row")]
async fn account_lists_payment_and_refund(world: &mut PbaWorld, label: String) {
    // Fetch and assert there's exactly one Payment row and one Refund row.
}

#[then(expr = "the refund row's type_label renders as {string}")]
async fn refund_type_label(world: &mut PbaWorld, label: String) {
    // Find the inbound payment row in the listing and assert its type_label.
}

#[then(expr = "the PB account others-pool balance is {int} paisa")]
async fn pb_others_balance_is(world: &mut PbaWorld, expected: u64) {
    let bal = world.client.get_pb_account_balance().account_id(world.current_account_id.clone().unwrap()).send().await.expect("balance");
    assert_eq!(bal.others_pool() as u64, expected);
}
```

For each Gherkin step in the feature, ensure there is a corresponding binding here. The bindings above are sketches with the right signatures and assertion logic; flesh out the bodies to match the existing payment-step style (e.g. how `world.last_payment_id` is populated, how `world.client` is built).

- [ ] **Step 3: Run the feature.**

Run: `just api-e2e` (or whichever target runs Cucumber against the spawned service)
Expected: all `payment_refund.feature` scenarios pass; existing payment / transfer-reversal scenarios stay green.

- [ ] **Step 4: Commit.**

```bash
git add crates/pba_service/tests/steps/payment_steps.rs crates/pba_service/tests/world.rs
git commit -m "test(e2e): payment refund step bindings + PbaWorld fields"
```

---

## Task 11: Admin UI — Refund button + form + history block

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs` — extend `TransactionDetailTemplate` and `transaction_detail`.
- Modify: `crates/pba_service/src/admin.rs` — register two new admin routes.
- Create: `crates/pba_service/templates/admin/payment_refund.html`
- Modify: `crates/pba_service/templates/admin/transaction_detail.html`

### Task 11a: extend `TransactionDetailTemplate` and the handler

- [ ] **Step 1: Add fields** to `TransactionDetailTemplate` (in `crates/pba_service/src/admin/handlers.rs`):

```rust
// Refund-related (Payment rows only)
can_refund: bool,
is_refund: bool,
refund_history: Vec<RefundHistoryRow>,
remaining_refundable_paisa: u64,
remaining_refundable_display: String,
refund_of_payment_id: String,  // empty unless is_refund
```

And a small render-time struct:

```rust
struct RefundHistoryRow {
    pub correlation_id: String,
    pub created_at: String,
    pub amount_to_self_display: String,
    pub amount_to_others_display: String,
    pub total_display: String,
}
```

- [ ] **Step 2: Populate the fields** at the bottom of `transaction_detail`:

```rust
// Payment-refund affordances.
let is_payment = matches!(txn.transaction_type, TransactionType::Payment);
let is_refund = is_payment && txn.reverses_transaction_id.is_some();

let (can_refund, refund_history, remaining_refundable_paisa, refund_of_payment_id) = if is_payment && !is_refund {
    // This is an original payment. Build the per-row remaining + history.
    let originals = state
        .transaction_repo
        .find_by_correlation_id(txn.correlation_id.unwrap_or(txn.id))
        .await
        .unwrap_or_default();

    let mut total_original = 0u64;
    let mut total_refunded = 0u64;
    let mut history_corrs: std::collections::BTreeMap<Uuid, (chrono::DateTime<chrono::Utc>, u64, u64)> =
        Default::default();

    for o in &originals {
        total_original += o.amount;
        let refs = state.transaction_repo.find_refunds_of(o.id).await.unwrap_or_default();
        for r in refs {
            total_refunded += r.amount;
            let entry = history_corrs
                .entry(r.correlation_id.unwrap_or(r.id))
                .or_insert((r.created_at, 0, 0));
            // entry.0 = earliest created_at within the correlation; refine to min if needed.
            if r.created_at < entry.0 { entry.0 = r.created_at; }
            match r.pool.as_deref() {
                Some("self") => entry.1 += r.amount,
                Some("others") => entry.2 += r.amount,
                _ => {}
            }
        }
    }

    let remaining = total_original.saturating_sub(total_refunded);
    let can = remaining > 0; // status/account active checks are enforced by the service on POST.

    let fmt = |a: u64| format!("{}.{:02}", a / 100, a % 100);
    let history: Vec<RefundHistoryRow> = history_corrs
        .into_iter()
        .map(|(cid, (ts, to_self, to_others))| RefundHistoryRow {
            correlation_id: cid.to_string(),
            created_at: ts.format("%Y-%m-%d %H:%M:%S").to_string(),
            amount_to_self_display: fmt(to_self),
            amount_to_others_display: fmt(to_others),
            total_display: fmt(to_self + to_others),
        })
        .collect();

    (can, history, remaining, String::new())
} else if is_refund {
    // This row is itself a refund. Look up the original payment row.
    let original = match txn.reverses_transaction_id {
        Some(id) => state.transaction_repo.get_transaction(id).await.ok(),
        None => None,
    };
    let original_payment_id = original
        .as_ref()
        .and_then(|r| r.correlation_id)
        .map(|c| c.to_string())
        .unwrap_or_default();
    (false, vec![], 0, original_payment_id)
} else {
    (false, vec![], 0, String::new())
};

let remaining_refundable_display = format!(
    "{}.{:02}", remaining_refundable_paisa / 100, remaining_refundable_paisa % 100
);
```

Wire those into the `render(TransactionDetailTemplate { ... })` call.

- [ ] **Step 3: Add the admin handlers** (place in `crates/pba_service/src/admin/handlers.rs`, near the transfer reversal admin handlers — or in `pb_handlers.rs` if that's where similar PB-admin actions live; confirmed at task time):

```rust
#[derive(Template)]
#[template(path = "admin/payment_refund.html")]
struct RefundPaymentTemplate {
    prefix: String,
    account_id: String,
    payment_id: String,
    remaining_display: String,
    error: Option<String>,
}

pub async fn refund_payment_form(
    State(state): State<AppState>,
    Path((account_id, payment_id)): Path<(Uuid, Uuid)>,
) -> Response {
    // Reuse the same remaining-refundable computation as transaction_detail's
    // helper; for brevity in the plan we sketch it here. In the implementation,
    // extract that computation into a small private helper to avoid duplication.
    let originals = state
        .transaction_repo
        .find_by_correlation_id(payment_id)
        .await
        .unwrap_or_default();
    let mut total = 0u64;
    let mut refunded = 0u64;
    for o in &originals {
        total += o.amount;
        refunded += state.transaction_repo.sum_refunds_of(o.id).await.unwrap_or(0);
    }
    let remaining = total.saturating_sub(refunded);
    let remaining_display = format!("{}.{:02}", remaining / 100, remaining % 100);

    render(RefundPaymentTemplate {
        prefix: state.path_prefix.clone(),
        account_id: account_id.to_string(),
        payment_id: payment_id.to_string(),
        remaining_display,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct RefundPaymentFormData {
    amount: String,
    description: Option<String>,
}

pub async fn process_refund_payment(
    State(state): State<AppState>,
    Path((account_id, payment_id)): Path<(Uuid, Uuid)>,
    axum::Form(form): axum::Form<RefundPaymentFormData>,
) -> Response {
    let amount_paisa: u64 = match form.amount.replace('.', "").parse() {
        Ok(v) => v,
        Err(_) => {
            return render(RefundPaymentTemplate {
                prefix: state.path_prefix.clone(),
                account_id: account_id.to_string(),
                payment_id: payment_id.to_string(),
                remaining_display: String::new(),
                error: Some("Amount must be a number (paisa)".into()),
            });
        }
    };

    match state
        .pb_payment_service
        .refund_payment(account_id, payment_id, amount_paisa, form.description.as_deref(), None, None)
        .await
    {
        Ok(r) => Redirect::to(&format!(
            "{}/admin/transactions/{}",
            state.path_prefix, r.correlation_id
        )).into_response(),
        Err(e) => {
            let msg = format!("{e}");
            render(RefundPaymentTemplate {
                prefix: state.path_prefix.clone(),
                account_id: account_id.to_string(),
                payment_id: payment_id.to_string(),
                remaining_display: String::new(),
                error: Some(msg),
            })
        }
    }
}
```

- [ ] **Step 4: Register the routes** in `crates/pba_service/src/admin.rs`, inside `create_router()`:

```rust
.route(
    "/admin/accounts/{account_id}/payments/{payment_id}/refund",
    get(handlers::refund_payment_form).post(handlers::process_refund_payment),
)
```

- [ ] **Step 5: Verify compile.**

Run: `cargo check -p pba-service`
Expected: clean build.

- [ ] **Step 6: Commit.**

```bash
git add crates/pba_service/src/admin/handlers.rs crates/pba_service/src/admin.rs
git commit -m "feat(admin): refund_payment_form + process_refund_payment handlers

Compute remaining-refundable, render the form, redirect to the refund detail
on success or re-render with inline error on failure."
```

### Task 11b: form template

- [ ] **Step 1: Create the template** at `crates/pba_service/templates/admin/payment_refund.html`. Follow the same style as `templates/admin/transfer_reverse.html` (read it for the exact stat-card and form-styling conventions used):

```html
{% extends "base.html" %}
{% block title %}Refund payment — PBA Admin{% endblock %}
{% block breadcrumb %}<a href="{{ prefix }}/admin">PBA Admin</a> <span class="sep">/</span>
  <a href="{{ prefix }}/admin/accounts">PB Accounts</a> <span class="sep">/</span>
  <a href="{{ prefix }}/admin/accounts/{{ account_id }}" class="mono">{{ account_id }}</a> <span class="sep">/</span>
  <a href="{{ prefix }}/admin/transactions/{{ payment_id }}" class="mono">payment</a> <span class="sep">/</span>
  <span class="current">Refund</span>{% endblock %}

{% block content %}
<div class="page-header">
  <div>
    <h1 class="page-title">Refund payment</h1>
    <p class="page-subtitle mono">{{ payment_id }}</p>
  </div>
</div>

<article>
  <header>Refund details</header>
  <div class="kv">
    <div>Remaining refundable</div><div>₹{{ remaining_display }}</div>
  </div>
  {% if let Some(err) = error %}
  <p style="color: var(--danger); margin-top: 1rem;"><strong>Error:</strong> {{ err }}</p>
  {% endif %}
  <form method="post" action="{{ prefix }}/admin/accounts/{{ account_id }}/payments/{{ payment_id }}/refund" style="margin-top: 1rem;">
    <label>Amount (paisa)
      <input type="number" name="amount" min="1" required>
    </label>
    <label>Description (optional)
      <input type="text" name="description">
    </label>
    <button type="submit" class="btn-sm" style="background: var(--danger); margin-top: 0.5rem;">Refund</button>
  </form>
</article>
{% endblock %}
```

- [ ] **Step 2: Verify the template renders.**

Spin up the admin UI locally: `just run` (or whichever runner the repo uses), navigate to `http://localhost:3030/admin/accounts/<acct>/payments/<pay>/refund`, confirm the form shows.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/templates/admin/payment_refund.html
git commit -m "feat(admin): payment refund form template"
```

### Task 11c: Refund button + history block on transaction detail

- [ ] **Step 1: Edit `transaction_detail.html`.** At an appropriate place in the existing template (near the existing `can_post_or_void` / reversal affordances), add:

```html
{% if is_payment %}
  {% if can_refund %}
    <a href="{{ prefix }}/admin/accounts/{{ account_id }}/payments/{{ id }}/refund"
       role="button" class="btn-sm" style="background: var(--danger);">
      Refund (₹{{ remaining_refundable_display }} remaining)
    </a>
  {% endif %}

  {% if is_refund %}
    <article>
      <header>Refund of payment</header>
      <p>This row is part of a refund of
        <a href="{{ prefix }}/admin/transactions/{{ refund_of_payment_id }}" class="mono">
          {{ refund_of_payment_id }}
        </a>.
      </p>
    </article>
  {% endif %}

  {% if !refund_history.is_empty() %}
    <article>
      <header>Refund history — ₹{{ remaining_refundable_display }} remaining</header>
      <table>
        <thead>
          <tr><th>When</th><th>To self</th><th>To others</th><th>Total</th><th>Refund id</th></tr>
        </thead>
        <tbody>
          {% for r in refund_history %}
          <tr>
            <td>{{ r.created_at }}</td>
            <td>₹{{ r.amount_to_self_display }}</td>
            <td>₹{{ r.amount_to_others_display }}</td>
            <td>₹{{ r.total_display }}</td>
            <td><a href="{{ prefix }}/admin/transactions/{{ r.correlation_id }}" class="mono">
              {{ r.correlation_id|truncate(12) }}
            </a></td>
          </tr>
          {% endfor %}
        </tbody>
      </table>
    </article>
  {% endif %}
{% endif %}
```

- [ ] **Step 2: Verify compile + render.**

Run: `cargo check -p pba-service` (Askama compiles templates against the struct).
Spin up locally and walk the flow: make a payment, view the transaction, click Refund, submit, verify the refund history shows.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/templates/admin/transaction_detail.html
git commit -m "feat(admin): refund button + history block on payment detail"
```

---

## Task 12: UI cucumber — `payment_refund_admin.feature`

**Files:**
- Create: `crates/pba_service/tests/ui_features/payment_refund_admin.feature`
- Modify: `crates/pba_service/tests/ui_steps/payment_steps.rs` (and/or wherever existing payment UI steps live)
- Modify: `crates/pba_service/tests/ui_e2e.rs` to register the new feature file if registration is explicit (mirror `transfer_reversal_admin.feature`).

- [ ] **Step 1: Create the feature file.**

```gherkin
@ui @refund
Feature: Admin UI refund flow

  Background:
    Given a fresh test environment

  Scenario: Refund button visible on settled payment
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I visit the transaction detail page for the last payment
    Then the page shows a "Refund" button

  Scenario: Refund button absent on refund row
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50000 paisa
    And I visit the transaction detail page for the last refund row
    Then the page does not show a "Refund" button
    And the page shows "Refund of payment"

  Scenario: Refund button absent on fully refunded payment
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I refund the last payment for 50000 paisa
    And I visit the transaction detail page for the last payment
    Then the page does not show a "Refund" button
    And the page shows a refund history entry for 50000 paisa total

  Scenario: Refund flow happy path
    Given a PB account "acct-1" with purpose "health"
    And the account has 60000 paisa in its others-pool
    And the account has 40000 paisa in its self-pool
    When I make a 100000 paisa payment to merchant "M1" mcc "8011"
    And I visit the transaction detail page for the last payment
    And I click "Refund"
    And the refund form pre-fills with remaining 100000 paisa
    And I submit the refund form with amount 50000 paisa
    Then I land on the refund detail page
    And the original payment detail page shows a refund history entry for 50000 paisa
    And the remaining refundable on the payment page shows 50000 paisa

  Scenario: Over-amount surfaces inline error
    Given a PB account "acct-1" with purpose "health"
    And the account has 50000 paisa in its others-pool
    When I make a 50000 paisa payment to merchant "M1" mcc "8011"
    And I visit the transaction detail page for the last payment
    And I click "Refund"
    And I submit the refund form with amount 50001 paisa
    Then the refund form shows an error containing "Refund amount invalid"
```

- [ ] **Step 2: Add UI step bindings.** Add the matching `#[when]` / `#[then]` steps. Reuse selectors from existing UI tests where possible (`transfer_reversal_admin.feature`'s steps are a good reference).

- [ ] **Step 3: Run the UI feature.**

Run: `just ui-e2e`
Expected: all `payment_refund_admin.feature` scenarios pass.

- [ ] **Step 4: Commit.**

```bash
git add crates/pba_service/tests/ui_features/payment_refund_admin.feature \
        crates/pba_service/tests/ui_steps/payment_steps.rs \
        crates/pba_service/tests/ui_e2e.rs
git commit -m "test(ui-e2e): payment refund admin UI scenarios"
```

---

## Task 13: Docs

**Files:**
- Modify: `README.md`
- Modify: `WHAT.md`

- [ ] **Step 1: Update `README.md`.** In the API table, add a row right after the existing payment row (consult the table for exact column shape):

```
| `POST /pb-accounts/{id}/payments/{id}/refund` | Refund a settled PB payment in whole or in part; multiple partials allowed. |
```

- [ ] **Step 2: Update `WHAT.md`.** Add a new subsection under the PB-accounts section, after the payment description:

```markdown
### Refunding a payment

Settled payments can be refunded by an admin in whole or in part. Each refund
is recorded as a new compensating transaction (1 or 2 rows mirroring the
payment's pool split) plus matching TigerBeetle transfer(s) debiting the
merchant settlement sentinel. Original payment rows are never mutated; each
refund row links back via `reverses_transaction_id`.

- Multiple partial refunds are allowed per payment; the sum must not exceed
  the original payment amount.
- Refund credits self-pool first up to that pool's remaining-unrefunded
  amount, then others-pool. This restores the holder's self-pool flexibility
  first.
- The PB account must be Active to accept a refund. Refund rows themselves
  cannot be refunded.
- Refunds are admin-only via `POST /pb-accounts/{id}/payments/{id}/refund`
  (and via the Refund button on the admin transaction-detail page).

Refunds do not re-validate the merchant's MCC — the MCC was already
validated when the original payment landed. The merchant settlement
sentinel has no balance constraint at TigerBeetle, so refunds never fail
with `InsufficientFunds`; over-amount is caught with `RefundAmountInvalid`
before TigerBeetle is touched.
```

- [ ] **Step 3: Commit.**

```bash
git add README.md WHAT.md
git commit -m "docs: payment refund"
```

---

## Final verification

- [ ] **Step 1: Run the full local CI.**

Run: `just local-ci`
Expected: fmt-check + clippy + build + tests + cog-check all pass.

- [ ] **Step 2: Run both Cucumber suites.**

Run: `just api-e2e && just ui-e2e`
Expected: all new and pre-existing scenarios pass.

- [ ] **Step 3: Confirm no regressions in transfer reversal.**

Spot-check by running just the transfer-reversal scenarios:

Run: `just api-e2e -- --tags @reversal` (or whatever the existing tag is)
Expected: PASS — the index tightening preserves at-most-one-transfer-reversal.

- [ ] **Step 4: Open the PR.**

```bash
gh pr create --title "feat: refund PB→merchant payments" --body "$(cat <<'EOF'
## Summary
- Admin-initiated refund of settled PB payments, with up to two compensating transaction rows mirroring the payment's pool split.
- Multiple partial refunds per payment, summing to ≤ original; self-first pool allocation.
- New TB transfer code 210, new `find_refunds_of` / `sum_refunds_of` repo helpers (type-agnostic — reused later for multi-partial transfer reversal).
- Schema change: tighten the existing `reverses_transaction_id` partial unique index to `WHERE type='transfer'`, so transfer reversal's at-most-one invariant is preserved while payment refunds can stack.

## Test plan
- [ ] `just local-ci` passes
- [ ] `just api-e2e` passes (incl. the new `payment_refund.feature` and pre-existing `transfer_reversal.feature`)
- [ ] `just ui-e2e` passes (incl. the new `payment_refund_admin.feature`)
- [ ] Walk the admin UI: make a payment, click Refund, submit, see the refund history block; verify the Refund button disappears once fully refunded.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Spec coverage map

| Spec section | Plan task(s) |
|---|---|
| Schema migration | Task 1 |
| Error variants | Task 2 |
| Domain `type_label` branch | Task 3 |
| Repo `find_refunds_of`, `sum_refunds_of` | Task 4 |
| Ledger code 210 + helpers | Task 5 |
| `refund_payment` service | Task 6 (6a–6h) |
| API DTOs | Task 7 |
| API handler + route | Task 8 |
| Smithy operation + SDK regen | Task 9 |
| Cucumber `payment_refund.feature` | Task 10 |
| Admin UI handlers + form template + button + history | Task 11 |
| UI Cucumber `payment_refund_admin.feature` | Task 12 |
| README + WHAT.md | Task 13 |
| Forward symmetry (transfer reversal future) | Task 1's index tightening + Task 4's type-agnostic helpers; no behavioural change |
