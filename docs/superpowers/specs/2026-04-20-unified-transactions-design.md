# Unified Transactions Table Design

## Summary

Replace the `deposits` table with a unified `transactions` table that fronts every TigerBeetle operation (deposits, payments, withdrawals) in PostgreSQL. This gives us rich metadata storage, flexible SQL querying, and a single source of truth for user-facing transaction lists. TigerBeetle remains the authoritative ledger for balances and financial atomicity.

## Motivation

Today, only deposits are tracked in PostgreSQL. Payments and withdrawals are fire-and-forget TigerBeetle transfers — their metadata (merchant, MCC, description) is logged to tracing and discarded. This makes it impossible to build a user-facing transaction history with context like merchant names, descriptions, or filtering by type.

Additionally, the current TigerBeetle-based transfer history has limitations:
- Max 8,190 results per query, no SQL-style filtering
- Cannot filter by transfer flags (must filter in application code)
- No metadata beyond amount, code, and three user_data fields
- Timed-out pending transfers appear as phantom entries

## Architecture

### Responsibilities

| System | Responsibility |
|--------|---------------|
| **PostgreSQL** | Transaction metadata, history queries, pagination, idempotency |
| **TigerBeetle** | Authoritative balances, financial atomicity, double-entry enforcement, overdraft prevention |

### Write Ordering

All transaction types follow PG-first ordering to prevent orphaned TigerBeetle transfers:

**Deposits (immediate):**
1. Begin PG transaction
2. Insert row (`status=posted`, `tb_transfer_id=0`)
3. Execute TB transfer
4. Commit PG transaction (on TB success) or rollback (on TB failure)

**Deposits (pending — two-phase):**
1. Begin PG transaction
2. Insert row (`status=pending`, `tb_transfer_id=0`)
3. Create TB pending transfer → get `tb_transfer_id`
4. Update row with real `tb_transfer_id`
5. Commit PG transaction (on TB success) or rollback (on TB failure)

Post/void lifecycle unchanged — TB operation first, then PG status update.

**Payments:**
1. Begin PG transaction
2. Insert row(s) — one per pool for split payments (`status=settled`)
3. Execute TB transfer(s)
4. Commit PG transaction (on TB success) or rollback (on TB failure)

**Withdrawals:**
1. Begin PG transaction
2. Insert row (`status=settled`)
3. Execute TB transfer
4. Commit PG transaction (on TB success) or rollback (on TB failure)

### Failure Modes

| Failure | PG state | TB state | Resolution |
|---------|----------|----------|------------|
| Crash before TB call | Row rolled back | Nothing | Clean — no orphan |
| TB rejects (insufficient funds, etc.) | Row rolled back | Nothing | Clean — error returned to client |
| Crash after TB, before PG commit | Row rolled back | Transfer exists | TB transfer is orphaned. Reconciliation job detects via TB transfer with no matching PG record. For pending deposits, TB auto-expires the transfer via timeout. See Future Work. |

## Schema

### Migration

Drop the `deposits` table and create `transactions`:

```sql
DROP TABLE IF EXISTS deposits;

CREATE TABLE transactions (
    id                UUID PRIMARY KEY,
    account_id        UUID NOT NULL REFERENCES accounts(id),
    type              TEXT NOT NULL,       -- 'deposit', 'payment', 'withdrawal'
    status            TEXT NOT NULL,       -- 'pending', 'posted', 'voided', 'settled'
    amount            BIGINT NOT NULL,
    pool              TEXT NOT NULL,       -- 'self', 'others'
    direction         TEXT NOT NULL,       -- 'inbound', 'outbound'
    -- deposit-specific
    source_ifsc       TEXT,
    source_account    TEXT,
    gateway_ref       TEXT,
    timeout_seconds   INTEGER CHECK (timeout_seconds > 0),
    -- payment-specific
    merchant_id       TEXT,
    merchant_mcc      TEXT,
    description       TEXT,
    -- TB linkage
    tb_transfer_id    NUMERIC(39,0) NOT NULL DEFAULT 0,
    -- idempotency
    idempotency_key   TEXT,
    -- timestamps
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_transactions_account ON transactions(account_id, created_at DESC);
CREATE INDEX idx_transactions_account_status ON transactions(account_id, status);
CREATE UNIQUE INDEX idx_transactions_idempotency ON transactions(account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
```

