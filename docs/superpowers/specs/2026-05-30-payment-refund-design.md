# Refunds of PB → Merchant Payments — Design

**Date:** 2026-05-30
**Status:** Draft

## Goal

Add admin-initiated **refund** of a settled PB → merchant payment. Each refund
is recorded as a new compensating transaction (1 or 2 rows mirroring the
payment's pool split) plus matching TB transfer(s) in the opposite direction
(merchant settlement debit → PB pool credit). The original payment rows are
never mutated; refunds link back via `reverses_transaction_id` on each refund
row.

## Background

`pb_payment_service::make_payment` debits a PB account's pools (others-first
allocation, may split across both) and credits the `MERCHANT_SETTLEMENT`
sentinel. Payments are always immediate-settled (no pending state). Today
there is no on-ledger path to move money back from the merchant sentinel to a
PB account — operators have to fix it out of band, which is not auditable on
the ledger.

The pattern is set by PR #38 (`transfer-reversal-design`, commit `d9abbc9`),
which added reversal of normal → PB transfers as a compensating transaction
pair linked via a new `reverses_transaction_id` column. Refunds reuse that
column and the same data-model shape, but adapt it to the realities of
payments:

- a payment can be split across two pools (rows), so a single refund may
  produce up to two compensating rows;
- merchant refunds in practice are often **multiple partial refunds** of one
  payment (per-item returns), so we allow many refunds totaling ≤ original.

## Non-goals

- **Refunding pending payments.** Payments are always settled in this
  codebase; there is no pending payment state to handle.
- **Refund of withdrawals, deposits, or normal-account transfers.** Scope is
  limited to PB → merchant payments (`txn_type='payment'`,
  `direction='outbound'`).
- **Merchant-initiated refund.** Admin-only for this iteration. The merchant
  self-serve path would require a new merchant auth surface that doesn't
  exist yet (the PB account holder is the only non-admin caller today).
- **MCC re-check on refund.** The MCC was already validated when the original
  payment landed; a refund restores state rather than authorising new spend.
- **Time-window or expiration of refund eligibility.** A settled payment
  remains refundable indefinitely as long as remaining-unrefunded > 0 and the
  PB account is Active.
- **Mutation of original payment rows.** `status='settled'` means "this money
  moved"; the fact that a refund happened lives entirely in the new refund
  rows via `reverses_transaction_id`.
- **Automatic onward action on the refunded funds.** After a refund, the
  credit sits in the PB account's pool(s); no automatic withdrawal or
  re-deposit. The admin can take further action separately.
- **Relaxing transfer reversal to multi-partial in this PR.** The schema
  change is forward-compatible (see Future symmetry below), but
  `transfer_service::reverse_transfer` keeps its at-most-one semantics
  untouched. A follow-up PR can adopt the same sum-check flow.

## Scope (high level)

| Area | Change |
|---|---|
| Schema | One migration: replace transfer-reversal's broad partial unique index with one restricted to `type='transfer'`. No new columns. |
| Smithy | New `RefundPBAccountPayment` operation in `model/payment.smithy`. |
| Routes | New `POST /pb-accounts/{account_id}/payments/{payment_id}/refund`. |
| Domain | One branch in `type_label()` for payment rows with `reverses_transaction_id` set ("Refund"). No new `TransactionType` variant, no new domain fields. |
| Repository | `transaction_repo` gains `find_refunds_of` and `sum_refunds_of` (both type-agnostic; reused by future transfer-reversal multi-partial work). |
| Service | New `pb_payment_service::refund_payment` method; no new service struct. |
| Tests | One new Cucumber feature, one new UI feature, unit tests on the new service method and the new repo helpers. |
| UI | Refund button on settled-payment detail; refund history block on partially/fully refunded payments; "Refund of [payment]" affordance on refund rows. |

## Architecture

The feature lives entirely inside `pb_payment_service` and its existing
collaborators — no new domain modules, no new services, no new TB sentinels.

```
crates/pba_service/src/
├── domain/transaction.rs                 (one type_label branch)
├── repository/
│   ├── ledger_repo.rs                    (new create_payment_refund + code 210)
│   └── transaction_repo.rs               (new find_refunds_of, sum_refunds_of)
├── service/pb_payment_service.rs         (new refund_payment method, RefundResult)
├── api/
│   ├── handlers/payment.rs               (new refund_payment handler)
│   ├── routes.rs                         (one new route)
│   └── dto.rs                            (RefundPaymentRequest, RefundResponse)
└── db/migrations/
    └── 20260530000001_payment_refund.sql
```

