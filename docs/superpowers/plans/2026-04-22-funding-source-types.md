# Funding Source Types Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Differentiate deposit funding sources into self/trust/third-party with separate TigerBeetle sentinel accounts, Postgres tracking, API changes, and a system accounts admin page.

**Architecture:** Replace the single `FUNDING_SOURCE_TB_ID` with three sentinel accounts. Add a `funding_type` column to Postgres. Modify deposit flow to select sentinel based on origin match + user input. Add `/admin/system-accounts` page showing sentinel balances from TB and PBA pool balances from Postgres.

**Tech Stack:** Rust, Axum, TigerBeetle (tigerbeetle-unofficial), PostgreSQL (sqlx), Smithy SDK codegen, Askama templates, Cucumber BDD tests

---

### File Map

| Action | File | Purpose |
|--------|------|---------|
| Modify | `crates/pba_service/src/repository/ledger_repo.rs` | Replace FUNDING_SOURCE_TB_ID with 3 sentinels, add lookup_sentinel_accounts |
| Modify | `crates/pba_service/src/domain/transaction.rs` | Add funding_type field to TransactionRecord |
| Create | `crates/pba_service/src/db/migrations/20260422000001_add_funding_type.sql` | Add funding_type column |
| Modify | `crates/pba_service/src/repository/transaction_repo.rs` | Add funding_type to insert, select, TransactionRow, pool_summary_extended |
| Modify | `crates/pba_service/src/service/deposit_service.rs` | Accept funding_type param, select sentinel, validate |
| Modify | `crates/pba_service/src/api/dto.rs` | Add funding_type to DepositRequest, DepositResponse, TransactionSummaryDto |
| Modify | `crates/pba_service/src/api/handlers.rs` | Pass funding_type through deposit handler |
| Modify | `crates/pba_service/src/error.rs` | Add FundingTypeRequired error variant |
| Modify | `model/deposit.smithy` | Add funding_type to Deposit input/output |
| Modify | `model/common.smithy` | Add FundingType enum |
| Modify | `model/transaction.smithy` | Add funding_type to TransactionSummary |
| Modify | `crates/pba_service/src/admin/handlers.rs` | Add funding_type to AllTransactionRow, add system_accounts_page handler |
| Modify | `crates/pba_service/src/admin/mod.rs` | Add /admin/system-accounts route |
| Modify | `crates/pba_service/templates/admin/transactions.html` | Add Funding Type column |
| Create | `crates/pba_service/templates/admin/system_accounts.html` | System accounts page template |
| Modify | `crates/pba_service/templates/base.html` | Add System Accounts nav link |
| Create | `crates/pba_service/tests/features/funding_types.feature` | BDD scenarios for funding type logic |
| Modify | `crates/pba_service/tests/steps/deposit_steps.rs` | Add funding_type step definitions |
| Modify | `crates/pba_service/tests/e2e.rs` | Add last_funding_type to PbaWorld |
| Modify | `crates/pba_service/tests/ui_features/admin_ui.feature` | Add system accounts UI scenario |
| Modify | `crates/pba_service/tests/ui_steps/account_steps.rs` | Add system accounts page steps |

---

### Task 1: Postgres Migration — Add funding_type Column

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260422000001_add_funding_type.sql`

- [ ] **Step 1: Create the migration file**

```sql
ALTER TABLE transactions ADD COLUMN funding_type TEXT;
```

- [ ] **Step 2: Verify migration compiles**

Run: `cargo build -p pba-service`
Expected: Build succeeds (migration is just a SQL file, no Rust changes yet)

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/src/db/migrations/20260422000001_add_funding_type.sql
git commit -m "feat: add funding_type column to transactions table"
```

---

### Task 2: TigerBeetle Sentinel Accounts — Replace Single with Three

**Files:**
- Modify: `crates/pba_service/src/repository/ledger_repo.rs:9-62`

- [ ] **Step 1: Replace sentinel constants**

In `crates/pba_service/src/repository/ledger_repo.rs`, replace lines 9-13:

```rust
/// Sentinel account IDs used as counterparties for deposits, payments, and withdrawals.
/// TigerBeetle disallows 0 and u128::MAX, so we use a fixed range that won't collide with UUID-derived IDs.
pub const SELF_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 10;
pub const MERCHANT_SETTLEMENT_TB_ID: u128 = u128::MAX - 11;
pub const WITHDRAWAL_SETTLEMENT_TB_ID: u128 = u128::MAX - 12;
pub const TRUST_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 13;
pub const THIRD_PARTY_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 14;
```

- [ ] **Step 2: Update init_sentinel_accounts to create all 5**

Replace the `init_sentinel_accounts` method (lines 39-62):

