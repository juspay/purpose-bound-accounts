# Normal Accounts (Single-Pool) Alongside Purpose-Bound Accounts — Design

**Date:** 2026-05-08
**Status:** Approved

## Goal

Introduce a second account kind — **normal accounts** — backed by a single TigerBeetle account and fronted by Postgres. Normal accounts coexist with the existing purpose-bound (PB) accounts in the same service. Money enters the normal-account ecosystem from the trust funding sentinel and exits via withdrawal or via internal transfer to a PB account's others-pool. The direct path from the trust sentinel into a PB account is removed; trust money now reaches a PB account only via a normal account.

## Background

Today the service models a single account kind: **purpose-bound accounts** (`accounts` table), each backed by a pair of linked TigerBeetle accounts (self-pool + others-pool). PB accounts support deposits (with funding type `self`/`trust`/`third_party`), MCC-validated payments (others-first split, then self), and withdrawals (self-pool only, to the withdrawal settlement sentinel).

Normal accounts are a simpler shape: a single TB account, no pools, no MCC restrictions, and a tighter operation surface. They function as a holding container for trust-sourced money — money arrives from the trust sentinel, then either departs to a PB account (as an internal transfer recorded as a deposit on the PB side) or is withdrawn to the existing withdrawal settlement sentinel.

## Non-goals

- No payment operation on normal accounts. Normal accounts cannot debit `MERCHANT_SETTLEMENT_TB_ID`. Merchant payments remain a PB-only operation.
- No PB → normal direction for transfers. Internal transfers are normal → PB only.
- No new sentinel TB accounts. Existing five sentinels are reused.
- No KYC enforcement, transfer caps, or holder-relationship checks between source and destination accounts.
- No retroactive backfill of historical data; no historical PB-trust deposits get rewritten.
- No shared `Account` / `LedgerAccount` Rust trait. The two kinds are deliberately separate.
- No staged rollout with table-rename views; the migration is delivered in three back-to-back PRs (see Rollout).

## Scope (high level)

| Area | Change |
|---|---|
| Schema | Rename `accounts` → `pb_accounts`; new `normal_accounts` table; extend `transactions` with `account_kind`, `correlation_id`, nullable `pool`. |
| Smithy | New operations for normal accounts and transfers; PB operations renamed (e.g. `CreateAccount` → `CreatePBAccount`); old names retained as `@deprecated` aliases. |
| Routes | Canonical `/pb-accounts/*` and `/normal-accounts/*`; legacy `/accounts/*` kept as in-process aliases with `Deprecation` / `Sunset` headers. |
| Domain | New `NormalAccount` struct; new `TransactionType::Transfer` variant; new `AccountKind` enum. |
| Repository | New `normal_account_repo`; `transaction_repo` extended with `account_kind` and `correlation_id`; `ledger_repo` gains normal-account creation, single-balance lookup, and internal-transfer helpers. |
| Service | Four new services — `normal_account_service`, `normal_deposit_service`, `normal_withdrawal_service`, `transfer_service`; `pb_deposit_service` rejects `funding_type='trust'`. |
| Tests | Three new Cucumber feature files (normal lifecycle, internal transfer, trust-direct removal); two admin UI features; unit tests for each new module. |

## Architecture

Two parallel domain modules sharing one `LedgerRepo`, one `TransactionRepo`, and one set of TB sentinels.

```
crates/pba_service/src/
├── domain/
│   ├── account.rs                  (existing — PB account: PurposeBoundAccount)
│   ├── account_kind.rs             (new — AccountKind enum: Pb | Normal)
│   ├── normal_account.rs           (new — NormalAccount)
│   ├── pool.rs                     (existing — PB-only)
│   └── transfer.rs                 (new — input/result helpers)
├── repository/
│   ├── pb_account_repo.rs          (renamed from account_repo.rs)
│   ├── normal_account_repo.rs      (new)
│   ├── ledger_repo.rs              (extended)
│   └── transaction_repo.rs         (extended — account_kind, correlation_id)
├── service/
│   ├── pb_account_service.rs       (renamed)
│   ├── pb_deposit_service.rs       (renamed; rejects funding_type='trust')
│   ├── pb_payment_service.rs       (renamed)
│   ├── pb_withdrawal_service.rs    (renamed)
│   ├── normal_account_service.rs   (new)
│   ├── normal_deposit_service.rs   (new)
│   ├── normal_withdrawal_service.rs (new)
│   ├── transfer_service.rs         (new)
│   └── deposit_timeout.rs          (extended — covers all pending rows)
└── api/
    ├── handlers.rs                 (re-exports + legacy wrapper helpers)
    ├── handlers/
    │   ├── pb.rs                   (PB-account handlers)
    │   ├── normal.rs               (normal-account handlers)
    │   ├── transfer.rs             (transfer handlers)
    │   └── transactions.rs         (cross-kind list endpoint)
    ├── routes.rs
    └── dto.rs
```

