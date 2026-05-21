# Transfer Reversal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add admin-initiated reversal of a posted normal→PB transfer, recorded as a new compensating transaction pair plus a TigerBeetle transfer in the opposite direction. The original transfer rows are never mutated; the link lives on the normal-side reversal row via `reverses_transaction_id`.

**Architecture:** A single new method on the existing `TransferService` (`reverse_transfer`) inserts two new transaction rows under a fresh `correlation_id` and writes one new TB transfer with code `410` (debit PB others-pool, credit normal account). Idempotency, active checks, and reversal-uniqueness are enforced in the service; over-debit is enforced by TigerBeetle's `DEBITS_MUST_NOT_EXCEED_CREDITS` flag on the PB others-pool. The HTTP API gets one new route under `/normal-accounts/{account_id}/transfers/{transfer_id}/reverse`; the admin UI gets a Reverse button on the existing transfer detail page.

**Tech Stack:** Rust (axum, sqlx, tokio), PostgreSQL, TigerBeetle (via `tigerbeetle_unofficial`), Smithy for the API model + generated Rust client SDK, Askama templates for the admin UI, Cucumber for BDD.

**Spec:** `docs/superpowers/specs/2026-05-21-transfer-reversal-design.md`.

**Branch:** `feat/transfer-reversal` (created from `main` at commit `a6c5548`).

---

## Pre-existing facts about the codebase to know before starting

- **Error response shape.** `AppError::into_response` writes `{ "error": "<PascalCaseVariantName>", "message": "<Display impl>" }`. New variants follow this. *Do not* invent a `snake_case` error code field.
- **`InsufficientFunds` → HTTP 422** (`UNPROCESSABLE_ENTITY`). Same mapping is reused on TB exceeds-credits via `AppError::ExceedsBalance`.
- **`TransferLegs::new()`** (in `crates/pba_service/src/domain/transfer.rs`) generates UUIDv7 ids — the project's convention for entity ids per commit `48268d1`. Reuse it for reversal pairs.
- **Auth.** The HTTP API router (`api/routes.rs`) is API-key authenticated and has *no* admin-role gate. The admin UI router (registered in `main.rs:313`) is gated by `auth::admin_auth::require_admin_session`. There is no `require_admin` extractor for the API today. We follow the same posture as `post_transfer`/`void_transfer`: the API endpoint is exposed to any authenticated API caller; the admin UI calls the same endpoint via the existing OIDC-session-gated `/admin/*` surface. A future PR can add an HTTP-layer admin role check; that is out of scope here.
- **No `mod.rs`.** Project preference: use `foo.rs`-style modules. Handlers files already follow this (`api/handlers/transfer.rs`). Do not introduce `mod.rs` files.
- **DTO style.** Rust struct fields are `snake_case`; serde uses defaults. Do not switch to `camelCase`.
- **Migration filenames.** `YYYYMMDDhhmmss_<snake_case>.sql` under `crates/pba_service/src/db/migrations/`. Use `20260521000001_transactions_reverses_transaction_id.sql`.
- **`tests/features/*.feature`** runs via `just api-e2e` and uses the **Smithy-generated client SDK** (`world.client.transfer_to_pb_account()...`). So API E2E tests for the new operation require the Smithy model to be added and the SDK regenerated *before* the test passes end-to-end.

## File map

| File | Disposition | Responsibility |
|---|---|---|
| `crates/pba_service/src/db/migrations/20260521000001_transactions_reverses_transaction_id.sql` | Create | Add nullable `reverses_transaction_id` column + partial unique index + plain partial index. |
| `crates/pba_service/src/error.rs` | Modify | New variants: `TransferNotReversible(String, String)`, `TransferAlreadyReversed(String)`, `ReversalAmountInvalid { requested: u64, original: u64 }`. |
| `crates/pba_service/src/domain/transaction.rs` | Modify | Add `reverses_transaction_id: Option<Uuid>` to `TransactionRecord`; branch in `type_label()`. |
| `crates/pba_service/src/repository/transaction_repo.rs` | Modify | Add `reverses_transaction_id` to the SQL select list, `TransactionRow`, `into_domain`, and `insert_in_tx` signature/SQL; add `find_reversal_of`. |
| `crates/pba_service/src/repository/ledger_repo.rs` | Modify | Add `INTERNAL_TRANSFER_REVERSAL_CODE: u16 = 410` and `create_internal_transfer_reversal` method. |
| `crates/pba_service/src/service/transfer_service.rs` | Modify | Add `ReversalResult` struct and `reverse_transfer` method; pass `None` for the new arg in existing `insert_in_tx` calls. |
| `crates/pba_service/src/api/dto.rs` | Modify | Add `ReverseTransferRequest`, `ReversalResponse`, `impl From<ReversalResult> for ReversalResponse`; add `reverses_transaction_id: Option<Uuid>` to `TransactionSummaryDto` and its `From` impl. |
| `crates/pba_service/src/api/handlers/transfer.rs` | Modify | Add `reverse_transfer` handler. |
| `crates/pba_service/src/api/routes.rs` | Modify | Add the new route inside the `normal` block. |
| `model/transfer.smithy` | Modify | Add `ReverseNormalAccountTransfer` operation + `ReversalResponseMixin`. |
| `model/main.smithy` | Modify | Register the new operation on the service. |
| `crates/pba_service/src/admin/transfer_handlers.rs` | Modify | Add `reverse_transfer_form`, `process_reverse_transfer` handlers; extend `TransferDetailTemplate` with `can_reverse`, `is_reversal`, `reversed_by_id`. |
| `crates/pba_service/src/admin.rs` | Modify | Register the two new admin routes. |
| `crates/pba_service/templates/admin/transfer_detail.html` | Modify | Add Reverse button and "Reversed by [link]" affordance. |
| `crates/pba_service/templates/admin/transfer_reverse.html` | Create | Reversal form template (amount + description). |
| `crates/pba_service/tests/features/transfer_reversal.feature` | Create | 13 BDD scenarios per the spec. |
| `crates/pba_service/tests/steps/transfer_steps.rs` | Modify | Add reversal step bindings (`I reverse N paisa…`, `the reversal fails with "…"`, etc.). |
| `crates/pba_service/tests/PbaWorld` fields | Modify | Add `last_reversal_id`, `last_reversal_correlation_id` to the world struct. |
| `crates/pba_service/tests/ui_features/transfer_reversal_admin.feature` | Create | 5 UI scenarios per the spec. |
| `crates/pba_service/tests/ui_steps/...` | Modify | Add step bindings for clicking Reverse, asserting button presence/absence, asserting the "Reversed by" affordance. |
| `README.md` | Modify | Add reversal row to the API table. |
| `WHAT.md` | Modify | Add a "Reversing a transfer" subsection. |

---

## Task 1: Migration — add `reverses_transaction_id` column

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260521000001_transactions_reverses_transaction_id.sql`

- [ ] **Step 1: Create the migration file.**

```sql
-- Add reverses_transaction_id link to support reversal of posted transfers.
-- See docs/superpowers/specs/2026-05-21-transfer-reversal-design.md.

ALTER TABLE transactions
    ADD COLUMN reverses_transaction_id UUID NULL;

-- Enforce at-most-one reversal per original transfer.
CREATE UNIQUE INDEX uq_transactions_reverses
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL;

-- Supports `find_reversal_of(original_id)` lookups.
CREATE INDEX idx_transactions_reverses_transaction_id
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL;
```

- [ ] **Step 2: Verify the migration applies cleanly against a fresh DB.**

Run: `just stop && just migrate` (this drops to whichever runner the repo uses; if `just migrate` is not safe against a live DB on your branch, run against the test DB via `DB_NAME=pba_service_test just migrate`).

Expected: migration runs without error; `\d transactions` in psql shows the new column and indexes.

- [ ] **Step 3: Commit.**

```bash
git add crates/pba_service/src/db/migrations/20260521000001_transactions_reverses_transaction_id.sql
git commit -m "feat(db): add reverses_transaction_id column on transactions