```rust
/// Create sentinel accounts that serve as counterparties for deposits, payments, and withdrawals.
/// These are idempotent — TigerBeetle returns `Exists` for already-created accounts which we ignore.
pub async fn init_sentinel_accounts(&self) -> Result<(), AppError> {
    let self_funding =
        tb::Account::new(SELF_FUNDING_SOURCE_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
    let merchant =
        tb::Account::new(MERCHANT_SETTLEMENT_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
    let withdrawal =
        tb::Account::new(WITHDRAWAL_SETTLEMENT_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
    let trust =
        tb::Account::new(TRUST_FUNDING_SOURCE_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
    let third_party =
        tb::Account::new(THIRD_PARTY_FUNDING_SOURCE_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL);

    match self
        .client
        .create_accounts(vec![self_funding, merchant, withdrawal, trust, third_party])
        .await
    {
        Ok(_) => {
            tracing::info!(
                "Created sentinel TB accounts (self_funding, merchant, withdrawal, trust, third_party)"
            );
        }
        Err(e) => {
            tracing::warn!("Sentinel account creation returned: {e:?} (may already exist)");
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Add lookup_sentinel_accounts method**

Add this method to the `impl LedgerRepo` block, after `init_sentinel_accounts`:

```rust
/// Look up all sentinel accounts and return their balances.
/// Returns a Vec of (name, credits_posted, debits_posted, credits_pending, debits_pending).
pub async fn lookup_sentinel_accounts(
    &self,
) -> Result<Vec<(String, u64, u64, u64, u64)>, AppError> {
    let ids = vec![
        SELF_FUNDING_SOURCE_TB_ID,
        TRUST_FUNDING_SOURCE_TB_ID,
        THIRD_PARTY_FUNDING_SOURCE_TB_ID,
        MERCHANT_SETTLEMENT_TB_ID,
        WITHDRAWAL_SETTLEMENT_TB_ID,
    ];
    let names = [
        "Self Funding Source",
        "Trust Funding Source",
        "Third Party Funding Source",
        "Merchant Settlement",
        "Withdrawal Settlement",
    ];

    let accounts = self
        .client
        .lookup_accounts(ids)
        .await
        .map_err(|e| AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}")))?;

    let mut results: Vec<(String, u64, u64, u64, u64)> = names
        .iter()
        .map(|n| (n.to_string(), 0, 0, 0, 0))
        .collect();

    for account in &accounts {
        let idx = match account.id() {
            id if id == SELF_FUNDING_SOURCE_TB_ID => 0,
            id if id == TRUST_FUNDING_SOURCE_TB_ID => 1,
            id if id == THIRD_PARTY_FUNDING_SOURCE_TB_ID => 2,
            id if id == MERCHANT_SETTLEMENT_TB_ID => 3,
            id if id == WITHDRAWAL_SETTLEMENT_TB_ID => 4,
            _ => continue,
        };
        results[idx] = (
            results[idx].0.clone(),
            u64::try_from(account.credits_posted()).unwrap_or(u64::MAX),
            u64::try_from(account.debits_posted()).unwrap_or(u64::MAX),
            u64::try_from(account.credits_pending()).unwrap_or(u64::MAX),
            u64::try_from(account.debits_pending()).unwrap_or(u64::MAX),
        );
    }

    Ok(results)
}
```

- [ ] **Step 4: Fix the import in deposit_service.rs**

In `crates/pba_service/src/service/deposit_service.rs`, line 9, change:

```rust
use crate::repository::ledger_repo::{LedgerRepo, FUNDING_SOURCE_TB_ID};
```

to:

```rust
use crate::repository::ledger_repo::{
    LedgerRepo, SELF_FUNDING_SOURCE_TB_ID, THIRD_PARTY_FUNDING_SOURCE_TB_ID,
    TRUST_FUNDING_SOURCE_TB_ID,
};
```

(The actual usage change comes in Task 4, but update the import now to keep it compiling — temporarily use `SELF_FUNDING_SOURCE_TB_ID` wherever `FUNDING_SOURCE_TB_ID` was used.)

- [ ] **Step 5: Replace FUNDING_SOURCE_TB_ID usage in deposit_service.rs**

In `crates/pba_service/src/service/deposit_service.rs`, replace both occurrences of `FUNDING_SOURCE_TB_ID` (lines 108 and 159) with `SELF_FUNDING_SOURCE_TB_ID`.

- [ ] **Step 6: Verify build**

Run: `cargo build -p pba-service`
Expected: Build succeeds

- [ ] **Step 7: Commit**

```bash
git add crates/pba_service/src/repository/ledger_repo.rs crates/pba_service/src/service/deposit_service.rs
git commit -m "feat: replace single funding source with three sentinel accounts"
```

---

### Task 3: Domain & Repository — Add funding_type to TransactionRecord

**Files:**
- Modify: `crates/pba_service/src/domain/transaction.rs:96-116`
- Modify: `crates/pba_service/src/repository/transaction_repo.rs:38-154`

- [ ] **Step 1: Add funding_type to TransactionRecord**

In `crates/pba_service/src/domain/transaction.rs`, add `funding_type` field after `description` (line 111):

```rust
pub funding_type: Option<String>,
```

So the struct becomes:

```rust
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
    pub funding_type: Option<String>,
    pub tb_transfer_id: u128,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Add funding_type to TransactionRow**

