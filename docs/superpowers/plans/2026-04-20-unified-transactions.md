# Unified Transactions Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the deposits-only PostgreSQL table with a unified `transactions` table that records all TigerBeetle operations, enabling rich user-facing transaction history.

**Architecture:** All transaction types (deposits, payments, withdrawals) are written PG-first inside a PostgreSQL transaction. TigerBeetle is called within the PG transaction; on TB failure the PG transaction is rolled back. A new `ListTransactions` API endpoint serves paginated history from PostgreSQL.

**Tech Stack:** Rust, Axum, SQLx (PostgreSQL), TigerBeetle, Smithy (model), Askama (admin templates), Cucumber BDD tests

---

## File Structure

| File | Responsibility |
|------|---------------|
| `model/common.smithy` | Add `DateTime` shape, transaction enums |
| `model/transaction.smithy` | New — `ListTransactions` operation |
| `model/deposit.smithy` | Add `idempotency_key` field |
| `model/payment.smithy` | Add `idempotency_key` field |
| `model/withdrawal.smithy` | Add `idempotency_key` field |
| `model/account.smithy` | Change `created_at`/`updated_at` to `DateTime` |
| `crates/pba_service/src/db/migrations/20260420000001_transactions_table.sql` | Drop deposits, create transactions |
| `crates/pba_service/src/domain/transaction.rs` | New — `TransactionRecord`, `TransactionType`, `TransactionStatus` |
| `crates/pba_service/src/repository/transaction_repo.rs` | New — replaces `deposit_repo.rs` |
| `crates/pba_service/src/service/deposit_service.rs` | Refactor to use `TransactionRepo` + PG transactions |
| `crates/pba_service/src/service/payment_service.rs` | Add `TransactionRepo` + PG transactions |
| `crates/pba_service/src/service/withdrawal_service.rs` | Add `TransactionRepo` + PG transactions |
| `crates/pba_service/src/service/deposit_timeout.rs` | Update to use `TransactionRepo` |
| `crates/pba_service/src/api/dto.rs` | Add `ListTransactionsResponse`, `TransactionSummaryDto`; update `AccountResponse` timestamps; add `idempotency_key` to request DTOs |
| `crates/pba_service/src/api/handlers.rs` | Add `list_transactions` handler |
| `crates/pba_service/src/api/routes.rs` | Add `GET /accounts/{account_id}/transactions` |
| `crates/pba_service/src/admin/handlers.rs` | Switch transfers fragment to use `TransactionRepo` |
| `crates/pba_service/src/main.rs` | Replace `DepositRepo` with `TransactionRepo`, pass `PgPool` to services |
| `crates/pba_service/src/error.rs` | Rename `DepositNotFound` → `TransactionNotFound`, `DepositNotPending` → `TransactionNotPending` |
| `tests/features/deposits.feature` | Minor: existing scenarios still pass (API unchanged) |
| `tests/features/transactions.feature` | New — pagination, idempotency scenarios |

**Files to delete:**
- `crates/pba_service/src/repository/deposit_repo.rs`
- `crates/pba_service/src/domain/deposit.rs`
- `crates/pba_service/src/domain/transfer.rs`

---

### Task 1: Smithy Model Updates

**Files:**
- Modify: `model/common.smithy`
- Create: `model/transaction.smithy`
- Modify: `model/main.smithy`
- Modify: `model/account.smithy`
- Modify: `model/deposit.smithy`
- Modify: `model/payment.smithy`
- Modify: `model/withdrawal.smithy`

- [ ] **Step 1: Add DateTime shape and transaction enums to common.smithy**

Replace the entire file:

```smithy
$version: "2"
namespace com.ppi.pba

/// Monetary amount in the smallest currency unit (e.g., paise for INR).
long Money

/// ISO 8601 date-time.
@timestampFormat("date-time")
timestamp DateTime

/// Transaction type.
@enum([
    { value: "deposit", name: "DEPOSIT" },
    { value: "payment", name: "PAYMENT" },
    { value: "withdrawal", name: "WITHDRAWAL" },
])
string TransactionType

/// Transaction status.
@enum([
    { value: "pending", name: "PENDING" },
    { value: "posted", name: "POSTED" },
    { value: "voided", name: "VOIDED" },
    { value: "settled", name: "SETTLED" },
])
string TransactionStatus

/// Pool type indicating the source of funds.
@enum([
    { value: "self", name: "SELF_POOL" },
    { value: "others", name: "OTHERS_POOL" },
])
string PoolType

/// Transaction direction.
@enum([
    { value: "inbound", name: "INBOUND" },
    { value: "outbound", name: "OUTBOUND" },
])
string TransactionDirection

/// Account status.
enum Status {
    ACTIVE
    FROZEN
    CLOSED
}

/// KYC tier level.
enum KycTier {
    MINIMUM
    FULL
}

/// Standard error structure.
structure ErrorResponse {
    @required
    error: String
    @required
    message: String
}
```

- [ ] **Step 2: Create transaction.smithy**

Create `model/transaction.smithy`:

```smithy
$version: "2"
namespace com.ppi.pba

/// List transactions for an account with offset/limit pagination.
@readonly
@http(method: "GET", uri: "/accounts/{account_id}/transactions")
operation ListTransactions {
    input := {
        @required
        @httpLabel
        account_id: String

        @httpQuery("offset")
        offset: Long

        @httpQuery("limit")
        limit: Long
    }
    output := {
        @required
        transactions: TransactionList

        @required
        total: Long

        @required
        offset: Long

        @required
        limit: Long
    }
    errors: [AccountNotFoundError]
}

list TransactionList {
    member: TransactionSummary
}

structure TransactionSummary {
    @required
    id: String

    @required
    type: TransactionType

    @required
    status: TransactionStatus

    @required
    amount: Money

    @required
    pool: PoolType

    @required
    direction: TransactionDirection

    description: String
    merchant_id: String
    merchant_mcc: String
    source_ifsc: String
    source_account: String
    gateway_ref: String

    @required
    created_at: DateTime
}
```

- [ ] **Step 3: Add ListTransactions to service operations in main.smithy**

Replace `model/main.smithy`:

```smithy
$version: "2"
namespace com.ppi.pba

use aws.protocols#restJson1

@restJson1
service PurposeBoundAccountService {
    version: "2026-04-14"
    operations: [
        CreateAccount
        GetAccount
        GetBalance
        Deposit
        PostDeposit
        VoidDeposit
        MakePayment
        Withdraw
        UpdateAccountStatus
        ListPurposeTypes
        GetPurposeType
        ListTransactions
    ]
}
```

- [ ] **Step 4: Update account.smithy timestamps to DateTime**

In `model/account.smithy`, change:
```
    created_at: String
```
to:
```
    created_at: DateTime
```

And change:
```
    updated_at: String
```
to:
```
    updated_at: DateTime
```

- [ ] **Step 5: Add idempotency_key to deposit.smithy input**

In `model/deposit.smithy`, add after `timeout_seconds: Integer` in the Deposit input:

```smithy
        idempotency_key: String
```

- [ ] **Step 6: Add idempotency_key to payment.smithy input**

In `model/payment.smithy`, add after `description: String` in the MakePayment input:

```smithy
        idempotency_key: String
```

- [ ] **Step 7: Add idempotency_key to withdrawal.smithy input**

In `model/withdrawal.smithy`, add after `amount: Money` in the Withdraw input:

```smithy
        idempotency_key: String
```

- [ ] **Step 8: Regenerate SDK and OpenAPI spec**

```bash
just smithy-build
```

- [ ] **Step 9: Commit**

```bash
git add model/ crates/pba_client/ crates/pba_service/src/api/openapi.json
git commit -m "feat: add Smithy transaction enums, DateTime type, ListTransactions operation, idempotency_key"
```

---

### Task 2: Database Migration

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260420000001_transactions_table.sql`

- [ ] **Step 1: Create migration file**

Create `crates/pba_service/src/db/migrations/20260420000001_transactions_table.sql`:

```sql
-- Replace deposits table with unified transactions table
DROP TABLE IF EXISTS deposits;