Nullable UUID link from a reversal's normal-side row back to the original
transfer's source-side row. Partial unique index enforces at-most-one
reversal per original transfer."
```

---

## Task 2: `AppError` variants

**Files:**
- Modify: `crates/pba_service/src/error.rs`

- [ ] **Step 1: Write a failing test.**

Append to `crates/pba_service/src/error.rs` (or place in a sibling test module if the file already has one):

```rust
#[cfg(test)]
mod reversal_error_tests {
    use super::*;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parts.status, parts.status); // sanity
        v
    }

    #[tokio::test]
    async fn transfer_not_reversible_returns_409_with_pascal_case_kind() {
        let err = AppError::TransferNotReversible("abc".into(), "not_posted".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "TransferNotReversible");
        assert!(body["message"].as_str().unwrap().contains("not_posted"));
    }

    #[tokio::test]
    async fn transfer_already_reversed_returns_409() {
        let err = AppError::TransferAlreadyReversed("xyz".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "TransferAlreadyReversed");
    }

    #[tokio::test]
    async fn reversal_amount_invalid_returns_400_with_amounts_in_message() {
        let err = AppError::ReversalAmountInvalid {
            requested: 1500,
            original: 1000,
        };
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "ReversalAmountInvalid");
        let msg = body["message"].as_str().unwrap();
        assert!(msg.contains("1500"));
        assert!(msg.contains("1000"));
    }
}
```

If `http-body-util` is not a dev-dep yet, add to `crates/pba_service/Cargo.toml` under `[dev-dependencies]`: `http-body-util = "0.1"`. (Check if it's already present from existing tests first via `grep http_body_util crates/pba_service/Cargo.toml`.)

- [ ] **Step 2: Run test to verify it fails (variants don't exist).**

Run: `cargo test -p pba_service error::reversal_error_tests -- --nocapture`

Expected: FAIL with `error[E0599]: no variant or associated item named ...` for the three new variants.

- [ ] **Step 3: Add the variants.**

In `crates/pba_service/src/error.rs`, in the `AppError` enum, add (placement after `TrustDepositRequiresTransfer` keeps semantically related variants grouped):

```rust
    /// The transfer cannot be reversed in its current state.
    /// `reason` is one of: "not_posted", "is_itself_a_reversal", "wrong_type".
    TransferNotReversible(String, String),
    /// A reversal already exists for this original transfer.
    TransferAlreadyReversed(String),
    /// Requested reversal amount is zero or exceeds the original transfer amount.
    ReversalAmountInvalid { requested: u64, original: u64 },
```

In `impl Display`:

```rust
            Self::TransferNotReversible(id, reason) => write!(
                f,
                "Transfer {id} cannot be reversed: {reason}"
            ),
            Self::TransferAlreadyReversed(id) => write!(
                f,
                "Transfer {id} has already been reversed"
            ),
            Self::ReversalAmountInvalid { requested, original } => write!(
                f,
                "Reversal amount {requested} is invalid for original transfer of {original}"
            ),
```

In `IntoResponse::into_response`'s match:

```rust
            AppError::TransferNotReversible(_, _) => (StatusCode::CONFLICT, "TransferNotReversible"),
            AppError::TransferAlreadyReversed(_) => (StatusCode::CONFLICT, "TransferAlreadyReversed"),
            AppError::ReversalAmountInvalid { .. } => (StatusCode::BAD_REQUEST, "ReversalAmountInvalid"),
```

- [ ] **Step 4: Run tests, see them pass.**

Run: `cargo test -p pba_service error -- --nocapture`

Expected: all three new tests PASS; previously passing error tests stay green.

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/error.rs crates/pba_service/Cargo.toml
git commit -m "feat(error): add reversal AppError variants

TransferNotReversible (409), TransferAlreadyReversed (409), and
ReversalAmountInvalid (400)."
```

---

## Task 3: Domain — `TransactionRecord.reverses_transaction_id` and `type_label`

**Files:**
- Modify: `crates/pba_service/src/domain/transaction.rs`

- [ ] **Step 1: Write a failing test.**

