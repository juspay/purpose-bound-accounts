# Transaction Detail Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `/admin/transactions/{id}` — a per-transaction detail page that displays every field on `TransactionRecord` plus contextual account info, with Post/Void actions for pending deposits, linked from existing list pages.

**Architecture:** Server-rendered askama page mirroring the `/admin/accounts/{id}` pattern. New repository method `get`, new template, three new admin handlers (detail, post, void), three new routes. Existing per-account Post/Void endpoints stay untouched. Folded in as cleanup: rename `src/admin/mod.rs` → `src/admin.rs` and `tests/ui_steps/mod.rs` → `tests/ui_steps.rs` (the file-per-module convention used everywhere else in the crate).

**Tech Stack:** Rust, axum 0.8, askama 0.13, sqlx (Postgres), tokio, cucumber + chromiumoxide for UI e2e.

**Spec:** `docs/superpowers/specs/2026-04-30-transaction-detail-page-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/pba_service/src/admin.rs` | Move (was `src/admin/mod.rs`) | Admin router; gains 3 new routes. |
| `crates/pba_service/src/admin/mod.rs` | **Delete** (after move) | — |
| `crates/pba_service/src/admin/handlers.rs` | Modify | Add `transaction_detail`, `post_transaction`, `void_transaction` handlers + `TransactionDetailTemplate` struct + unit tests. Modify `AllTransactionRow` and `TransferRow` to include `id`. |
| `crates/pba_service/src/repository/transaction_repo.rs` | Modify | Add `get_transaction(id)` method (unscoped lookup; the existing `get_by_id(id, account_id)` stays for the per-account flow). |
| `crates/pba_service/templates/admin/transaction_detail.html` | Create | New template. |
| `crates/pba_service/templates/admin/transactions.html` | Modify | Wrap timestamp cell in a link to the detail page. |
| `crates/pba_service/templates/admin/transfers_fragment.html` | Modify | Wrap timestamp cell in a link to the detail page. |
| `crates/pba_service/tests/ui_steps.rs` | Move (was `tests/ui_steps/mod.rs`) | Re-exports step modules; gains `pub mod transaction_steps;`. |
| `crates/pba_service/tests/ui_steps/mod.rs` | **Delete** (after move) | — |
| `crates/pba_service/tests/ui_steps/transaction_steps.rs` | Create | UI cucumber steps for the new scenarios. |
| `crates/pba_service/tests/ui_features/admin_ui.feature` | Modify | Add 3 new scenarios. |

---

## Task 1: Rename `src/admin/mod.rs` → `src/admin.rs`

**Files:**
- Move: `crates/pba_service/src/admin/mod.rs` → `crates/pba_service/src/admin.rs`

- [ ] **Step 1: Move the file**

```bash
cd /Users/natarajankannan/src/purpose-bound-accounts
git mv crates/pba_service/src/admin/mod.rs crates/pba_service/src/admin.rs
```

- [ ] **Step 2: Verify the build still works**

Run: `cargo build -p pba-service`
Expected: build succeeds, no errors.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor: rename admin/mod.rs → admin.rs (file-per-module style)

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 2: Add `TransactionRepo::get_transaction(id)`

The existing `get_by_id(id, account_id)` requires the account_id, which the detail page URL doesn't carry. Add an unscoped lookup that returns `AppError::TransactionNotFound` when missing.

**Files:**
- Modify: `crates/pba_service/src/repository/transaction_repo.rs`

- [ ] **Step 1: Add the method**

Insert immediately after the existing `get_by_id` method (around line 237). Open `crates/pba_service/src/repository/transaction_repo.rs` and add:

```rust
    pub async fn get_transaction(&self, id: Uuid) -> Result<TransactionRecord, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key,
                   created_at, updated_at
            FROM transactions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::TransactionNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }
```

- [ ] **Step 2: Verify the build**

Run: `cargo build -p pba-service`
Expected: build succeeds.

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/src/repository/transaction_repo.rs
git commit -m "feat(repo): add TransactionRepo::get_transaction for unscoped lookup

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 3: Define `TransactionDetailTemplate` struct (no template file yet)