In `crates/pba_service/src/repository/transaction_repo.rs`, add to `TransactionRow` struct (after `description`, line 54):

```rust
funding_type: Option<String>,
```

- [ ] **Step 3: Update into_domain to map funding_type**

In `crates/pba_service/src/repository/transaction_repo.rs`, in `into_domain()` (line 62-85), add after `description: self.description,`:

```rust
funding_type: self.funding_type,
```

- [ ] **Step 4: Update insert_in_tx to accept and store funding_type**

In `crates/pba_service/src/repository/transaction_repo.rs`, modify the `insert_in_tx` method:

Add `funding_type: Option<&str>,` parameter after `description: Option<&str>,` (after line 114).

Update the SQL query to include `funding_type`:

```rust
let row = sqlx::query_as::<_, TransactionRow>(
    r#"
    INSERT INTO transactions (id, account_id, type, status, amount, pool, direction,
                              source_ifsc, source_account, gateway_ref, timeout_seconds,
                              merchant_id, merchant_mcc, description, funding_type,
                              tb_transfer_id, idempotency_key)
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16::numeric, $17)
    RETURNING id, account_id, type, status, amount, pool, direction,
              source_ifsc, source_account, gateway_ref, timeout_seconds,
              merchant_id, merchant_mcc, description, funding_type,
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
.bind(funding_type)
.bind(&tb_id_str)
.bind(idempotency_key)
.fetch_one(tx.as_mut())
.await
.map_err(|e| AppError::DatabaseError(e.to_string()))?;
```

- [ ] **Step 5: Update all SELECT queries to include funding_type**

In every query that selects from `transactions` and maps to `TransactionRow`, add `funding_type` to the column list. The affected methods are:

- `get_by_id` (~line 198)
- `find_by_idempotency_key` (~line 223)
- `list_by_account` (~line 247)
- `list_all` (~line 304)
- `update_status` (the RETURNING clause)
- `list_pending_by_account` (~line 383)
- `find_timed_out_pending` (~line 406)

For each, add `funding_type,` after `description,` in both the SELECT and RETURNING column lists.

- [ ] **Step 6: Update all callers of insert_in_tx to pass funding_type**

In `crates/pba_service/src/service/deposit_service.rs`, both calls to `insert_in_tx` (pending and posted paths) need the new `funding_type` parameter. For now, pass `None` — the actual value will be wired in Task 4.

Add `None, // funding_type` after the `description` argument (which is also `None`) in both calls (lines ~98 and ~150).

In `crates/pba_service/src/service/payment_service.rs`, find the `insert_in_tx` call and add `None, // funding_type` after the `description` argument.

In `crates/pba_service/src/service/withdrawal_service.rs`, find the `insert_in_tx` call and add `None, // funding_type` after the `description` argument.

- [ ] **Step 7: Verify build**

Run: `cargo build -p pba-service`
Expected: Build succeeds

- [ ] **Step 8: Commit**

```bash
git add crates/pba_service/src/domain/transaction.rs crates/pba_service/src/repository/transaction_repo.rs crates/pba_service/src/service/
git commit -m "feat: add funding_type to domain model and repository layer"
```

---

### Task 4: Deposit Service — Funding Type Logic

**Files:**
- Modify: `crates/pba_service/src/service/deposit_service.rs`
- Modify: `crates/pba_service/src/error.rs`

- [ ] **Step 1: Add FundingTypeRequired error variant**

In `crates/pba_service/src/error.rs`, add to the `AppError` enum:

```rust
FundingTypeRequired,
```

Add the Display match arm:

```rust
Self::FundingTypeRequired => write!(f, "funding_type is required for non-origin deposits (must be 'trust' or 'third_party')"),
```

Add the IntoResponse match arm:

```rust
AppError::FundingTypeRequired => (StatusCode::BAD_REQUEST, "FundingTypeRequired"),
```

- [ ] **Step 2: Update deposit method signature**

In `crates/pba_service/src/service/deposit_service.rs`, add `funding_type: Option<&str>,` parameter to the `deposit` method, after `source_account_number: &str,`:

```rust
#[allow(clippy::too_many_arguments)]
pub async fn deposit(
    &self,
    account_id: Uuid,
    source_ifsc: &str,
    source_account_number: &str,
    funding_type: Option<&str>,
    amount: u64,
    pending: bool,
    gateway_ref: Option<&str>,
    timeout_seconds: Option<u32>,
    idempotency_key: Option<&str>,
) -> Result<TransactionRecord, AppError> {
```

- [ ] **Step 3: Replace pool determination logic with funding source selection**

Replace the pool determination block (lines 66-72) with:

```rust
let is_self = account.is_origin_source(source_ifsc, source_account_number);

let (pool, resolved_funding_type, debit_sentinel) = if is_self {
    ("self", "self", SELF_FUNDING_SOURCE_TB_ID)
} else {
    match funding_type {
        Some("trust") => ("others", "trust", TRUST_FUNDING_SOURCE_TB_ID),
        Some("third_party") => ("others", "third_party", THIRD_PARTY_FUNDING_SOURCE_TB_ID),
        _ => return Err(AppError::FundingTypeRequired),
    }
};

let credit_tb_id = if is_self {
    account.tb_self_account_id
} else {
    account.tb_others_account_id
};
```

- [ ] **Step 4: Replace SELF_FUNDING_SOURCE_TB_ID with debit_sentinel in TB calls**

In both the pending and posted paths, replace `SELF_FUNDING_SOURCE_TB_ID` with `debit_sentinel`:

Pending path (~line 108):
```rust
.create_pending_transfer(
    debit_sentinel,
    credit_tb_id,
```

Posted path (~line 159):
```rust
.create_transfer(
    debit_sentinel,
    credit_tb_id,
```

- [ ] **Step 5: Pass funding_type to insert_in_tx**

In both calls to `insert_in_tx`, replace `None, // funding_type` with `Some(resolved_funding_type),`.

- [ ] **Step 6: Update the API handler to pass funding_type**

In `crates/pba_service/src/api/handlers.rs`, update the `deposit` handler (~line 80-92) to pass `req.funding_type.as_deref()`:

```rust
let result = state
    .deposit_service
    .deposit(
        account_id,
        &req.source_ifsc,
        &req.source_account_number,
        req.funding_type.as_deref(),
        req.amount,
        req.pending,
        req.gateway_ref.as_deref(),
        req.timeout_seconds,
        req.idempotency_key.as_deref(),
    )
    .await?;
```

- [ ] **Step 7: Add funding_type to DepositRequest and DepositResponse**

In `crates/pba_service/src/api/dto.rs`:

Add to `DepositRequest` (after `source_account_number`):

```rust
pub funding_type: Option<String>,
```

Add to `DepositResponse` (after `pool`):

```rust
pub funding_type: String,
```

- [ ] **Step 8: Update deposit handler response to include funding_type**

In `crates/pba_service/src/api/handlers.rs`, in the deposit handler response (~line 96-104), add:

```rust
funding_type: result.funding_type.unwrap_or_default(),
```

- [ ] **Step 9: Add funding_type to TransactionSummaryDto**

In `crates/pba_service/src/api/dto.rs`, add to `TransactionSummaryDto` (after `gateway_ref`):

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub funding_type: Option<String>,
```

Update the `From<TransactionRecord>` impl to include:

```rust
funding_type: t.funding_type,
```

- [ ] **Step 10: Update admin deposit form handler**

In `crates/pba_service/src/admin/handlers.rs`, find the `process_deposit` handler. It calls `deposit_service.deposit(...)`. Add `None,` for the `funding_type` parameter in the correct position (after `source_account_number`, before `amount`). The admin deposit form doesn't support funding_type selection yet — self-deposits from admin always match origin.

- [ ] **Step 11: Verify build**

Run: `cargo build -p pba-service`
Expected: Build succeeds

- [ ] **Step 12: Commit**

```bash
git add crates/pba_service/src/service/deposit_service.rs crates/pba_service/src/error.rs crates/pba_service/src/api/dto.rs crates/pba_service/src/api/handlers.rs crates/pba_service/src/admin/handlers.rs
git commit -m "feat: implement funding type selection in deposit flow"
```

---

### Task 5: Smithy Model & SDK Regeneration

**Files:**
- Modify: `model/common.smithy`
- Modify: `model/deposit.smithy`
- Modify: `model/transaction.smithy`

- [ ] **Step 1: Add FundingType enum to common.smithy**

In `model/common.smithy`, add after the `TransactionDirection` definition (~line 40):

```
/// Funding source type for deposits.
@enum([
    { value: "trust", name: "TRUST" },
    { value: "third_party", name: "THIRD_PARTY" },
])
string FundingType
```

- [ ] **Step 2: Add funding_type to Deposit input and output**

In `model/deposit.smithy`, add to the Deposit input (after `source_account_number`):

```
        funding_type: FundingType