Crate names (`pba_service`, `pba_client`) and the `PurposeBoundAccount` struct keep their existing spellings — the redundancy argument only applies where the prefix would collide with `Account` / `accounts`.

## Schema & migrations

Three migrations under `crates/pba_service/src/db/migrations/`, applied at startup via sqlx-migrate (existing pattern).

### M1 — `20260508000001_rename_accounts_to_pb_accounts.sql`

```sql
ALTER TABLE accounts RENAME TO pb_accounts;
ALTER INDEX idx_accounts_origin_purpose RENAME TO idx_pb_accounts_origin_purpose;
ALTER INDEX idx_accounts_holder         RENAME TO idx_pb_accounts_holder;
```

### M2 — `20260508000002_normal_accounts.sql`

```sql
CREATE TABLE normal_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    holder_id VARCHAR(64) NOT NULL,
    origin_ifsc VARCHAR(11),                       -- nullable
    origin_account_number VARCHAR(20),             -- nullable
    vpa VARCHAR(50),
    virtual_ifsc VARCHAR(11),
    virtual_account_number VARCHAR(20),
    tb_account_id NUMERIC(39) NOT NULL,
    kyc_tier VARCHAR(10) NOT NULL DEFAULT 'minimum',
    status VARCHAR(10) NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_normal_accounts_holder ON normal_accounts (holder_id);
```

No origin-bank uniqueness — origin is optional and not coupled to a purpose.

### M3 — `20260508000003_transactions_kind_correlation.sql`

```sql
ALTER TABLE transactions
    ADD COLUMN account_kind VARCHAR(10) NOT NULL DEFAULT 'pb',
    ADD COLUMN correlation_id UUID NULL,
    ALTER COLUMN pool DROP NOT NULL;

ALTER TABLE transactions ALTER COLUMN account_kind DROP DEFAULT;

CREATE INDEX idx_transactions_account_kind_account
    ON transactions (account_kind, account_id, created_at DESC);

CREATE INDEX idx_transactions_correlation
    ON transactions (correlation_id) WHERE correlation_id IS NOT NULL;

-- Idempotency unique constraint becomes (kind, account, key)
ALTER TABLE transactions DROP CONSTRAINT IF EXISTS transactions_account_id_idempotency_key_key;
CREATE UNIQUE INDEX uq_transactions_idempotency
    ON transactions (account_kind, account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
```

The default-then-drop pattern on `account_kind` backfills existing rows as `'pb'` in one statement, then forces explicit kind on subsequent inserts. Exact name of the existing idempotency constraint will be confirmed during plan write-up via `\d transactions`.

`TransactionType::Transfer` is a Rust-side enum addition (string column, no SQL change). If a `CHECK` constraint exists on `transaction_type`, M3 also extends it.

### What stays untouched

`purpose_mcc_allowlist` (PB-only, unchanged); `tb_self_account_id` / `tb_others_account_id` columns on `pb_accounts`; all existing transaction columns (direction, amount, gateway_ref, status, tb_transfer_id, merchant fields, funding_type) reused as-is.

### Coupling with code deploy

M1 makes the deploy not zero-downtime — during a rolling deploy across multiple instances, in-flight pods on old code briefly hit `pb_accounts` and 500. For this repo's current single-node `process-compose` runtime that's acceptable.

## API surface

### Smithy operations

**New (file-per-resource: `model/normal_account.smithy`, `model/transfer.smithy`):**