We add the struct first so the unit tests in Task 4 can compile against it. We won't add the `#[derive(Template)]` until Task 5 — that requires the template file to exist.

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs`

- [ ] **Step 1: Add the struct (no Template derive yet)**

Append to `crates/pba_service/src/admin/handlers.rs` (after the existing `PurposeTypesTemplate` definition):

```rust
struct TransactionDetailTemplate {
    prefix: String,
    id: String,
    id_short: String,
    account_id: String,
    holder_id: String,
    purpose_code: String,
    tb_transfer_id: String,
    idempotency_key: String,
    transaction_type_label: String,
    status: String,
    status_class: String,
    direction: String,
    direction_class: String,
    pool: String,
    funding_type: String,
    amount: String,
    source_ifsc: String,
    source_account: String,
    gateway_ref: String,
    merchant_id: String,
    merchant_mcc: String,
    description: String,
    created_at: String,
    updated_at: String,
    timeout_seconds: String,
    is_deposit: bool,
    is_payment: bool,
    is_withdrawal: bool,
    can_post_or_void: bool,
}
```

- [ ] **Step 2: Verify build**

Run: `cargo build -p pba-service`
Expected: build succeeds (struct is unused — that's fine for now; it will be used in Task 4's tests).

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/src/admin/handlers.rs
git commit -m "feat(admin): add TransactionDetailTemplate struct

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 4: Write failing template-render unit tests

We pick TDD: write the tests first against `TransactionDetailTemplate`, watch them fail (no `Template` derive yet, no template file), then in Task 5 we create the template + add the derive to make them pass.

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs`

- [ ] **Step 1: Add the test module at the bottom of `handlers.rs`**

Append:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;

    fn deposit_pending_fixture() -> TransactionDetailTemplate {
        TransactionDetailTemplate {
            prefix: "".to_string(),
            id: "11111111-1111-1111-1111-111111111111".to_string(),
            id_short: "11111111".to_string(),
            account_id: "22222222-2222-2222-2222-222222222222".to_string(),
            holder_id: "holder-xyz".to_string(),
            purpose_code: "health".to_string(),
            tb_transfer_id: "9999999999".to_string(),
            idempotency_key: "idem-123".to_string(),
            transaction_type_label: "Deposit (Pending)".to_string(),
            status: "pending".to_string(),
            status_class: "status-frozen".to_string(),
            direction: "Inbound".to_string(),
            direction_class: "inbound".to_string(),
            pool: "Self".to_string(),
            funding_type: "origin".to_string(),
            amount: "50.00".to_string(),
            source_ifsc: "HDFC0001234".to_string(),
            source_account: "1234567890".to_string(),
            gateway_ref: "gw-ref-77".to_string(),
            merchant_id: "—".to_string(),
            merchant_mcc: "—".to_string(),
            description: "—".to_string(),
            created_at: "2026-04-30 12:00:00".to_string(),
            updated_at: "2026-04-30 12:00:00".to_string(),
            timeout_seconds: "—".to_string(),
            is_deposit: true,
            is_payment: false,
            is_withdrawal: false,
            can_post_or_void: true,
        }
    }

    fn payment_posted_fixture() -> TransactionDetailTemplate {
        TransactionDetailTemplate {
            prefix: "".to_string(),
            id: "33333333-3333-3333-3333-333333333333".to_string(),
            id_short: "33333333".to_string(),
            account_id: "22222222-2222-2222-2222-222222222222".to_string(),
            holder_id: "holder-xyz".to_string(),
            purpose_code: "health".to_string(),
            tb_transfer_id: "8888888888".to_string(),
            idempotency_key: "—".to_string(),
            transaction_type_label: "Payment".to_string(),
            status: "posted".to_string(),
            status_class: "status-active".to_string(),
            direction: "Outbound".to_string(),
            direction_class: "outbound".to_string(),
            pool: "Others".to_string(),
            funding_type: "—".to_string(),
            amount: "12.34".to_string(),
            source_ifsc: "—".to_string(),
            source_account: "—".to_string(),
            gateway_ref: "—".to_string(),
            merchant_id: "MERCH-1".to_string(),
            merchant_mcc: "8011".to_string(),
            description: "Doctor visit".to_string(),
            created_at: "2026-04-30 12:00:00".to_string(),
            updated_at: "2026-04-30 12:00:00".to_string(),
            timeout_seconds: "—".to_string(),
            is_deposit: false,
            is_payment: true,
            is_withdrawal: false,
            can_post_or_void: false,
        }
    }

    #[test]
    fn renders_all_deposit_fields() {
        let html = deposit_pending_fixture().render().expect("render");
        for needle in [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
            "holder-xyz",
            "health",
            "9999999999",
            "idem-123",
            "Deposit (Pending)",
            "pending",
            "Inbound",
            "Self",
            "origin",
            "50.00",
            "HDFC0001234",
            "1234567890",
            "gw-ref-77",
        ] {
            assert!(
                html.contains(needle),
                "expected `{}` in rendered HTML, got:\n{}",
                needle,
                html
            );
        }
    }

    #[test]
    fn shows_post_and_void_when_pending_deposit() {
        let html = deposit_pending_fixture().render().expect("render");
        assert!(
            html.contains("/admin/transactions/11111111-1111-1111-1111-111111111111/post"),
            "expected Post form action in HTML:\n{}",
            html
        );
        assert!(
            html.contains("/admin/transactions/11111111-1111-1111-1111-111111111111/void"),
            "expected Void form action in HTML:\n{}",
            html
        );
    }

    #[test]
    fn hides_actions_when_not_pending() {
        let html = payment_posted_fixture().render().expect("render");
        assert!(
            !html.contains("/post"),
            "did not expect Post form when can_post_or_void is false:\n{}",
            html
        );
        assert!(
            !html.contains("/void"),
            "did not expect Void form when can_post_or_void is false:\n{}",
            html
        );
    }

    #[test]
    fn renders_merchant_section_for_payment() {
        let html = payment_posted_fixture().render().expect("render");
        assert!(html.contains("MERCH-1"), "merchant_id missing: {}", html);
        assert!(html.contains("8011"), "merchant_mcc missing: {}", html);
        assert!(
            html.contains("Doctor visit"),
            "description missing: {}",
            html
        );
        // For a payment, source IFSC value should not appear (we render "—"
        // for the absent source fields, so check the original value isn't there).
        assert!(
            !html.contains("HDFC0001234"),
            "payment should not show deposit-only source IFSC: {}",
            html
        );
    }
}
```

- [ ] **Step 2: Run the new tests to confirm they fail**

Run: `cargo test -p pba-service --lib admin::handlers::tests`
Expected: FAILS to compile — `TransactionDetailTemplate` doesn't implement `Template` (the `#[derive(Template)]` attribute hasn't been added yet, and no template file exists).