Replace the existing `tests` module at the bottom of the file with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account_kind::AccountKind;
    use uuid::Uuid;

    fn make(transaction_type: TransactionType, reverses: Option<Uuid>) -> TransactionRecord {
        TransactionRecord {
            id: Uuid::now_v7(),
            account_id: Uuid::now_v7(),
            account_kind: AccountKind::Normal,
            transaction_type,
            status: TransactionStatus::Posted,
            amount: 0,
            pool: None,
            direction: TransactionDirection::Outbound,
            source_ifsc: None,
            source_account: None,
            gateway_ref: None,
            timeout_seconds: None,
            merchant_id: None,
            merchant_mcc: None,
            description: None,
            funding_type: None,
            tb_transfer_id: 0,
            idempotency_key: None,
            correlation_id: None,
            reverses_transaction_id: reverses,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn transfer_round_trips() {
        assert_eq!(TransactionType::Transfer.as_str(), "transfer");
        assert_eq!(
            TransactionType::from_str("transfer"),
            Some(TransactionType::Transfer)
        );
    }

    #[test]
    fn reversal_label_when_reverses_transaction_id_is_set() {
        let r = make(TransactionType::Transfer, Some(Uuid::now_v7()));
        assert_eq!(r.type_label(), "Reversal");
    }

    #[test]
    fn transfer_label_when_reverses_transaction_id_is_none() {
        let r = make(TransactionType::Transfer, None);
        assert_eq!(r.type_label(), "Transfer");
    }
}
```

- [ ] **Step 2: Run, verify failure.**

Run: `cargo test -p pba_service domain::transaction::tests -- --nocapture`

Expected: FAIL (`reverses_transaction_id` field missing).

- [ ] **Step 3: Add the field.**

In `TransactionRecord` (after `correlation_id: Option<Uuid>` to keep related ids together):

```rust
    pub reverses_transaction_id: Option<Uuid>,
```

In `type_label()`, change the `Transfer` branches to:

```rust
            (TransactionType::Transfer, _) if self.reverses_transaction_id.is_some() => "Reversal",
            (TransactionType::Transfer, TransactionStatus::Pending) => "Transfer (Pending)",
            (TransactionType::Transfer, TransactionStatus::Posted)
            | (TransactionType::Transfer, TransactionStatus::Settled) => "Transfer",
            (TransactionType::Transfer, TransactionStatus::Voided) => "Transfer (Voided)",
```

The `if let` style guard goes **before** the more specific status-keyed branches so reversal wins regardless of status (in this iteration reversal rows are always `Posted`, but the guard is robust).

- [ ] **Step 4: Run, verify pass.**

Run: `cargo test -p pba_service domain::transaction -- --nocapture`

Expected: PASS for all three tests. (Compile will fail elsewhere — `TransactionRow::into_domain` constructs the struct literal without the new field. That's the next task; expect a `cargo build` failure now and fix in Task 4.)

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/domain/transaction.rs
git commit -m "feat(domain): add reverses_transaction_id to TransactionRecord

When set on a Transfer row, type_label() renders 'Reversal'."
```

---

## Task 4: Transaction repo — column read/write + `find_reversal_of`

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs`

- [ ] **Step 1: Add the column to `TransactionRow`.**

In the `#[derive(sqlx::FromRow)] struct TransactionRow { … }` block, after `correlation_id`:

```rust
    reverses_transaction_id: Option<Uuid>,
```

In `into_domain`, after `correlation_id: self.correlation_id,`:

```rust
            reverses_transaction_id: self.reverses_transaction_id,
```

- [ ] **Step 2: Update every SELECT to include the new column.**

Every SELECT in `transaction_repo.rs` must be updated: `insert_in_tx` RETURNING, `update_status` RETURNING, `get_by_id`, `get_transaction`, `find_by_idempotency_key`, `list_by_account`, `list_all`, `list_pending_by_account`, `find_timed_out_pending`, `find_by_correlation_id`. In each, insert `reverses_transaction_id` into the projection immediately before `created_at, updated_at`. sqlx maps by column name, so position in the projection does not have to match `TransactionRow`'s field order, but keeping them aligned (new field before `created_at` in both places) makes the diffs easy to follow.

Concrete diff for `get_by_id` (apply the same edit to every other SELECT):

```diff
             SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                    source_ifsc, source_account, gateway_ref, timeout_seconds,
                    merchant_id, merchant_mcc, description, funding_type,
                    tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
-                   created_at, updated_at
+                   reverses_transaction_id, created_at, updated_at
```

- [ ] **Step 3: Update `insert_in_tx` to accept and bind `reverses_transaction_id`.**

Add a parameter at the end of the argument list:

```rust
        correlation_id: Option<Uuid>,
        reverses_transaction_id: Option<Uuid>,
    ) -> Result<TransactionRecord, AppError> {
```

In the INSERT statement, add the column to the column list and `$20` to VALUES:

```sql
INSERT INTO transactions (id, account_id, account_kind, type, status, amount, pool, direction,
                          source_ifsc, source_account, gateway_ref, timeout_seconds,
                          merchant_id, merchant_mcc, description, funding_type,
                          tb_transfer_id, idempotency_key, correlation_id, reverses_transaction_id)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17::numeric, $18, $19, $20)
RETURNING id, account_id, account_kind, type, status, amount, pool, direction,
          source_ifsc, source_account, gateway_ref, timeout_seconds,
          merchant_id, merchant_mcc, description, funding_type,
          tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
          reverses_transaction_id, created_at, updated_at
```

Add the bind after `.bind(correlation_id)`:

```rust
        .bind(reverses_transaction_id)
```

- [ ] **Step 4: Add `find_reversal_of`.**

At the bottom of `impl TransactionRepo`:

```rust
    /// Find the reversal row (if any) whose `reverses_transaction_id` matches the given
    /// original transfer's source-side row id. Returns the normal-side reversal row
    /// (the only row in a reversal pair that carries `reverses_transaction_id`).
    #[allow(dead_code)]
    pub async fn find_reversal_of(
        &self,
        original_source_id: Uuid,
    ) -> Result<Option<TransactionRecord>, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE reverses_transaction_id = $1
            "#,
        )
        .bind(original_source_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_domain()))
    }
```

- [ ] **Step 5: Update every call site of `insert_in_tx`.**

`grep -n "insert_in_tx" crates/pba_service/src/` and add `None` as the final argument to each existing call. Expected call sites: `pb_deposit_service`, `pb_payment_service` (multiple), `pb_withdrawal_service`, `normal_deposit_service`, `normal_withdrawal_service`, `transfer_service::transfer` (two insertions). All existing callers pass `None` — they are not reversal rows.

- [ ] **Step 6: Add a focused integration test for the new repo helpers.**

Add to (or create) `crates/pba_service/src/repository/transaction_repo.rs` test module — gated by `#[cfg(test)]` and `#[sqlx::test]`. If the project uses an external integration-test pattern instead of inline `#[sqlx::test]`, place the test alongside other repo tests instead. Check existing test placement first via `grep -rn "#\[sqlx::test\]" crates/pba_service/src/repository/`.

Test:

```rust
#[sqlx::test(migrations = "src/db/migrations")]
async fn find_reversal_of_returns_the_normal_side_row(pool: PgPool) {
    use crate::domain::account_kind::AccountKind;
    use crate::domain::transaction::{TransactionDirection, TransactionStatus, TransactionType};

    let repo = TransactionRepo::new(pool.clone());

    // Insert an original transfer's normal-side row.
    let original_src = Uuid::now_v7();
    let original_dst = Uuid::now_v7();
    let acct_normal = Uuid::now_v7();
    let acct_pb = Uuid::now_v7();
    let corr_orig = Uuid::now_v7();

    let mut tx = pool.begin().await.unwrap();
    repo.insert_in_tx(
        &mut tx, original_src, acct_normal, AccountKind::Normal,
        TransactionType::Transfer, TransactionStatus::Posted, 1000, None,
        TransactionDirection::Outbound, None, None, None, None, None, None,
        None, Some("trust"), 0, None, Some(corr_orig), None,
    ).await.unwrap();
    repo.insert_in_tx(
        &mut tx, original_dst, acct_pb, AccountKind::Pb,
        TransactionType::Deposit, TransactionStatus::Posted, 1000, Some("others"),
        TransactionDirection::Inbound, None, None, None, None, None, None,
        None, Some("trust"), 0, None, Some(corr_orig), None,
    ).await.unwrap();
    tx.commit().await.unwrap();

    // None exists yet.
    assert!(repo.find_reversal_of(original_src).await.unwrap().is_none());

    // Insert a reversal pair under a new correlation; only the normal-side row links back.
    let reversal_pb = Uuid::now_v7();
    let reversal_normal = Uuid::now_v7();
    let corr_rev = Uuid::now_v7();
    let mut tx = pool.begin().await.unwrap();
    repo.insert_in_tx(
        &mut tx, reversal_pb, acct_pb, AccountKind::Pb,
        TransactionType::Transfer, TransactionStatus::Posted, 1000, Some("others"),
        TransactionDirection::Outbound, None, None, None, None, None, None,
        None, Some("trust"), 0, None, Some(corr_rev), None,
    ).await.unwrap();
    repo.insert_in_tx(
        &mut tx, reversal_normal, acct_normal, AccountKind::Normal,
        TransactionType::Transfer, TransactionStatus::Posted, 1000, None,
        TransactionDirection::Inbound, None, None, None, None, None, None,
        None, Some("trust"), 0, None, Some(corr_rev), Some(original_src),
    ).await.unwrap();
    tx.commit().await.unwrap();

    let found = repo.find_reversal_of(original_src).await.unwrap().expect("reversal found");
    assert_eq!(found.id, reversal_normal);
    assert_eq!(found.account_kind, AccountKind::Normal);
    assert_eq!(found.reverses_transaction_id, Some(original_src));
}

#[sqlx::test(migrations = "src/db/migrations")]
async fn duplicate_reversal_rejected_by_unique_index(pool: PgPool) {
    use crate::domain::account_kind::AccountKind;
    use crate::domain::transaction::{TransactionDirection, TransactionStatus, TransactionType};

    let repo = TransactionRepo::new(pool.clone());

    let original_src = Uuid::now_v7();
    let acct_normal = Uuid::now_v7();
    let corr1 = Uuid::now_v7();
    let corr2 = Uuid::now_v7();

    let mut tx = pool.begin().await.unwrap();
    repo.insert_in_tx(
        &mut tx, Uuid::now_v7(), acct_normal, AccountKind::Normal,
        TransactionType::Transfer, TransactionStatus::Posted, 1000, None,
        TransactionDirection::Inbound, None, None, None, None, None, None,
        None, Some("trust"), 0, None, Some(corr1), Some(original_src),
    ).await.unwrap();
    tx.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let err = repo.insert_in_tx(
        &mut tx, Uuid::now_v7(), acct_normal, AccountKind::Normal,
        TransactionType::Transfer, TransactionStatus::Posted, 1000, None,
        TransactionDirection::Inbound, None, None, None, None, None, None,
        None, Some("trust"), 0, None, Some(corr2), Some(original_src),
    ).await.unwrap_err();
    match err {
        AppError::DatabaseError(msg) => assert!(msg.contains("uq_transactions_reverses") || msg.contains("duplicate")),
        other => panic!("expected DatabaseError, got {other:?}"),
    }
}
```

If `#[sqlx::test]` isn't already configured in the crate, follow the existing repo-test pattern (look at how `pb_account_repo` tests are wired) — do not introduce a new test harness.

- [ ] **Step 7: Run, verify pass.**

Run: `cargo test -p pba_service repository::transaction_repo -- --nocapture` (or the project's repo-test runner if different).

Expected: PASS. Also: `cargo build -p pba_service` succeeds (the new `None` args at call sites compile).

- [ ] **Step 8: Commit.**

```bash
git add crates/pba_service/src/repository/transaction_repo.rs \
        crates/pba_service/src/repository/ledger_repo.rs \
        crates/pba_service/src/service/
git commit -m "feat(repo): transaction_repo carries reverses_transaction_id

insert_in_tx takes a new trailing arg; all SELECTs and the row struct
include the column. find_reversal_of returns the normal-side reversal
row whose link matches a given original transfer id."
```

(The commit pulls in the trivial `None` arg added to existing call sites in service files — they were touched in Step 5.)

---

## Task 5: Ledger repo — code 410 + `create_internal_transfer_reversal`

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs`

- [ ] **Step 1: Add the constant.**

Near the other transfer-code constants (lines ~25–26):

```rust
const INTERNAL_TRANSFER_REVERSAL_CODE: u16 = 410;
```

- [ ] **Step 2: Add the method.**

Below `create_pending_internal_transfer`:

```rust
    /// Immediate reversal of an internal transfer.
    ///
    /// Debits the PB others-pool, credits the source normal account. TigerBeetle's
    /// `DEBITS_MUST_NOT_EXCEED_CREDITS` flag on the others-pool enforces that the
    /// reversal cannot debit below zero; the caller maps `AppError::ExceedsBalance`
    /// to `AppError::InsufficientFunds` with the observed balance.
    pub async fn create_internal_transfer_reversal(
        &self,
        debit_pb_others_tb_id: u128,
        credit_normal_tb_id: u128,
        amount: u64,
    ) -> Result<(), AppError> {
        self.create_transfer(
            debit_pb_others_tb_id,
            credit_normal_tb_id,
            amount,
            INTERNAL_TRANSFER_REVERSAL_CODE,
        )
        .await
    }
```

(`create_transfer` is the existing private/internal helper that all `create_*_transfer` methods route through, and it already maps TB `ExceedsCredits` to `AppError::ExceedsBalance`. Verify by searching for `fn create_transfer` in the file before relying on it.)

- [ ] **Step 3: Add a focused integration test (TigerBeetle round-trip).**

Repo tests for the ledger typically need TigerBeetle running. Follow the existing pattern (search for `create_internal_transfer` in `crates/pba_service/src/repository/ledger_repo.rs` tests). If there is no inline test module, the unit test goes into the integration test crate that wraps TB. Skip this step only if no ledger tests exist anywhere in the crate; otherwise add:

```rust
#[tokio::test]
#[ignore = "requires running TigerBeetle"]
async fn reversal_moves_balance_pb_others_to_normal() {
    let repo = LedgerRepo::new(0, vec!["3000".into()]);
    repo.init_sentinel_accounts().await.unwrap();

    let pb_self = (u128::MAX - 1000) & !(1u128 << 127);
    let pb_others = pb_self | (1u128 << 127);
    let normal = u128::MAX - 1001;

    repo.create_account_pair(pb_self, pb_others).await.unwrap();
    repo.create_normal_account(normal).await.unwrap();

    // Fund the others-pool via a third-party deposit (uses code 100, funding_type='third_party').
    repo.create_transfer(
        crate::repository::ledger_repo::THIRD_PARTY_FUNDING_SOURCE_TB_ID,
        pb_others,
        500,
        100,
    ).await.unwrap();

    repo.create_internal_transfer_reversal(pb_others, normal, 300).await.unwrap();

    let pb_bal = repo.get_balance(pb_self, pb_others).await.unwrap();
    let normal_bal = repo.get_single_balance(normal).await.unwrap();
    assert_eq!(pb_bal.others_contribution, 200);
    assert_eq!(normal_bal.posted, 300);
}
```

Adjust constants / helper names if the actual ledger_repo helpers differ; the spirit of the test is "fund others-pool with 500, reverse 300, expect others=200 and normal=300."

- [ ] **Step 4: Build, run unit + integration tests.**

Run: `cargo build -p pba_service && cargo test -p pba_service ledger -- --nocapture` (the integration test stays `#[ignore]` for default runs; manual `cargo test -- --ignored` exercises it).

Expected: build PASS; unit tests PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/repository/ledger_repo.rs
git commit -m "feat(ledger): code 410 create_internal_transfer_reversal

Debits a PB others-pool TB account and credits a normal-account TB account.
DEBITS_MUST_NOT_EXCEED_CREDITS on the others-pool means TB itself enforces
'cannot reverse more than the pool currently holds' and surfaces
AppError::ExceedsBalance, which the service layer maps to InsufficientFunds."
```

---

## Task 6: Service — `reverse_transfer` on `TransferService`

**Files:**
- Modify: `crates/pba_service/src/service/transfer_service.rs`

- [ ] **Step 1: Define `ReversalResult`.**

Below the existing `TransferResult` struct:

```rust
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReversalResult {
    pub reversal_id: Uuid,            // normal-side reversal row id
    pub original_transfer_id: Uuid,   // T_src.id of the original
    pub source_account_id: Uuid,      // the normal account being credited
    pub destination_account_id: Uuid, // the PB account being debited
    pub amount: u64,
    pub original_amount: u64,
    pub status: TransactionStatus,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

- [ ] **Step 2: Write the failing unit tests.**

Add a `mod tests` to `transfer_service.rs` if one doesn't exist. If the file doesn't have unit tests today, look at how `pb_payment_service` is tested for the in-crate convention (mock TB or sqlx test DB). The repo currently leans on cucumber for cross-cutting tests, so the simplest path is: write the table of expected behaviour in cucumber (Task 11) and rely on it. **Skip writing inline `mod tests` for the service if no neighbouring service has one** — keep parity with existing structure. If neighbours *do* have unit tests, mirror their style and include at least:

```rust
#[tokio::test]
async fn rejects_amount_zero() {
    // arrange a posted original transfer via repo
    // call reverse_transfer with amount=0
    // assert ReversalAmountInvalid { requested: 0, original: <orig> }
}

#[tokio::test]
async fn rejects_amount_above_original() {
    // arrange a posted original transfer of 1000
    // call reverse_transfer with amount=1001
    // assert ReversalAmountInvalid { requested: 1001, original: 1000 }
}
```

(Cucumber coverage in Task 10 is the canonical test surface for this method.)

- [ ] **Step 3: Implement `reverse_transfer`.**

Add to `impl TransferService`, below `void_transfer`. Use this exact body:

```rust
    #[allow(clippy::too_many_arguments)]
    pub async fn reverse_transfer(
        &self,
        source_normal_id: Uuid,
        original_transfer_id: Uuid,
        amount: u64,
        gateway_ref: Option<&str>,
        description: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<ReversalResult, AppError> {
        // Step 1: Idempotency replay.
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Normal, source_normal_id, key)
                .await?
            {
                let correlation_id = existing.correlation_id.ok_or_else(|| {
                    AppError::DatabaseError(
                        "reversal source row missing correlation_id".to_string(),
                    )
                })?;
                let legs = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                if legs.len() != 2 {
                    return Err(AppError::DatabaseError(
                        "reversal correlation has != 2 legs".to_string(),
                    ));
                }
                return Ok(self.reversal_legs_to_result(&legs, source_normal_id));
            }
        }

        // Step 2: Load and validate the original source row.
        let original = self
            .transaction_repo
            .get_by_id(original_transfer_id, source_normal_id)
            .await?;

        if original.account_kind != AccountKind::Normal
            || original.transaction_type != TransactionType::Transfer
            || original.direction != TransactionDirection::Outbound
        {
            return Err(AppError::TransferNotReversible(
                original_transfer_id.to_string(),
                "wrong_type".to_string(),
            ));
        }
        if original.status != TransactionStatus::Posted {
            return Err(AppError::TransferNotReversible(
                original_transfer_id.to_string(),
                "not_posted".to_string(),
            ));
        }
        if original.reverses_transaction_id.is_some() {
            return Err(AppError::TransferNotReversible(
                original_transfer_id.to_string(),
                "is_itself_a_reversal".to_string(),
            ));
        }

        // Step 3: Reject if already reversed.
        if self
            .transaction_repo
            .find_reversal_of(original_transfer_id)
            .await?
            .is_some()
        {
            return Err(AppError::TransferAlreadyReversed(
                original_transfer_id.to_string(),
            ));
        }

        // Step 4: Validate amount.
        if amount == 0 || amount > original.amount {
            return Err(AppError::ReversalAmountInvalid {
                requested: amount,
                original: original.amount,
            });
        }

        // Step 5: Resolve destination PB account from the original's correlation pair.
        let original_corr = original.correlation_id.ok_or_else(|| {
            AppError::DatabaseError(
                "original transfer row missing correlation_id".to_string(),
            )
        })?;
        let original_legs = self
            .transaction_repo
            .find_by_correlation_id(original_corr)
            .await?;
        let dst_leg = original_legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .ok_or_else(|| {
                AppError::DatabaseError("original transfer missing pb leg".to_string())
            })?;
        let destination_pb_id = dst_leg.account_id;
        let destination = self.pb_account_repo.get_account(destination_pb_id).await?;

        // Step 6: Active checks on both sides.
        let source = self.normal_account_repo.get_account(source_normal_id).await?;
        if !source.status.is_active() {
            return Err(AppError::NormalAccountNotActive(
                source_normal_id.to_string(),
            ));
        }
        if !destination.status.is_active() {
            return Err(AppError::PbAccountNotActive(destination_pb_id.to_string()));
        }

        // Step 7: Insert the two reversal rows.
        let legs = TransferLegs::new();
        let pb_side_id = legs.source_txn_id;       // arbitrarily reuse: pb-side is the "source" of the reversal (debited)
        let normal_side_id = legs.destination_txn_id; // normal-side is credited
        let correlation_id = legs.correlation_id;

        let mut tx = self.transaction_repo.pool().begin().await?;

        // PB-side debit row.
        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                pb_side_id,
                destination_pb_id,
                AccountKind::Pb,
                TransactionType::Transfer,
                TransactionStatus::Posted,
                amount,
                Some("others"),
                TransactionDirection::Outbound,
                None,
                None,
                gateway_ref,
                None,            // no timeout — reversal is immediate
                None,
                None,
                description,
                Some("trust"),
                0,               // tb_transfer_id filled after the TB call
                None,            // no idempotency key on the pb-side row
                Some(correlation_id),
                None,            // reverses_transaction_id NULL on pb-side
            )
            .await?;

        // Normal-side credit row.
        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                normal_side_id,
                source_normal_id,
                AccountKind::Normal,
                TransactionType::Transfer,
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
                description,
                Some("trust"),
                0,
                idempotency_key, // idempotency key lives here, mirrors transfer()
                Some(correlation_id),
                Some(original_transfer_id), // <-- the link
            )
            .await?;

        // Step 8: Execute the TB transfer.
        let tb_result = self
            .ledger_repo
            .create_internal_transfer_reversal(
                destination.tb_others_account_id,
                source.tb_account_id,
                amount,
            )
            .await;

        match tb_result {
            Ok(()) => {
                // Step 9: Persist tb_transfer_id. The ledger helper does not return the
                // transfer id (it doesn't bubble up from create_transfer); we follow the
                // existing convention from transfer() and leave tb_transfer_id=0 for
                // immediate (non-pending) transfers. Document this by skipping the UPDATE.
                tx.commit().await?;
                let updated_legs = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                Ok(self.reversal_legs_to_result(&updated_legs, source_normal_id))
            }
            Err(AppError::ExceedsBalance) => {
                // Roll back PG tx, fetch fresh balance, surface InsufficientFunds.
                drop(tx); // explicit; would also drop on scope exit
                let balance = self
                    .ledger_repo
                    .get_single_balance(destination.tb_others_account_id)
                    .await
                    .unwrap_or(crate::repository::ledger_repo::SingleBalance {
                        posted: 0,
                        pending: 0,
                    });
                Err(AppError::InsufficientFunds {
                    requested: amount,
                    available: balance.posted,
                })
            }
            Err(e) => Err(e),
        }
    }

    fn reversal_legs_to_result(
        &self,
        legs: &[TransactionRecord],
        source_normal_id: Uuid,
    ) -> ReversalResult {
        let normal_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Normal)
            .expect("reversal correlation has a normal leg");
        let pb_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .expect("reversal correlation has a pb leg");
        ReversalResult {
            reversal_id: normal_leg.id,
            original_transfer_id: normal_leg
                .reverses_transaction_id
                .expect("normal-side reversal row carries reverses_transaction_id"),
            source_account_id: source_normal_id,
            destination_account_id: pb_leg.account_id,
            amount: normal_leg.amount,
            original_amount: normal_leg.amount, // overwritten by handler with original.amount
            status: normal_leg.status,
            correlation_id: normal_leg
                .correlation_id
                .expect("reversal leg has correlation_id"),
            created_at: normal_leg.created_at,
        }
    }
```

**Note on `original_amount`:** the legs alone don't carry the original transfer's amount, so the inline `reversal_legs_to_result` sets it to the leg amount. The handler that calls `reverse_transfer` (Task 8) doesn't have the original easily either at the response-shaping point. Two options:
- (a) Have `reverse_transfer` keep a local copy of `original.amount` and pass it to a builder that returns the result with the real `original_amount` filled in.
- (b) Look up the original by id post-commit.

**Pick (a)** — it's free and stays in one function. Replace the final `Ok(self.reversal_legs_to_result(...))` after commit with:

```rust
                let updated_legs = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                let mut result = self.reversal_legs_to_result(&updated_legs, source_normal_id);
                result.original_amount = original.amount;
                Ok(result)
```

And in the idempotency-replay branch, look up the original by following the reversal's `reverses_transaction_id`:

```rust
                let mut result = self.reversal_legs_to_result(&legs, source_normal_id);
                if let Some(orig_id) = legs
                    .iter()
                    .find_map(|l| l.reverses_transaction_id)
                {
                    if let Ok(orig) = self
                        .transaction_repo
                        .get_transaction(orig_id)
                        .await
                    {
                        result.original_amount = orig.amount;
                    }
                }
                return Ok(result);
```

- [ ] **Step 4: Build.**

Run: `cargo build -p pba_service`

Expected: PASS. (Compile errors here usually mean a `TransferLegs` field or `AccountKind` variant name drifted — search and fix.)

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/service/transfer_service.rs
git commit -m "feat(service): TransferService::reverse_transfer

Admin-callable, posted-only, one-shot reversal. Inserts a new
compensating transaction pair under a fresh correlation_id and writes
one code-410 TB transfer (debit PB others, credit normal). Maps TB
exceeds-credits to InsufficientFunds with the observed balance.
At-most-one-reversal enforced by repo helper + DB partial unique index."
```

---

## Task 7: API DTOs

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`

- [ ] **Step 1: Add `reverses_transaction_id` to `TransactionSummaryDto`.**

In the struct, after `correlation_id`:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_transaction_id: Option<Uuid>,
```

In the `From<TransactionRecord>` impl, after `correlation_id: t.correlation_id,`:

```rust
            reverses_transaction_id: t.reverses_transaction_id,
```

- [ ] **Step 2: Add reversal DTOs.**

At the bottom of the file, after the existing Transfer DTOs:

```rust
// ── Transfer Reversal ──

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ReverseTransferRequest {
    pub amount: u64,
    pub gateway_ref: Option<String>,
    pub description: Option<String>,
    pub idempotency_key: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct ReversalResponse {
    pub reversal_id: Uuid,
    pub original_transfer_id: Uuid,
    pub source_account_id: Uuid,
    pub destination_account_id: Uuid,
    pub amount: u64,
    pub original_amount: u64,
    pub status: String,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::transfer_service::ReversalResult> for ReversalResponse {
    fn from(r: crate::service::transfer_service::ReversalResult) -> Self {
        Self {
            reversal_id: r.reversal_id,
            original_transfer_id: r.original_transfer_id,
            source_account_id: r.source_account_id,
            destination_account_id: r.destination_account_id,
            amount: r.amount,
            original_amount: r.original_amount,
            status: r.status.as_str().to_string(),
            correlation_id: r.correlation_id,
            created_at: r.created_at,
        }
    }
}
```

- [ ] **Step 3: Build.**

Run: `cargo build -p pba_service`

Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/pba_service/src/api/dto.rs
git commit -m "feat(api): reversal DTOs and TransactionSummary reverses_transaction_id"
```

---

## Task 8: API handler + route

**Files:**
- Modify: `crates/pba_service/src/api/handlers/transfer.rs`
- Modify: `crates/pba_service/src/api/routes.rs`

- [ ] **Step 1: Add the handler.**

In `crates/pba_service/src/api/handlers/transfer.rs`, below `void_transfer`:

```rust
pub async fn reverse_transfer(
    State(state): State<AppState>,
    Path((source_id, transfer_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<crate::api::dto::ReverseTransferRequest>,
) -> Result<(StatusCode, Json<crate::api::dto::ReversalResponse>), AppError> {
    if let Some(d) = req.description.as_deref() {
        if d.len() > 256 {
            return Err(AppError::Validation(
                "description must be \u{2264} 256 chars".into(),
            ));
        }
    }
    let result = state
        .transfer_service
        .reverse_transfer(
            source_id,
            transfer_id,
            req.amount,
            req.gateway_ref.as_deref(),
            req.description.as_deref(),
            req.idempotency_key.as_deref(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(result.into())))
}
```

The `amount == 0` and `amount > original` checks live in the service layer (Task 6, Step 3) and return `ReversalAmountInvalid` — do not duplicate them here. The handler only enforces what depends purely on the HTTP request shape (description length), matching the style of `initiate_transfer`.

- [ ] **Step 2: Add the route.**

In `crates/pba_service/src/api/routes.rs`, inside the `let normal = Router::new() ... ` chain, after the existing `/transfers/{transfer_id}/void` route:

```rust
        .route(
            "/normal-accounts/{account_id}/transfers/{transfer_id}/reverse",
            post(handlers::transfer::reverse_transfer),
        )
```

- [ ] **Step 3: Build + run unit tests.**

Run: `cargo build -p pba_service && cargo test -p pba_service`

Expected: build PASS; existing unit tests still PASS (no regressions in error/domain tests from Tasks 2–6).

- [ ] **Step 4: Smoke test via curl (optional but recommended).**

Start the service: `just run-bg` (wait for it to come up; `just logs` to watch).

Create a normal account, a PB account, deposit + transfer, then attempt reverse. Specifically:

```bash
API_KEY=$(echo -n "pba-api:pba-api-secret" | base64)

# Create normal account
NORMAL=$(curl -s -H "Authorization: ApiKey $API_KEY" -H "Content-Type: application/json" \
  -d '{"holder_id":"reverse-smoke-1","origin_ifsc":"HDFC0099999","origin_account_number":"9099999001"}' \
  http://localhost:3030/normal-accounts | jq -r .id)

# Create PB account
PB=$(curl -s -H "Authorization: ApiKey $API_KEY" -H "Content-Type: application/json" \
  -d '{"holder_id":"reverse-smoke-1","purpose_code":"health","origin_ifsc":"HDFC0099999","origin_account_number":"9099999002"}' \
  http://localhost:3030/pb-accounts | jq -r .id)

# Deposit to normal, then transfer
curl -s -H "Authorization: ApiKey $API_KEY" -H "Content-Type: application/json" \
  -d '{"amount":10000}' \
  http://localhost:3030/normal-accounts/$NORMAL/deposits

TRANSFER=$(curl -s -H "Authorization: ApiKey $API_KEY" -H "Content-Type: application/json" \
  -d "{\"destination_pb_account_id\":\"$PB\",\"amount\":5000}" \
  http://localhost:3030/normal-accounts/$NORMAL/transfers | jq -r .transfer_id)

# Reverse it
curl -s -H "Authorization: ApiKey $API_KEY" -H "Content-Type: application/json" \
  -d '{"amount":5000}' \
  http://localhost:3030/normal-accounts/$NORMAL/transfers/$TRANSFER/reverse
```

Expected output of the last call: HTTP 201 with body containing `reversal_id`, `original_transfer_id`, `amount: 5000`, `original_amount: 5000`, `status: "posted"`. Then a second call with the same `transfer_id` returns HTTP 409 with `"error":"TransferAlreadyReversed"`.

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/src/api/handlers/transfer.rs \
        crates/pba_service/src/api/routes.rs
git commit -m "feat(api): POST /normal-accounts/{id}/transfers/{id}/reverse

Thin handler wrapping TransferService::reverse_transfer. Description
length validated here; amount and state checks are in the service layer."
```

---

## Task 9: Smithy model + SDK regen

**Files:**
- Modify: `model/transfer.smithy`
- Modify: `model/main.smithy`

- [ ] **Step 1: Add the operation in `model/transfer.smithy`.**

Append at the end of the file (after `VoidNormalAccountTransfer`):

```smithy
/// Reverse a posted normal→PB transfer.
///
/// Records a new compensating transaction pair (PB others-pool debit + normal
/// account credit) plus a new TB transfer in the opposite direction. The
/// original transfer rows are not mutated; the reversal links back via
/// `reverses_transaction_id` on the normal-side reversal row.
///
/// Only `posted` transfers can be reversed. Pending transfers should be
/// cancelled via VoidNormalAccountTransfer. At most one reversal per original
/// transfer. Both source and destination accounts must be Active. The PB
/// others-pool must have sufficient balance; if not, returns InsufficientFunds
/// with the available amount.
@http(
    method: "POST",
    uri: "/normal-accounts/{account_id}/transfers/{transfer_id}/reverse",
    code: 201
)
operation ReverseNormalAccountTransfer {
    input := {
        @required @httpLabel account_id: String
        @required @httpLabel transfer_id: String
        @required amount: Money
        gateway_ref: String
        description: String
        idempotency_key: String
    }
    output := with [ReversalResponseMixin] {}
    errors: [AccountNotFoundError]
}

@mixin
structure ReversalResponseMixin {
    @required reversal_id: String
    @required original_transfer_id: String
    @required source_account_id: String
    @required destination_account_id: String
    @required amount: Money
    @required original_amount: Money
    @required status: String
    @required correlation_id: String
    @required created_at: DateTime
}
```

- [ ] **Step 2: Register the operation in `model/main.smithy`.**

Open `model/main.smithy`, find the service definition that lists operations, and append `ReverseNormalAccountTransfer` to the `operations: [...]` block alongside `TransferToPBAccount`, `PostNormalAccountTransfer`, `VoidNormalAccountTransfer`.

- [ ] **Step 3: Validate + regenerate the SDK.**

Run:

```bash
just smithy-validate
just smithy-build
```

Expected: both commands succeed; `crates/pba_client/src/` gets a new `reverse_normal_account_transfer.rs` or similar file matching the existing naming pattern.

- [ ] **Step 4: Build the workspace.**

Run: `cargo build`

Expected: PASS. The generated client should compile cleanly.

- [ ] **Step 5: Commit.**

```bash
git add model/transfer.smithy model/main.smithy crates/pba_client/
git commit -m "feat(smithy): ReverseNormalAccountTransfer operation + SDK regen"
```

---

## Task 10: Cucumber feature + steps

**Files:**
- Create: `crates/pba_service/tests/features/transfer_reversal.feature`
- Modify: `crates/pba_service/tests/steps/transfer_steps.rs`
- Modify: `crates/pba_service/tests/` (the `PbaWorld` struct — find its definition; likely in `tests/e2e.rs` or `tests/steps/mod.rs`)

- [ ] **Step 1: Add reversal fields to `PbaWorld`.**

Run `grep -n "last_transfer_id" crates/pba_service/tests/ -r` to find the world struct. In the struct, add:

```rust
    pub last_reversal_id: Option<String>,
    pub last_reversal_status: Option<String>,
    pub last_reversal_correlation_id: Option<String>,
    pub last_reversal_original_amount: Option<i64>,
```

Update any `Default` impl or `new`/`init` constructor to set these to `None`.

- [ ] **Step 2: Write the failing feature file.**

Create `crates/pba_service/tests/features/transfer_reversal.feature` with the 13 scenarios from the spec. Use the existing `internal_transfer.feature` as a syntactic template. Excerpt of the first three scenarios — repeat the same shape for the rest:

```gherkin
Feature: Reversal of normal → PB transfers

  Scenario: Full reversal restores source balance and decrements others-pool
    Given a normal account exists for holder "rev-alice-01"
    And the normal account has balance 10000
    And a "health" account exists for holder "rev-alice-01" with origin IFSC "HDFC0021001" and account number "9021001001"
    When I transfer 5000 paisa from the normal account to the PB account
    Then the transfer is successful
    When I reverse 5000 paisa from the transfer
    Then the reversal is successful
    And the reversal status field is "posted"
    And the normal account balance is 10000
    And the PB account others-pool balance is 0

  Scenario: Partial reversal moves only the requested amount
    Given a normal account exists for holder "rev-bob-01"
    And the normal account has balance 10000
    And a "education" account exists for holder "rev-bob-01" with origin IFSC "HDFC0022002" and account number "9022002001"
    When I transfer 5000 paisa from the normal account to the PB account
    When I reverse 3000 paisa from the transfer
    Then the reversal is successful
    And the normal account balance is 8000
    And the PB account others-pool balance is 2000

  Scenario: Pending transfer cannot be reversed
    Given a normal account exists for holder "rev-carla-01"
    And the normal account has balance 5000
    And a "food" account exists for holder "rev-carla-01" with origin IFSC "HDFC0023003" and account number "9023003001"
    When I create a pending transfer of 1500 paisa from the normal account to the PB account with timeout 120
    And I attempt to reverse 1500 paisa from the transfer
    Then the reversal fails with "TransferNotReversible" reason "not_posted"
```

Add scenarios 4–13 covering: already-reversed; reversal-of-reversal; amount=0; amount>original; insufficient others-pool (transfer ₹1000 → payment ₹700 → attempt full reversal → fails `InsufficientFunds` with `available=300` → ₹300 reversal succeeds); source frozen → `NormalAccountNotActive`; destination frozen → `PbAccountNotActive` then reactivate + retry; idempotency replay; wrong source account in URL → `TransactionNotFound`; per-transaction listing shows both originals and reversals.

The wording must match step regexes you implement in Step 3.

- [ ] **Step 3: Implement steps in `tests/steps/transfer_steps.rs`.**

Append:

```rust
#[when(regex = r#"^I reverse (\d+) paisa from the transfer$"#)]
async fn reverse_transfer(world: &mut PbaWorld, amount: i64) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let transfer_id = world
        .last_transfer_id
        .as_ref()
        .expect("No transfer ID")
        .clone();
    let result = world
        .client
        .reverse_normal_account_transfer()
        .account_id(&normal_account_id)
        .transfer_id(&transfer_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_reversal_id = Some(output.reversal_id().to_string());
            world.last_reversal_status = Some(output.status().to_string());
            world.last_reversal_correlation_id = Some(output.correlation_id().to_string());
            world.last_reversal_original_amount = Some(*output.original_amount());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

#[when(regex = r#"^I attempt to reverse (\d+) paisa from the transfer$"#)]
async fn attempt_reverse_transfer(world: &mut PbaWorld, amount: i64) {
    reverse_transfer(world, amount).await;
}

#[when(
    regex = r#"^I reverse (\d+) paisa from the transfer with idempotency key "([^"]*)"$"#
)]
async fn reverse_transfer_with_idempotency(
    world: &mut PbaWorld,
    amount: i64,
    idempotency_key: String,
) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let transfer_id = world
        .last_transfer_id
        .as_ref()
        .expect("No transfer ID")
        .clone();
    let result = world
        .client
        .reverse_normal_account_transfer()
        .account_id(&normal_account_id)
        .transfer_id(&transfer_id)
        .amount(amount)
        .idempotency_key(&idempotency_key)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_reversal_id = Some(output.reversal_id().to_string());
            world.last_reversal_status = Some(output.status().to_string());
            world.last_reversal_correlation_id = Some(output.correlation_id().to_string());
            world.last_reversal_original_amount = Some(*output.original_amount());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

#[then(regex = r#"^the reversal is successful$"#)]
async fn reversal_is_successful(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "Expected reversal to succeed, got error: {:?}",
        world.last_error
    );
    assert!(world.last_reversal_id.is_some(), "No reversal ID captured");
}

#[then(regex = r#"^the reversal status field is "([^"]*)"$"#)]
async fn reversal_status_field_is(world: &mut PbaWorld, expected: String) {
    let actual = world
        .last_reversal_status
        .as_ref()
        .expect("No reversal status captured");
    assert_eq!(actual, &expected);
}

#[then(regex = r#"^the reversal fails with "([^"]*)"$"#)]
async fn reversal_fails_with(world: &mut PbaWorld, expected_kind: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected reversal to fail, but it succeeded");
    assert_eq!(err.kind, expected_kind);
}

#[then(regex = r#"^the reversal fails with "([^"]*)" reason "([^"]*)"$"#)]
async fn reversal_fails_with_reason(
    world: &mut PbaWorld,
    expected_kind: String,
    expected_reason: String,
) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected reversal to fail, but it succeeded");
    assert_eq!(err.kind, expected_kind);
    // The reason is embedded in the message; assert the message contains it.
    // PbaError needs a `message` field if it doesn't have one. Check existing
    // structure and adapt: if PbaError today only carries `kind`, extend it to
    // capture the message string from the SDK error for this assertion.
    assert!(
        err.message
            .as_ref()
            .map_or(false, |m| m.contains(&expected_reason)),
        "Expected reason '{expected_reason}' in error message; got {:?}",
        err.message
    );
}
```

If `PbaError` only carries `kind` today (see the `extract_transfer_error_kind` helper in this file), extend the struct in the same commit:

```rust
// In tests/e2e.rs or wherever PbaError lives:
pub struct PbaError {
    pub kind: String,
    pub message: Option<String>,
}
```

And update `extract_transfer_error_kind` (or add a parallel `extract_transfer_error_message`) to populate the message.

- [ ] **Step 4: Run, verify the feature passes.**

Run: `just api-e2e`

Expected: all reversal scenarios PASS; pre-existing transfer scenarios PASS unchanged.

- [ ] **Step 5: Commit.**

```bash
git add crates/pba_service/tests/features/transfer_reversal.feature \
        crates/pba_service/tests/steps/transfer_steps.rs \
        crates/pba_service/tests/  # picks up PbaWorld struct edits
git commit -m "test(e2e): reversal scenarios

13 scenarios per the spec, covering happy paths, all rejection modes,
idempotency, and per-transaction visibility."
```

---

## Task 11: Admin UI — Reverse button, form, server handlers

**Files:**
- Modify: `crates/pba_service/src/admin/transfer_handlers.rs`
- Modify: `crates/pba_service/src/admin.rs`
- Modify: `crates/pba_service/templates/admin/transfer_detail.html`
- Create: `crates/pba_service/templates/admin/transfer_reverse.html`

- [ ] **Step 1: Extend `TransferDetailTemplate`.**

In `crates/pba_service/src/admin/transfer_handlers.rs`, find the `TransferDetailTemplate` struct and add:

```rust
    can_reverse: bool,
    is_reversal: bool,
    reversed_by_id: Option<String>,
```

In the `transfer_detail` handler, populate these:

```rust
    let can_reverse = source_leg.status == TransactionStatus::Posted
        && source_leg.reverses_transaction_id.is_none();
    let is_reversal = source_leg.reverses_transaction_id.is_some();
    let reversed_by_id = state
        .transaction_repo
        .find_reversal_of(source_leg.id)
        .await
        .ok()
        .flatten()
        .map(|r| r.id.to_string());
```

Pass these into the template constructor.

- [ ] **Step 2: Add the reverse form handler.**

Below `void_transfer` in `transfer_handlers.rs`:

```rust
#[derive(Template)]
#[template(path = "admin/transfer_reverse.html")]
struct TransferReverseTemplate {
    prefix: String,
    transfer_id: String,
    source_account_id: String,
    destination_account_id: String,
    original_amount: String,
    error: Option<String>,
}

pub async fn reverse_transfer_form(
    State(state): State<AppState>,
    Path(transfer_id): Path<Uuid>,
) -> Response {
    let source_row = match state.transaction_repo.get_transaction(transfer_id).await {
        Ok(row) => row,
        Err(_) => return (StatusCode::NOT_FOUND, "Transfer not found").into_response(),
    };
    let legs = match source_row
        .correlation_id
        .map(|c| state.transaction_repo.find_by_correlation_id(c))
    {
        Some(fut) => match fut.await {
            Ok(legs) => legs,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response(),
        },
        None => return (StatusCode::NOT_FOUND, "Transfer correlation missing").into_response(),
    };
    let dest_leg = match legs
        .iter()
        .find(|l| l.account_kind == AccountKind::Pb)
    {
        Some(l) => l,
        None => return (StatusCode::NOT_FOUND, "Destination leg missing").into_response(),
    };

    render(TransferReverseTemplate {
        prefix: state.path_prefix.clone(),
        transfer_id: transfer_id.to_string(),
        source_account_id: source_row.account_id.to_string(),
        destination_account_id: dest_leg.account_id.to_string(),
        original_amount: source_row.amount_display(),
        error: None,
    })
}

#[derive(Deserialize)]
pub struct ReverseTransferForm {
    pub amount_paisa: u64,
    pub description: Option<String>,
}

pub async fn process_reverse_transfer(
    State(state): State<AppState>,
    Path(transfer_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<ReverseTransferForm>,
) -> Response {
    let source_row = match state.transaction_repo.get_transaction(transfer_id).await {
        Ok(row) => row,
        Err(_) => return (StatusCode::NOT_FOUND, "Transfer not found").into_response(),
    };

    match state
        .transfer_service
        .reverse_transfer(
            source_row.account_id,
            transfer_id,
            form.amount_paisa,
            None,
            form.description.as_deref(),
            None,
        )
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transfers/{transfer_id}"),
        ))
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to reverse transfer {transfer_id}: {e}");
            // Re-render the form with the error message.
            let legs = source_row
                .correlation_id
                .and_then(|c| {
                    futures::executor::block_on(state.transaction_repo.find_by_correlation_id(c))
                        .ok()
                });
            let dest_account_id = legs
                .as_ref()
                .and_then(|l| l.iter().find(|x| x.account_kind == AccountKind::Pb))
                .map(|l| l.account_id.to_string())
                .unwrap_or_default();
            render(TransferReverseTemplate {
                prefix: state.path_prefix.clone(),
                transfer_id: transfer_id.to_string(),
                source_account_id: source_row.account_id.to_string(),
                destination_account_id: dest_account_id,
                original_amount: source_row.amount_display(),
                error: Some(e.to_string()),
            })
        }
    }
}
```

`futures::executor::block_on` is a code smell; replace with an async refactor of the error path if it's easy — otherwise mirror whatever pattern existing handlers use. Search `tracing::error.*Failed to (post|void) transfer` for the closest existing example and copy its shape exactly. If existing handlers just redirect without re-rendering the form on error, **do the same** here — drop the re-render and just `Redirect::to(...)` with a query-string error param, matching `post_transfer`/`void_transfer` (lines 320–375 of the existing file).

- [ ] **Step 3: Wire the routes.**

In `crates/pba_service/src/admin.rs`, in the transfer routes block (around lines 121–127), add:

```rust
        .route(
            "/admin/transfers/{transfer_id}/reverse",
            get(transfer_handlers::reverse_transfer_form)
                .post(transfer_handlers::process_reverse_transfer),
        )