| Operation | URL |
|---|---|
| `CreateNormalAccount` | `POST /normal-accounts` |
| `GetNormalAccount` | `GET /normal-accounts/{accountId}` |
| `ListNormalAccounts` | `GET /normal-accounts` |
| `UpdateNormalAccountStatus` | `PATCH /normal-accounts/{accountId}/status` |
| `GetNormalAccountBalance` | `GET /normal-accounts/{accountId}/balance` |
| `DepositToNormalAccount` | `POST /normal-accounts/{accountId}/deposits` |
| `PostNormalAccountDeposit` | `POST /normal-accounts/{accountId}/deposits/{depositId}/post` |
| `VoidNormalAccountDeposit` | `POST /normal-accounts/{accountId}/deposits/{depositId}/void` |
| `TransferToPBAccount` | `POST /normal-accounts/{accountId}/transfers` |
| `PostNormalAccountTransfer` | `POST /normal-accounts/{accountId}/transfers/{transferId}/post` |
| `VoidNormalAccountTransfer` | `POST /normal-accounts/{accountId}/transfers/{transferId}/void` |
| `WithdrawFromNormalAccount` | `POST /normal-accounts/{accountId}/withdrawals` |
| `ListNormalAccountTransactions` | `GET /normal-accounts/{accountId}/transactions` |

**Renamed PB operations:**

| Old name | New name | Canonical URL |
|---|---|---|
| `CreateAccount` | `CreatePBAccount` | `POST /pb-accounts` |
| `GetAccount` | `GetPBAccount` | `GET /pb-accounts/{accountId}` |
| `GetBalance` | `GetPBAccountBalance` | `GET /pb-accounts/{accountId}/balance` |
| `Deposit` | `DepositToPBAccount` | `POST /pb-accounts/{accountId}/deposits` |
| `PostDeposit` | `PostPBAccountDeposit` | `POST /pb-accounts/{accountId}/deposits/{depositId}/post` |
| `VoidDeposit` | `VoidPBAccountDeposit` | `POST /pb-accounts/{accountId}/deposits/{depositId}/void` |
| `MakePayment` | `MakePBAccountPayment` | `POST /pb-accounts/{accountId}/payments` |
| `Withdraw` | `WithdrawFromPBAccount` | `POST /pb-accounts/{accountId}/withdrawals` |
| `UpdateAccountStatus` | `UpdatePBAccountStatus` | `PATCH /pb-accounts/{accountId}/status` |
| `ListTransactions` | `ListPBAccountTransactions` | `GET /pb-accounts/{accountId}/transactions` |

Old operation names are retained in Smithy as `@deprecated(message: "Use <new>", since: "2026-05-08")` aliases pointing at the legacy URLs (`/accounts/*`) and using the same request/response shapes as their new counterparts. The generated SDK exposes both surfaces.

`ListAllTransactions`, `ListPurposeTypes`, `GetPurposeType` are unchanged. `ListAllTransactions` rows now include `account_kind`.

### Routes (`api/routes.rs`)

Three blocks merged into the protected router: `pb` (canonical), `normal` (canonical), `legacy` (in-process aliases for `/accounts/*`). Legacy handlers are one-line wrappers over the canonical handlers that attach `Deprecation: true` and `Sunset: <date>` response headers.

### Handler organisation

Following the file-per-module preference, `api/handlers.rs` becomes a re-export shim over `api/handlers/{pb,normal,transfer,transactions}.rs`.

### DTOs

New shapes in `api/dto.rs`:

```rust
NormalAccountDto                { id, holderId, originIfsc?, originAccountNumber?, vpa?,
                                  virtualIfsc?, virtualAccountNumber?, kycTier, status,
                                  createdAt, updatedAt }
CreateNormalAccountRequest      { holderId, originIfsc?, originAccountNumber? }
NormalAccountBalanceDto         { balance, pending }    // single u64s

DepositToNormalAccountRequest   { amount, pending, gatewayRef?, timeoutSeconds?, idempotencyKey? }
                                // no funding_type, no source_*; always trust sentinel

WithdrawFromNormalAccountRequest{ amount, gatewayRef?, idempotencyKey? }

TransferToPBAccountRequest      { destinationPbAccountId, amount, pending, gatewayRef?,
                                  timeoutSeconds?, description?, idempotencyKey? }
TransferResponse                { transferId, sourceAccountId, destinationAccountId,
                                  amount, status, correlationId, createdAt }
```

`TransactionDto` (used by all listing endpoints) gains `accountKind: "pb" | "normal"` and `correlationId?: UUID`. PB DTOs unchanged.

## Service & repository layout

### Repository layer