```

Add to the Deposit output (after `pool`):

```
        @required
        funding_type: String
```

Also add `funding_type: String` to PostDeposit output and VoidDeposit output (same position, after `pool`), since they return the same shape.

- [ ] **Step 3: Add funding_type to TransactionSummary**

In `model/transaction.smithy`, add to the `TransactionSummary` structure (after `gateway_ref`, line 104):

```
    funding_type: String
```

- [ ] **Step 4: Regenerate SDK**

Run: `just smithy-build`
Expected: SDK regenerated at `crates/pba_client/`

- [ ] **Step 5: Verify build with new SDK**

Run: `cargo build -p pba-service`
Expected: Build succeeds

- [ ] **Step 6: Commit**

```bash
git add model/ crates/pba_client/ crates/pba_service/src/api/openapi.json
git commit -m "feat: add funding_type to Smithy model and regenerate SDK"
```

---

### Task 6: Admin UI — Funding Type Column and System Accounts Page

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs`
- Modify: `crates/pba_service/src/admin/mod.rs`
- Modify: `crates/pba_service/templates/admin/transactions.html`
- Create: `crates/pba_service/templates/admin/system_accounts.html`
- Modify: `crates/pba_service/templates/base.html`

- [ ] **Step 1: Add funding_type to AllTransactionRow and transactions_page**

In `crates/pba_service/src/admin/handlers.rs`, add to `AllTransactionRow` struct (~line 609-620):

```rust
funding_type: String,
```

In the `transactions_page` handler, in the map closure (~line 673-684), add:

```rust
funding_type: t.funding_type.as_deref().unwrap_or("—").to_string(),
```

- [ ] **Step 2: Add Funding Type column to transactions.html**

In `crates/pba_service/templates/admin/transactions.html`, add `<th>Funding Type</th>` after the `<th>Pool</th>` header (~line 41).

Add `<td>{{ t.funding_type }}</td>` after the pool `<td>` in the body row (~line 53).

- [ ] **Step 3: Create system_accounts.html template**

Create `crates/pba_service/templates/admin/system_accounts.html`:

```html
{% extends "base.html" %}

{% block title %}System Accounts - PBA Admin{% endblock %}

{% block content %}
<h1>System Accounts</h1>

<h2>Sentinel Accounts (TigerBeetle)</h2>
{% if sentinel_accounts.is_empty() %}
<p>No sentinel accounts found.</p>
{% else %}
<table>
    <thead>
        <tr>
            <th>Account</th>
            <th>Credits Posted</th>
            <th>Debits Posted</th>
            <th>Credits Pending</th>
            <th>Debits Pending</th>
        </tr>
    </thead>
    <tbody>
        {% for a in &sentinel_accounts %}
        <tr>
            <td><strong>{{ a.name }}</strong></td>
            <td class="inbound">{{ a.credits_posted }}</td>
            <td class="outbound">{{ a.debits_posted }}</td>
            <td class="status-frozen">{{ a.credits_pending }}</td>
            <td class="status-frozen">{{ a.debits_pending }}</td>
        </tr>
        {% endfor %}
    </tbody>
</table>
{% endif %}

<h2>PBA Pool Balances (Postgres)</h2>
<table>
    <thead>
        <tr>
            <th>Pool</th>
            <th>Credits Posted</th>
            <th>Debits Posted</th>
            <th>Credits Pending</th>
            <th>Debits Pending</th>
        </tr>
    </thead>
    <tbody>
        {% for p in &pool_balances %}
        <tr>
            <td><strong>{{ p.name }}</strong></td>
            <td class="inbound">{{ p.credits_posted }}</td>
            <td class="outbound">{{ p.debits_posted }}</td>
            <td class="status-frozen">{{ p.credits_pending }}</td>
            <td class="status-frozen">{{ p.debits_pending }}</td>
        </tr>
        {% endfor %}
    </tbody>
</table>
{% endblock %}
```

- [ ] **Step 4: Add system_accounts_page handler**

In `crates/pba_service/src/admin/handlers.rs`, add the structs and handler:

```rust
#[derive(Template)]
#[template(path = "admin/system_accounts.html")]
struct SystemAccountsTemplate {
    sentinel_accounts: Vec<SentinelAccountRow>,
    pool_balances: Vec<PoolBalanceRow>,
}

struct SentinelAccountRow {
    name: String,
    credits_posted: String,
    debits_posted: String,
    credits_pending: String,
    debits_pending: String,
}

struct PoolBalanceRow {
    name: String,
    credits_posted: String,
    debits_posted: String,
    credits_pending: String,
    debits_pending: String,
}

pub async fn system_accounts_page(State(state): State<AppState>) -> Response {
    let fmt = |amt: u64| format!("{}.{:02}", amt / 100, amt % 100);

    // Sentinel accounts from TigerBeetle
    let sentinel_accounts = match state.ledger_repo.lookup_sentinel_accounts().await {
        Ok(accounts) => accounts
            .into_iter()
            .map(|(name, cp, dp, cpend, dpend)| SentinelAccountRow {
                name,
                credits_posted: fmt(cp),
                debits_posted: fmt(dp),
                credits_pending: fmt(cpend),
                debits_pending: fmt(dpend),
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to lookup sentinel accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "TigerBeetle error").into_response();
        }
    };

    // PBA pool balances from Postgres
    let pool_summary = match state.transaction_repo.pool_summary_extended().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get pool summary: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let pool_balances = vec![
        PoolBalanceRow {
            name: "Self Pool (all accounts)".to_string(),
            credits_posted: fmt(pool_summary.self_inbound),
            debits_posted: fmt(pool_summary.self_outbound),
            credits_pending: fmt(pool_summary.pending_self_inbound),
            debits_pending: fmt(pool_summary.pending_self_outbound),
        },
        PoolBalanceRow {
            name: "Others Pool (all accounts)".to_string(),
            credits_posted: fmt(pool_summary.others_inbound),
            debits_posted: fmt(pool_summary.others_outbound),
            credits_pending: fmt(pool_summary.pending_others_inbound),
            debits_pending: fmt(pool_summary.pending_others_outbound),
        },
    ];

    render(SystemAccountsTemplate {
        sentinel_accounts,
        pool_balances,
    })
}
```

- [ ] **Step 5: Add pool_summary_extended to transaction_repo**

In `crates/pba_service/src/repository/transaction_repo.rs`, add a new struct and method:

```rust
#[derive(Debug, Default)]
pub struct PoolSummaryExtended {
    pub self_inbound: u64,
    pub self_outbound: u64,
    pub others_inbound: u64,
    pub others_outbound: u64,
    pub pending_self_inbound: u64,
    pub pending_self_outbound: u64,
    pub pending_others_inbound: u64,
    pub pending_others_outbound: u64,
}
```

```rust
pub async fn pool_summary_extended(&self) -> Result<PoolSummaryExtended, AppError> {
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        r#"
        SELECT pool, direction, status, COALESCE(SUM(amount), 0)::bigint AS total
        FROM transactions
        WHERE status IN ('posted', 'settled', 'pending')
        GROUP BY pool, direction, status
        "#,
    )
    .fetch_all(&self.pool)
    .await?;

    let mut summary = PoolSummaryExtended::default();
    for (pool, direction, status, total) in rows {
        let amt = total as u64;
        match (pool.as_str(), direction.as_str(), status.as_str()) {
            ("self", "inbound", "posted" | "settled") => summary.self_inbound += amt,
            ("self", "outbound", "posted" | "settled") => summary.self_outbound += amt,
            ("others", "inbound", "posted" | "settled") => summary.others_inbound += amt,
            ("others", "outbound", "posted" | "settled") => summary.others_outbound += amt,
            ("self", "inbound", "pending") => summary.pending_self_inbound += amt,
            ("self", "outbound", "pending") => summary.pending_self_outbound += amt,
            ("others", "inbound", "pending") => summary.pending_others_inbound += amt,
            ("others", "outbound", "pending") => summary.pending_others_outbound += amt,
            _ => {}
        }
    }
    Ok(summary)
}
```

- [ ] **Step 6: Add route and nav link**

In `crates/pba_service/src/admin/mod.rs`, add:

```rust
.route("/admin/system-accounts", get(handlers::system_accounts_page))
```

In `crates/pba_service/templates/base.html`, add after the "All Transactions" link (~line 31):

```html
<li><a href="/admin/system-accounts">System Accounts</a></li>
```

- [ ] **Step 7: Verify build**

Run: `cargo build -p pba-service`
Expected: Build succeeds

- [ ] **Step 8: Commit**

```bash
git add crates/pba_service/src/admin/ crates/pba_service/templates/ crates/pba_service/src/repository/transaction_repo.rs
git commit -m "feat: add funding type column and system accounts admin page"
```

---

### Task 7: E2E Tests — Funding Type API Scenarios

**Files:**
- Create: `crates/pba_service/tests/features/funding_types.feature`
- Modify: `crates/pba_service/tests/steps/deposit_steps.rs`
- Modify: `crates/pba_service/tests/e2e.rs`
- Modify: `crates/pba_service/tests/steps/mod.rs`

- [ ] **Step 1: Add last_funding_type to PbaWorld**

In `crates/pba_service/tests/e2e.rs`, add to `PbaWorld` struct:

```rust
/// Last deposit funding type
last_funding_type: Option<String>,
```