## Schema & migrations

### `20260530000001_payment_refund.sql`

```sql
-- Tighten the transfer-reversal uniqueness so payment refunds can have many
-- rows pointing at the same original payment row.
DROP INDEX uq_transactions_reverses;
CREATE UNIQUE INDEX uq_transactions_reverses_transfer
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL AND type = 'transfer';
```

The plain partial index `idx_transactions_reverses_transaction_id` from PR
#38 is unchanged. It supports both lookup paths:
`transaction_repo::find_reversal_of` (single row, transfers) and the new
`find_refunds_of` (many rows, payments).

The tightened index preserves the existing at-most-one-reversal invariant
for transfers without leaking into payment refunds.

### What stays untouched

- `pb_accounts`, `normal_accounts`, `purpose_mcc_allowlist` — unchanged.
- TB sentinels — none added; refunds debit the existing
  `MERCHANT_SETTLEMENT_TB_ID`.
- `TransactionType` Rust enum and the `type` SQL column — reuse `'payment'`.
- The `idx_transactions_reverses_transaction_id` partial index — unchanged.

## Data model

### Row layout for a refunded payment

Consider an original payment with `correlation_id = P_id` that split across
both pools — `P_others` (₹600, pool='others') and `P_self` (₹400, pool='self').
Admin issues two sequential refunds: ₹500 then ₹500.

| Row | account_id | txn_type | direction | pool | status | correlation_id | reverses_transaction_id | amount |
|---|---|---|---|---|---|---|---|---|
| original others | acct | payment | outbound | others | settled | P_id | NULL | 600 |
| original self | acct | payment | outbound | self | settled | P_id | NULL | 400 |
| refund 1 self-leg | acct | payment | inbound | self | settled | R1_id | **P_self.id** | 400 |
| refund 1 others-leg | acct | payment | inbound | others | settled | R1_id | **P_others.id** | 100 |
| refund 2 others-leg | acct | payment | inbound | others | settled | R2_id | **P_others.id** | 500 |

After both refunds, `sum_refunds_of(P_self.id) = 400` (= P_self.amount) and
`sum_refunds_of(P_others.id) = 600` (= P_others.amount); the payment is fully
refunded and a third refund attempt returns `RefundAmountInvalid { requested,
remaining: 0 }`.

`account_kind`, `funding_type` and `merchant_id` / `merchant_mcc` on the
refund rows mirror the original payment row they reverse, so per-account
filtering and merchant-keyed lookups continue to work.

### Domain types