- **`pb_account_repo.rs`** — direct rename of `account_repo.rs`; SQL updated to reference `pb_accounts`. Public method names unchanged.
- **`normal_account_repo.rs`** — `create_account`, `get_account`, `list_accounts`, `update_status`, `count_accounts_by_status`. No MCC methods.
- **`ledger_repo.rs`** — three new methods: `create_normal_account`, `get_single_balance`, `create_internal_transfer` (immediate + pending variants). Existing `post_pending_transfer` / `void_pending_transfer` reused.
- **`transaction_repo.rs`** — every insert/query gains `account_kind`; `pool` is now `Option<&str>`; new `correlation_id` parameter; new `find_by_correlation_id` method. `find_by_idempotency_key` keyed on `(kind, account_id, key)`.

### Service layer

| Service | Role |
|---|---|
| `pb_account_service` | renamed; behaviour unchanged |
| `pb_deposit_service` | renamed; **rejects `funding_type='trust'`** at top of `deposit()` with `AppError::TrustDepositRequiresTransfer` |
| `pb_payment_service` | renamed; behaviour unchanged |
| `pb_withdrawal_service` | renamed; behaviour unchanged |
| `normal_account_service` | CRUD + status; mirrors PB account service shape |
| `normal_deposit_service` | trust-only inflow (always `TRUST_FUNDING_SOURCE_TB_ID`); immediate + pending lifecycle; no `funding_type`/`source_*` fields on input |
| `normal_withdrawal_service` | immediate only; debit normal_tb → credit `WITHDRAWAL_SETTLEMENT_TB_ID` |
| `transfer_service` | normal → PB transfers; immediate + pending lifecycle; orchestrates the two journal rows and one TB transfer |
| `deposit_timeout` | extended to scan all pending rows, not just deposits |

### Where rules live

| Rule | Location |
|---|---|
| Reject `funding_type='trust'` on PB deposits | `pb_deposit_service::deposit` (early return) |
| Single-source debit for normal deposits (TRUST sentinel) | `normal_deposit_service` (no branching) |
| Normal accounts cannot pay merchants | not modelled (operation does not exist) |
| `funding_type='trust'` on both transfer legs | `transfer_service` (hard-coded at insert time) |
| Idempotency keyed on `(account_kind, account_id, key)` | `transaction_repo` + DB unique index |
| `correlation_id` set only on transfer legs | `transfer_service`; everything else passes `None` |

### `AppState` delta

```rust
pub struct AppState {
    pub pb_account_service:     Arc<PbAccountService>,
    pub pb_deposit_service:     Arc<PbDepositService>,
    pub pb_payment_service:     Arc<PbPaymentService>,
    pub pb_withdrawal_service:  Arc<PbWithdrawalService>,
    pub normal_account_service: Arc<NormalAccountService>,
    pub normal_deposit_service: Arc<NormalDepositService>,
    pub normal_withdrawal_service: Arc<NormalWithdrawalService>,
    pub transfer_service:       Arc<TransferService>,
}
```

No shared `Account` trait. The transfer service takes both repos as concrete dependencies; that is the only place both kinds meet.

## Ledger conventions

### TB account codes

| Code | Meaning |
|---|---|
| 1 | PB self pool (existing) |
| 2 | PB others pool (existing) |
| **3** | **Normal account** (new) |
| 99 | Sentinel (existing) |

### TB account flags

| Account kind | Flags |
|---|---|
| PB self pool | `DEBITS_MUST_NOT_EXCEED_CREDITS \| HISTORY \| LINKED` |
| PB others pool | `DEBITS_MUST_NOT_EXCEED_CREDITS \| HISTORY` |
| Normal | `DEBITS_MUST_NOT_EXCEED_CREDITS \| HISTORY` |
| Sentinels | as today |

### TB transfer codes

| Code | Operation |
|---|---|
| 100 | PB deposit — immediate |
| 101 | PB deposit — pending |
| **110** | **Normal deposit — immediate** |
| **111** | **Normal deposit — pending** |
| 200 | PB payment |
| 300 | PB withdrawal |
| **310** | **Normal withdrawal** |
| **400** | **Internal transfer normal → PB others — immediate** |
| **401** | **Internal transfer normal → PB others — pending** |

### Sentinel use map (after this change)