CREATE TABLE transactions (
    id                UUID PRIMARY KEY,
    account_id        UUID NOT NULL REFERENCES accounts(id),
    type              TEXT NOT NULL,
    status            TEXT NOT NULL,
    amount            BIGINT NOT NULL,
    pool              TEXT NOT NULL,
    direction         TEXT NOT NULL,
    source_ifsc       TEXT,
    source_account    TEXT,
    gateway_ref       TEXT,
    timeout_seconds   INTEGER CHECK (timeout_seconds > 0),
    merchant_id       TEXT,
    merchant_mcc      TEXT,
    description       TEXT,
    tb_transfer_id    NUMERIC(39,0) NOT NULL DEFAULT 0,
    idempotency_key   TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_transactions_account ON transactions(account_id, created_at DESC);
CREATE INDEX idx_transactions_account_status ON transactions(account_id, status);
CREATE UNIQUE INDEX idx_transactions_idempotency ON transactions(account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
```

- [ ] **Step 2: Commit**

```bash
git add crates/pba_service/src/db/migrations/20260420000001_transactions_table.sql
git commit -m "feat: add transactions table migration (replaces deposits)"
```

---

### Task 3: Domain Layer

**Files:**
- Create: `crates/pba_service/src/domain/transaction.rs`
- Modify: `crates/pba_service/src/domain.rs` (the module file)
- Delete: `crates/pba_service/src/domain/deposit.rs`
- Delete: `crates/pba_service/src/domain/transfer.rs`

- [ ] **Step 1: Create domain/transaction.rs**

Create `crates/pba_service/src/domain/transaction.rs`:

```rust
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Deposit,
    Payment,
    Withdrawal,
}

impl TransactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deposit => "deposit",
            Self::Payment => "payment",
            Self::Withdrawal => "withdrawal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "deposit" => Some(Self::Deposit),
            "payment" => Some(Self::Payment),
            "withdrawal" => Some(Self::Withdrawal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Posted,
    Voided,
    Settled,
}

impl TransactionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Posted => "posted",
            Self::Voided => "voided",
            Self::Settled => "settled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "posted" => Some(Self::Posted),
            "voided" => Some(Self::Voided),
            "settled" => Some(Self::Settled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionDirection {
    Inbound,
    Outbound,
}

impl TransactionDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "inbound" => Some(Self::Inbound),
            "outbound" => Some(Self::Outbound),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Inbound => "Inbound",
            Self::Outbound => "Outbound",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub transaction_type: TransactionType,
    pub status: TransactionStatus,
    pub amount: u64,
    pub pool: String,
    pub direction: TransactionDirection,
    pub source_ifsc: Option<String>,
    pub source_account: Option<String>,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub merchant_id: Option<String>,
    pub merchant_mcc: Option<String>,
    pub description: Option<String>,
    pub tb_transfer_id: u128,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TransactionRecord {
    pub fn amount_display(&self) -> String {
        format!("{}.{:02}", self.amount / 100, self.amount % 100)
    }

    pub fn type_label(&self) -> &'static str {
        match (self.transaction_type, self.status) {
            (TransactionType::Deposit, TransactionStatus::Pending) => "Deposit (Pending)",
            (TransactionType::Deposit, TransactionStatus::Posted) => "Deposit",
            (TransactionType::Deposit, TransactionStatus::Voided) => "Deposit (Voided)",
            (TransactionType::Payment, _) => "Payment",
            (TransactionType::Withdrawal, _) => "Withdrawal",
            _ => "Unknown",
        }
    }
}
```

- [ ] **Step 2: Update domain module declarations**

Find the file that declares domain modules (likely `crates/pba_service/src/domain.rs` or a `mod.rs`). Replace `pub mod deposit;` and `pub mod transfer;` with `pub mod transaction;`. Keep other modules (`pub mod account;`, `pub mod pool;`, `pub mod purpose;`) unchanged.

- [ ] **Step 3: Delete old domain files**

```bash
rm crates/pba_service/src/domain/deposit.rs
rm crates/pba_service/src/domain/transfer.rs
```

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/domain/
git commit -m "feat: add transaction domain model, remove deposit/transfer domains"
```

---

### Task 4: Transaction Repository

**Files:**
- Create: `crates/pba_service/src/repository/transaction_repo.rs`
- Modify: `crates/pba_service/src/repository.rs` (module declarations)
- Delete: `crates/pba_service/src/repository/deposit_repo.rs`

- [ ] **Step 1: Create repository/transaction_repo.rs**

Create `crates/pba_service/src/repository/transaction_repo.rs`:

```rust
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction as PgTransaction};
use uuid::Uuid;

use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;

pub struct TransactionRepo {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct TransactionRow {
    id: Uuid,
    account_id: Uuid,
    #[sqlx(rename = "type")]
    transaction_type: String,
    status: String,
    amount: i64,
    pool: String,
    direction: String,
    source_ifsc: Option<String>,
    source_account: Option<String>,
    gateway_ref: Option<String>,
    timeout_seconds: Option<i32>,
    merchant_id: Option<String>,
    merchant_mcc: Option<String>,
    description: Option<String>,
    tb_transfer_id: String,
    idempotency_key: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TransactionRow {
    fn into_domain(self) -> TransactionRecord {
        TransactionRecord {
            id: self.id,
            account_id: self.account_id,
            transaction_type: TransactionType::from_str(&self.transaction_type)
                .unwrap_or(TransactionType::Deposit),
            status: TransactionStatus::from_str(&self.status)
                .unwrap_or(TransactionStatus::Pending),
            amount: self.amount as u64,
            pool: self.pool,
            direction: TransactionDirection::from_str(&self.direction)
                .unwrap_or(TransactionDirection::Inbound),
            source_ifsc: self.source_ifsc,
            source_account: self.source_account,
            gateway_ref: self.gateway_ref,
            timeout_seconds: self.timeout_seconds.map(|s| s as u32),
            merchant_id: self.merchant_id,
            merchant_mcc: self.merchant_mcc,
            description: self.description,
            tb_transfer_id: self.tb_transfer_id.parse().unwrap_or(0),
            idempotency_key: self.idempotency_key,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl TransactionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn insert_in_tx(
        &self,
        tx: &mut PgTransaction<'_, Postgres>,
        id: Uuid,
        account_id: Uuid,
        transaction_type: TransactionType,
        status: TransactionStatus,
        amount: u64,
        pool: &str,
        direction: TransactionDirection,
        source_ifsc: Option<&str>,
        source_account: Option<&str>,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        merchant_id: Option<&str>,
        merchant_mcc: Option<&str>,
        description: Option<&str>,
        tb_transfer_id: u128,
        idempotency_key: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        let tb_id_str = tb_transfer_id.to_string();
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            INSERT INTO transactions (id, account_id, type, status, amount, pool, direction,
                                      source_ifsc, source_account, gateway_ref, timeout_seconds,
                                      merchant_id, merchant_mcc, description,
                                      tb_transfer_id, idempotency_key)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15::numeric, $16)
            RETURNING id, account_id, type, status, amount, pool, direction,
                      source_ifsc, source_account, gateway_ref, timeout_seconds,
                      merchant_id, merchant_mcc, description,
                      tb_transfer_id::text as tb_transfer_id, idempotency_key,
                      created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(account_id)
        .bind(transaction_type.as_str())
        .bind(status.as_str())
        .bind(amount as i64)
        .bind(pool)
        .bind(direction.as_str())
        .bind(source_ifsc)
        .bind(source_account)
        .bind(gateway_ref)
        .bind(timeout_seconds.map(|s| s as i32))
        .bind(merchant_id)
        .bind(merchant_mcc)
        .bind(description)
        .bind(&tb_id_str)
        .bind(idempotency_key)
        .fetch_one(tx.as_mut())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn update_tb_transfer_id_in_tx(
        &self,
        tx: &mut PgTransaction<'_, Postgres>,
        id: Uuid,
        tb_transfer_id: u128,
    ) -> Result<(), AppError> {
        let tb_id_str = tb_transfer_id.to_string();
        sqlx::query(
            r#"UPDATE transactions SET tb_transfer_id = $2::numeric, updated_at = now() WHERE id = $1"#,
        )
        .bind(id)
        .bind(&tb_id_str)
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        new_status: TransactionStatus,
    ) -> Result<TransactionRecord, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            UPDATE transactions SET status = $2, updated_at = now()
            WHERE id = $1
            RETURNING id, account_id, type, status, amount, pool, direction,
                      source_ifsc, source_account, gateway_ref, timeout_seconds,
                      merchant_id, merchant_mcc, description,
                      tb_transfer_id::text as tb_transfer_id, idempotency_key,
                      created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(new_status.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::TransactionNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn get_by_id(
        &self,
        id: Uuid,
        account_id: Uuid,
    ) -> Result<TransactionRecord, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key,
                   created_at, updated_at
            FROM transactions
            WHERE id = $1 AND account_id = $2
            "#,
        )
        .bind(id)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::TransactionNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn find_by_idempotency_key(
        &self,
        account_id: Uuid,
        key: &str,
    ) -> Result<Option<TransactionRecord>, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key,
                   created_at, updated_at
            FROM transactions
            WHERE account_id = $1 AND idempotency_key = $2
            "#,
        )
        .bind(account_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_domain()))
    }

    pub async fn list_by_account(
        &self,
        account_id: Uuid,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key,
                   created_at, updated_at
            FROM transactions
            WHERE account_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(account_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn count_by_account(&self, account_id: Uuid) -> Result<i64, AppError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM transactions WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn list_pending_by_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key,
                   created_at, updated_at
            FROM transactions
            WHERE account_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            "#,
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn find_timed_out_pending(&self) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key,
                   created_at, updated_at
            FROM transactions
            WHERE status = 'pending'
              AND timeout_seconds IS NOT NULL
              AND created_at + timeout_seconds * interval '1 second' < now()
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }
}
```

- [ ] **Step 2: Update repository module declarations**

In the repository module file, replace `pub mod deposit_repo;` with `pub mod transaction_repo;`. Keep `pub mod account_repo;` and `pub mod ledger_repo;`.

- [ ] **Step 3: Delete old deposit_repo.rs**

```bash
rm crates/pba_service/src/repository/deposit_repo.rs
```

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/repository/
git commit -m "feat: add transaction_repo, remove deposit_repo"
```

---

### Task 5: Error Updates

**Files:**
- Modify: `crates/pba_service/src/error.rs`

- [ ] **Step 1: Rename error variants**

In `crates/pba_service/src/error.rs`, rename:
- `DepositNotFound(String)` → `TransactionNotFound(String)`
- `DepositNotPending(String)` → `TransactionNotPending(String)`

Update the `Display` impl:
```rust
Self::TransactionNotFound(id) => write!(f, "Transaction not found: {id}"),
Self::TransactionNotPending(id) => write!(f, "Transaction is not in pending state: {id}"),
```

Update the `IntoResponse` impl:
```rust
AppError::TransactionNotFound(_) => (StatusCode::NOT_FOUND, "TransactionNotFound"),
AppError::TransactionNotPending(_) => (StatusCode::CONFLICT, "TransactionNotPending"),
```

- [ ] **Step 2: Commit**

```bash
git add crates/pba_service/src/error.rs
git commit -m "refactor: rename deposit error variants to transaction"
```

---

### Task 6: Refactor DepositService

**Files:**
- Modify: `crates/pba_service/src/service/deposit_service.rs`

- [ ] **Step 1: Rewrite deposit_service.rs**

Replace the entire file with:

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::transaction::{TransactionDirection, TransactionRecord, TransactionStatus, TransactionType};
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::{LedgerRepo, FUNDING_SOURCE_TB_ID};
use crate::repository::transaction_repo::TransactionRepo;

const DEPOSIT_TRANSFER_CODE: u16 = 100;
const PENDING_DEPOSIT_TRANSFER_CODE: u16 = 101;

pub struct DepositService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
}

impl DepositService {
    pub fn new(
        account_repo: Arc<AccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
        }
    }

    pub async fn deposit(
        &self,
        account_id: Uuid,
        source_ifsc: &str,
        source_account_number: &str,
        amount: u64,
        pending: bool,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        idempotency_key: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        // Idempotency check
        if let Some(key) = idempotency_key {
            if let Some(existing) = self.transaction_repo.find_by_idempotency_key(account_id, key).await? {
                return Ok(existing);
            }
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        let is_self = account.is_origin_source(source_ifsc, source_account_number);
        let credit_tb_id = if is_self {
            account.tb_self_account_id
        } else {
            account.tb_others_account_id
        };
        let pool = if is_self { "self" } else { "others" };
        let deposit_id = Uuid::new_v4();

        let mut tx = self.transaction_repo.pool().begin().await?;

        if pending {
            let timeout = timeout_seconds.unwrap_or(self.default_timeout_seconds);

            // Insert PG row (status=pending, tb_transfer_id=0)
            let record = self.transaction_repo.insert_in_tx(
                &mut tx, deposit_id, account_id,
                TransactionType::Deposit, TransactionStatus::Pending,
                amount, pool, TransactionDirection::Inbound,
                Some(source_ifsc), Some(source_account_number),
                gateway_ref, Some(timeout),
                None, None, None, 0, idempotency_key,
            ).await?;

            // Create pending transfer in TigerBeetle
            let tb_transfer_id = self.ledger_repo.create_pending_transfer(
                FUNDING_SOURCE_TB_ID, credit_tb_id, amount,
                PENDING_DEPOSIT_TRANSFER_CODE, timeout,
            ).await.map_err(|e| {
                tracing::error!("TB pending transfer failed, rolling back: {e}");
                e
            })?;

            // Update with real TB transfer ID
            self.transaction_repo.update_tb_transfer_id_in_tx(&mut tx, deposit_id, tb_transfer_id).await?;

            tx.commit().await?;
            // Return record with updated tb_transfer_id
            Ok(TransactionRecord { tb_transfer_id, ..record })
        } else {
            // Insert PG row (status=posted)
            let record = self.transaction_repo.insert_in_tx(
                &mut tx, deposit_id, account_id,
                TransactionType::Deposit, TransactionStatus::Posted,
                amount, pool, TransactionDirection::Inbound,
                Some(source_ifsc), Some(source_account_number),
                gateway_ref, None,
                None, None, None, 0, idempotency_key,
            ).await?;

            // Execute TB transfer
            self.ledger_repo.create_transfer(
                FUNDING_SOURCE_TB_ID, credit_tb_id, amount, DEPOSIT_TRANSFER_CODE,
            ).await.map_err(|e| {
                tracing::error!("TB transfer failed, rolling back: {e}");
                e
            })?;

            tx.commit().await?;
            Ok(record)
        }
    }

    pub async fn post_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
    ) -> Result<TransactionRecord, AppError> {
        let txn = self.transaction_repo.get_by_id(deposit_id, account_id).await?;

        if txn.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(deposit_id.to_string()));
        }

        // Post in TigerBeetle
        self.ledger_repo.post_pending_transfer(txn.tb_transfer_id).await?;

        // Update PG
        let updated = self.transaction_repo.update_status(deposit_id, TransactionStatus::Posted).await?;

        tracing::info!(deposit_id = %deposit_id, account_id = %account_id, amount = txn.amount, "Pending deposit posted");
        Ok(updated)
    }

    pub async fn void_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
        _reason: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        let txn = self.transaction_repo.get_by_id(deposit_id, account_id).await?;

        if txn.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(deposit_id.to_string()));
        }

        // Void in TigerBeetle
        self.ledger_repo.void_pending_transfer(txn.tb_transfer_id).await?;

        // Update PG
        let updated = self.transaction_repo.update_status(deposit_id, TransactionStatus::Voided).await?;

        tracing::info!(deposit_id = %deposit_id, account_id = %account_id, amount = txn.amount, "Pending deposit voided");
        Ok(updated)
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/pba_service/src/service/deposit_service.rs
git commit -m "refactor: deposit service uses TransactionRepo with PG transactions"
```

---

### Task 7: Refactor PaymentService

**Files:**
- Modify: `crates/pba_service/src/service/payment_service.rs`

- [ ] **Step 1: Rewrite payment_service.rs**

Replace the entire file with:

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::pool::PaymentSplit;
use crate::domain::transaction::{TransactionDirection, TransactionRecord, TransactionStatus, TransactionType};
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::{LedgerRepo, MERCHANT_SETTLEMENT_TB_ID};
use crate::repository::transaction_repo::TransactionRepo;

const PAYMENT_TRANSFER_CODE: u16 = 200;
const MAX_SPLIT_RETRIES: u32 = 3;

pub struct PaymentService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}

