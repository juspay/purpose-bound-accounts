# Reversal of Normal → PB Transfers — Design

**Date:** 2026-05-21
**Status:** Approved

## Goal

Add admin-initiated **reversal** of a posted normal → PB transfer. Reversal is
recorded as a new compensating transaction pair (PB others-pool debit + normal
account credit) plus a new TB transfer in the opposite direction. The original
transfer rows are never mutated.

## Background

Phase 3 of the normal-accounts work (commit `9dafc47`) introduced normal → PB
transfers as the only path for trust money to enter a PB account. A transfer
either lands immediately (`pending=false` → `status='posted'`) or is staged with
a TB pending transfer that can later be posted or voided. The existing
`VoidNormalAccountTransfer` operation covers the pending case.

There is currently no path to reclaim money from a transfer that has already
posted. In practice, the sponsor (normal-account holder) sometimes funds a PB
account before it is fully established that the PB account holder meets the
sponsor's matching requirements (e.g. enrollment in a specific program). When
that fails, the sponsor needs the money back. Today the operator's only option
is out-of-band recovery, which is not auditable on-ledger.

## Non-goals

- **Reversing pending transfers.** `VoidNormalAccountTransfer` already covers
  that path; the new operation refuses pending originals and tells the caller
  to use void instead.
- **Partial reversal of a shortfall.** If the destination PB others-pool has
  been spent down below the requested amount, we reject with `InsufficientFunds`
  and surface the available balance. The admin chooses to retry with a smaller
  amount or accept the loss. We do not attempt a "recover what you can" partial
  pass.
- **Multiple reversals per transfer.** At most one reversal per original
  transfer, enforced by a DB partial unique index.
- **Reversal of payments, withdrawals, or PB-side deposits.** Scope is limited
  to normal → PB transfers (`txn_type='transfer'`, `direction='outbound'`,
  source-side row).
- **Time-window or expiration of reversal eligibility.** A posted transfer
  remains reversible indefinitely as long as the PB others-pool balance allows.
- **Sponsor-self-serve reversal.** Admin-only for this iteration. The
  sponsor-initiated path can be added later by simply not gating the endpoint
  on the admin role.
- **Automatic onward withdrawal of the reclaimed funds back to the sponsor's
  bank.** After a reversal, the funds sit in the source normal account. The
  admin can then call `WithdrawFromNormalAccount` separately if desired.
- **Mutation of the original transfer rows.** `status='posted'` means "this
  money moved and is settled"; the fact that a reversal happened lives entirely
  in the new reversal pair via `reverses_transaction_id`.

## Scope (high level)

| Area | Change |
|---|---|
| Schema | One migration: nullable `reverses_transaction_id` column on `transactions`, partial unique index. |
| Smithy | New `ReverseNormalAccountTransfer` operation in `model/transfer.smithy`. |
| Routes | New `POST /normal-accounts/{account_id}/transfers/{transfer_id}/reverse`. |
| Domain | One field on `TransactionRecord`; one branch in `type_label()`. No new `TransactionType` variant. |
| Repository | `transaction_repo` insert/select gains `reverses_transaction_id`; new `find_reversal_of`. `ledger_repo` gains `create_internal_transfer_reversal` with TB code 410. |
| Service | New `transfer_service::reverse_transfer` method; no new service struct. |
| Tests | One new Cucumber feature, one new UI feature, unit tests on the affected modules. |
| UI | Reverse button on posted-transfer detail; "Reversed by [link]" affordance on already-reversed originals. |

## Architecture

The feature lives entirely inside the existing `transfer_service` and its
collaborators — no new domain modules, no new services, no new TB sentinels.

```
crates/pba_service/src/
├── domain/transaction.rs              (one field, one type_label branch)
├── repository/
│   ├── ledger_repo.rs                 (new create_internal_transfer_reversal + code 410)
│   └── transaction_repo.rs            (reverses_transaction_id everywhere; find_reversal_of)
├── service/transfer_service.rs        (new reverse_transfer method, ReversalResult)
├── api/
│   ├── handlers/transfer.rs           (new reverse_transfer handler)
│   ├── routes.rs                      (one new route)
│   └── dto.rs                         (ReverseTransferRequest, ReversalResponse)
└── db/migrations/
    └── 20260521000001_transactions_reverses_transaction_id.sql
```