- [ ] **Step 3: Commit (red state)**

```bash
git add crates/pba_service/src/admin/handlers.rs
git commit -m "test(admin): failing unit tests for transaction detail template render

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 5: Create the template file and wire `#[derive(Template)]`

Make the failing tests from Task 4 pass.

**Files:**
- Create: `crates/pba_service/templates/admin/transaction_detail.html`
- Modify: `crates/pba_service/src/admin/handlers.rs` (add `#[derive(Template)]`)

- [ ] **Step 1: Create the template**

Create `crates/pba_service/templates/admin/transaction_detail.html`:

```html
{% extends "base.html" %}

{% block title %}Transaction {{ id_short }} - PBA Admin{% endblock %}

{% block content %}
<h1>Transaction {{ id_short }}</h1>

<p><a href="{{ prefix }}/admin/transactions" role="button" class="outline">&larr; Back to all transactions</a></p>

<article>
    <header><strong>Identity</strong></header>
    <div class="grid">
        <div>
            <p><strong>Transaction ID:</strong> {{ id }}</p>
            <p><strong>Account ID:</strong> <a href="{{ prefix }}/admin/accounts/{{ account_id }}">{{ account_id }}</a></p>
            <p><strong>Holder ID:</strong> {{ holder_id }}</p>
            <p><strong>Purpose:</strong> {{ purpose_code }}</p>
        </div>
        <div>
            <p><strong>TigerBeetle Transfer ID:</strong> {{ tb_transfer_id }}</p>
            <p><strong>Idempotency Key:</strong> {{ idempotency_key }}</p>
        </div>
    </div>
</article>

<article>
    <header><strong>Classification</strong></header>
    <div class="grid">
        <div>
            <p><strong>Type:</strong> {{ transaction_type_label }}</p>
            <p><strong>Status:</strong> <span class="{{ status_class }}">{{ status }}</span></p>
            <p><strong>Direction:</strong> <span class="{{ direction_class }}">{{ direction }}</span></p>
        </div>
        <div>
            <p><strong>Pool:</strong> {{ pool }}</p>
            <p><strong>Funding Type:</strong> {{ funding_type }}</p>
        </div>
    </div>
</article>

<article>
    <header><strong>Amount</strong></header>
    <p style="font-size: 1.5rem;"><strong>₹{{ amount }}</strong></p>
</article>

{% if is_deposit %}
<article>
    <header><strong>Source</strong></header>
    <div class="grid">
        <div>
            <p><strong>Source IFSC:</strong> {{ source_ifsc }}</p>
            <p><strong>Source Account:</strong> {{ source_account }}</p>
        </div>
        <div>
            <p><strong>Gateway Ref:</strong> {{ gateway_ref }}</p>
        </div>
    </div>
</article>
{% else if is_payment %}
<article>
    <header><strong>Merchant</strong></header>
    <div class="grid">
        <div>
            <p><strong>Merchant ID:</strong> {{ merchant_id }}</p>
            <p><strong>Merchant MCC:</strong> {{ merchant_mcc }}</p>
        </div>
        <div>
            <p><strong>Description:</strong> {{ description }}</p>
        </div>
    </div>
</article>
{% else if is_withdrawal %}
<article>
    <header><strong>Source / Merchant</strong></header>
    <p>Withdrawals have no external source or merchant.</p>
</article>
{% endif %}

<article>
    <header><strong>Timing</strong></header>
    <div class="grid">
        <div>
            <p><strong>Created:</strong> {{ created_at }}</p>
            <p><strong>Updated:</strong> {{ updated_at }}</p>
        </div>
        <div>
            <p><strong>Timeout (seconds):</strong> {{ timeout_seconds }}</p>
        </div>
    </div>
</article>

{% if can_post_or_void %}
<article>
    <header><strong>Actions</strong></header>
    <form method="post" action="{{ prefix }}/admin/transactions/{{ id }}/post" class="inline-form">
        <button type="submit" class="outline" style="color: #2e7d32; border-color: #2e7d32;">Post</button>
    </form>
    <form method="post" action="{{ prefix }}/admin/transactions/{{ id }}/void" class="inline-form">
        <button type="submit" class="outline" style="color: #c62828; border-color: #c62828;">Void</button>
    </form>
</article>
{% endif %}
{% endblock %}
```