No new field on `TransactionRecord`; `reverses_transaction_id` already added
by PR #38. `type_label()` gains a branch: when `transaction_type ==
Payment` and `reverses_transaction_id.is_some()`, render `"Refund"`.

No new `TransactionType` variant. No new `TransactionDirection` variant.

## Ledger conventions

### New TB transfer code

| Code | Operation |
|---|---|
| 200 | PB payment (existing) |
| **210** | **PB payment refund — merchant settlement → PB pool — immediate** |

No pending variant — refunds are always immediate (admin chose to refund).

### Direction and accounts

A refund TB transfer:

- **Debit:** `MERCHANT_SETTLEMENT_TB_ID` (sentinel, code 4, only `LINKED`
  flag — debiting is unconstrained at the TB layer).
- **Credit:** the PB account's `tb_self_account_id` or
  `tb_others_account_id`, depending on which pool the refund row credits.
- **Amount:** the allocator-computed per-pool amount (see service step 6).

When a refund spans both pools (e.g. ₹500 against a ₹400-self / ₹600-others
payment with self_remaining=₹400, others_remaining=₹600), we issue
**linked TB transfers** via `ledger_repo::create_payment_refund_split` —
one code-210 transfer crediting self, one code-210 crediting others. They
land atomically (TB's LINKED flag) so either both rows are visible or
neither.

When a refund touches one pool only, we issue a single code-210 transfer via
`ledger_repo::create_payment_refund`.

### Conservation invariants

The invariant block at the top of `ledger_repo.rs` is updated:

> 2. For each PB account: `tb_others_account_id.credits_posted -
>    .debits_posted` equals (others-pool deposits ‑ others-pool payments **+
>    others-pool payment refunds**).
> 3. Same as (2) for `tb_self_account_id`.

One new documented invariant:

> 6. For each merchant: (sum of `MERCHANT_SETTLEMENT_TB_ID` credits at code
>    200 keyed by merchant_id) − (sum of debits at code 210 keyed by
>    merchant_id) equals the net outstanding settlement balance for that
>    merchant.
>
>    (The TB transfer carries no merchant_id; this invariant is computed
>    from the PG `transactions` table where merchant_id is recorded on every
>    payment and refund row.)

### ID derivation

No new `tb_*_id` helpers. The refund moves balance between an existing
sentinel and two existing pool accounts; ids come from the PB account row
the existing way (`pb_accounts.tb_self_account_id` /
`tb_others_account_id`).

## Service layer

### `pb_payment_service::refund_payment`

```rust
pub async fn refund_payment(
    &self,
    pb_account_id: Uuid,          // url param; validated against original rows
    original_payment_id: Uuid,    // = original payment's correlation_id
    amount: u64,                  // 0 < amount ≤ remaining-unrefunded total
    description: Option<&str>,
    gateway_ref: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<RefundResult, AppError>
```

**Flow:**

1. **Idempotency replay.** If `idempotency_key` is set, look up
   `(AccountKind::Pb, pb_account_id, key)`. On hit, fetch the row's
   `correlation_id`, reload all refund rows under it, and return.
   Idempotency key lives on the **primary** refund row (the self-leg if it
   exists, else the others-leg) — consistent with `make_payment`'s
   convention.
2. **Load original payment rows.** `transaction_repo::find_by_correlation_id
   (original_payment_id)` returns 1 or 2 rows. Reject unless every row has
   `account_id = pb_account_id`, `txn_type='payment'`,
   `direction='outbound'`, `status='settled'`, and
   `reverses_transaction_id IS NULL` (the original is not itself a refund).
   Map failures to `RefundNotRefundable(id, reason)` with `reason ∈
   {not_settled, is_itself_a_refund, wrong_type, wrong_account}`.
3. **Compute per-pool remaining-unrefunded.** Identify `P_self` (row with
   `pool='self'`, may be absent) and `P_others` (row with `pool='others'`,
   may be absent). For each present row:
   `remaining = row.amount - sum_refunds_of(row.id)`.
4. **Validate amount.** `0 < amount ≤ self_remaining + others_remaining`,
   else `RefundAmountInvalid { requested, remaining: total_remaining }`. If
   `total_remaining == 0`, return `PaymentFullyRefunded(original_payment_id)`
   for a clearer error (this is a strict subset of `RefundAmountInvalid`
   with `remaining=0` and is provided as a dedicated variant for client
   ergonomics).
5. **PB account active check.** Reload `pb_account_repo::get_account(
   pb_account_id)`. If not `Active` → `PbAccountNotActive`. Admin must
   reactivate first, mirroring transfer reversal.
6. **Allocate amount self-first.**
   `take_self = min(amount, self_remaining)`,
   `take_others = amount - take_self`.
   (When `P_self` is absent, `self_remaining = 0` and the whole amount
   goes to others. When `P_others` is absent, the validation in step 4
   ensures `amount ≤ self_remaining`.)
7. **Insert refund rows in one PG transaction.** Generate a new
   `correlation_id = R_id` (`Uuid::now_v7()`).
   - If `take_self > 0`: row with `txn_type='payment'`,
     `direction='inbound'`, `pool='self'`, `status='settled'`,
     `funding_type=P_self.funding_type`, `merchant_id=P_self.merchant_id`,
     `merchant_mcc=P_self.merchant_mcc`, `correlation_id=R_id`,
     `reverses_transaction_id=P_self.id`,
     `idempotency_key=$key` (if provided),
     `tb_transfer_id=0` (filled after the TB call).
   - If `take_others > 0`: same shape, `pool='others'`,
     `reverses_transaction_id=P_others.id`, and
     `idempotency_key=NULL` when the self-leg already carries the key
     (idempotency_key is unique per `(account_kind, account_id, key)`).
   - When only one leg exists, that leg carries the idempotency key.
8. **Execute the TB transfer(s).**
   - Both legs: `ledger_repo::create_payment_refund_split(
     pb_self_tb_id, pb_others_tb_id, take_self, take_others)` → two
     linked code-210 transfers.
   - One leg: `ledger_repo::create_payment_refund(pb_pool_tb_id, amount)`
     → single code-210 transfer.
   - On any TB error → roll back PG transaction and propagate. The
     merchant sentinel has no balance constraint, so no `ExceedsBalance`
     path here.
9. **Persist `tb_transfer_id`** on both refund rows via
   `UPDATE transactions SET tb_transfer_id=$1 WHERE correlation_id=$2`,
   one statement per row (each TB transfer has its own id).
10. **COMMIT.** Reload refund rows and return `RefundResult`.

### `RefundResult`

```rust
pub struct RefundResult {
    pub refund_id: Uuid,                // = R_id (correlation_id)
    pub original_payment_id: Uuid,      // P_id
    pub account_id: Uuid,
    pub amount: u64,                    // = take_self + take_others
    pub amount_to_self: u64,
    pub amount_to_others: u64,
    pub original_amount: u64,           // P_self.amount + P_others.amount
    pub remaining_refundable: u64,      // after this refund
    pub status: TransactionStatus,      // always Settled
    pub correlation_id: Uuid,           // = R_id
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

### Where rules live

| Rule | Location |
|---|---|
| Original must be a `payment`, settled, not a refund | service step 2 |
| Sum of refund amounts ≤ original per-pool | service steps 3–4 (no DB constraint — service-only by design) |
| Self-first allocation | service step 6 |
| Atomicity of multi-pool refund | TB linked transfers (step 8) |
| Idempotency keyed on the primary refund row | service step 7 |
| PB account must be Active | service step 5 |
| At-most-one-reversal preserved for transfers | tightened partial unique index on `reverses_transaction_id` `WHERE type='transfer'` |

## Repository layer

### `ledger_repo.rs`

- New constant `PAYMENT_REFUND_CODE: u16 = 210`.
- New method `create_payment_refund(credit_pb_pool_tb_id: u128, amount:
  u64)` — single code-210 transfer, debit `MERCHANT_SETTLEMENT_TB_ID`,
  credit the given pool. Wraps `create_transfer`.
- New method `create_payment_refund_split(credit_pb_self_tb_id: u128,
  credit_pb_others_tb_id: u128, amount_self: u64, amount_others: u64)` —
  two linked code-210 transfers, both debiting
  `MERCHANT_SETTLEMENT_TB_ID`. Used when both legs are non-zero. Wraps
  `create_linked_transfers` with mirrored direction (the existing helper
  was written for the payment direction; the refund variant either reuses
  it with swapped debit/credit args or adds a thin sibling — confirmed at
  plan write-up).

### `transaction_repo.rs`

- New helper `find_refunds_of(original_row_id: Uuid) ->
  Result<Vec<TransactionRecord>, AppError>` — returns every refund row
  whose `reverses_transaction_id` matches. Used by `find_reversal_of`
  callers that want the multi-row case (it returns Vec; existing single-row
  callers can `.into_iter().next()` or keep using `find_reversal_of`).
- New helper `sum_refunds_of(original_row_id: Uuid) -> Result<u64,
  AppError>` — small aggregate (`SELECT COALESCE(SUM(amount), 0) FROM
  transactions WHERE reverses_transaction_id = $1`). Used by service step 3.
- `insert_in_tx` signature is unchanged — `reverses_transaction_id` was
  already added by PR #38.

Both new helpers are **type-agnostic**: they work for transfers as well as
payments. When a future PR relaxes transfer reversal to multi-partial,
`transfer_service::reverse_transfer` can adopt the same `sum_refunds_of`
flow without further repo changes.

## API surface

### Smithy operation (`model/payment.smithy`)

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

`{payment_id}` is the original payment's `correlation_id` (= the primary
row's id for payments written by `make_payment`). `{account_id}` is the PB
account; the service validates the original rows belong to it.

### Route (`api/routes.rs`)

```
.route(
    "/pb-accounts/{account_id}/payments/{payment_id}/refund",
    post(handlers::payment::refund_payment),
)
```

### DTOs (`api/dto.rs`)

```rust
RefundPaymentRequest {
    amount: u64,
    description: Option<String>,
    gatewayRef: Option<String>,
    idempotencyKey: Option<String>,
}

RefundResponse {
    refundId: Uuid,
    originalPaymentId: Uuid,
    accountId: Uuid,
    amount: u64,
    amountToSelf: u64,
    amountToOthers: u64,
    originalAmount: u64,
    remainingRefundable: u64,
    status: String,
    correlationId: Uuid,
    createdAt: DateTime<Utc>,
}
```

`TransactionDto` already carries `reversesTransactionId: Option<Uuid>` from
PR #38 — no change.

### Handler

Add `refund_payment` to `api/handlers/payment.rs` — a thin wrapper that
extracts the path params and request body, calls
`pb_payment_service.refund_payment(...)`, and maps `RefundResult` to
`RefundResponse`.

### Error mappings

| Variant | HTTP | Body |
|---|---|---|
| `RefundNotRefundable(id, reason)` | 409 | `{ "error": "refund_not_refundable", "id": "…", "reason": "not_settled\|is_itself_a_refund\|wrong_type\|wrong_account" }` |
| `RefundAmountInvalid { requested, remaining }` | 400 | `{ "error": "refund_amount_invalid", "requested": …, "remaining": … }` |
| `PaymentFullyRefunded(id)` | 409 | `{ "error": "payment_fully_refunded", "id": "…" }` |
| `PbAccountNotActive` (existing) | 409 | `{ "error": "pb_account_not_active", "id": "…" }` |
| `TransactionNotFound` (existing) | 404 | `{ "error": "transaction_not_found", "id": "…" }` |

### Auth

Admin-only. Gated by the same admin-role mechanism used by transfer
reversal (resolved at plan write-up — reuse existing extractor if present,
otherwise inherit the same approach transfer reversal took).

### Admin UI

The transaction detail page (`templates/admin/transaction_detail.html`)
gains, when the row is a settled payment:

- A **Refund** button — visible only when `txn_type='payment'`,
  `status='settled'`, `reverses_transaction_id IS NULL`, and
  `remaining_refundable > 0`.
- A **Refund history** block — appears when the payment has at least one
  refund row. Lists each refund by date, total amount, split breakdown
  (`amount_to_self` / `amount_to_others`), and a link to the refund
  correlation_id. Above the list: `"Remaining refundable: ₹X.XX"`.

A new template `templates/admin/payment_refund.html` provides the refund
form: amount input pre-filled with `remaining_refundable`, an editable
description field, submit. On `RefundAmountInvalid`, the error renders
inline with the `remaining` figure highlighted.

On a refund row's detail page (a row with `txn_type='payment'`,
`reverses_transaction_id IS NOT NULL`): an affordance
`"Refund of [original payment link]"` and no Refund button.

## Testing strategy

### Unit tests

| File | Tests |
|---|---|
| `service/pb_payment_service.rs` | `refund_payment` full refund of single-pool payment (self-only, others-only); full refund of split payment; partial refund self-only when self_remaining covers it; partial refund spanning self+others; sequential partial refunds totaling original (second refund is others-only after first drains self); reject `amount=0`; reject `amount > total_remaining`; reject already-fully-refunded payment (`PaymentFullyRefunded`); reject refunding a refund row; reject when PB account frozen / closed; idempotency replay returns same refund rows without a second TB call; both refund rows share the new correlation_id; `reverses_transaction_id` set on every refund row pointing at the matching pool row. |
| `repository/transaction_repo.rs` | `find_refunds_of` returns every matching row in id order; `sum_refunds_of` returns 0 on no rows and the correct aggregate otherwise; tightened partial unique index still rejects two transfer-reversals targeting the same source row; same index allows multiple refund rows pointing at the same payment row. |
| `repository/ledger_repo.rs` | `create_payment_refund` writes one code-210 TB transfer debiting `MERCHANT_SETTLEMENT_TB_ID` and crediting the given pool; `create_payment_refund_split` writes two linked code-210 transfers (both debit merchant), both land atomically on success and neither lands on simulated failure of the second. |
| `domain/transaction.rs` | `type_label()` returns `"Refund"` when `reverses_transaction_id.is_some()` on a payment row, and `"Reversal"` on a transfer row (existing behaviour preserved). |

### Cucumber BDD — new feature: `payment_refund.feature`

Scenarios:

1. **Happy path — full refund of single-pool (others) payment.**
2. **Happy path — full refund of split (self + others) payment** → two
   refund rows produced, balances restored to pre-payment state.
3. **Partial refund self-only** — refund ≤ original self-row amount, leaves
   others-row untouched.
4. **Partial refund spanning self + others** — drains self-row, partially
   draws on others-row.
5. **Sequential partial refunds totaling original** — first refund
   ₹500 (self-drained + part of others), second refund ₹500 (others-only),
   total = original; third attempt returns `PaymentFullyRefunded`.
6. **Reject amount > total remaining** → 400 `refund_amount_invalid` with
   correct `remaining`.
7. **Reject amount = 0** → 400 `refund_amount_invalid`.
8. **Reject refunding a refund row** → 409 `refund_not_refundable` with
   `reason='is_itself_a_refund'`.
9. **Reject when PB account frozen** → 409 `pb_account_not_active`;
   reactivate and retry succeeds.
10. **Idempotency replay** — same `idempotency_key` twice → second call
    returns the same refund pair; assert TB transfer count via the
    explorer is unchanged.
11. **Per-account visibility** — `GET /pb-accounts/{id}/transactions` and
    `ListAllTransactions` show original payment + refund rows; refund
    rows render with direction `inbound` and `type_label='Refund'`.
12. **Wrong PB account in URL** — original belongs to `acct_A`, URL says
    `acct_B` → 404 `transaction_not_found`.
13. **Linked-transfer atomicity** — simulated TB failure on the others
    leg rolls back the self leg (no orphan rows, no TB-only credits).

### UI tests — new feature: `payment_refund_admin.feature`

1. **Refund button appears** on settled-payment detail page (single-pool
   and split-pool variants).
2. **Refund button absent** on a refund row's detail page.
3. **Refund button absent** on a fully-refunded payment.
4. **Refund flow** — admin opens the form, amount is pre-filled with the
   correct `remaining_refundable`, submits, page refreshes and shows the
   refund history block with the new entry.
5. **Over-amount surfaces inline error** with the available remaining.

### Regression coverage

The existing transfer-reversal Cucumber suite stays green — refund is
purely additive at the API surface and the schema change preserves the
"at most one reversal per transfer" invariant via the tightened partial
unique index. The existing payment Cucumber suite stays green — `pb_payment_service::make_payment` is untouched.

### Coverage gates

`just local-ci` runs everything; no new gates. Optional Cucumber tag
`@refund` for local development.

## Future symmetry

The schema change and the new repo helpers are designed to be reused when
the codebase relaxes **transfer reversal** to multi-partial as well:

- The tightened partial unique index `WHERE type='transfer'` is the only
  thing preventing multi-partial transfer reversal today. Dropping it
  (one-line migration) is sufficient at the schema layer.
- `find_refunds_of` and `sum_refunds_of` are type-agnostic by design —
  they operate on `reverses_transaction_id`, not on `type`. A future
  `transfer_service::reverse_transfer` rewrite can substitute its
  current single-row precondition (`find_reversal_of` returns None) for
  the same sum-check flow (`sum_refunds_of(original_id) + amount ≤
  original.amount`) without further repo changes.

This refund PR does not change transfer-reversal behaviour. The follow-up
work, if pursued, would be a focused PR that drops the partial unique
index, rewrites `reverse_transfer` to the sum-check flow, and reshapes
the transfer-reversal Cucumber scenarios that currently assert
`transfer_already_reversed` into the new `reversal_amount_invalid` shape.

## Rollout plan

Single PR. The change is additive at the schema layer (one index swap, no
columns), at the Smithy layer (one new operation), and at the service
layer (one new method on an existing service). There is no multi-instance
ordering hazard: the index rename happens in one statement; only new code
writes payment refunds.

### Documentation updates

- `README.md` — extend the API table with the new
  `POST /pb-accounts/{id}/payments/{id}/refund` row; one-line description.
- `WHAT.md` — short "Refunding a payment" subsection in the PB-accounts
  section, noting admin-only, multiple partial allowed, self-first pool
  routing, and the merchant sentinel as the counterparty.

## Open items (none blocking)

- Confirm at plan write-up whether `ledger_repo::create_payment_refund_split`
  can reuse `create_linked_transfers` directly (with swapped debit/credit
  arguments) or whether a small sibling helper is cleaner. Mechanical;
  decided at code time.
- Confirm at plan write-up the admin-role gate used for transfer reversal
  and reuse it verbatim. If absent, follow the same approach transfer
  reversal took.