| Sentinel | Counterparty for |
|---|---|
| `SELF_FUNDING_SOURCE_TB_ID` | PB deposits where source matches origin |
| `TRUST_FUNDING_SOURCE_TB_ID` | **Only** Normal-account inbound deposits |
| `THIRD_PARTY_FUNDING_SOURCE_TB_ID` | PB deposits with `funding_type='third_party'` |
| `MERCHANT_SETTLEMENT_TB_ID` | PB payments only |
| `WITHDRAWAL_SETTLEMENT_TB_ID` | PB withdrawals + Normal withdrawals |

No new sentinels.

### Conservation invariants (documented at the top of `ledger_repo.rs`)

1. `TRUST_FUNDING_SOURCE_TB_ID.debits_posted - .credits_posted` equals (sum of all normal-account balances) + (portion of PB-others balances received via internal transfer).
2. PB-others pool is never credited from `TRUST_FUNDING_SOURCE_TB_ID` directly — only from internal transfers (codes 400/401) or third-party deposits (code 100 with `funding_type='third_party'`).
3. `WITHDRAWAL_SETTLEMENT_TB_ID` credits partition by source-account `code`: 1 = PB self, 3 = normal.
4. `MERCHANT_SETTLEMENT_TB_ID` credits come only from `code=1` or `code=2` accounts. A credit from `code=3` is a bug.

### ID derivation

```rust
pub fn tb_self_id(uuid: Uuid)   -> u128;          // unchanged
pub fn tb_others_id(uuid: Uuid) -> u128;          // unchanged (high bit of byte 0 flipped)
pub fn tb_normal_id(uuid: Uuid) -> u128 {         // new
    u128::from_be_bytes(*uuid.as_bytes())         // raw bytes; collisions ~ 2^-122
}
```

`tb_normal_id` and `tb_self_id` produce the same bytes today. They are separate functions so call sites express intent and so the schemes can diverge later without churning callers.

## Transfer flow

### Immediate transfer (`pending=false`)

`POST /normal-accounts/{src}/transfers` with `{ destinationPbAccountId, amount, gatewayRef?, description?, idempotencyKey? }`.

```
1. transfer_service.transfer(src, dst, amount, pending=false, …)
2. idempotency check on (Normal, src, key); if hit, return both legs via correlation_id
3. load src normal account (Active); load dst PB account (Active)
4. balance check: ledger_repo.get_single_balance(src.tb_account_id) >= amount
5. correlation_id = Uuid::new_v4(); src_txn_id, dst_txn_id = Uuid::new_v4(), Uuid::new_v4()
6. BEGIN PG tx
   insert src-side row:
     account_kind='normal', txn_type='transfer', direction='outbound', pool=NULL,
     funding_type='trust', status='posted', idempotency_key, correlation_id, tb_transfer_id=0
   insert dst-side row:
     account_kind='pb',     txn_type='deposit',  direction='inbound',  pool='others',
     funding_type='trust', status='posted', idempotency_key=NULL, correlation_id, tb_transfer_id=0
7. ledger_repo.create_internal_transfer(src.tb_account_id, dst.tb_others_account_id, amount, code=400)
8. on AppError::ExceedsBalance: ROLLBACK and retry up to 3x with fresh balance (mirrors payment_service)
9. UPDATE transactions SET tb_transfer_id = $1 WHERE correlation_id = $2
10. COMMIT
11. return TransferResult { transferId = src_txn_id, correlationId, … }
```

The `UPDATE … WHERE correlation_id` updates both legs in one statement.

### Pending transfer (`pending=true`)

Step 6 inserts both rows with `status='pending'`. Step 7 calls `create_pending_internal_transfer(…, code=401, timeout=…)` which returns `tb_transfer_id`. Steps 9–11 unchanged.

### Post pending transfer

`POST /normal-accounts/{src}/transfers/{transferId}/post`:

1. Load src-side row by `(src, transferId)`; must be `status='pending'`, `txn_type='transfer'`.
2. `ledger_repo.post_pending_transfer(row.tb_transfer_id)`.
3. `UPDATE transactions SET status='posted', updated_at=now() WHERE correlation_id = $1`.
4. Return `TransferResult` with both legs.

The URL's `{transferId}` is the src-side row's id (the natural identifier for the source-account holder). The dst-side row id is internal.

### Void pending transfer

Symmetric: `void_pending_transfer(row.tb_transfer_id)` then `UPDATE … SET status='voided' WHERE correlation_id = $1`.