- [ ] **Step 2: Add `#[derive(Template)]` to the struct**

In `crates/pba_service/src/admin/handlers.rs`, change:

```rust
struct TransactionDetailTemplate {
```

to:

```rust
#[derive(Template)]
#[template(path = "admin/transaction_detail.html")]
struct TransactionDetailTemplate {
```

- [ ] **Step 3: Run the unit tests to confirm green**

Run: `cargo test -p pba-service --lib admin::handlers::tests`
Expected: 4 tests pass (`renders_all_deposit_fields`, `shows_post_and_void_when_pending_deposit`, `hides_actions_when_not_pending`, `renders_merchant_section_for_payment`).

- [ ] **Step 4: Commit (green state)**

```bash
git add crates/pba_service/templates/admin/transaction_detail.html crates/pba_service/src/admin/handlers.rs
git commit -m "feat(admin): transaction detail template

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 6: Add `transaction_detail` GET handler + route

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs`
- Modify: `crates/pba_service/src/admin.rs`

- [ ] **Step 1: Add the handler**

Append to `crates/pba_service/src/admin/handlers.rs` (after the existing handlers, before the `#[cfg(test)] mod tests` block):

```rust
pub async fn transaction_detail(
    State(state): State<AppState>,
    Path(transaction_id): Path<Uuid>,
) -> Response {
    let txn = match state.transaction_repo.get_transaction(transaction_id).await {
        Ok(t) => t,
        Err(crate::error::AppError::TransactionNotFound(_)) => {
            return (StatusCode::NOT_FOUND, "Transaction not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to get transaction: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let dash = "—".to_string();

    let (holder_id, purpose_code) = match state.account_repo.get_account(txn.account_id).await {
        Ok(a) => (a.holder_id, a.purpose_code),
        Err(e) => {
            tracing::warn!(
                "Failed to load parent account for transaction {transaction_id}: {e}"
            );
            (dash.clone(), dash.clone())
        }
    };

    let id_str = txn.id.to_string();
    let id_short = id_str.chars().take(8).collect::<String>();

    let status_class = match txn.status {
        TransactionStatus::Pending => "status-frozen",
        TransactionStatus::Posted | TransactionStatus::Settled => "status-active",
        TransactionStatus::Voided => "status-closed",
    }
    .to_string();

    let pool = if txn.pool == "self" {
        "Self".to_string()
    } else {
        "Others".to_string()
    };

    let can_post_or_void = matches!(
        txn.transaction_type,
        crate::domain::transaction::TransactionType::Deposit
    ) && matches!(txn.status, TransactionStatus::Pending);

    render(TransactionDetailTemplate {
        prefix: state.path_prefix.clone(),
        id: id_str,
        id_short,
        account_id: txn.account_id.to_string(),
        holder_id,
        purpose_code,
        tb_transfer_id: txn.tb_transfer_id.to_string(),
        idempotency_key: txn.idempotency_key.unwrap_or_else(|| dash.clone()),
        transaction_type_label: txn.type_label().to_string(),
        status: txn.status.as_str().to_string(),
        status_class,
        direction: txn.direction.label().to_string(),
        direction_class: txn.direction.css_class().to_string(),
        pool,
        funding_type: txn.funding_type.unwrap_or_else(|| dash.clone()),
        amount: txn.amount_display(),
        source_ifsc: txn.source_ifsc.unwrap_or_else(|| dash.clone()),
        source_account: txn.source_account.unwrap_or_else(|| dash.clone()),
        gateway_ref: txn.gateway_ref.unwrap_or_else(|| dash.clone()),
        merchant_id: txn.merchant_id.unwrap_or_else(|| dash.clone()),
        merchant_mcc: txn.merchant_mcc.unwrap_or_else(|| dash.clone()),
        description: txn.description.unwrap_or_else(|| dash.clone()),
        created_at: txn.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: txn.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        timeout_seconds: txn
            .timeout_seconds
            .map(|s| s.to_string())
            .unwrap_or_else(|| dash.clone()),
        is_deposit: matches!(
            txn.transaction_type,
            crate::domain::transaction::TransactionType::Deposit
        ),
        is_payment: matches!(
            txn.transaction_type,
            crate::domain::transaction::TransactionType::Payment
        ),
        is_withdrawal: matches!(
            txn.transaction_type,
            crate::domain::transaction::TransactionType::Withdrawal
        ),
        can_post_or_void,
    })
}
```

