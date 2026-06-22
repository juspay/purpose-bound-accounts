# Two-Phase Reversal and Refund — Design

**Date:** 2026-06-22
**Status:** Draft

## Goal

Extend the existing two-phase lifecycle (`pending` → `post` / `void`) to two
operations that today land in a single shot:

- **Reversal of normal → PB transfers** (PR #38, `transfer_service::reverse_transfer`).
- **Refund of PB → merchant payments** (PR #40, `pb_payment_service::refund_payment`).

After this change, callers may initiate either as `Pending` with a timeout, then
resolve it later via `post` (commits the money movement) or `void` (rolls back).
The shape mirrors existing pending transfers and deposits — no new business
semantics, just feature parity.

## Background

The codebase already supports two-phase money movement for:

- **Deposits** (`POST /pb-accounts/{id}/deposits` with `pending: bool`, plus
  `POST /…/deposits/{id}/post|void`).
- **Transfers** (`POST /normal-accounts/{id}/transfers` with `pending: bool`,
  plus `POST /…/transfers/{id}/post|void`).

Both rely on TigerBeetle's pending-transfer primitive: a pending TB transfer
reserves the debit until it is posted (final commit) or voided (release). The
row's `status` column tracks `Pending → Posted/Settled` or `Pending → Voided`.
A `timeout_seconds` column on `transactions` stores the requested expiry.

Reversal and refund were both designed and shipped as single-shot only:

- `reverse_transfer` rejects originals whose status is not `Posted` and always
  inserts new rows with `status='posted'`.
- `refund_payment` always inserts rows with `status='settled'` and dispatches
  the TB transfer immediately.

This spec closes the parity gap without introducing new business motivations
(no gateway-confirmation integration is planned; both `post` and `void` remain
caller-initiated).

## Non-goals

- **External gateway integration.** Post/void are caller-initiated, not
  triggered by webhook callbacks or other external signals.
- **External gateway integration** beyond TB's pending-transfer timeout.
  See "Background expiry" below — the existing poller already covers the DB
  side for any pending row type, so this is a non-goal only in the sense that
  no new worker is built; coverage is automatic.
- **Partial post or partial void.** Once initiated as pending, a refund or
  reversal posts or voids in full. Splitting a pending refund into two posts
  is not supported.
- **New transaction types.** Pending refund rows stay
  `transaction_type='payment'`; pending reversal rows stay
  `transaction_type='transfer'`. Same `reverses_transaction_id` linkage. The
  `status` column carries the lifecycle state.
- **Schema migrations.** All required columns already exist
  (`status`, `timeout_seconds`, `tb_transfer_id`, `reverses_transaction_id`).
  The partial unique index from PR #38 is verified during implementation but
  not expected to change.
- **Sponsor-self-serve post/void.** Admin-only for this iteration, consistent
  with the existing reversal/refund posture.

## Reservation semantics

To prevent the TOCTOU race that PR #40's follow-up just fixed for settled
refunds, pending refunds and pending reversals must reserve their slice of the
budget:

- `sum_refunds_of(original_row_id)` and `sum_refunds_of_in_tx(...)` widen to
  match rows with `status IN ('pending','settled')`, explicitly excluding
  `voided`. A pending refund of 300 against a payment of 1000 leaves 700
  remaining-refundable for the next concurrent attempt.
- `find_reversal_of(original_transfer_id)` widens to match rows with
  `status IN ('pending','posted')`. A posted transfer with a pending reversal
  cannot be re-reversed until that pending one voids.
- Voiding a pending refund or reversal restores the budget. The next initiate
  call sees the freed capacity.

## Background expiry

The existing `run_deposit_timeout_poller` in `service/deposit_timeout.rs` is
already row-type agnostic:

- `transaction_repo::find_timed_out_pending()` selects any row where
  `status='pending' AND timeout_seconds IS NOT NULL AND now() > created_at +
  timeout_seconds`. No filter on `transaction_type` or row shape.
- The void path updates `WHERE correlation_id = $1 AND status = 'pending'`,
  also generic.

Pending refund (`type='payment'`) and pending reversal (`type='transfer'`
with `reverses_transaction_id` set) rows inherit expiry the moment we
populate `timeout_seconds` on them — which the initiate flow already does.
TigerBeetle's own pending-transfer timeout voids the TB transfer
independently; the poller keeps the DB rows in sync.

Two small touch-ups land in this PR for accuracy, not functionality:

1. **Rename** `run_deposit_timeout_poller` → `run_pending_timeout_poller` and
   its log messages ("Pending deposit timed out" → "Pending transaction timed
   out").
2. **Test coverage**: one timeout-expiry scenario per new feature file
   asserting that an aged-out pending refund/reversal becomes Voided and
   restores its reservation (remaining-refundable for refund;
   reversal-eligibility for the original transfer).

The reservation-restore guarantee (refund remaining goes back up, reversal
eligibility returns) follows automatically from the widened
`sum_refunds_of` / `find_reversal_of` status filters — voided rows are
excluded.

## Scope (high level)

| Area | Change |
|---|---|
| Schema | None expected. Verify PR #38's partial unique index covers pending rows during plan execution. |
| Smithy | Two new operations (`PostPBAccountRefund`, `VoidPBAccountRefund`). Extend `ReverseNormalAccountTransfer` and `RefundPBAccountPayment` inputs with optional `pending` + `timeout_seconds`. |
| Routes | Two new routes (`POST /pb-accounts/{id}/refunds/{id}/post`, `POST /pb-accounts/{id}/refunds/{id}/void`). No new routes for reversal — the existing `/transfers/{id}/post|void` handlers already accept pending reversal rows. |
| Domain | None. Reversal rows remain `transaction_type='transfer'`; refund rows remain `transaction_type='payment'`. |
| Repository | `transaction_repo`: widen `sum_refunds_of(_in_tx)` and `find_reversal_of` status filters. `ledger_repo`: three new TB helpers for pending refund single-leg, pending refund split (LINKED), pending reversal. |
| Service | `transfer_service::reverse_transfer` gains `pending` + `timeout_seconds`. `pb_payment_service::refund_payment` gains the same. New `post_refund` and `void_refund` methods on `pb_payment_service`. |
| Admin UI | "Hold as pending" mode on both initiate forms. Post/Void buttons on pending refund detail pages (reversal detail page inherits buttons from existing transfer detail page if its predicate already allows reversal rows). Status column on refund-history table gains Pending/Voided. |
| Worker | Rename `run_deposit_timeout_poller` → `run_pending_timeout_poller`; update log strings. No logic change. |
| Tests | Two new Cucumber API features, two new UI features, concurrency assertion that pending refunds reserve remaining, timeout-expiry scenarios for both halves, ledger unit tests for the new TB helpers. |

## Architecture

The feature is symmetric across two existing services. No new modules.

```
crates/pba_service/src/
├── admin/handlers.rs                      (Post/Void refund handlers + form updates)
├── admin.rs                               (route wiring for new admin endpoints)
├── api/dto.rs                             (pending/timeout fields on existing DTOs, new post/void DTOs)
├── api/handlers/pb.rs                     (post_refund, void_refund)
├── api/routes.rs                          (/refunds/{id}/post|void)
├── repository/
│   ├── ledger_repo.rs                     (three new pending TB helpers)
│   └── transaction_repo.rs                (status-aware sum_refunds_of / find_reversal_of)
├── service/
│   ├── deposit_timeout.rs                 (rename to pending_timeout; log strings)
│   ├── pb_payment_service.rs              (pending refund initiate + post_refund + void_refund)
│   └── transfer_service.rs                (pending reversal initiate; post/void unchanged)
└── templates/admin/
    ├── payment_refund.html                (Hold/Settle mode toggle, optional timeout input)
    ├── transfer_reverse.html              (same)
    └── transaction_detail.html            (Post/Void buttons on pending refund rows; Pending badge)
```

## API contract

### Reversal half (`/normal-accounts/{account_id}/transfers/{transfer_id}/...`)

**Initiate** — extend the existing endpoint:

```
POST /normal-accounts/{account_id}/transfers/{transfer_id}/reverse
  body: {
    amount: integer,
    pending: bool = false,                        (new)
    timeout_seconds: integer | null = null,       (new, ignored when pending=false)
    gateway_ref: string | null,
    description: string | null,
    idempotency_key: string | null,
  }
  → 201 ReverseResponse {
      reversal_id, correlation_id, original_amount, status,  ...
      // status is "pending" or "posted" depending on the request
  }
```

**Post / Void** — reuse the existing handlers. A pending reversal row is a
`transaction_type='transfer'` row in `status='pending'`, which is exactly what
the existing `post_transfer` / `void_transfer` handlers accept today. The
`reversal_id` returned by ReverseResponse is the normal-side credit row's id;
that's the id callers pass:

```
POST /normal-accounts/{account_id}/transfers/{reversal_id}/post
POST /normal-accounts/{account_id}/transfers/{reversal_id}/void
```

No new endpoints, no SDK regen for reversal post/void.

### Refund half (`/pb-accounts/{account_id}/...`)

**Initiate** — extend the existing endpoint:

```
POST /pb-accounts/{account_id}/payments/{payment_id}/refund
  body: {
    amount: integer,
    pending: bool = false,                        (new)
    timeout_seconds: integer | null = null,       (new, ignored when pending=false)
    description: string | null,
    gateway_ref: string | null,
    idempotency_key: string | null,
  }
  → 201 RefundResponse {
      refund_id, original_payment_id, account_id,
      amount, amount_to_self, amount_to_others,
      original_amount, remaining_refundable, status, ...
      // status is "pending" or "settled" depending on the request
  }
```

**Post / Void** — new endpoints:

```
POST /pb-accounts/{account_id}/refunds/{refund_id}/post
  → 200 PostRefundResponse { refund_id, status: "settled", correlation_id, ... }

POST /pb-accounts/{account_id}/refunds/{refund_id}/void
  → 200 VoidRefundResponse { refund_id, status: "voided", correlation_id, ... }
```

`refund_id` is the refund's `correlation_id`, which equals the primary row's
id (mirrors the make_payment / refund convention introduced in PR #40).

### Validation and errors

- `pending` defaults to `false`. Existing callers get identical behavior.
- `timeout_seconds`, when provided with `pending=false`, is ignored (no error).
- `timeout_seconds`, when omitted with `pending=true`, falls back to the
  service's `default_timeout_seconds`.
- Post / void on a non-Pending row → `TransactionNotPending` (400).
- Post / void on a non-existent or wrong-account refund id →
  `TransactionNotFound` (404).
- **Idempotent same-direction.** Post on an already-posted row → 200 no-op;
  void on an already-voided row → 200 no-op. Mixed-direction (post a voided
  row, void a posted row) → `TransactionNotPending`. The new `post_refund` /
  `void_refund` handlers implement this from the start. For the reversal
  half (which reuses `post_transfer` / `void_transfer`), the plan first
  verifies whether those handlers are already idempotent; if not, a small
  one-line fix in `transfer_service` lands as part of this PR so refund and
  reversal share the same posture.

## Service-layer flow

### `transfer_service::reverse_transfer`

Signature gains `pending: bool` and `timeout_seconds: Option<u32>`.

1. Idempotency replay — unchanged.
2. Eligibility — load and validate the original (Posted, correct kind, not
   itself a reversal). Unchanged.
3. **`find_reversal_of(original_id)` widens** to match `status IN
   ('pending','posted')` so a posted transfer with a pending reversal blocks
   re-reversal.
4. Amount validation — unchanged.
5. Insert reversal rows (PB-side debit + normal-side credit) with `status =
   Pending if pending else Posted` and `timeout_seconds` set on both legs when
   pending.
6. TB call:
   - `pending=false` → `create_internal_transfer_reversal(...)` (existing,
     immediate).
   - `pending=true` → `create_pending_internal_transfer_reversal(dest_pb_tb,
     source_normal_tb, amount, timeout)` (new). Returns a real
     `tb_transfer_id`; persist on both DB legs.
7. Commit DB tx. Return `ReverseResponse` with `status` reflecting the row
   state.

Post and void use the existing `post_transfer` / `void_transfer` methods —
no service-layer additions for the reversal half.

### `pb_payment_service::refund_payment`

Signature gains `pending: bool` and `timeout_seconds: Option<u32>`.

1. Idempotency replay — unchanged.
2. **Begin DB tx, `SELECT FOR UPDATE` on originals** — already the TOCTOU-safe
   flow.
3. Per-pool remaining via `sum_refunds_of_in_tx`, which now **counts Pending +
   Settled rows** (excludes Voided).
4. Amount validation — unchanged.
5. Allocation (self-first, then others) — unchanged.
6. Insert refund rows (one or two) with `status = Pending if pending else
   Settled` and `timeout_seconds` per leg.
7. TB call:
   - Single leg, `pending=false` → existing `create_payment_refund`.
   - Single leg, `pending=true` → new `create_pending_payment_refund(...)`.
     Returns `tb_transfer_id`, stored on the row.
   - Split, `pending=false` → existing `create_payment_refund_split`.
   - Split, `pending=true` → new `create_pending_payment_refund_split(...)`.
     Returns `(id_self, id_others)`, both LINKED + pending; persisted.
8. Commit DB tx. Return `RefundResult` with `status` reflecting the row state.

### New `pb_payment_service::post_refund` / `void_refund`

Both methods take `(pb_account_id, refund_id)` and follow the same skeleton:

1. Load all rows where `correlation_id = refund_id`. Reject if no rows.
2. Validate every row has `account_kind=Pb`, `transaction_type=Payment`,
   `account_id=pb_account_id`. Otherwise `TransactionNotFound`.
3. If every row is already in the target state (`Settled` for post, `Voided`
   for void), return the snapshot as a no-op (idempotent).
4. Otherwise every row must be `Pending`. Any other status →
   `TransactionNotPending`.
5. Per leg, call `post_pending_transfer(tb_transfer_id)` or
   `void_pending_transfer(tb_transfer_id)`. For LINKED splits, TB resolves
   both atomically when the first is posted/voided; the second call is a
   no-op against the resolved id (service still iterates each row for
   uniformity).
6. `UPDATE transactions SET status = $1, updated_at = now() WHERE
   correlation_id = $2` — mirrors `post_transfer`'s pattern.
7. Return the updated snapshot.

## Repository / ledger layer

### `transaction_repo`

- `sum_refunds_of(original_row_id) -> u64` and
  `sum_refunds_of_in_tx(tx, original_row_id) -> u64`: query filter widens from
  `WHERE reverses_transaction_id = $1` to
  `WHERE reverses_transaction_id = $1 AND status IN ('pending','settled')`.
- `find_reversal_of(original_transfer_id) -> Option<TransactionRecord>`:
  query filter widens to include `status IN ('pending','posted')`.
- No other repo changes.

### `ledger_repo`

Three new helpers; signatures mirror their non-pending counterparts plus a
`timeout` parameter, and they return the TB-generated transfer id(s):

- `create_pending_internal_transfer_reversal(debit_pb_tb, credit_normal_tb,
  amount, timeout) -> u128`.
- `create_pending_payment_refund(credit_pb_pool_tb, amount, timeout) -> u128`.
- `create_pending_payment_refund_split(credit_pb_self_tb, credit_pb_others_tb,
  amount_self, amount_others, timeout) -> (u128, u128)`. Both transfers carry
  the `LINKED` flag plus `pending`; the second leg is the un-linked terminator
  of the chain per TB semantics.

Existing `post_pending_transfer(tb_id)` and `void_pending_transfer(tb_id)` are
already generic and reused for all post/void paths.

The reversal helper may collapse into the existing
`create_pending_internal_transfer` if the only difference is the transfer
code (210 vs 410). Decision deferred to the plan.

## Admin UI

### Initiate forms

- `templates/admin/transfer_reverse.html` and
  `templates/admin/payment_refund.html` each gain a **Mode** radio:
  - "Settle now" (default) → submits with `pending=false`.
  - "Hold as pending" → reveals a **Timeout (seconds)** input with the
    service's default as placeholder; submits with `pending=true` and
    `timeout_seconds=<input or null>`.

### Detail pages

- **Pending reversal detail page** — existing transaction detail page (PR #21)
  renders Post/Void buttons for any `transaction_type='transfer'` row in
  `status='pending'`. A pending reversal row matches that predicate and gets
  the buttons for free. Verification step in the plan: ensure the predicate
  does not exclude rows with `reverses_transaction_id` set.
- **Pending refund detail page** — payment-typed rows do not currently render
  post/void controls. Add a Pending status badge and a Post / Void button pair
  when the row is in `status='pending'`. POSTs from the buttons target the new
  `/pb-accounts/{id}/refunds/{id}/post|void` admin endpoints.

### Refund history table (on the original payment's detail page)

- Status column gains `Pending` / `Settled` / `Voided` labels.
- Voided rows render struck-through and are excluded from the "Total refunded"
  sum (consistent with the widened `sum_refunds_of` filter).
- Pending rows link to the refund's detail page where Post/Void live.

### Transaction list filters

No changes — the status column already renders Pending/Posted/Voided for
transfers and deposits; refund Pending state slots in via the same template
column.

## Tests

### Cucumber API features

- New `tests/features/transfer_reversal_two_phase.feature` (~7 scenarios):
  1. Pending reversal then post — normal balance credited only after post.
  2. Pending reversal then void — no balance change, original re-reversible.
  3. Pending reversal blocks a second reversal attempt.
  4. Post on an already-posted reversal is idempotent.
  5. Void on an already-voided reversal is idempotent.
  6. Mixed-direction post-then-void rejected with `TransactionNotPending`.
  7. Pending reversal with short timeout ages out → background poller voids,
     original transfer re-reversible.
- New `tests/features/payment_refund_two_phase.feature` (~9 scenarios):
  1. Pending single-pool refund then post.
  2. Pending split refund then post (LINKED TB legs resolve atomically).
  3. Pending refund then void — remaining refundable restored.
  4. Concurrent pending-refund initiates reserve remaining (no over-refund).
  5. Post idempotency.
  6. Void idempotency.
  7. Post on a non-existent refund id → 404.
  8. Full lifecycle: pay → pending refund → void → new pending refund → post.
  9. Pending refund with short timeout ages out → background poller voids,
     remaining refundable restored.
- Extend `payment_refund.feature` and `transfer_reversal.feature` (existing)
  with one scenario each asserting that `pending=false` default behavior is
  unchanged.

### Cucumber UI features

- New `tests/ui_features/payment_refund_two_phase_admin.feature` (~4
  scenarios): refund form submitted in pending mode, pending detail page
  renders Post/Void buttons, post via UI moves status to Settled, void via UI
  moves status to Voided and restores remaining.
- New `tests/ui_features/transfer_reversal_two_phase_admin.feature` (~3
  scenarios): reversal form submitted in pending mode, post via existing
  transfer detail page Post button, void via existing Void button.

### Unit tests

- `ledger_repo` unit tests for each new TB helper: assert returned
  `tb_transfer_id` is non-zero and the TB account's pending balance reflects
  the reserve.

### Verification gate

`just e2e-all` green, rustfmt clean, clippy clean, conventional-commit titles
on all commits.

## Open items resolved during plan

- TB LINKED post/void semantics: confirm whether resolving the head of the
  link automatically resolves the tail (in which case `post_refund` only
  needs to call `post_pending_transfer` once per refund correlation, not per
  leg). If not, the loop posts each leg.
- Idempotent post/void behavior on already-resolved rows: confirm
  `post_transfer` returns 200 no-op rather than an error, and mirror it.
- `find_reversal_of` widening interaction with the PR #38 partial unique
  index — verify the index does not need to widen too (current scope:
  `WHERE type='transfer'`, no status filter; expected fine).
- Whether `create_pending_internal_transfer_reversal` collapses into the
  existing `create_pending_internal_transfer` by parameterizing the code.

## Out of scope (explicit non-deliverables)

- Webhook callbacks driving post/void.
- Self-serve (non-admin) post/void.
- Partial post or partial void.
- Schema migrations.
- New timeout worker. Background expiry is in scope but achieved entirely by
  populating `timeout_seconds` on the new pending rows and renaming the
  existing poller; no new worker logic.