impl PaymentService {
    pub fn new(
        account_repo: Arc<AccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            transaction_repo,
        }
    }

    pub async fn make_payment(
        &self,
        account_id: Uuid,
        amount: u64,
        merchant_mcc: &str,
        merchant_id: &str,
        description: &str,
        idempotency_key: Option<&str>,
    ) -> Result<PaymentResult, AppError> {
        // Idempotency check
        if let Some(key) = idempotency_key {
            if let Some(existing) = self.transaction_repo.find_by_idempotency_key(account_id, key).await? {
                // Reconstruct PaymentResult from existing records
                // For split payments, there may be two records with same idempotency key — return the first
                return Ok(PaymentResult {
                    account_id: existing.account_id,
                    amount: existing.amount,
                    from_others: if existing.pool == "others" { existing.amount } else { 0 },
                    from_self: if existing.pool == "self" { existing.amount } else { 0 },
                    merchant_id: existing.merchant_id.unwrap_or_default(),
                    merchant_mcc: existing.merchant_mcc.unwrap_or_default(),
                });
            }
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        // Validate MCC
        let mcc_allowed = self.account_repo.is_mcc_allowed(&account.purpose_code, merchant_mcc).await?;
        if !mcc_allowed {
            return Err(AppError::InvalidMcc {
                mcc: merchant_mcc.to_string(),
                purpose_code: account.purpose_code.clone(),
            });
        }

        // Retry loop for stale balance
        let mut last_err = None;
        for attempt in 0..MAX_SPLIT_RETRIES {
            if attempt > 0 {
                tracing::info!(account_id = %account_id, attempt, "Retrying payment with fresh balance");
            }

            let balance = self.ledger_repo
                .get_balance(account.tb_self_account_id, account.tb_others_account_id)
                .await?;

            let split = match PaymentSplit::calculate(&balance, amount) {
                Some(s) => s,
                None => {
                    return Err(AppError::InsufficientFunds {
                        requested: amount,
                        available: balance.total(),
                    });
                }
            };

            // Begin PG transaction
            let mut tx = self.transaction_repo.pool().begin().await?;

            // Insert transaction row(s)
            if split.from_others > 0 {
                self.transaction_repo.insert_in_tx(
                    &mut tx, Uuid::new_v4(), account_id,
                    TransactionType::Payment, TransactionStatus::Settled,
                    split.from_others, "others", TransactionDirection::Outbound,
                    None, None, None, None,
                    Some(merchant_id), Some(merchant_mcc), Some(description),
                    0, idempotency_key,
                ).await?;
            }
            if split.from_self > 0 {
                // For split payments, only first row gets the idempotency key (unique constraint)
                let idem_key = if split.from_others > 0 { None } else { idempotency_key };
                self.transaction_repo.insert_in_tx(
                    &mut tx, Uuid::new_v4(), account_id,
                    TransactionType::Payment, TransactionStatus::Settled,
                    split.from_self, "self", TransactionDirection::Outbound,
                    None, None, None, None,
                    Some(merchant_id), Some(merchant_mcc), Some(description),
                    0, idem_key,
                ).await?;
            }

            // Execute TB transfer(s)
            let tb_result = self.execute_transfer(&account, &split).await;

            match tb_result {
                Ok(()) => {
                    tx.commit().await?;
                    tracing::info!(
                        account_id = %account_id, merchant_id, merchant_mcc,
                        amount, from_others = split.from_others, from_self = split.from_self,
                        "Payment processed"
                    );
                    return Ok(PaymentResult {
                        account_id,
                        amount,
                        from_others: split.from_others,
                        from_self: split.from_self,
                        merchant_id: merchant_id.to_string(),
                        merchant_mcc: merchant_mcc.to_string(),
                    });
                }
                Err(AppError::ExceedsBalance) => {
                    // Rollback happens automatically when tx is dropped
                    last_err = Some(AppError::ExceedsBalance);
                }
                Err(e) => return Err(e),
            }
        }

        let balance = self.ledger_repo
            .get_balance(account.tb_self_account_id, account.tb_others_account_id)
            .await?;

        Err(last_err.unwrap_or(AppError::InsufficientFunds {
            requested: amount,
            available: balance.total(),
        }))
    }

    async fn execute_transfer(
        &self,
        account: &crate::domain::account::PurposeBoundAccount,
        split: &PaymentSplit,
    ) -> Result<(), AppError> {
        if split.from_others > 0 && split.from_self > 0 {
            self.ledger_repo.create_linked_transfers(
                account.tb_others_account_id, account.tb_self_account_id,
                MERCHANT_SETTLEMENT_TB_ID,
                split.from_others, split.from_self, PAYMENT_TRANSFER_CODE,
            ).await
        } else if split.from_others > 0 {
            self.ledger_repo.create_transfer(
                account.tb_others_account_id, MERCHANT_SETTLEMENT_TB_ID,
                split.from_others, PAYMENT_TRANSFER_CODE,
            ).await
        } else {
            self.ledger_repo.create_transfer(
                account.tb_self_account_id, MERCHANT_SETTLEMENT_TB_ID,
                split.from_self, PAYMENT_TRANSFER_CODE,
            ).await
        }
    }
}