## Schema & migrations

### `20260521000001_transactions_reverses_transaction_id.sql`

```sql
ALTER TABLE transactions
    ADD COLUMN reverses_transaction_id UUID NULL;

CREATE UNIQUE INDEX uq_transactions_reverses
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL;

CREATE INDEX idx_transactions_reverses_transaction_id
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL;
```

The partial unique index enforces **one reversal per original transfer** at the
DB level. The plain partial index supports the "find the reversal row that
points at this original" lookup (`find_reversal_of`).

Only the **source-side (normal-side) leg** of the new reversal pair carries
`reverses_transaction_id`, pointing at the source-side leg of the original
transfer. Picking the normal-side leg is consistent with how the original
transfer exposes its identity: `{transfer_id}` in URLs is always the
source-side row's id. The PB-side reversal row leaves
`reverses_transaction_id` NULL; the two reversal rows find each other via the
reversal's own `correlation_id`.

### What stays untouched

- `purpose_mcc_allowlist` — unchanged.
- `pb_accounts`, `normal_accounts` — unchanged.
- TB sentinels — none added; no existing sentinel is debited or credited by a
  reversal.
- `TransactionType` Rust enum and the `transaction_type` SQL column — reuse
  `'transfer'`. Any `CHECK` constraint on `transaction_type` is unaffected.

## Data model

### Row layout for a reversed transfer

For an original transfer with correlation `C1`, source row `T_src`, destination
row `T_dst`, a reversal adds two rows under a new correlation `C2`:

| Row | account_kind | account_id | txn_type | direction | pool | status | funding_type | correlation_id | reverses_transaction_id |
|---|---|---|---|---|---|---|---|---|---|
| original src | normal | src | transfer | outbound | NULL | posted | trust | C1 | NULL |
| original dst | pb | dst | deposit | inbound | others | posted | trust | C1 | NULL |
| reversal pb-side | pb | dst | transfer | outbound | others | posted | trust | C2 | NULL |
| reversal normal-side | normal | src | transfer | inbound | NULL | posted | trust | C2 | **T_src.id** |

Both reversal rows use `txn_type='transfer'`. The reversal's `correlation_id`
is independent of the original — `find_by_correlation_id` keeps its "exactly
two legs" guarantee. `funding_type='trust'` is preserved so the conservation
invariants stay clean.

### Domain types

`TransactionRecord` gains:

```rust
pub reverses_transaction_id: Option<Uuid>,
```

No new `TransactionType` variant. `type_label()` adds a branch: when
`reverses_transaction_id.is_some()` and `transaction_type == Transfer`, render
"Reversal" (and `"Reversal (Voided)"` etc. for non-posted statuses, though
reversal rows are always posted in this iteration).

## Ledger conventions

### New TB transfer code

| Code | Operation |
|---|---|
| 100 | PB deposit — immediate |
| 101 | PB deposit — pending |
| 110 | Normal deposit — immediate |
| 111 | Normal deposit — pending |
| 200 | PB payment |
| 300 | PB withdrawal |
| 310 | Normal withdrawal |
| 400 | Internal transfer normal → PB others — immediate |
| 401 | Internal transfer normal → PB others — pending |
| **410** | **Internal transfer reversal PB others → normal — immediate** |

No pending variant for reversal — reversal is always immediate. (A pending
reversal would mean "tentatively give the money back"; not a needed concept,
and the admin can simply not call reverse if uncertain.)

### Direction and accounts

The reversal TB transfer:

- **Debit:** the destination PB account's `tb_others_account_id` (code 2).
- **Credit:** the source normal account's `tb_account_id` (code 3).
- **Amount:** the admin-specified amount, validated `0 < amount ≤ original`.

The PB others-pool has the `DEBITS_MUST_NOT_EXCEED_CREDITS` flag, so TB itself
is the source of truth on "is there enough left in the pool?" If the pool has
been spent down, TB returns its exceeds-credits error, which `ledger_repo` maps
to `AppError::ExceedsBalance`; the service then surfaces `AppError::
InsufficientFunds { requested, available }` after fetching a fresh
`get_balance` reading on the PB others-pool.