```

- [ ] **Step 4: Add the form template.**

Create `crates/pba_service/templates/admin/transfer_reverse.html` following the style of `transfer_detail.html` / `admin/normal_transfer.html` (the closest existing form template — read it first). Approximate content:

```html
{% extends "admin/base.html" %}

{% block content %}
<h1>Reverse Transfer</h1>

{% if let Some(err) = error %}
  <div class="error">{{ err }}</div>
{% endif %}

<form method="post" action="{{ prefix }}/admin/transfers/{{ transfer_id }}/reverse">
  <dl>
    <dt>Transfer ID</dt><dd>{{ transfer_id }}</dd>
    <dt>Source (normal) account</dt><dd>{{ source_account_id }}</dd>
    <dt>Destination (PB) account</dt><dd>{{ destination_account_id }}</dd>
    <dt>Original amount</dt><dd>₹{{ original_amount }}</dd>
  </dl>

  <label>
    Amount to reverse (paisa)
    <input type="number" name="amount_paisa" min="1" required>
    <small>Maximum: the original amount in paisa.</small>
  </label>

  <label>
    Description (optional)
    <input type="text" name="description" maxlength="256">
  </label>

  <button type="submit" class="danger">Reverse</button>
  <a href="{{ prefix }}/admin/transfers/{{ transfer_id }}">Cancel</a>