pub struct PaymentResult {
    pub account_id: Uuid,
    pub amount: u64,
    pub from_others: u64,
    pub from_self: u64,
    pub merchant_id: String,
    pub merchant_mcc: String,
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/pba_service/src/service/payment_service.rs
git commit -m "refactor: payment service uses TransactionRepo with PG transactions"
```

---

### Task 8: Refactor WithdrawalService

**Files:**
- Modify: `crates/pba_service/src/service/withdrawal_service.rs`

- [ ] **Step 1: Rewrite withdrawal_service.rs**

Replace the entire file with:

```rust
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::transaction::{TransactionDirection, TransactionStatus, TransactionType};
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::{LedgerRepo, WITHDRAWAL_SETTLEMENT_TB_ID};
use crate::repository::transaction_repo::TransactionRepo;

const WITHDRAWAL_TRANSFER_CODE: u16 = 300;

pub struct WithdrawalService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}

impl WithdrawalService {
    pub fn new(
        account_repo: Arc<AccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            transaction_repo,
        }
    }

    pub async fn withdraw(
        &self,
        account_id: Uuid,
        amount: u64,
        idempotency_key: Option<&str>,
    ) -> Result<WithdrawalResult, AppError> {
        // Idempotency check
        if let Some(key) = idempotency_key {
            if let Some(existing) = self.transaction_repo.find_by_idempotency_key(account_id, key).await? {
                return Ok(WithdrawalResult {
                    account_id: existing.account_id,
                    amount: existing.amount,
                });
            }
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        let balance = self.ledger_repo
            .get_balance(account.tb_self_account_id, account.tb_others_account_id)
            .await?;

        if balance.self_contribution < amount {
            return Err(AppError::InsufficientFunds {
                requested: amount,
                available: balance.self_contribution,
            });
        }

        let mut tx = self.transaction_repo.pool().begin().await?;

        self.transaction_repo.insert_in_tx(
            &mut tx, Uuid::new_v4(), account_id,
            TransactionType::Withdrawal, TransactionStatus::Settled,
            amount, "self", TransactionDirection::Outbound,
            None, None, None, None,
            None, None, None, 0, idempotency_key,
        ).await?;

        self.ledger_repo.create_transfer(
            account.tb_self_account_id, WITHDRAWAL_SETTLEMENT_TB_ID,
            amount, WITHDRAWAL_TRANSFER_CODE,
        ).await.map_err(|e| {
            tracing::error!("TB withdrawal failed, rolling back: {e}");
            e
        })?;

        tx.commit().await?;

        Ok(WithdrawalResult { account_id, amount })
    }
}