### Conservation invariants

The invariant block at the top of `ledger_repo.rs` is updated:

> 1. `TRUST_FUNDING_SOURCE_TB_ID.debits_posted - .credits_posted` equals
>    (sum of all normal-account balances) + (portion of PB-others balances
>    received via internal transfer **net of reversals**).

Invariants 2–4 stay correct as written — code 410 doesn't touch any sentinel,
only moves balance between a `code=2` (PB others) and `code=3` (normal)
account. One new invariant is documented:

> 5. (Sum of PB-others credits at codes 400/401) − (sum of PB-others debits at
>    code 410), across all PB accounts, equals the portion of normal-account
>    balances that arrived via reversal of transfers.

### ID derivation

No new `tb_*_id` helpers. The reversal moves balance between two existing TB
accounts; both ids are looked up the existing way (`pb_accounts.
tb_others_account_id`, `normal_accounts.tb_account_id`).

## Service layer

### `transfer_service::reverse_transfer`

```rust
pub async fn reverse_transfer(
    &self,
    source_normal_id: Uuid,     // url param; validated against original row
    original_transfer_id: Uuid, // original source-side row id (T_src.id)
    amount: u64,                // admin-specified; 0 < amount ≤ original
    gateway_ref: Option<&str>,
    description: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<ReversalResult, AppError>
```

**Flow:**

1. **Idempotency replay.** If `idempotency_key` is set, look up
   `(AccountKind::Normal, source_normal_id, key)`. On hit, fetch the two legs
   via the existing row's `correlation_id` and return. The idempotency key
   lives on the **normal-side reversal row**, consistent with `transfer()`'s
   convention.
2. **Load the original source row** via `transaction_repo::get_by_id(
   original_transfer_id, source_normal_id)`. If not found → `TransactionNotFound`.
   Reject unless: `account_kind='normal'`, `txn_type='transfer'`,
   `direction='outbound'`, `status='posted'`, `reverses_transaction_id IS NULL`
   (the original is not itself a reversal). Map each failure to
   `TransferNotReversible(id, reason)` with `reason ∈ {not_posted,
   is_itself_a_reversal, wrong_type}`.
3. **Reject if already reversed.** `find_reversal_of(original_transfer_id)`.
   If a row exists → `TransferAlreadyReversed(id)`. (Belt-and-suspenders
   alongside the partial unique index; gives a clean error before TB is
   touched.)
4. **Validate amount.** `0 < amount ≤ original.amount`, else
   `ReversalAmountInvalid { requested, original }`.
5. **Resolve destination PB account.** Follow the original's `correlation_id`,
   pick the `AccountKind::Pb` leg, re-load the PB account row to obtain
   `tb_others_account_id` and current status.
6. **Active checks (symmetric with forward transfer).** Source normal account
   must be `Active`, else `NormalAccountNotActive`. Destination PB account must
   be `Active`, else `PbAccountNotActive`. The admin must reactivate a frozen
   PB destination before reversing.
7. **Insert the two reversal rows in one PG transaction.**
   - **PB-side debit row:** `account_kind='pb'`, `account_id=dst`,
     `txn_type='transfer'`, `direction='outbound'`, `pool='others'`,
     `status='posted'`, `funding_type='trust'`, `correlation_id=C2`,
     `reverses_transaction_id=NULL`, `idempotency_key=NULL`, `tb_transfer_id=0`
     (filled after the TB call).
   - **Normal-side credit row:** `account_kind='normal'`, `account_id=src`,
     `txn_type='transfer'`, `direction='inbound'`, `pool=NULL`,
     `status='posted'`, `funding_type='trust'`, `correlation_id=C2`,
     `reverses_transaction_id=T_src.id`, `idempotency_key=$key`,
     `tb_transfer_id=0`.