</form>
{% endblock %}
```

Use whatever CSS class names / form patterns the existing admin templates use — open `transfer_detail.html` and `normal_transfer.html` first and match the style.

- [ ] **Step 5: Update `transfer_detail.html`.**

Open `crates/pba_service/templates/admin/transfer_detail.html`. After the existing Post/Void buttons (gated by `can_post_or_void`), add:

```html
{% if can_reverse %}
  <a href="{{ prefix }}/admin/transfers/{{ transfer_id }}/reverse" class="button danger">
    Reverse
  </a>
{% else if reversed_by_id.is_some() %}
  {% if let Some(rid) = reversed_by_id %}
    <span class="info">
      Reversed by
      <a href="{{ prefix }}/admin/transactions/{{ rid }}">{{ rid }}</a>
    </span>
  {% endif %}
{% endif %}

{% if is_reversal %}
  <span class="info">This row is a reversal.</span>
{% endif %}
```

Verify the exact Askama syntax against neighbouring templates — the project uses askama 0.12+ but may have local conventions.

- [ ] **Step 6: Manual smoke test.**

Run: `just run-bg` (or `just run` and tail logs). Open `http://localhost:3030/admin`, log in, create a normal account, fund it, transfer to a PB account, navigate to `/admin/transfers/{transfer_id}`, click **Reverse**, submit the form. Verify the page redirects back and now shows "Reversed by …". Click that link, verify the reversal row's detail page shows "This row is a reversal" and offers no Reverse button.