### Auto-timeout

`deposit_timeout.rs` is generalised to scan all pending rows. For transfer pairs (two rows sharing `tb_transfer_id`), the void + status update flips both legs via the `correlation_id` predicate. TB returns "already voided" for transfers that auto-voided past timeout, which the loop treats as success.

### Failure modes

| Failure point | PG state | TB state | Outcome |
|---|---|---|---|
| Insufficient balance | nothing inserted | nothing | 400 `InsufficientFunds` |
| TB rejects transfer (e.g., src frozen mid-flight) | rolled back | nothing | 5xx; up to 3 retries on `ExceedsBalance` |
| TB succeeds, PG step 9 fails | rolled back | TB has the move | drift — same hazard PB code already has |
| TB succeeds, COMMIT fails | rolled back | TB has the move | drift |

Pending transfers self-heal across this drift class: if step 7' succeeds and step 10' fails, the pending TB transfer auto-voids after `timeout`. For large transfers, prefer pending+post.

### Why a single TB transfer

A transfer is one debit-credit pair across two pre-existing TB accounts. The PB "linked transfers" pattern is for payment splits (others-first then self-fallback) which is genuinely two transfers; here it is one.

## Error handling & validation

### New `AppError` variants

```rust
NormalAccountNotFound(String),         // 404
NormalAccountNotActive(String),        // 409
TrustDepositRequiresTransfer,          // 400
TransferDestinationNotPb(String),      // 400
TransferSourceFrozenOrClosed(String),  // 409
CorrelationLookupFailed(String),       // 500
```

Renames: `AccountNotFound → PbAccountNotFound`, `AccountNotActive → PbAccountNotActive`. Sites updated mechanically; tests catch any miss.

### HTTP body shapes (additions)

| Variant | Status | Body |
|---|---|---|
| `NormalAccountNotFound` | 404 | `{ "error": "normal_account_not_found", "id": "…" }` |
| `NormalAccountNotActive` | 409 | `{ "error": "normal_account_not_active", "id": "…" }` |
| `TrustDepositRequiresTransfer` | 400 | `{ "error": "trust_deposit_requires_transfer", "hint": "POST /normal-accounts/{srcId}/transfers instead" }` |
| `TransferDestinationNotPb` | 400 | `{ "error": "transfer_destination_not_pb", "id": "…" }` |
| `TransferSourceFrozenOrClosed` | 409 | `{ "error": "transfer_source_frozen_or_closed", "id": "…" }` |

The `hint` on `TrustDepositRequiresTransfer` is intentional — it points migrating callers to the new endpoint.

### Request validation

| Endpoint | Validation |
|---|---|
| `POST /normal-accounts` | `holderId` non-empty; if `originIfsc`/`originAccountNumber` present, both present |
| `POST /normal-accounts/{id}/deposits` | `amount > 0`; `pending` is bool; `timeoutSeconds` only when `pending=true` |
| `POST /normal-accounts/{id}/transfers` | `amount > 0`; `destinationPbAccountId` is a UUID; `timeoutSeconds` only with `pending=true`; `description` ≤ 256 chars |
| `POST /normal-accounts/{id}/withdrawals` | `amount > 0` |
| `POST /pb-accounts/{id}/deposits` | existing rules + `funding_type != 'trust'` |

### Not validated (deliberately)

- Holder match between source and destination accounts. Cross-holder transfers are an expected use case (employer→employee, family→relative).
- Per-day or per-amount transfer caps.
- KYC tier matching.

## Testing strategy

### Unit tests

| File | Tests |
|---|---|
| `domain/normal_account.rs` | `tb_normal_id` distinct from sentinels |
| `domain/transfer.rs` | input validation helpers, `correlation_id` generation |
| `service/normal_deposit_service.rs` | active-check, balance updated post-deposit (mock TB), idempotency replay |
| `service/normal_withdrawal_service.rs` | balance check, insufficient-funds path, idempotency replay |
| `service/transfer_service.rs` | both-active check, balance check, leg row insertion, retry on `ExceedsBalance`, post & void update both legs, auto-timeout |
| `service/pb_deposit_service.rs` | `funding_type='trust'` → `TrustDepositRequiresTransfer` |
| `repository/transaction_repo.rs` | `account_kind` round-trip; `find_by_correlation_id` returns both legs; per-kind idempotency uniqueness |