pub struct WithdrawalResult {
    pub account_id: Uuid,
    pub amount: u64,
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/pba_service/src/service/withdrawal_service.rs
git commit -m "refactor: withdrawal service uses TransactionRepo with PG transactions"
```

---

### Task 9: Update deposit_timeout.rs

**Files:**
- Modify: `crates/pba_service/src/service/deposit_timeout.rs`

- [ ] **Step 1: Update to use TransactionRepo**

Replace the entire file with:

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::domain::transaction::TransactionStatus;
use crate::repository::transaction_repo::TransactionRepo;

pub async fn run_deposit_timeout_poller(
    transaction_repo: Arc<TransactionRepo>,
    poll_interval_seconds: u64,
) {
    let interval = Duration::from_secs(poll_interval_seconds);
    tracing::info!(poll_interval_seconds, "Starting deposit timeout poller");

    loop {
        tokio::time::sleep(interval).await;

        match transaction_repo.find_timed_out_pending().await {
            Ok(timed_out) => {
                for txn in timed_out {
                    match transaction_repo.update_status(txn.id, TransactionStatus::Voided).await {
                        Ok(_) => {
                            tracing::warn!(
                                transaction_id = %txn.id,
                                account_id = %txn.account_id,
                                gateway_ref = txn.gateway_ref.as_deref().unwrap_or("none"),
                                amount = txn.amount,
                                "Pending deposit timed out and voided"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                transaction_id = %txn.id,
                                error = %e,
                                "Failed to update timed-out deposit status"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to query timed-out deposits");
            }
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/pba_service/src/service/deposit_timeout.rs
git commit -m "refactor: deposit timeout poller uses TransactionRepo"
```

---

### Task 10: API Layer — DTOs, Handlers, Routes

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`
- Modify: `crates/pba_service/src/api/handlers.rs`
- Modify: `crates/pba_service/src/api/routes.rs`

- [ ] **Step 1: Update dto.rs**

Add to `DepositRequest`:
```rust
    pub idempotency_key: Option<String>,
```

Add to `PaymentRequest`:
```rust
    pub idempotency_key: Option<String>,
```

Add to `WithdrawalRequest`:
```rust
    pub idempotency_key: Option<String>,
```

Change `AccountResponse` timestamp fields from `String` to use `chrono::DateTime<chrono::Utc>` with serde:
```rust
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
```

Update the `From<PurposeBoundAccount>` impl to remove `.to_rfc3339()`:
```rust
            created_at: a.created_at,
            updated_at: a.updated_at,
```

Update `DepositResponse` to use `TransactionRecord`:
```rust
#[derive(Debug, Serialize)]
pub struct DepositResponse {
    pub deposit_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub pool: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
}
```

Add new DTOs at the end of the file:

```rust
// ── Transactions ──

#[derive(Debug, Deserialize)]
pub struct ListTransactionsQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListTransactionsResponse {
    pub transactions: Vec<TransactionSummaryDto>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct TransactionSummaryDto {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub status: String,
    pub amount: u64,
    pub pool: String,
    pub direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_mcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::domain::transaction::TransactionRecord> for TransactionSummaryDto {
    fn from(t: crate::domain::transaction::TransactionRecord) -> Self {
        Self {
            id: t.id,
            transaction_type: t.transaction_type.as_str().to_string(),
            status: t.status.as_str().to_string(),
            amount: t.amount,
            pool: t.pool,
            direction: t.direction.as_str().to_string(),
            description: t.description,
            merchant_id: t.merchant_id,
            merchant_mcc: t.merchant_mcc,
            source_ifsc: t.source_ifsc,
            source_account: t.source_account,
            gateway_ref: t.gateway_ref,
            created_at: t.created_at,
        }
    }
}
```

- [ ] **Step 2: Update API handlers**

In `crates/pba_service/src/api/handlers.rs`:

Update the `deposit` handler to pass `idempotency_key` and convert `TransactionRecord` to `DepositResponse`:

```rust
pub async fn deposit(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<DepositRequest>,
) -> Result<(axum::http::StatusCode, Json<DepositResponse>), AppError> {
    let result = state
        .deposit_service
        .deposit(
            account_id,
            &req.source_ifsc,
            &req.source_account_number,
            req.amount,
            req.pending,
            req.gateway_ref.as_deref(),
            req.timeout_seconds,
            req.idempotency_key.as_deref(),
        )
        .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(DepositResponse {
            deposit_id: result.id,
            account_id: result.account_id,
            amount: result.amount,
            pool: result.pool,
            status: result.status.as_str().to_string(),
            gateway_ref: result.gateway_ref,
            timeout_seconds: result.timeout_seconds,
        }),
    ))
}
```

Update `post_deposit` and `void_deposit` similarly (use `result.id` instead of `result.deposit_id`, `result.status.as_str()`, etc.).

Update `make_payment` to pass `idempotency_key`:
```rust
    let result = state
        .payment_service
        .make_payment(
            account_id,
            req.amount,
            &req.merchant_mcc,
            &req.merchant_id,
            &req.description,
            req.idempotency_key.as_deref(),
        )
        .await?;
```

Update `withdraw` to pass `idempotency_key`:
```rust
    let result = state
        .withdrawal_service
        .withdraw(account_id, req.amount, req.idempotency_key.as_deref())
        .await?;
```

Add new handler:

```rust
// ── Transactions ──

pub async fn list_transactions(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<ListTransactionsQuery>,
) -> Result<Json<ListTransactionsResponse>, AppError> {
    // Verify account exists
    let _ = state.account_service.get_account(account_id).await?;

    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let transactions = state.transaction_repo.list_by_account(account_id, offset, limit).await?;
    let total = state.transaction_repo.count_by_account(account_id).await?;

    Ok(Json(ListTransactionsResponse {
        transactions: transactions.into_iter().map(|t| t.into()).collect(),
        total,
        offset,
        limit,
    }))
}
```

- [ ] **Step 3: Add route**

In `crates/pba_service/src/api/routes.rs`, add after the withdrawal route:

```rust
        // Transactions
        .route(
            "/accounts/{account_id}/transactions",
            get(handlers::list_transactions),
        )
```

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/api/
git commit -m "feat: add ListTransactions API endpoint, idempotency_key to request DTOs"
```

---

### Task 11: Admin UI — Switch to TransactionRepo

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs`

- [ ] **Step 1: Update account_transfers_fragment**

Replace the `account_transfers_fragment` handler to query from `TransactionRepo` instead of `LedgerRepo`:

```rust
pub async fn account_transfers_fragment(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let transfers = match state
        .transaction_repo
        .list_by_account(account_id, 0, 100)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to get transactions: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let rows: Vec<TransferRow> = transfers
        .into_iter()
        .map(|t| TransferRow {
            timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            transfer_type: t.type_label().to_string(),
            direction: t.direction.label().to_string(),
            direction_class: t.direction.css_class().to_string(),
            pool: if t.pool == "self" { "Self" } else { "Others" }.to_string(),
            amount: t.amount_display(),
        })
        .collect();

    render(TransfersFragmentTemplate { transfers: rows })
}
```

- [ ] **Step 2: Update pending deposits in account_detail**

Update the pending deposits fetch in `account_detail` to use `state.transaction_repo.list_pending_by_account(account_id)` instead of `state.deposit_service.deposit_repo.list_pending_by_account(account_id)`.

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/src/admin/
git commit -m "refactor: admin UI uses TransactionRepo for history and pending deposits"
```

---

### Task 12: Wire Up main.rs

**Files:**
- Modify: `crates/pba_service/src/main.rs`

- [ ] **Step 1: Update main.rs**

Replace `DepositRepo` with `TransactionRepo`:

```rust
use repository::transaction_repo::TransactionRepo;
```

Remove:
```rust
use repository::deposit_repo::DepositRepo;
```

Update `AppState`:
```rust
#[derive(Clone)]
pub struct AppState {
    pub account_service: Arc<AccountService>,
    pub deposit_service: Arc<DepositService>,
    pub payment_service: Arc<PaymentService>,
    pub withdrawal_service: Arc<WithdrawalService>,
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}
```

Initialize `TransactionRepo`:
```rust
    let transaction_repo = Arc::new(TransactionRepo::new(pg_pool.clone()));
```

Update service construction:
```rust
    let deposit_service = Arc::new(DepositService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
        Arc::clone(&transaction_repo),
        config.deposit_timeout_seconds,
    ));
    let payment_service = Arc::new(PaymentService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
        Arc::clone(&transaction_repo),
    ));
    let withdrawal_service = Arc::new(WithdrawalService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
        Arc::clone(&transaction_repo),
    ));
```

Update state:
```rust
    let state = AppState {
        account_service,
        deposit_service,
        payment_service,
        withdrawal_service,
        account_repo,
        ledger_repo,
        transaction_repo: Arc::clone(&transaction_repo),
    };
```

Update timeout poller:
```rust
    tokio::spawn(service::deposit_timeout::run_deposit_timeout_poller(
        Arc::clone(&transaction_repo),
        config.deposit_poller_interval_seconds,
    ));
```

- [ ] **Step 2: Commit**

```bash
git add crates/pba_service/src/main.rs
git commit -m "refactor: wire TransactionRepo into AppState and all services"
```

---

### Task 13: Build and Fix Compilation

**Files:**
- Various — fix any remaining compilation errors

- [ ] **Step 1: Build**

```bash
cargo build 2>&1
```

Fix any remaining issues — likely:
- Remove unused imports of `domain::deposit`, `domain::transfer`, `deposit_repo`
- Update admin handler `process_deposit` to pass `idempotency_key: None`
- Update any test step definitions that reference `DepositStatus` or `deposit_repo`

- [ ] **Step 2: Run existing E2E tests**

```bash
just e2e
```

Expected: All 42 scenarios pass.

- [ ] **Step 3: Commit fixes**

```bash
git add -A
git commit -m "fix: resolve compilation issues after transaction repo migration"
```

---

### Task 14: Add Transaction List Cucumber Tests

**Files:**
- Create: `tests/features/transactions.feature`
- Modify: `tests/steps/` (add transaction list step definitions)

- [ ] **Step 1: Create transactions.feature**

Create `crates/pba_service/tests/features/transactions.feature`:

```gherkin
Feature: Transaction History
  Transaction history lists all deposits, payments, and withdrawals for an account.

  Scenario: Transaction list shows deposits and payments
    Given a "health" account exists for holder "t1t1t1t1-t1t1-t1t1-t1t1-t1t1t1t1t1t1" with origin IFSC "HDFC0091111" and account number "9111100001"
    When I deposit 5000 from IFSC "HDFC0091111" account "9111100001"
    And I deposit 3000 from IFSC "ICIC0009999" account "9876543210"
    And I make a payment of 1000 to merchant "MER001" with MCC "8011"
    Then the transaction list should have 4 entries
    And the transaction list should include a "deposit" with amount 5000
    And the transaction list should include a "payment" with amount 1000

  Scenario: Transaction list respects offset and limit
    Given a "health" account exists for holder "t2t2t2t2-t2t2-t2t2-t2t2-t2t2t2t2t2t2" with origin IFSC "HDFC0092222" and account number "9222200001"
    When I deposit 1000 from IFSC "HDFC0092222" account "9222200001"
    And I deposit 2000 from IFSC "HDFC0092222" account "9222200001"
    And I deposit 3000 from IFSC "HDFC0092222" account "9222200001"
    Then the transaction list with offset 0 and limit 2 should have 2 entries
    And the total transaction count should be 3

  @api
  Scenario: Idempotent deposit returns same result
    Given a "health" account exists for holder "t3t3t3t3-t3t3-t3t3-t3t3-t3t3t3t3t3t3" with origin IFSC "HDFC0093333" and account number "9333300001"
    When I deposit 5000 from IFSC "HDFC0093333" account "9333300001" with idempotency key "idem-dep-001"
    And I deposit 5000 from IFSC "HDFC0093333" account "9333300001" with idempotency key "idem-dep-001"
    Then the self contribution should be 5000
    And the transaction list should have 1 entries
```

- [ ] **Step 2: Add step definitions for transaction list**

Add to the appropriate step definition file (e.g., `tests/steps/transaction_steps.rs`):

```rust
use cucumber::then;
use crate::World;

#[then(regex = r"^the transaction list should have (\d+) entries$")]
async fn then_transaction_list_count(world: &mut World, count: usize) {
    let account_id = world.account_id.as_ref().expect("no account_id");
    let resp = world.client
        .get(&format!("{}/accounts/{}/transactions", world.base_url, account_id))
        .send()
        .await
        .expect("Failed to get transactions");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let txns = body["transactions"].as_array().unwrap();
    assert_eq!(txns.len(), count, "Expected {count} transactions, got {}", txns.len());
}

#[then(regex = r#"^the transaction list should include a "([^"]+)" with amount (\d+)$"#)]
async fn then_transaction_list_includes(world: &mut World, txn_type: String, amount: u64) {
    let account_id = world.account_id.as_ref().expect("no account_id");
    let resp = world.client
        .get(&format!("{}/accounts/{}/transactions", world.base_url, account_id))
        .send()
        .await
        .expect("Failed to get transactions");
    let body: serde_json::Value = resp.json().await.unwrap();
    let txns = body["transactions"].as_array().unwrap();
    let found = txns.iter().any(|t| {
        t["type"].as_str() == Some(&txn_type) && t["amount"].as_u64() == Some(amount)
    });
    assert!(found, "No {txn_type} transaction with amount {amount} found");
}

#[then(regex = r"^the transaction list with offset (\d+) and limit (\d+) should have (\d+) entries$")]
async fn then_transaction_list_paginated(world: &mut World, offset: i64, limit: i64, count: usize) {
    let account_id = world.account_id.as_ref().expect("no account_id");
    let resp = world.client
        .get(&format!(
            "{}/accounts/{}/transactions?offset={}&limit={}",
            world.base_url, account_id, offset, limit
        ))
        .send()
        .await
        .expect("Failed to get transactions");
    let body: serde_json::Value = resp.json().await.unwrap();
    let txns = body["transactions"].as_array().unwrap();
    assert_eq!(txns.len(), count);
}

#[then(regex = r"^the total transaction count should be (\d+)$")]
async fn then_total_count(world: &mut World, expected: i64) {
    let account_id = world.account_id.as_ref().expect("no account_id");
    let resp = world.client
        .get(&format!("{}/accounts/{}/transactions", world.base_url, account_id))
        .send()
        .await
        .expect("Failed to get transactions");
    let body: serde_json::Value = resp.json().await.unwrap();
    let total = body["total"].as_i64().unwrap();
    assert_eq!(total, expected);
}
```

- [ ] **Step 3: Run all tests**

```bash
just e2e
```

Expected: All scenarios pass including new transaction list tests.

- [ ] **Step 4: Commit**

```bash
git add tests/
git commit -m "test: add transaction list and idempotency Cucumber scenarios"
```

---

### Task 15: Run UI E2E Tests

- [ ] **Step 1: Run UI E2E tests**

```bash
just ui-e2e
```

Expected: All scenarios pass — the admin UI now reads from TransactionRepo.

- [ ] **Step 2: If failures, fix and commit**

Fix any issues with the admin UI handlers or templates and commit.

---

### Task 16: Clean Up Unused Code

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs` — remove `get_account_transfers` method
- Delete: any remaining references to `TransferRecord`, `TransferType`, etc.

- [ ] **Step 1: Remove get_account_transfers from ledger_repo**

Remove the `get_account_transfers` method and its related imports (`TransferRecord`, `TransferFlags` for the PENDING filter, `account::Filter`, `account::FilterFlags`).

Keep `create_transfer`, `create_pending_transfer`, `post_pending_transfer`, `void_pending_transfer`, `create_linked_transfers`, `get_balance`, `init_sentinel_accounts`.

- [ ] **Step 2: Remove domain/transfer.rs imports**

Remove any remaining `use crate::domain::transfer::*` imports across the codebase.

- [ ] **Step 3: Build and test**

```bash
cargo build && just e2e
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove unused TigerBeetle transfer history code"
```