- [ ] **Step 7: Commit.**

```bash
git add crates/pba_service/src/admin/transfer_handlers.rs \
        crates/pba_service/src/admin.rs \
        crates/pba_service/templates/admin/transfer_detail.html \
        crates/pba_service/templates/admin/transfer_reverse.html
git commit -m "feat(admin): Reverse button + form on transfer detail page

Posted transfers get a Reverse action; reversal rows are flagged; once
reversed, the original row shows a link to the reversal pair."
```

---

## Task 12: UI Cucumber feature

**Files:**
- Create: `crates/pba_service/tests/ui_features/transfer_reversal_admin.feature`
- Modify: `crates/pba_service/tests/ui_steps/...` (whichever step module owns transfer admin steps — check `tests/ui_features/transfer_admin.feature` and its corresponding steps file)

- [ ] **Step 1: Write the feature file using the 5 scenarios from the spec.**

```gherkin
Feature: Admin UI — reverse a normal→PB transfer

  Scenario: Reverse button appears on a posted transfer
    Given an admin is logged in
    And a normal account "ui-rev-1" has been funded with 5000 paisa
    And a transfer of 2000 paisa has been made from "ui-rev-1" to a "health" PB account
    When the admin opens the transfer detail page
    Then the Reverse button is visible

  Scenario: Reverse button is absent on a pending transfer
    Given an admin is logged in
    And a pending transfer of 1000 paisa exists from a normal account to a PB account
    When the admin opens the transfer detail page
    Then the Reverse button is not visible
    And the Post and Void buttons are visible

  Scenario: Reverse button is absent on a reversal row
    Given an admin is logged in
    And a transfer of 1500 paisa from a normal account has been fully reversed
    When the admin opens the reversal row's detail page
    Then the Reverse button is not visible
    And the page shows "This row is a reversal"

  Scenario: Reversal action flow
    Given an admin is logged in
    And a normal account "ui-rev-flow" has been funded with 10000 paisa
    And a transfer of 4000 paisa has been made from "ui-rev-flow" to a "education" PB account
    When the admin opens the transfer detail page
    And the admin clicks Reverse
    And the admin submits the reverse form with amount 4000
    Then the page shows "Reversed by"
    And the link navigates to the reversal row

  Scenario: Insufficient others-pool balance is surfaced
    Given an admin is logged in
    And a transfer of 1000 paisa has been made from a normal account to a PB account
    And a payment of 700 paisa has been made from the PB account
    When the admin opens the transfer detail page
    And the admin clicks Reverse
    And the admin submits the reverse form with amount 1000
    Then the form shows an InsufficientFunds error with available 300
```