### Status Values by Type

| Type | Statuses |
|------|----------|
| Deposit (immediate) | `posted` |
| Deposit (pending) | `pending` → `posted` or `voided` |
| Payment | `settled` |
| Withdrawal | `settled` |

### Split Payments

Payments that draw from both pools produce **two rows** — one per pool, each with its own amount. This matches how TigerBeetle stores them as linked transfers and keeps the `amount` column accurate per row.

## Smithy Model

### Enums

```smithy
@enum([
    { value: "deposit", name: "DEPOSIT" },
    { value: "payment", name: "PAYMENT" },
    { value: "withdrawal", name: "WITHDRAWAL" },
])
string TransactionType

@enum([
    { value: "pending", name: "PENDING" },
    { value: "posted", name: "POSTED" },
    { value: "voided", name: "VOIDED" },
    { value: "settled", name: "SETTLED" },
])
string TransactionStatus

@enum([
    { value: "self", name: "SELF_POOL" },
    { value: "others", name: "OTHERS_POOL" },
])
string PoolType

@enum([
    { value: "inbound", name: "INBOUND" },
    { value: "outbound", name: "OUTBOUND" },
])
string TransactionDirection
```

### ListTransactions Operation

```smithy
@readonly
@http(method: "GET", uri: "/accounts/{account_id}/transactions")
operation ListTransactions {
    input: ListTransactionsInput,
    output: ListTransactionsOutput,
    errors: [AccountNotFoundError],
}

structure ListTransactionsInput {
    @required
    @httpLabel
    account_id: String,

    @httpQuery("offset")
    offset: Long,

    @httpQuery("limit")
    limit: Long,
}

structure ListTransactionsOutput {
    @required
    transactions: TransactionList,

    @required
    total: Long,

    @required
    offset: Long,

    @required
    limit: Long,
}

list TransactionList {
    member: TransactionSummary,
}

structure TransactionSummary {
    @required
    id: String,

    @required
    type: TransactionType,

    @required
    status: TransactionStatus,

    @required
    amount: Long,

    @required
    pool: PoolType,

    @required
    direction: TransactionDirection,

    description: String,
    merchant_id: String,
    merchant_mcc: String,
    source_ifsc: String,
    source_account: String,
    gateway_ref: String,

    @required
    created_at: Timestamp,
}
```

### Timestamp Migration

All `String` timestamp fields across existing Smithy models are changed to the built-in `Timestamp` type. This affects:

- `AccountDetail`: `created_at`, `updated_at`
- `DepositOutput`: `created_at` (if present)
- `TransactionSummary`: `created_at`

Smithy `Timestamp` serializes as ISO 8601 in JSON (`2026-04-20T14:30:00Z`) and generates `date-time` format in OpenAPI spec.

### Idempotency Key

Added as an optional field on existing deposit, payment, and withdrawal input structures:

```smithy
idempotency_key: String
```

When provided and a duplicate `(account_id, idempotency_key)` is found, the existing transaction is returned instead of creating a new one.

## Repository Layer

`deposit_repo.rs` is replaced by `transaction_repo.rs`:

| Method | Purpose |
|--------|---------|
| `insert_in_tx(&mut PgTransaction, ...)` | Insert within a PG transaction (all types) |
| `update_tb_transfer_id_in_tx(&mut PgTransaction, id, tb_transfer_id)` | Set real TB transfer ID after TB call (within same PG transaction) |
| `update_status(id, new_status)` | For pending deposit post/void lifecycle |
| `get_by_id(id, account_id)` | Look up single transaction |
| `find_by_idempotency_key(account_id, key)` | Check for existing transaction before insert |
| `list_by_account(account_id, offset, limit)` | Paginated list, ordered by `created_at DESC` |
| `count_by_account(account_id)` | Total count for pagination |
| `list_pending_by_account(account_id)` | For admin UI pending deposits section |

