# Contribution Return — Design

**Date:** 2026-07-01
**Status:** Draft

## Goal

Add an admin-initiated **contribution return** operation on PB accounts.
The operation debits the others pool and routes each unit of money back to
the specific contribution it originated from:

- **Sponsor (`funding_type='trust'`)** contributions route back to the
  originating sponsor's normal account (on-ledger, direct credit).
- **Third-party (`funding_type='third_party'`)** contributions route back to
  `THIRD_PARTY_FUNDING_SOURCE_TB_ID` (off-ledger settlement via downstream
  payout).

Includes two-phase (`pending → post/void`) support from day one and multiple
partial returns per original.

## Background

The others pool in a PB account can be funded by two paths today:

- A **normal → PB transfer** (PR #35, service `transfer_service::transfer`).
  Source is an on-ledger normal account. Rows land with `pool='others'`,
  `funding_type='trust'`.
- A **PB deposit with `funding_type='third_party'`**. Source is
  `THIRD_PARTY_FUNDING_SOURCE_TB_ID` (a sentinel account created at bootstrap
  in `ledger_repo`). Rows land with `pool='others'`, `funding_type='third_party'`.

The existing exit paths for others-pool money are asymmetric:

- User self-withdraw (`pb_withdrawal_service`) debits only the self pool.
- Payment refund (PR #40) debits others via the merchant sentinel — a
  different upstream (merchant paid PB, refund undoes that).
- Transfer reversal (PR #38, PR #42) reverses **one** posted transfer to
  its origin sponsor, single-shot per original.

There is no path to return arbitrary others-pool money to its contributors
without picking one specific transfer to reverse. That gap matters for
scenarios where an admin needs to return, for example, $50 of sponsor
matching contributions to the sponsor(s) who provided them, without the
caller having to know which specific historical transfer(s) that amount
corresponds to.

This spec closes that gap symmetrically for both `trust` and `third_party`
funding types.

## Non-goals

- **Automatic return on user self-withdraw.** Discretionary admin action
  only. No coupling to `pb_withdrawal_service`. Enforcing sponsor-match
  invariants on user withdrawals is a separate concern that this operation
  does not tackle.
- **Cross-funding-type returns in a single call.** Each call is either
  `trust` or `third_party`, never mixed. Prevents accidental sponsor money
  leaking into a third-party payout channel or vice versa.
- **Overriding FIFO allocation.** V1 always draws oldest-first across active
  originals of the requested `funding_type`. Callers cannot pin a specific
  original to draw from.
- **New transaction type.** Return rows are `TransactionType::Withdrawal`
  with `pool='others'`, `direction='outbound'`, and `reverses_transaction_id`
  set — distinguishable from user self-withdrawals by the pool and the link.
- **Sponsor-self-serve or third-party-self-serve returns.** Admin-only in
  v1. Both categories of contributor could get self-serve later; not in
  scope.
- **Return-of-return.** A return row itself cannot be reversed. Use void
  (for pending) or a compensating deposit to undo a settled return.

## Reservation semantics

Pending returns reserve their slice of the returnable budget so a burst of
concurrent initiates cannot over-return.

- `sum_returns_of_in_tx(original_row_id)` counts return rows in
  `status IN ('pending','settled')`, excluding voided. Identical shape to
  the widened `sum_refunds_of` from PR #42.
- `find_returnable_originals_for_update(pb_account_id, funding_type)` uses
  `SELECT ... FOR UPDATE` on the picked originals so the check + inserts
  are lock-consistent inside a single tx.
- Voiding a pending return restores the budget. The next initiate sees the
  freed capacity.

## Scope (high level)

| Area | Change |
|---|---|
| Schema | None. Existing columns and indexes cover the query plans. |
| Smithy | New `ReturnPBAccountContribution`, `PostPBAccountContributionReturn`, `VoidPBAccountContributionReturn` operations; new `GetPBAccountContributionSummary` read op. |
| Routes | Three new REST routes under `/pb-accounts/{account_id}/contribution-returns/...` plus one read at `/pb-accounts/{account_id}/contributions/summary`. |
| Domain | None. Reuses `TransactionType::Withdrawal`; the pool + link + funding_type distinguish return rows. |
| Repository | `transaction_repo` gains `find_returnable_originals_for_update`, `sum_returns_of_in_tx`, `sum_others_contributions`, `sum_others_returns`. Renames `sum_refunds_of[_in_tx]` → `sum_returns_of[_in_tx]` and `find_refunds_of` → `find_returns_of` for name/contract alignment. |
| Ledger | New `CONTRIBUTION_RETURN_CODE = 310` plus `create_contribution_return` and `create_pending_contribution_return`. No LINKED-split — FIFO produces independent per-original TB transfers. |
| Service | New `pb_contribution_return_service` with `return_contribution`, `post_contribution_return`, `void_contribution_return`, and a shared `resolve_contribution_return(direction: Post|Void)` helper mirroring the refund pattern. Also a `summary(pb_account_id)` read. |
| Admin UI | Contributions panel on PB account detail; return form with Mode toggle; Post/Void buttons on pending return detail; "Returned by" affordance on original transfer/deposit detail pages. |
| Tests | Two new Cucumber API feature files, one new UI feature file, extensions to two existing features for the "Returned by" affordance. |

## Architecture

The feature is a new siblings service alongside the existing PB services.
No new domain modules.

```
crates/pba_service/src/
├── admin/handlers.rs                   (contributions panel, return form,
│                                        post/void admin routes)
├── admin/transfer_handlers.rs          ("Returned by" affordance on
│                                        transfer detail)
├── admin.rs                            (route wiring)
├── api/dto.rs                          (ContributionReturnRequest/Response,
│                                        summary DTOs)
├── api/handlers/pb.rs                  (return_contribution,
│                                        post_contribution_return,
│                                        void_contribution_return,
│                                        get_contribution_summary)
├── api/routes.rs                       (/contribution-returns/{...})
├── repository/
│   ├── ledger_repo.rs                  (CONTRIBUTION_RETURN_CODE,
│                                        create_[pending_]contribution_return)
│   └── transaction_repo.rs             (FIFO fetch, sum_returns_of variants,
│                                        sum_others_contributions/returns,
│                                        rename refunds→returns)
├── service/
│   └── pb_contribution_return_service.rs   (new)
└── templates/admin/
    ├── contribution_return.html        (new form)
    ├── pb_account_detail.html          (Contributions panel)
    ├── transaction_detail.html         (pending return post/void buttons)
    ├── transfer_detail.html            ("Returned by" affordance)
    └── deposit_detail.html             ("Returned by" affordance, if present)
```

## API contract

### Initiate — new endpoint

```
POST /pb-accounts/{account_id}/contribution-returns
  body: {
    amount: integer,
    funding_type: "trust" | "third_party",
    pending: bool = false,
    timeout_seconds: integer | null = null,   // ignored when pending=false
    gateway_ref: string | null,
    description: string | null,
    idempotency_key: string | null,
  }
  → 201 ContributionReturnResponse {
      return_id: string,                       // == correlation_id
      correlation_id: string,
      account_id: string,
      funding_type: "trust" | "third_party",
      amount: integer,                         // total across allocations
      allocations: [
        {
          original_transaction_id: string,     // the transfer or deposit being drawn from
          amount: integer,                     // portion of this return coming from that original
        }
      ],
      remaining_returnable_after: integer,     // for this funding_type, after this call
      status: "pending" | "settled",
      created_at: string,
  }
```

- `remaining_returnable_after` gives the caller the post-call budget for the
  same `funding_type` without a follow-up read. Matches the semantic of
  refund's `remaining_refundable`.
- `allocations` is one entry per row inserted (1..N), enabling the audit
  story client-side without a follow-up fetch.

### Post / Void — new endpoints

```
POST /pb-accounts/{account_id}/contribution-returns/{return_id}/post
  → 200 ContributionReturnResponse { status: "settled", ... }

POST /pb-accounts/{account_id}/contribution-returns/{return_id}/void
  → 200 ContributionReturnResponse { status: "voided", ... }
```

`return_id` is the return's `correlation_id` (equals the primary row's id,
per the make_payment / refund convention).

### Read — summary

```
GET /pb-accounts/{account_id}/contributions/summary
  → ContributionSummaryResponse {
      trust: {
        total_contributed: integer,
        total_returned: integer,     // pending + settled
        remaining_returnable: integer,
      },
      third_party: {
        total_contributed: integer,
        total_returned: integer,
        remaining_returnable: integer,
      },
  }
```

Cheap: two `sum(...)` queries per funding type. Called by the admin
Contributions panel and (optionally) by external callers building similar
UI.

### Validation and errors

- `pending=false` (default) preserves compatibility with the shape existing
  admin flows expect.
- `timeout_seconds` provided with `pending=false` is ignored (no error).
- `timeout_seconds` omitted with `pending=true` falls back to the service's
  `default_pending_timeout_seconds`.
- `amount == 0` or `amount > remaining_returnable_of(funding_type)` →
  `ContributionAmountInvalid { requested, remaining }` (400).
- No active originals of the requested `funding_type` →
  `ContributionFullyReturned` (409).
- PB account not `Active` → `PbAccountNotActive` (409, existing variant).
- Post / void on a non-Pending row → `TransactionNotPending` (400).
- Post / void on a non-existent or wrong-account return id →
  `TransactionNotFound` (404).
- **Idempotent same-direction.** Post on already-posted / void on
  already-voided → 200 no-op. Mixed-direction (post a voided row, void a
  posted row) → `TransactionNotPending`. Mirrors PR #42's refund posture.

## Service-layer flow

### `pb_contribution_return_service::return_contribution`

Signature:
```rust
pub async fn return_contribution(
    &self,
    pb_account_id: Uuid,
    amount: u64,
    funding_type: &str,           // "trust" | "third_party"
    pending: bool,
    timeout_seconds: Option<u32>,
    gateway_ref: Option<&str>,
    description: Option<&str>,
    idempotency_key: Option<&str>,
) -> Result<ContributionReturnResult, AppError>;
```

1. **Idempotency replay.** Lookup by `(AccountKind::Pb, pb_account_id,
   idempotency_key)`. On hit, rebuild `ContributionReturnResult` from the
   stored rows via `find_by_correlation_id` — including per-row
   `allocations` and the recomputed `remaining_returnable_after`.
2. **Begin DB transaction.**
3. **Find candidate originals for update.** Call
   `find_returnable_originals_for_update(&mut tx, pb_account_id,
   funding_type)`. This returns FIFO-ordered `TransactionRecord`s with
   `SELECT ... FOR UPDATE` holding the lock.
4. **Compute per-original remaining.** For each candidate, `remaining[i] =
   original[i].amount - sum_returns_of_in_tx(&mut tx, original[i].id)`.
   Skip zeros. Drop candidates whose `remaining[i] == 0` — they're fully
   returned already.
5. **Total-available check.** `total_available = sum(remaining[i])`.
   - `if total_available == 0` → `ContributionFullyReturned`.
   - `if amount == 0 || amount > total_available` →
     `ContributionAmountInvalid { requested: amount, remaining: total_available }`.
6. **PB account active check.** Loaded outside the tx; reject with
   `PbAccountNotActive` if not.
7. **FIFO allocation.** Walk originals oldest-first:
   ```
   let mut allocations = Vec::new();
   let mut amount_left = amount;
   for (original, remaining) in candidates_with_remaining {
       if amount_left == 0 { break; }
       let take = amount_left.min(remaining);
       allocations.push((original, take));
       amount_left -= take;
   }
   ```
   Invariant post-loop: `amount_left == 0` (guaranteed by the check in
   step 5).
8. **Insert one Withdrawal row per allocation.**
   - Same `correlation_id = return_correlation_id` across all rows.
   - First row's `id = return_correlation_id` (mirrors make_payment /
     refund).
   - `pool = 'others'`, `direction = 'outbound'`,
     `transaction_type = 'withdrawal'`.
   - `reverses_transaction_id = original.id` per row.
   - `funding_type` copied from the original.
   - `status = Pending if pending else Settled`; `timeout_seconds` set on
     each row when pending.
   - `idempotency_key` on the first row only.
9. **TB transfers, one per allocation.** For each `(original, take)`:
   - `trust`: resolve credit destination by looking up the original
     transfer's normal-side leg via `find_by_correlation_id(original.correlation_id)`,
     then fetching the normal account's `tb_account_id` from
     `NormalAccountRepo`.
   - `third_party`: `credit_destination_tb_id = THIRD_PARTY_FUNDING_SOURCE_TB_ID`.
   - `pending=false`: `create_contribution_return(debit_pb_others_tb_id,
     credit_destination_tb_id, take)`.
   - `pending=true`: `create_pending_contribution_return(..., timeout)`.
     Persist the returned `tb_transfer_id` on the corresponding DB row
     via UPDATE inside `tx`.
10. **Commit `tx`.**
11. **Return `ContributionReturnResult`** with the per-row allocation
    breakdown and post-call `remaining_returnable = total_available - amount`.

### Shared helper: `resolve_contribution_return`

Mirrors PR #42's `resolve_refund` in shape and behaviour:

```rust
enum ContributionReturnResolution { Post, Void }

async fn resolve_contribution_return(
    &self,
    pb_account_id: Uuid,
    return_id: Uuid,
    direction: ContributionReturnResolution,
) -> Result<ContributionReturnResult, AppError>;
```

1. Begin tx.
2. `find_by_correlation_id_for_update(&mut tx, return_id)`.
3. Validate every row: `account_kind=Pb`, `account_id=pb_account_id`,
   `transaction_type=Withdrawal`, `pool='others'`,
   `reverses_transaction_id.is_some()`. Otherwise `TransactionNotFound`.
4. Idempotent same-direction: if every row is already in target status,
   commit tx and return snapshot.
5. Otherwise every row must be `Pending`. Else `TransactionNotPending`.
6. Per-row `post_pending_transfer(tb_id)` or `void_pending_transfer(tb_id)`,
   tolerating `AppError::TbPendingAlreadyResolved`.
7. `UPDATE transactions SET status = ? WHERE correlation_id = ? AND status = 'pending'`
   using `execute(&mut *tx)`.
8. Commit.
9. Rebuild snapshot from the pool via `find_by_correlation_id` and return.

### Public wrappers

```rust
pub async fn post_contribution_return(
    &self,
    pb_account_id: Uuid,
    return_id: Uuid,
) -> Result<ContributionReturnResult, AppError> {
    self.resolve_contribution_return(pb_account_id, return_id, ContributionReturnResolution::Post).await
}

pub async fn void_contribution_return(
    &self,
    pb_account_id: Uuid,
    return_id: Uuid,
) -> Result<ContributionReturnResult, AppError> {
    self.resolve_contribution_return(pb_account_id, return_id, ContributionReturnResolution::Void).await
}
```

### `summary` read

```rust
pub async fn summary(
    &self,
    pb_account_id: Uuid,
) -> Result<ContributionSummary, AppError>;
```

Four queries: `sum_others_contributions("trust")`,
`sum_others_returns("trust")`, `sum_others_contributions("third_party")`,
`sum_others_returns("third_party")`. Computes `remaining_returnable =
contributed - returned` per funding_type.

## Repository / ledger layer

### `transaction_repo`

Renames (safe refactor):

- `sum_refunds_of` → `sum_returns_of`
- `sum_refunds_of_in_tx` → `sum_returns_of_in_tx`
- `find_refunds_of` → `find_returns_of`

The rename reflects the true contract: any row that carries
`reverses_transaction_id` is a return-of-something, regardless of the
concrete transaction_type. Existing refund call-sites update to the new
names. Behaviour is unchanged.

New methods:

```rust
pub async fn find_returnable_originals_for_update(
    &self,
    tx: &mut Transaction<'_, Postgres>,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<Vec<TransactionRecord>, AppError>;
```

```sql
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
```

```rust
pub async fn sum_others_contributions(
    &self,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<u64, AppError>;
```

```sql
SELECT COALESCE(SUM(amount), 0)::bigint
FROM transactions
WHERE account_id = $1
  AND account_kind = 'pb'
  AND pool = 'others'
  AND funding_type = $2
  AND direction = 'inbound'
  AND status IN ('posted', 'settled')
  AND reverses_transaction_id IS NULL
```

```rust
pub async fn sum_others_returns(
    &self,
    pb_account_id: Uuid,
    funding_type: &str,
) -> Result<u64, AppError>;
```

```sql
SELECT COALESCE(SUM(amount), 0)::bigint
FROM transactions
WHERE account_id = $1
  AND account_kind = 'pb'
  AND pool = 'others'
  AND funding_type = $2
  AND type = 'withdrawal'
  AND status IN ('pending', 'settled')
```

### `ledger_repo`

New constant + two helpers:

```rust
pub const CONTRIBUTION_RETURN_CODE: u16 = 310;

pub async fn create_contribution_return(
    &self,
    debit_pb_others_tb_id: u128,
    credit_destination_tb_id: u128,
    amount: u64,
) -> Result<(), AppError>;

pub async fn create_pending_contribution_return(
    &self,
    debit_pb_others_tb_id: u128,
    credit_destination_tb_id: u128,
    amount: u64,
    timeout_seconds: u32,
) -> Result<u128, AppError>;
```

Both delegate to the existing `create_transfer` /
`create_pending_transfer` primitives, passing
`CONTRIBUTION_RETURN_CODE`. Reused verbatim: `post_pending_transfer`,
`void_pending_transfer`.

No LINKED-split helper. FIFO produces independent debits (each with its
own credit destination for `trust` returns; a uniform destination for
`third_party`). Each allocation gets its own single-leg TB transfer, so
LINKED gains nothing.

### Schema

No migration. Existing columns (`type`, `status`, `pool`, `direction`,
`funding_type`, `reverses_transaction_id`, `correlation_id`,
`timeout_seconds`, `tb_transfer_id`) cover everything. Existing indexes
on `(reverses_transaction_id)` and `(account_id, created_at)` cover the
FIFO fetch and the aggregate reads.

## Admin UI

### PB account detail — new "Contributions" panel

Rendered above the transaction list on the PB account detail page when
either `total_contributed` is non-zero:

```
Contributions
────────────────────────────────────────
Trust (sponsor)      Contributed: ₹5,000.00   Returned: ₹1,200.00   Returnable: ₹3,800.00   [Return...]
Third-party          Contributed: ₹2,500.00   Returned: ₹0.00       Returnable: ₹2,500.00   [Return...]
```

- Data from `GET /pb-accounts/{id}/contributions/summary`.
- `[Return...]` links to `/admin/accounts/{id}/contribution-returns/new?funding_type=trust|third_party`.
- Panel hidden when both totals are zero.

### Return form (`templates/admin/contribution_return.html`)

Fields:

- **Funding type**: read-only chip reflecting the URL query param.
- **Amount**: `<input type="number">`, `max` attribute pre-filled from
  `remaining_returnable`.
- **Mode**: radio with `Settle now` (default) / `Hold as pending`.
- **Timeout (seconds, optional)**: `<input type="number">`, revealed when
  Mode is Pending. Placeholder shows `default_pending_timeout_seconds`.
- **Description**: optional text.
- **Gateway ref**: optional text.

Submit posts to `/admin/accounts/{account_id}/contribution-returns` with
form fields:

```rust
#[derive(Deserialize)]
pub struct ContributionReturnForm {
    pub amount_paisa: u64,
    pub funding_type: String,
    #[serde(default)]
    pub mode: Option<String>,        // "settle" | "pending"
    // Option<String> not Option<u32>: an empty submit ("timeout_seconds=")
    // would fail number parsing; parse manually. Matches the pattern
    // standardised in PR #42.
    #[serde(default)]
    pub timeout_seconds: Option<String>,
    pub description: Option<String>,
    pub gateway_ref: Option<String>,
}
```

Redirect on success to `/admin/transactions/{return_id}`.

### Transaction detail page — pending return controls

Return rows match `transaction_type='withdrawal' AND pool='others' AND
reverses_transaction_id IS NOT NULL`. When `status='pending'`, add a
Pending badge and Post return / Void return button pair, POSTing to
`/admin/accounts/{id}/contribution-returns/{return_id}/post|void`.

Compute `is_pending_contribution_return: bool` in the template context
similarly to `is_pending_refund` from PR #42:

```rust
let is_pending_contribution_return =
    txn.status == TransactionStatus::Pending
        && txn.transaction_type == TransactionType::Withdrawal
        && txn.pool.as_deref() == Some("others")
        && txn.reverses_transaction_id.is_some();
```

### "Returned by" affordance on original transfer / deposit detail

Uses the renamed `find_returns_of(original_row_id)`. When any return rows
point at the row being displayed, render a list:

```
Returned in part
────────────────
2026-07-15 12:34   ₹500.00   settled   → view
2026-07-16 09:12   ₹250.00   pending   → view
```

Applied to both `transfer_detail.html` (sponsor transfers) and
`deposit_detail.html` (third-party deposits) if the deposit detail page
already exists. If it doesn't yet, extend the generic
`transaction_detail.html` refund-history-style block.

### Refund history table on payment detail

No change. Refunds live on payment detail; contribution returns do not.
Return rows have `reverses_transaction_id` pointing at transfer / deposit
originals, so they don't appear in `sum_returns_of(payment_row_id)`.

### Rename impact on existing UI

The `sum_refunds_of` → `sum_returns_of` and `find_refunds_of` →
`find_returns_of` renames touch:

- `admin/handlers.rs` (`build_transaction_detail_template` refund
  history loop; refund-remaining computation).
- `pb_payment_service::refund_payment` (2 call-sites: idempotency replay
  and main path).
- `pb_payment_service::resolve_refund` (rebuild helper).

Pure rename, behaviour identical.

## Tests

### Cucumber API features

**New `tests/features/contribution_return.feature`** (~11 scenarios):

1. Full return of a single trust contribution — sponsor's normal-account
   balance credited.
2. Full return of a single third-party contribution —
   `THIRD_PARTY_FUNDING_SOURCE_TB_ID` credit; PB others-pool balance
   drops.
3. Partial return of a single original — `remaining_returnable_after`
   reflects the remaining amount.
4. FIFO across two trust contributions — return spans both originals;
   response `allocations` array shows the split; oldest normal-account
   credited first, then next.
5. Return amount exceeding total available → `ContributionAmountInvalid`
   with `remaining` populated.
6. Return of zero → `ContributionAmountInvalid`.
7. `funding_type` with no active originals → `ContributionFullyReturned`.
8. Trust and third-party pools are independent — returning all trust does
   not affect third-party's `remaining_returnable`.
9. Frozen PB account rejects return; reactivation allows it.
10. Idempotency replay returns the same `correlation_id` and same
    allocations.
11. Concurrent return initiates reserve `remaining_returnable` — burst of
    pending returns cannot exceed total available (proves
    `sum_returns_of_in_tx` reservation).

**New `tests/features/contribution_return_two_phase.feature`** (~7
scenarios):

1. Pending return then post — sponsor balance credited only after post.
2. Pending return then void — sponsor balance unchanged,
   `remaining_returnable` restored.
3. Pending return blocks a second return that would exceed reserved
   capacity.
4. Post on already-posted return is a same-direction no-op.
5. Void on already-voided return is a same-direction no-op.
6. Mixed-direction post-then-void rejected with `TransactionNotPending`.
7. Pending return with short `timeout_seconds` ages out via
   `pending_timeout` poller — status flips to `voided`,
   `remaining_returnable` restored.

**Extensions to existing features:**

- `deposits.feature`: one scenario asserting a third-party deposit's
  detail page shows the "Returned by" affordance after a return.
- `transfer_reversal.feature`: one scenario asserting a sponsor
  transfer's detail page shows the "Returned by" affordance after a
  return (in addition to any existing "Reversed by" affordance).

### Cucumber UI features

**New `tests/ui_features/contribution_return_admin.feature`** (~5
scenarios):

1. Contributions panel renders with correct `trust` / `third_party`
   totals.
2. Return form pre-selects funding_type from the panel's `[Return...]`
   button.
3. Full return via UI credits sponsor's normal account and updates the
   panel's `Returnable` figure.
4. Pending return via UI renders Pending badge + Post/Void buttons on
   detail page.
5. Post via UI flips status to `settled` and shows the finalised return
   on the origin transfer's detail page.

### Verification gate

`just e2e-all` green, `just fmt-check` clean, `just lint` clean,
Conventional Commit titles on every commit.

## Known limitations (accepted for v1; follow-up tickets)

- **Multi-allocation TB non-atomicity.** When a single return spans multiple
  originals (FIFO), the service issues one TB transfer per allocation
  independently (no `LINKED` chain). If allocation N (N>1) fails after
  allocation 1 has already committed to TB, the PG transaction rolls back
  but the earlier TB debits/credits remain — leaving PG and TB out of sync.
  For the `pending=true` path this self-heals on retry (TB pending times
  out; a fresh initiate re-drives it). For the `pending=false` path, the
  most likely failure trigger is a concurrent operation draining the
  others-pool below the outstanding amount, which surfaces as an
  `ExceedsBalance` TigerBeetle error. Follow-up: batch the N transfers via
  `create_transfers([...])` with `LINKED` flag on all-but-last so TB
  guarantees per-batch atomicity.
- **Transfer-reversal invisible to `remaining_returnable` summary.** A
  prior `transfer_service::reverse_transfer` debits the trust others-pool
  but is a `type='transfer'` row, not `type='withdrawal'`. `sum_others_returns`
  filters on `type='withdrawal'`, so the summary's `remaining_returnable`
  overstates the returnable amount after a reversal. TB's
  `debits_must_not_exceed_credits` guard still prevents actual over-return
  at ledger time, but the admin UI and API response can show numbers larger
  than what the TB others-pool will actually honour. Follow-up: broaden
  `sum_others_returns` to also count `type='transfer' AND direction='outbound'`
  rows, or clamp `remaining_returnable` to the actual others-pool TB
  balance.

## Out of scope

- Webhook callbacks driving post/void.
- Self-serve (sponsor-initiated or third-party-initiated) returns.
- Partial post / partial void.
- Overriding FIFO with LIFO or proportional allocation.
- Auto-return on user self-withdraw.
- Extending self-pool withdraw to also debit others when `funding_type`
  permits.
- Schema migrations.