- [ ] **Step 2: Implement the UI steps.**

Open the existing `tests/ui_features/transfer_admin.feature` and its corresponding steps file (`ls tests/ui_steps/` and grep for `transfer`). Implement the new steps following the same Chromedriver pattern — `world.driver.find_element(...)` and `click()` on the Reverse button selector.

Specifically, you'll need:
- `the Reverse button is visible / not visible` — assert presence/absence of an element with text "Reverse" inside the action buttons region.
- `the admin clicks Reverse` — find that anchor and click.
- `the admin submits the reverse form with amount N` — fill the `amount_paisa` input, submit the form.
- `the page shows "Reversed by"` — assert page contains that text.
- `the link navigates to the reversal row` — click "Reversed by" link, assert the resulting URL matches `/admin/transactions/{uuid}`.
- `the form shows an InsufficientFunds error with available N` — assert the rendered error contains "InsufficientFunds" or matches the error message wording chosen in Task 11 Step 2.

Match selector style and test scaffolding to existing ui_steps; do not invent new patterns.

- [ ] **Step 3: Run the UI tests.**

Run: `just ui-e2e`

Expected: all 5 scenarios PASS; the pre-existing `transfer_admin.feature` keeps passing.

- [ ] **Step 4: Commit.**

```bash
git add crates/pba_service/tests/ui_features/transfer_reversal_admin.feature \
        crates/pba_service/tests/ui_steps/
git commit -m "test(ui-e2e): transfer reversal admin scenarios"
```