- [ ] **Step 2: Register the route**

Open `crates/pba_service/src/admin.rs`. The existing line is:

```rust
        .route("/admin/transactions", get(handlers::transactions_page))
```

Add immediately after it:

```rust
        .route(
            "/admin/transactions/{transaction_id}",
            get(handlers::transaction_detail),
        )
```

- [ ] **Step 3: Verify the build**

Run: `cargo build -p pba-service`
Expected: build succeeds.

- [ ] **Step 4: Run all unit tests to confirm nothing regressed**

Run: `cargo test -p pba-service --lib`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/src/admin/handlers.rs crates/pba_service/src/admin.rs
git commit -m "feat(admin): GET /admin/transactions/{id} detail page

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 7: Add `post_transaction` and `void_transaction` handlers + routes

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs`
- Modify: `crates/pba_service/src/admin.rs`

- [ ] **Step 1: Add the two handlers**

Append to `crates/pba_service/src/admin/handlers.rs` (after `transaction_detail`, before `#[cfg(test)]`):

```rust
pub async fn post_transaction(
    State(state): State<AppState>,
    Path(transaction_id): Path<Uuid>,
) -> Response {
    let txn = match state.transaction_repo.get_transaction(transaction_id).await {
        Ok(t) => t,
        Err(_) => return (StatusCode::NOT_FOUND, "Transaction not found").into_response(),
    };
    if let Err(e) = state
        .deposit_service
        .post_deposit(txn.account_id, transaction_id)
        .await
    {
        tracing::error!("Failed to post deposit from detail page: {e}");
    }
    Redirect::to(&prefixed(
        &state,
        &format!("/admin/transactions/{transaction_id}"),
    ))
    .into_response()
}

pub async fn void_transaction(
    State(state): State<AppState>,
    Path(transaction_id): Path<Uuid>,
) -> Response {
    let txn = match state.transaction_repo.get_transaction(transaction_id).await {
        Ok(t) => t,
        Err(_) => return (StatusCode::NOT_FOUND, "Transaction not found").into_response(),
    };
    if let Err(e) = state
        .deposit_service
        .void_deposit(txn.account_id, transaction_id, None)
        .await
    {
        tracing::error!("Failed to void deposit from detail page: {e}");
    }
    Redirect::to(&prefixed(
        &state,
        &format!("/admin/transactions/{transaction_id}"),
    ))
    .into_response()
}
```

- [ ] **Step 2: Register both routes**

Open `crates/pba_service/src/admin.rs`. Add immediately after the `/admin/transactions/{transaction_id}` GET route:

```rust
        .route(
            "/admin/transactions/{transaction_id}/post",
            axum::routing::post(handlers::post_transaction),
        )
        .route(
            "/admin/transactions/{transaction_id}/void",
            axum::routing::post(handlers::void_transaction),
        )
```

- [ ] **Step 3: Verify the build**