## Service Layer Changes

### DepositService

- Replace `DepositRepo` dependency with `TransactionRepo`
- Immediate deposits: wrap PG insert + TB call in a PG transaction
- Pending deposits: wrap PG insert + TB call + `tb_transfer_id` update in a PG transaction; rollback if TB fails
- Post/void: unchanged pattern, references `TransactionRepo` instead of `DepositRepo`

### PaymentService

- Add `TransactionRepo` dependency
- Wrap PG insert(s) + TB call in a PG transaction
- Split payments produce two PG rows within the same transaction
- Add idempotency check before insert

### WithdrawalService

- Add `TransactionRepo` dependency
- Wrap PG insert + TB call in a PG transaction
- Add idempotency check before insert

### PgPool Plumbing

Services that use PG transactions need access to the `PgPool` directly (to call `pool.begin()`). The `TransactionRepo` methods accept `&mut PgTransaction` so the service controls the transaction boundary.

## Admin UI Changes

- Transfer history fragment switches from `ledger_repo.get_account_transfers()` to `transaction_repo.list_by_account()`
- Same table layout — Type, Pool, Direction, Amount columns
- Additional columns available: Description, Merchant ID (can be added incrementally)
- Pending deposits section continues to use `transaction_repo.list_pending_by_account()`

## What Gets Removed

| File/Code | Replacement |
|-----------|-------------|
| `deposit_repo.rs` | `transaction_repo.rs` |
| `domain/deposit.rs` (`DepositRecord`, `DepositStatus`) | `domain/transaction.rs` (`TransactionRecord`, `TransactionStatus`, `TransactionType`) |
| `domain/transfer.rs` (`TransferRecord`, `TransferType`, etc.) | No longer needed — history comes from PG |
| `ledger_repo.get_account_transfers()` | Can be kept for internal debugging but no longer used for display |
| `deposits` migration | Replaced by `transactions` migration |
| `DepositStatus::Created` variant | No longer needed — PG transaction rollback handles failures |

## Testing

Existing Cucumber BDD scenarios continue to work — the step definitions call the same service-level APIs. Internal implementation changes (PG-first, new table) are transparent to the test layer.

New scenarios to add:
- Transaction list pagination (offset/limit, total count)
- Idempotency: duplicate `idempotency_key` returns existing transaction
- Split payment produces two transaction rows
- Transaction list excludes `created` status records
- Transaction list ordered by `created_at DESC`

## Scope Boundaries

**In scope:**
- Unified `transactions` table replacing `deposits`
- PG-first write ordering for all transaction types
- `ListTransactions` API endpoint with offset/limit pagination
- Smithy enums for type, status, pool, direction
- Idempotency key (optional) on all mutation endpoints
- Admin UI switches to PG-backed history
- Cucumber tests for new functionality

**Out of scope:**
- Account-to-account transfers (future work — would add a new `TransactionType::Transfer`)
- User-facing UI (admin only for now)
- Reconciliation job for detecting PG/TB drift
- Archiving or TTL on old transactions

## Future Work

### Reconciliation Job

A periodic background job to detect PG/TB drift caused by the rare "crash after TB succeeds, before PG commit" scenario:

1. Query TB for transfers in a recent time window (e.g., last hour)
2. For each TB transfer, check if a matching PG row exists (by `tb_transfer_id`)
3. Orphaned TB transfers (no PG match) are flagged for review or auto-reconciled
4. For pending deposits, orphaned TB transfers self-heal via timeout expiry — no manual intervention needed
5. For immediate deposits/payments/withdrawals, orphaned transfers may need manual review to determine if the client should be notified

Build this when handling real money or when SLA requirements demand it. At current scale, the risk window (microseconds between TB response and PG commit) makes this extremely rare.