### Cucumber BDD

Three new feature files:

- **`normal_account_lifecycle.feature`** — create / get / list / freeze / reactivate / balance for a normal account; deposit (immediate + pending lifecycle); withdrawal; per-account transactions list.
- **`internal_transfer.feature`** — happy-path immediate transfer; pending + post; pending + void; pending auto-voided after timeout; idempotency replay; insufficient-balance rejection; source-frozen and destination-frozen rejections; both legs visible via `correlation_id`.
- **`trust_direct_deposit_removed.feature`** — explicit assertion that `funding_type='trust'` is rejected on `/pb-accounts/{id}/deposits` and the `/accounts/{id}/deposits` legacy alias; `'self'` and `'third_party'` still work.

### UI tests

Two new browser features:

- **`normal_account_admin.feature`** — admin lists, creates, sees balance, sees transactions for normal accounts.
- **`transfer_admin.feature`** — admin initiates a transfer from a normal-account detail page; both legs appear paired by `correlation_id`; admin can post or void a pending one.

### Regression coverage

The existing PBA Cucumber suite must still pass unchanged — the working test of the in-process aliases. The only existing scenarios that need a mod are those that use `funding_type='trust'`; they flip to the new `trust_deposit_requires_transfer` rejection (and the original setup path is replaced with a transfer-based scenario where appropriate). Find them with `grep -ri 'funding_type.*trust\|fundingType.*trust' features/`.

### Coverage gates

No new gates beyond `just local-ci`. Optional Cucumber tags `@transfer` / `@normal-account` for local development.

## Rollout plan

### PR split

**PR 1 — Renames & structural moves (zero behaviour change).**
Pure renames: `accounts` → `pb_accounts` (M1); `account_*.rs` → `pb_account_*.rs`; service/repo struct renames; SQL updates; `AppState` field renames. Smithy ops keep their old names. Tests updated mechanically. Reviewer verifies with `git diff -M` showing pure renames + import updates.

**PR 2 — Normal accounts (additive, no PBA behaviour change).**
M2 + M3. New domain / repo / service / handlers / routes for normal accounts. New Smithy operations for normal accounts. New Cucumber features for normal-account lifecycle. Smithy `@deprecated` aliases for renamed PB ops. Legacy `/accounts/*` HTTP routes added as in-process aliases. New unit and admin UI tests. PB behaviour unchanged.

**PR 3 — Transfers + trust-deposit removal (the breaking change).**
New transfer service / handlers / routes / Smithy operations. `pb_deposit_service` rejects `funding_type='trust'`. New Cucumber features for transfers and trust-rejection. Existing `funding_type='trust'` test scenarios updated to use the transfer flow.

**Why this split:** PR 1 is mechanical and trivially reviewable. PR 2 is large but additive — easy to ship and exercise. PR 3 is the only PR with a behavioural break, isolated and easy to revert.

A single combined PR is acceptable if reviewer cycles are scarce; the migration is internally consistent — no half-state where schema and code diverge.

### Deployment ordering

Each PR's migration runs at startup via sqlx-migrate. PR 1's rename causes brief 5xx during a multi-instance rolling deploy; for the current single-node `process-compose` runtime that is fine. PRs 2 and 3 are forward-compatible.

### Documentation updates

- `README.md` — extend the API table to include `/normal-accounts/*` and `/pb-accounts/*`; mark `/accounts/*` deprecated → `/pb-accounts/*`; mention the transfer flow.
- `WHAT.md` — add a short "Normal accounts" section describing them as the inbound funding container for trust money.
- `model/main.smithy` — operation list updated; `model/normal_account.smithy` and `model/transfer.smithy` referenced.

### Sunset of `/accounts/*` shim

- PR 2 merge: shim live with `Deprecation: true` and `Sunset: <merge_date + 90d>` headers.
- After 60 days: review server access logs for `/accounts/*` traffic; contact callers if non-zero.
- After 90 days: delete the legacy routes and the deprecated Smithy operations in a small follow-up PR.

## Open items (none blocking)

- Confirm exact name of the existing idempotency unique constraint on `transactions` (one `\d transactions` during plan write-up).
- Confirm whether `transaction_type` has a `CHECK` constraint that needs the `'transfer'` value added.

Both are mechanical and resolved during the implementation plan, not the design.