---

## Task 13: Documentation

**Files:**
- Modify: `README.md`
- Modify: `WHAT.md`

- [ ] **Step 1: Update `README.md`.**

In the API table, after the row for `POST /normal-accounts/{id}/transfers/{id}/void`, add:

```markdown
| `POST` | `/normal-accounts/{id}/transfers/{id}/reverse` | Reverse a posted transfer (admin) |
```

- [ ] **Step 2: Update `WHAT.md`.**

Add a "Reversing a transfer" subsection under the normal-accounts section. One paragraph:

```markdown
### Reversing a transfer

If a posted normal→PB transfer needs to be unwound (for example, the PB
account holder did not meet the sponsor's matching requirements), an admin
can reverse it. The reversal is recorded as a new transaction pair
(PB others-pool debit + normal account credit) linked back to the original
via `reverses_transaction_id`. The original transfer rows are not mutated.
A transfer can be reversed at most once. If the PB others-pool has been
spent below the requested amount, the reversal is rejected with
`InsufficientFunds`. Pending transfers continue to be cancelled via void.
```

- [ ] **Step 3: Run `just local-ci`.**

Expected: all checks PASS.

- [ ] **Step 4: Commit.**

```bash
git add README.md WHAT.md
git commit -m "docs: reversal of normal→PB transfers"
```

---

## Final verification

- [ ] **Run the full local CI suite.**

```bash
just local-ci
just api-e2e
just ui-e2e
```

Expected: every check passes. If `just local-ci` already includes the e2e targets, skip the redundant calls.

- [ ] **Manual ledger invariant check.**

After running e2e (which exercises reversal), open the admin "System accounts" page (`/admin/system-accounts`) and confirm the sentinel balances reconcile:
- `TRUST_FUNDING_SOURCE` net debits = sum of (normal-account balances) + (PB-others received via transfer, net of reversals).
- `WITHDRAWAL_SETTLEMENT` only credited from code 1 (PB self) or code 3 (normal); never code 2 (PB others).

If the admin page does not show this breakdown, query TigerBeetle directly: `lookup_accounts` on the five sentinel IDs and validate the invariants from `ledger_repo.rs` documentation.

- [ ] **Open a PR.**

Title: `feat: reverse normal→PB transfers (#xx)`. Body must link to the spec at `docs/superpowers/specs/2026-05-21-transfer-reversal-design.md` and call out the schema migration explicitly.