And in `Default` impl, add:

```rust
last_funding_type: None,
```

- [ ] **Step 2: Create funding_types.feature**

Create `crates/pba_service/tests/features/funding_types.feature`:

```gherkin
Feature: Funding Source Types
  Deposits are classified by funding source type: self, trust, or third_party.
  Self-deposits are auto-detected when the source IFSC/account matches the account origin.
  Non-origin deposits must specify funding_type as "trust" or "third_party".

  Scenario: Self-deposit auto-detected from origin bank
    Given a "health" account exists for holder "f4444444-4444-4444-4444-444444444444" with origin IFSC "HDFC0094444" and account number "9444400001"
    When I deposit 5000 from IFSC "HDFC0094444" account "9444400001"
    Then the deposit should go to "self" pool
    And the funding type should be "self"

  Scenario: Trust deposit from non-origin source
    Given a "health" account exists for holder "f5555555-5555-5555-5555-555555555555" with origin IFSC "HDFC0095555" and account number "9555500001"
    When I deposit 10000 from IFSC "ICIC0001234" account "1234567890" with funding type "trust"
    Then the deposit should go to "others" pool
    And the funding type should be "trust"

  Scenario: Third-party deposit from non-origin source
    Given a "health" account exists for holder "f6666666-6666-6666-6666-666666666666" with origin IFSC "HDFC0096666" and account number "9666600001"
    When I deposit 3000 from IFSC "SBIN0005678" account "5678901234" with funding type "third_party"
    Then the deposit should go to "others" pool
    And the funding type should be "third_party"

  Scenario: Non-origin deposit without funding type is rejected
    Given a "health" account exists for holder "f7777777-7777-7777-7777-777777777777" with origin IFSC "HDFC0097777" and account number "9777700001"
    When I attempt to deposit 2000 from IFSC "ICIC0009999" account "9999999999" without funding type
    Then the operation should be rejected

  Scenario: Transactions listing includes funding type
    Given a "health" account exists for holder "f8888888-8888-8888-8888-888888888888" with origin IFSC "HDFC0098888" and account number "9888800001"
    When I deposit 5000 from IFSC "HDFC0098888" account "9888800001"
    And I deposit 3000 from IFSC "ICIC0001111" account "1111111111" with funding type "trust"
    And I list all transactions
    Then the transactions list should contain a funding type "self"
    And the transactions list should contain a funding type "trust"
```

- [ ] **Step 3: Add funding type step definitions**

In `crates/pba_service/tests/steps/deposit_steps.rs`, add new step definitions:

```rust
#[when(
    regex = r#"^I deposit (\d+) from IFSC "([^"]*)" account "([^"]*)" with funding type "([^"]*)"$"#
)]
async fn deposit_with_funding_type(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    funding_type: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .funding_type(pba_client::types::FundingType::from(funding_type.as_str()))
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_funding_type = Some(output.funding_type().to_string());
        }
        Err(e) => panic!("Deposit with funding type failed: {e:?}"),
    }
}

#[when(
    regex = r#"^I attempt to deposit (\d+) from IFSC "([^"]*)" account "([^"]*)" without funding type$"#
)]
async fn attempt_deposit_without_funding_type(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "funding_type_required".into(),
            });
        }
    }
}

#[then(regex = r#"^the funding type should be "([^"]*)"$"#)]
async fn funding_type_should_be(world: &mut PbaWorld, expected: String) {
    // For self-deposits, the funding type is returned in the response
    // For non-self deposits, we stored it in last_funding_type
    if let Some(ft) = &world.last_funding_type {
        assert_eq!(ft, &expected, "Expected funding type '{expected}', got '{ft}'");
    } else {
        // Check from last deposit — self-deposits set pool but might not set funding_type yet
        // Re-read via deposit response if needed
        panic!("No funding type recorded");
    }
}
```

- [ ] **Step 4: Update existing deposit step to capture funding_type**

In `crates/pba_service/tests/steps/deposit_steps.rs`, update the existing `deposit` step (~line 47-66) to also capture `funding_type`:

```rust
#[when(regex = r#"^I deposit (\d+) from IFSC "([^"]*)" account "([^"]*)"$"#)]
async fn deposit(world: &mut PbaWorld, amount: i64, ifsc: String, account_number: String) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_funding_type = Some(output.funding_type().to_string());
        }
        Err(e) => panic!("Deposit failed: {e:?}"),
    }
}
```

- [ ] **Step 5: Add transaction funding type assertion step**

In `crates/pba_service/tests/steps/transaction_steps.rs`, add:

```rust
#[then(regex = r#"^the transactions list should contain a funding type "([^"]*)"$"#)]
async fn transactions_contain_funding_type(world: &mut PbaWorld, expected_type: String) {
    let types = world
        .all_transactions_funding_types
        .as_ref()
        .expect("No all-transactions result");
    assert!(
        types.iter().any(|t| t.as_deref() == Some(expected_type.as_str())),
        "Expected a funding type '{expected_type}', got: {types:?}"
    );
}
```

- [ ] **Step 6: Add all_transactions_funding_types to PbaWorld**

In `crates/pba_service/tests/e2e.rs`, add to `PbaWorld`:

```rust
all_transactions_funding_types: Option<Vec<Option<String>>>,
```

And in `Default`:

```rust
all_transactions_funding_types: None,
```

- [ ] **Step 7: Update list_all_transactions step to capture funding_types**

In `crates/pba_service/tests/steps/transaction_steps.rs`, update both `list_all_transactions` and `list_all_transactions_with_limit` to also capture funding types:

Add after the `all_transactions_account_ids` assignment:

```rust
world.all_transactions_funding_types = Some(
    result
        .transactions()
        .iter()
        .map(|t| t.funding_type().map(|s| s.to_string()))
        .collect(),
);
```

- [ ] **Step 8: Update the existing others-pool deposit step**

The existing `account_has_balances` step (lines 5-45 in deposit_steps.rs) deposits to others-pool from a non-origin bank. It now needs a `funding_type`. Update the others deposit call:

```rust
if others_amount > 0 {
    world
        .client
        .deposit()
        .account_id(&account_id)
        .source_ifsc("OTHER0009999")
        .source_account_number("9999999999")
        .amount(others_amount)
        .funding_type(pba_client::types::FundingType::from("third_party"))
        .send()
        .await
        .expect("Failed to deposit to others-pool");
}
```

- [ ] **Step 9: Verify build**

Run: `cargo build -p pba-service --tests`
Expected: Build succeeds

- [ ] **Step 10: Commit**

```bash
git add crates/pba_service/tests/
git commit -m "feat: add funding type E2E test scenarios and step definitions"
```

---

### Task 8: UI E2E Tests — System Accounts Page

**Files:**
- Modify: `crates/pba_service/tests/ui_features/admin_ui.feature`
- Modify: `crates/pba_service/tests/ui_steps/account_steps.rs`

- [ ] **Step 1: Add UI scenarios**

In `crates/pba_service/tests/ui_features/admin_ui.feature`, add:

```gherkin
  Scenario: System accounts page shows sentinel accounts and pool balances
    When I visit the system accounts page
    Then I should see "Sentinel Accounts" on the page
    And I should see "Self Funding Source" on the page
    And I should see "Trust Funding Source" on the page
    And I should see "Third Party Funding Source" on the page
    And I should see "Merchant Settlement" on the page
    And I should see "Withdrawal Settlement" on the page
    And I should see "PBA Pool Balances" on the page

  Scenario: Transactions page shows funding type column
    When I visit the all transactions page
    Then I should see "Funding Type" on the page
```

- [ ] **Step 2: Add system accounts page step**

In `crates/pba_service/tests/ui_steps/account_steps.rs`, add:

```rust
#[when("I visit the system accounts page")]
async fn visit_system_accounts(world: &mut UiWorld) {
    let base_url = world.base_url();
    let page = world.page();
    page.goto(&format!("{base_url}/admin/system-accounts"))
        .await
        .expect("Failed to navigate to system accounts page");
    sleep(Duration::from_millis(500)).await;
}
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p pba-service --tests`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/ui_features/ crates/pba_service/tests/ui_steps/
git commit -m "feat: add system accounts UI E2E test scenarios"
```

---

### Task 9: Run All Tests and Fix Issues

- [ ] **Step 1: Run format check**

Run: `just fmt-check`
Expected: No formatting issues. If issues found, run `just fmt` and re-check.

- [ ] **Step 2: Run lints**

Run: `just lint`
Expected: No clippy warnings.

- [ ] **Step 3: Run unit tests**

Run: `just test`
Expected: All tests pass.

- [ ] **Step 4: Run API E2E tests**

Run: `just api-e2e`
Expected: All scenarios pass including new funding_types.feature.

- [ ] **Step 5: Run UI E2E tests**

Run: `just ui-e2e`
Expected: All scenarios pass including system accounts page.

- [ ] **Step 6: Fix any failures**

If any tests fail, debug and fix. Common issues:
- Smithy SDK type names may differ from expected (check generated code in `crates/pba_client/src/types/`)
- `funding_type()` accessor may return `Option<&str>` or `&str` depending on whether it's required in Smithy
- Existing tests that deposit to others-pool without funding_type will now fail — ensure all are updated

- [ ] **Step 7: Final commit**

```bash
git add -A
git commit -m "fix: resolve test failures and formatting issues"
```