Run: `cargo build -p pba-service`
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/admin/handlers.rs crates/pba_service/src/admin.rs
git commit -m "feat(admin): Post/Void actions on transaction detail page

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 8: Link timestamp cell in `transactions.html`

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs` (add `id` to `AllTransactionRow`)
- Modify: `crates/pba_service/templates/admin/transactions.html`

- [ ] **Step 1: Add `id` field to `AllTransactionRow`**

In `crates/pba_service/src/admin/handlers.rs`, find the existing struct:

```rust
struct AllTransactionRow {
    timestamp: String,
    account_id: String,
    ...
}
```

Add `id: String,` as the first field. The struct becomes:

```rust
struct AllTransactionRow {
    id: String,
    timestamp: String,
    account_id: String,
    account_id_short: String,
    transfer_type: String,
    status: String,
    status_class: String,
    pool: String,
    funding_type: String,
    direction: String,
    direction_class: String,
    amount: String,
}
```

In `transactions_page`, the row construction has:

```rust
            AllTransactionRow {
                timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                account_id: t.account_id.to_string(),
```

Add `id: t.id.to_string(),` as the first field of the `AllTransactionRow { ... }` initializer:

```rust
            AllTransactionRow {
                id: t.id.to_string(),
                timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                account_id: t.account_id.to_string(),
```

- [ ] **Step 2: Update the template**

In `crates/pba_service/templates/admin/transactions.html`, find:

```html
            <td>{{ t.timestamp }}</td>
```

Replace with:

```html
            <td><a href="{{ prefix }}/admin/transactions/{{ t.id }}">{{ t.timestamp }}</a></td>
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p pba-service`
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/admin/handlers.rs crates/pba_service/templates/admin/transactions.html
git commit -m "feat(admin): link timestamp to detail page in all-transactions list

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 9: Link timestamp cell in `transfers_fragment.html`

**Files:**
- Modify: `crates/pba_service/src/admin/handlers.rs` (add `id` to `TransferRow`)
- Modify: `crates/pba_service/templates/admin/transfers_fragment.html`

- [ ] **Step 1: Add `id` field to `TransferRow`**

In `crates/pba_service/src/admin/handlers.rs`, find:

```rust
struct TransferRow {
    timestamp: String,
    transfer_type: String,
    direction: String,
    direction_class: String,
    pool: String,
    amount: String,
}
```

Change to:

```rust
struct TransferRow {
    id: String,
    timestamp: String,
    transfer_type: String,
    direction: String,
    direction_class: String,
    pool: String,
    amount: String,
}
```

In `account_transfers_fragment`, the row construction is:

```rust
            TransferRow {
                timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
```

Add `id: t.id.to_string(),` as the first field:

```rust
            TransferRow {
                id: t.id.to_string(),
                timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
```

- [ ] **Step 2: Update the template**

In `crates/pba_service/templates/admin/transfers_fragment.html`, find:

```html
            <td>{{ t.timestamp }}</td>
```

Replace with:

```html
            <td><a href="{{ prefix }}/admin/transactions/{{ t.id }}">{{ t.timestamp }}</a></td>
```

- [ ] **Step 3: Verify build**

Run: `cargo build -p pba-service`
Expected: build succeeds.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/admin/handlers.rs crates/pba_service/templates/admin/transfers_fragment.html
git commit -m "feat(admin): link timestamp to detail page in account transfers fragment

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 10: Rename `tests/ui_steps/mod.rs` → `tests/ui_steps.rs`

**Files:**
- Move: `crates/pba_service/tests/ui_steps/mod.rs` → `crates/pba_service/tests/ui_steps.rs`

- [ ] **Step 1: Move the file**

```bash
cd /Users/natarajankannan/src/purpose-bound-accounts
git mv crates/pba_service/tests/ui_steps/mod.rs crates/pba_service/tests/ui_steps.rs
```

- [ ] **Step 2: Verify the test target still compiles**

Run: `cargo test -p pba-service --test ui_e2e --no-run`
Expected: compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor: rename ui_steps/mod.rs → ui_steps.rs (file-per-module style)

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 11: Add `transaction_steps.rs` and the first cucumber scenario

This task introduces the new step file, registers it, and adds the first scenario: viewing the most recent transaction's detail page.

**Files:**
- Create: `crates/pba_service/tests/ui_steps/transaction_steps.rs`
- Modify: `crates/pba_service/tests/ui_steps.rs` (register module)
- Modify: `crates/pba_service/tests/ui_features/admin_ui.feature` (add scenario)

- [ ] **Step 1: Register the new step module**

In `crates/pba_service/tests/ui_steps.rs`, change:

```rust
pub mod account_steps;
pub mod deposit_steps;
pub mod payment_steps;
pub mod purpose_steps;
pub mod withdrawal_steps;
```

to:

```rust
pub mod account_steps;
pub mod deposit_steps;
pub mod payment_steps;
pub mod purpose_steps;
pub mod transaction_steps;
pub mod withdrawal_steps;
```

- [ ] **Step 2: Create `tests/ui_steps/transaction_steps.rs`**

```rust
use cucumber::{then, when};
use std::time::Duration;
use tokio::time::sleep;

use crate::UiWorld;

/// Stash the current detail page URL on the world (we reuse `last_deposit_id`-style
/// state via the page itself; nothing extra to track here).

async fn current_page_content(world: &mut UiWorld) -> String {
    let page = world.ensure_page().await;
    page.content().await.expect("Failed to read page content")
}

/// Navigate to the all-transactions list and click the timestamp link of the
/// first row, which leads to /admin/transactions/{id}.
async fn open_first_transaction_detail(world: &mut UiWorld) {
    let url = world.url("/admin/transactions");
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transactions list");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    // Click the first link whose href starts with /admin/transactions/<uuid> (with
    // a trailing UUID segment, distinguishing it from the list page itself).
    let js = r#"
        (() => {
            const links = Array.from(document.querySelectorAll("a[href*='/admin/transactions/']"));
            const detail = links.find(a => /\/admin\/transactions\/[0-9a-f-]{36}$/.test(a.getAttribute('href')));
            if (!detail) throw new Error('no detail link found on transactions list');
            detail.click();
        })();
    "#;
    page.evaluate(js)
        .await
        .expect("Failed to click first transaction detail link");
    sleep(Duration::from_millis(600)).await;
}

#[when("I view the most recent transaction's detail page")]
async fn when_view_most_recent_detail(world: &mut UiWorld) {
    open_first_transaction_detail(world).await;
}

#[then("the transaction detail should show the transaction ID")]
async fn then_show_transaction_id(world: &mut UiWorld) {
    let content = current_page_content(world).await;
    assert!(
        content.contains("Transaction ID:"),
        "expected `Transaction ID:` label on detail page, got snippet: {}",
        &content[..content.len().min(500)]
    );
}

#[then("the transaction detail should show the account ID")]
async fn then_show_account_id(world: &mut UiWorld) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID on world for assertion");
    let content = current_page_content(world).await;
    assert!(
        content.contains(&account_id),
        "expected account ID `{}` on detail page",
        account_id
    );
}

#[then(regex = r#"^the transaction detail should show amount "([^"]*)"$"#)]
async fn then_show_amount(world: &mut UiWorld, expected: String) {
    let content = current_page_content(world).await;
    let needle = format!("₹{}", expected);
    assert!(
        content.contains(&needle),
        "expected amount `{}` on detail page",
        needle
    );
}

#[then(regex = r#"^the transaction detail should show type "([^"]*)"$"#)]
async fn then_show_type(world: &mut UiWorld, expected: String) {
    let content = current_page_content(world).await;
    // The detail page renders the type via the `type_label()` helper, which for a
    // posted deposit is just "Deposit". The label is shown after a "Type:" prefix.
    let marker = "Type:</strong>";
    let idx = content
        .find(marker)
        .unwrap_or_else(|| panic!("`Type:` label not found on detail page"));
    let after = &content[idx + marker.len()..];
    let snippet = &after[..after.len().min(200)];
    assert!(
        snippet.contains(&expected),
        "expected type `{}` on detail page, after-Type snippet: {}",
        expected,
        snippet
    );
}
```

- [ ] **Step 3: Add the scenario**

Append to `crates/pba_service/tests/ui_features/admin_ui.feature`:

```
  Scenario: Transaction detail page shows all fields
    Given a "health" account exists for holder "d6666666-6666-6666-6666-666666666666" with origin IFSC "HDFC0006666" and account number "6666600001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I view the most recent transaction's detail page
    Then the transaction detail should show the transaction ID
    And the transaction detail should show the account ID
    And the transaction detail should show amount "50.00"
    And the transaction detail should show type "Deposit"
```

- [ ] **Step 4: Build the test target**

Run: `cargo test -p pba-service --test ui_e2e --no-run`
Expected: compiles successfully.

- [ ] **Step 5: Run the UI e2e suite**

Run: `just ui-e2e`
Expected: all scenarios including the new one pass.

(If `just ui-e2e` is not available — start infra with `just e2e-start`, run `just ui-e2e-run`, then `just e2e-stop`.)

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/tests/ui_steps.rs crates/pba_service/tests/ui_steps/transaction_steps.rs crates/pba_service/tests/ui_features/admin_ui.feature
git commit -m "test(ui): scenario for transaction detail page rendering all fields

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 12: Add cucumber scenario — Post a pending deposit from detail page

**Files:**
- Modify: `crates/pba_service/tests/ui_steps/transaction_steps.rs`
- Modify: `crates/pba_service/tests/ui_features/admin_ui.feature`

- [ ] **Step 1: Add the new step implementations**

Append to `crates/pba_service/tests/ui_steps/transaction_steps.rs`:

```rust
#[cucumber::given(regex = r"^the account has a pending deposit of (\d+) in the self-pool$")]
async fn given_pending_self_deposit(world: &mut UiWorld, amount: i64) {
    // Reuse the existing pending-deposit step by funneling through the same form path.
    let origin_ifsc = world.origin_ifsc.clone().expect("origin IFSC missing");
    let origin_acct = world
        .origin_account_number
        .clone()
        .expect("origin account number missing");
    crate::ui_steps::deposit_steps::create_pending_deposit_for_test(
        world,
        amount,
        &origin_ifsc,
        &origin_acct,
    )
    .await;
}

#[when("I view that pending deposit's detail page")]
async fn when_view_pending_deposit_detail(world: &mut UiWorld) {
    let deposit_id = world
        .last_deposit_id
        .clone()
        .expect("no pending deposit id on world");
    let url = world.url(&format!("/admin/transactions/{}", deposit_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transaction detail");
    sleep(Duration::from_millis(500)).await;
}

#[when("I click the Post button")]
async fn when_click_post_button(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let js = r#"
        (() => {
            const form = document.querySelector("form[action$='/post']");
            if (!form) throw new Error('Post form not found on detail page');
            form.submit();
        })();
    "#;
    page.evaluate(js).await.expect("Failed to click Post");
    // Wait for the redirect back to the same detail page (status will have changed).
    sleep(Duration::from_millis(800)).await;
}

#[then(regex = r#"^the transaction detail status should be "([^"]*)"$"#)]
async fn then_detail_status(world: &mut UiWorld, expected: String) {
    let content = current_page_content(world).await;
    // Status is rendered as: <strong>Status:</strong> <span class="...">{{ status }}</span>
    let marker = "Status:</strong>";
    let idx = content
        .find(marker)
        .unwrap_or_else(|| panic!("`Status:` label not found on detail page"));
    let after = &content[idx + marker.len()..];
    if let Some(span_start) = after.find("<span") {
        let span = &after[span_start..];
        if let Some(gt) = span.find('>') {
            let inner = &span[gt + 1..];
            if let Some(end) = inner.find("</span>") {
                let actual = inner[..end].trim();
                assert_eq!(
                    actual, expected,
                    "transaction status mismatch on detail page"
                );
                return;
            }
        }
    }
    panic!("could not parse status from detail page snippet");
}
```

- [ ] **Step 2: Add a public test helper in `deposit_steps.rs`**

The new step in Step 1 calls `crate::ui_steps::deposit_steps::create_pending_deposit_for_test(...)`, which doesn't exist yet — add it.

In `crates/pba_service/tests/ui_steps/deposit_steps.rs`, find the existing `do_pending_deposit` private helper. Just below it, add a public wrapper:

```rust
pub async fn create_pending_deposit_for_test(
    world: &mut UiWorld,
    amount: i64,
    ifsc: &str,
    account_number: &str,
) {
    let ok = do_pending_deposit(world, amount, ifsc, account_number, None).await;
    assert!(
        ok,
        "Expected pending deposit to succeed in test setup but it stayed on the form page"
    );
}
```

- [ ] **Step 3: Add the scenario**

Append to `crates/pba_service/tests/ui_features/admin_ui.feature`:

```
  Scenario: Posting a pending deposit from the detail page
    Given a "health" account exists for holder "d7777777-7777-7777-7777-777777777777" with origin IFSC "HDFC0007777" and account number "7777700001"
    And the account has a pending deposit of 5000 in the self-pool
    When I view that pending deposit's detail page
    And I click the Post button
    Then the transaction detail status should be "posted"
```

- [ ] **Step 4: Build and run UI e2e**

Run: `cargo test -p pba-service --test ui_e2e --no-run`
Expected: compiles.

Run: `just ui-e2e`
Expected: all scenarios pass, including the new "Posting a pending deposit from the detail page".

- [ ] **Step 5: Commit**

```bash
git add crates/pba_service/tests/ui_steps/transaction_steps.rs crates/pba_service/tests/ui_steps/deposit_steps.rs crates/pba_service/tests/ui_features/admin_ui.feature
git commit -m "test(ui): scenario for posting pending deposit from detail page

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 13: Add cucumber scenario — Detail page hides Post/Void for posted transactions

**Files:**
- Modify: `crates/pba_service/tests/ui_steps/transaction_steps.rs`
- Modify: `crates/pba_service/tests/ui_features/admin_ui.feature`

- [ ] **Step 1: Add the assertion steps**

Append to `crates/pba_service/tests/ui_steps/transaction_steps.rs`:

```rust
#[then("the Post button should not be visible")]
async fn then_post_not_visible(world: &mut UiWorld) {
    let content = current_page_content(world).await;
    assert!(
        !content.contains("/post\""),
        "did not expect Post form on this detail page; snippet: {}",
        &content[..content.len().min(400)]
    );
}

#[then("the Void button should not be visible")]
async fn then_void_not_visible(world: &mut UiWorld) {
    let content = current_page_content(world).await;
    assert!(
        !content.contains("/void\""),
        "did not expect Void form on this detail page; snippet: {}",
        &content[..content.len().min(400)]
    );
}
```

- [ ] **Step 2: Add the scenario**

Append to `crates/pba_service/tests/ui_features/admin_ui.feature`:

```
  Scenario: Detail page hides Post and Void for posted transactions
    Given a "health" account exists for holder "d8888888-8888-8888-8888-888888888888" with origin IFSC "HDFC0008888" and account number "8888800001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I view the most recent transaction's detail page
    Then the Post button should not be visible
    And the Void button should not be visible
```

- [ ] **Step 3: Run UI e2e**

Run: `just ui-e2e`
Expected: all scenarios pass, including the new one.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/ui_steps/transaction_steps.rs crates/pba_service/tests/ui_features/admin_ui.feature
git commit -m "test(ui): scenario for hiding Post/Void on posted-transaction detail page

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 14: Final verification

- [ ] **Step 1: Run the full local-ci pipeline**

Run: `just local-ci`
Expected: format, lint, build, and unit tests all pass.

- [ ] **Step 2: Run the full UI + API e2e suite**

Run: `just e2e-all`
Expected: all scenarios pass.

- [ ] **Step 3: Visual smoke check (optional but recommended)**

Run: `just run-bg`. In a browser, visit `http://localhost:3030/admin/transactions`, click a timestamp; verify the detail page renders. Click a pending deposit's detail link; click Post; verify status flips to `posted` on the same page. Stop services with `just stop`.

If the smoke check uncovers any visual issue, fix and add a follow-up commit before considering this plan complete. If the dev environment is unavailable, document that in the PR description; the cucumber suite plus unit tests are the gating signal.