8. **Execute the TB transfer** via `ledger_repo::create_internal_transfer_reversal
   (pb_others_tb_id, normal_tb_id, amount)` — code 410.
   - On `AppError::ExceedsBalance` → roll back the PG transaction, fetch a
     fresh reading via `ledger_repo::get_single_balance(pb_others_tb_id)`,
     and return `AppError::InsufficientFunds { requested: amount,
     available: balance.posted }`.
   - **No retry loop.** Unlike `transfer()`, retrying does not help: under
     concurrent merchant payments, the others-pool balance only decreases. The
     admin must lower the amount or wait.
   - On any other TB error → roll back and propagate.
9. **Persist `tb_transfer_id`** on both reversal rows:
   `UPDATE transactions SET tb_transfer_id=$1 WHERE correlation_id=$2`.
10. **COMMIT.** Reload both reversal rows and return `ReversalResult`.

### `ReversalResult`

```rust
pub struct ReversalResult {
    pub reversal_id: Uuid,          // normal-side reversal row id
    pub original_transfer_id: Uuid, // T_src.id
    pub source_account_id: Uuid,    // normal account (credited by the reversal)
    pub destination_account_id: Uuid, // PB account (debited by the reversal)
    pub amount: u64,
    pub original_amount: u64,
    pub status: TransactionStatus,  // always Posted in this iteration
    pub correlation_id: Uuid,       // C2
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

### Where rules live

| Rule | Location |
|---|---|
| Original must be posted, outbound, not itself a reversal | `transfer_service::reverse_transfer` step 2 |
| At most one reversal per original | DB partial unique index + explicit pre-check (step 3) |
| `0 < amount ≤ original.amount` | `transfer_service::reverse_transfer` step 4 |
| Both accounts must be Active | `transfer_service::reverse_transfer` step 6 |
| PB others-pool must have enough balance | TB (`DEBITS_MUST_NOT_EXCEED_CREDITS`) → mapped to `InsufficientFunds` (step 8) |
| `reverses_transaction_id` set only on the normal-side reversal row | `transfer_service::reverse_transfer` step 7 |
| Idempotency keyed on the normal-side reversal row | `transfer_service::reverse_transfer` step 7 |

## Repository layer

### `ledger_repo.rs`

- New constant `INTERNAL_TRANSFER_REVERSAL_CODE: u16 = 410`.
- New method `create_internal_transfer_reversal(debit_pb_others_tb_id,
  credit_normal_tb_id, amount)` — wraps `create_transfer` with code 410.
  Errors map identically to `create_internal_transfer`; TB's
  `CreateTransferErrorKind::ExceedsCredits` (or current equivalent — exact
  variant confirmed at plan write-up) on the debit account maps to
  `AppError::ExceedsBalance`.

### `transaction_repo.rs`

- `insert_in_tx` adds a `reverses_transaction_id: Option<Uuid>` parameter
  (existing `#[allow(clippy::too_many_arguments)]` covers this).
- All read paths (`get_by_id`, `find_by_idempotency_key`,
  `find_by_correlation_id`, `list_*`) select and populate
  `reverses_transaction_id` on `TransactionRecord`.
- New helper `find_reversal_of(original_id: Uuid) -> Result<Option<
  TransactionRecord>, AppError>` — returns the normal-side reversal row whose
  `reverses_transaction_id` matches.

## API surface

### Smithy operation (`model/transfer.smithy`)

```smithy
/// Reverse a posted normal→PB transfer.
///
/// Records a new compensating transaction pair (PB others-pool debit + normal
/// account credit) plus a new TB transfer in the opposite direction. The
/// original transfer rows are not mutated; the reversal links back via
/// `reverses_transaction_id` on the normal-side reversal row.
///
/// Only `posted` transfers can be reversed. Pending transfers should be
/// cancelled via `VoidNormalAccountTransfer`. At most one reversal per
/// original transfer. Both source and destination accounts must be Active.
/// The PB others-pool must have sufficient balance; if not, returns
/// InsufficientFunds with the available amount.
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

`{transfer_id}` is the original transfer's source-row id (the same id returned
by `TransferToPBAccount`). `{account_id}` is the source normal account; the
service validates the original row belongs to it.

### Route (`api/routes.rs`)

```
.route(
    "/normal-accounts/{account_id}/transfers/{transfer_id}/reverse",
    post(handlers::transfer::reverse_transfer),
)
```

### DTOs (`api/dto.rs`)

```rust
ReverseTransferRequest {
    amount: u64,
    gatewayRef: Option<String>,
    description: Option<String>,
    idempotencyKey: Option<String>,
}

ReversalResponse {
    reversalId: Uuid,
    originalTransferId: Uuid,
    sourceAccountId: Uuid,
    destinationAccountId: Uuid,
    amount: u64,
    originalAmount: u64,
    status: String,
    correlationId: Uuid,
    createdAt: DateTime<Utc>,
}
```

`TransactionDto` (used by all list endpoints) gains
`reversesTransactionId: Option<Uuid>`.

### Handler

Add `reverse_transfer` to `api/handlers/transfer.rs` — a thin wrapper that
extracts the path params and request body, calls
`transfer_service.reverse_transfer(...)`, and maps `ReversalResult` to
`ReversalResponse`.

### Error mappings

| Variant | HTTP | Body |
|---|---|---|
| `TransferNotReversible(id, reason)` | 409 | `{ "error": "transfer_not_reversible", "id": "…", "reason": "not_posted\|is_itself_a_reversal\|wrong_type" }` |
| `TransferAlreadyReversed(id)` | 409 | `{ "error": "transfer_already_reversed", "id": "…" }` |
| `ReversalAmountInvalid { requested, original }` | 400 | `{ "error": "reversal_amount_invalid", "requested": …, "original": … }` |
| `InsufficientFunds { requested, available }` (existing) | 409 | `{ "error": "insufficient_funds", "requested": …, "available": … }` |
| `TransactionNotFound` (existing) | 404 | `{ "error": "transaction_not_found", "id": "…" }` |
| `NormalAccountNotActive` (existing) | 409 | `{ "error": "normal_account_not_active", "id": "…" }` |
| `PbAccountNotActive` (existing) | 409 | `{ "error": "pb_account_not_active", "id": "…" }` |

### Auth

Admin-only. Gated by the existing admin-role check used by other admin
operations. If the codebase does not yet have an admin-role gate distinct from
the general auth check, the implementation plan adds a `require_admin`
extractor as a sub-task; if it does, reuse it (confirmed via grep at plan
write-up).

### Admin UI

The existing transaction detail page (commit `e616661`) gains a **Reverse**
button on posted transfer rows. The button:

- Is hidden on pending transfers (Post/Void already cover that path).
- Is hidden on rows where `transaction_type=transfer` but
  `reverses_transaction_id IS NOT NULL` (i.e. the row itself is a reversal).
- Is hidden when the original has already been reversed; replaced by a
  "Reversed by [link]" affordance that navigates to the reversal pair.

Clicking Reverse opens a modal pre-filled with the original amount; admin can
edit it and optionally add a description, then submits. On success the page
refreshes and shows the affordance described above. On `InsufficientFunds`,
the modal shows the error inline with the `available` amount.

## Testing strategy

### Unit tests

| File | Tests |
|---|---|
| `service/transfer_service.rs` | `reverse_transfer` happy path (full amount); partial amount; rejects pending original; rejects already-reversed original; rejects reversal-of-reversal; amount > original rejected; amount = 0 rejected; both-active check (source frozen, destination frozen); insufficient others-pool balance → `InsufficientFunds` with available reading; idempotency replay returns same pair without a second TB call; both reversal rows share new correlation_id; `reverses_transaction_id` set only on the normal-side row. |
| `repository/transaction_repo.rs` | `reverses_transaction_id` round-trip on insert/select; `find_reversal_of` returns the normal-side row only; partial unique index rejects two reversals of the same original. |
| `repository/ledger_repo.rs` | `create_internal_transfer_reversal` writes a code-410 TB transfer in the right direction (debit PB others, credit normal); TB exceeds-credits maps to `AppError::ExceedsBalance`. |
| `domain/transaction.rs` | `type_label()` returns "Reversal" when `reverses_transaction_id.is_some()` on a transfer row. |

### Cucumber BDD — new feature: `transfer_reversal.feature`

Scenarios:

1. **Happy path full reversal** — transfer ₹1000 immediate posted; admin
   reverses ₹1000; PB others-pool returns to pre-transfer balance; normal
   account balance restored; both reversal legs visible via the new
   correlation_id; original transfer rows unchanged.
2. **Happy path partial reversal** — same setup, admin reverses ₹600; PB
   others-pool ₹400 lower than pre-transfer; normal account ₹600 higher than
   post-transfer; original transfer rows still show ₹1000, status posted.
3. **Pending transfer cannot be reversed** — reverse endpoint returns 409
   `transfer_not_reversible` with `reason='not_posted'`.
4. **Already-reversed transfer cannot be reversed again** — reverse once
   succeeds, second attempt returns 409 `transfer_already_reversed`.
5. **Reversal cannot itself be reversed** — attempt to reverse a reversal row
   returns 409 `transfer_not_reversible` with `reason='is_itself_a_reversal'`.
6. **Amount > original rejected** — ₹1000 original, ₹1001 reversal → 400
   `reversal_amount_invalid`.
7. **Amount = 0 rejected** — 400 `reversal_amount_invalid`.
8. **Insufficient others-pool balance** — transfer ₹1000, PB holder makes a
   ₹700 payment (others-first leaves ₹300), admin attempts ₹1000 reversal →
   409 `insufficient_funds` with `available=300`. Subsequent ₹300 reversal
   succeeds.
9. **Source normal account frozen** — freeze source, attempt reversal → 409
   `normal_account_not_active`.
10. **Destination PB account frozen** — freeze destination, attempt reversal
    → 409 `pb_account_not_active`. Reactivate, retry succeeds.
11. **Idempotency replay** — same `idempotency_key` twice → second call
    returns the same reversal pair, no second TB transfer (assert via TB
    transfer count).
12. **Wrong source account in URL** — original's source is `src_A`, URL says
    `src_B` → 404 `transaction_not_found`.
13. **Per-transaction visibility** — after reversal, `GET /normal-accounts/{id}
    /transactions` shows original + reversal credit row; `GET /pb-accounts/{id}
    /transactions` shows original + reversal debit row; `ListAllTransactions`
    shows all four.

### UI tests — new feature: `transfer_reversal_admin.feature`

1. **Reverse button appears on posted-transfer detail page.**
2. **Reverse button absent on pending-transfer detail page.**
3. **Reverse button absent on a reversal row's detail page.**
4. **Reverse action flow** — modal opens with amount pre-filled, admin edits
   and submits, page refreshes, original row shows "Reversed by [link]"
   navigating to the reversal pair.
5. **Insufficient-funds error surfaced in modal** with the available balance.

### Regression coverage

The existing transfer Cucumber suite stays green — reversal is purely additive
at the API surface. `ListAllTransactions` shape adds
`reversesTransactionId: Option<Uuid>`; existing assertions that whitelist
fields will need a one-line update. `funding_type='trust'` rejection on PB
deposits (introduced in the normal-accounts phase) is unaffected.

### Coverage gates

`just local-ci` runs everything; no new gates. Optional Cucumber tag
`@reversal` for local development.

## Rollout plan

Single PR. The change is additive at the schema layer (one nullable column +
two partial indexes), at the Smithy layer (one new operation), and at the
service layer (one new method on an existing service). There is no
multi-instance ordering hazard: rows inserted by the new code carry the new
field; existing readers ignore unknown columns; the migration is forward-only
and safe to run before the new code is deployed.

### Documentation updates

- `README.md` — extend the API table with the new `POST
  /normal-accounts/{id}/transfers/{id}/reverse` row; one-line description.
- `WHAT.md` — short "Reversing a transfer" subsection in the normal-accounts
  section, noting that reversal is admin-only and is rejected if the PB
  others-pool has been spent below the requested amount.

## Open items (none blocking)

- Confirm at plan write-up the exact `CreateTransferErrorKind` variant returned
  when a debit would exceed `DEBITS_MUST_NOT_EXCEED_CREDITS` on the others-pool
  (one `cargo doc --open` on the TB crate; matches the existing mapping in
  `create_internal_transfer`).
- Confirm at plan write-up the existing admin-role gate (or add `require_admin`
  if absent). Mechanical, not a design choice.
