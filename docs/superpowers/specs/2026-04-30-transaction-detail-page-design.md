# Transaction Detail Page — Design

**Date:** 2026-04-30
**Status:** Approved

## Goal

Add a per-transaction detail page in the admin UI that surfaces every field on a single `TransactionRecord`, plus contextual account info, with Post/Void actions when the transaction is a pending deposit. Linkable from the existing all-transactions list and per-account transfers fragment.

## Non-goals

- No new business logic — Post/Void reuse existing services.
- No changes to the API/Smithy SDK.
- No edit/delete operations beyond the existing Post/Void flows.

## Scope

| Area | Change |
|---|---|
| Repository | Add `TransactionRepository::get_transaction(id) -> Result<TransactionRecord, AppError>` returning `AppError::NotFound` when absent. |
| Handlers | Add `transaction_detail` (GET), `post_transaction` (POST), `void_transaction` (POST) in `src/admin/handlers.rs`. |
| Template | Add `templates/admin/transaction_detail.html` extending `base.html`. |
| Router | Add three routes under `/admin/transactions/{id}` in the admin module. As part of this work, also rename `src/admin/mod.rs` → `src/admin.rs` to match the file-per-module convention used everywhere else in `src/` (api, auth, domain, repository, service, db). |
| Existing list pages | Make the timestamp cell in `templates/admin/transactions.html` and `templates/admin/transfers_fragment.html` a link to the new detail page. |
| Tests | Unit tests for template render shaping; UI Cucumber e2e scenarios in `admin_ui.feature`. As part of this work, rename `tests/ui_steps/mod.rs` → `tests/ui_steps.rs` (same file-per-module cleanup; we are touching this file to register the new step module). `tests/steps/mod.rs` is left alone — out of scope for this spec. |

## Architecture

```
GET /admin/transactions/{id}
   handler -> transaction_repo.get_transaction(id)        [Postgres]
           -> account_repo.get_account(tx.account_id)      [Postgres, for holder/purpose]
           -> render TransactionDetailTemplate

POST /admin/transactions/{id}/post
   handler -> transaction_repo.get_transaction(id)         [recover account_id]
           -> deposit_service.post_deposit(account_id, id)
           -> 303 redirect to /admin/transactions/{id}

POST /admin/transactions/{id}/void
   handler -> transaction_repo.get_transaction(id)
           -> deposit_service.void_deposit(account_id, id, None)
           -> 303 redirect to /admin/transactions/{id}
```

The new `/admin/transactions/{id}/post` and `/void` routes are deliberately separate from the existing `/admin/accounts/{aid}/deposits/{id}/post|void` routes so they can redirect back to the transaction detail page (the existing routes redirect to the account page; both flows remain valid).

If the parent account lookup fails during detail rendering, the page is still rendered with `holder_id` and `purpose_code` shown as `—` (warn-logged). This avoids cascading a 500 from a follow-on lookup; the transaction itself is the page's primary subject.

## UI layout

Single-column page (matches `account_detail.html` styling), `base.html` chrome.

**Header:** `Transaction {id_short}` (first 8 chars) + back link to `/admin/transactions`.

**Identity card:**
- Transaction ID (full UUID)
- Account ID (link to `/admin/accounts/{account_id}`)
- Holder ID (from joined account, `—` on lookup failure)
- Purpose Code (from joined account, `—` on lookup failure)
- TigerBeetle Transfer ID
- Idempotency Key (`—` if absent)

**Classification card:**
- Type — `Deposit` / `Payment` / `Withdrawal`
- Status — `Pending` / `Posted` / `Voided` / `Settled` (color-classed)
- Direction — `Inbound` / `Outbound` (color-classed)
- Pool — `Self` / `Others`
- Funding Type (`—` if absent)

**Amount card:**
- Amount — `₹X.YZ`

**Source / Merchant card** (one branch, by transaction type):
- *Deposit:* Source IFSC, Source Account, Gateway Ref
- *Payment:* Merchant ID, Merchant MCC, Description
- *Withdrawal:* placeholder section noting "no source/merchant"

**Timing card:**
- Created At, Updated At, Timeout Seconds (only when set)

**Actions** (rendered only when `type == Deposit` AND `status == Pending`):
- Post button → `POST /admin/transactions/{id}/post`
- Void button → `POST /admin/transactions/{id}/void`

## Linking from existing pages

- `templates/admin/transactions.html` — wrap the timestamp cell in `<a href="{{ prefix }}/admin/transactions/{{ t.id }}">…</a>`. Requires adding `id` to `AllTransactionRow`.
- `templates/admin/transfers_fragment.html` — same wrap. Requires adding `id` to `TransferRow`.

## Error handling

| Failure | Response |
|---|---|
| Path UUID malformed | 400 (axum extractor default) |
| `get_transaction` not found | 404 with body `"Transaction not found"` |
| `get_transaction` DB error | 500 `"Database error"`, `tracing::error!` logged |
| Parent account lookup fails | Page renders with `—` for holder/purpose; warn-logged |
| `post_deposit` / `void_deposit` fails (status changed, etc.) | `tracing::error!` logged; 303 redirect back to detail page (matches existing per-account handler pattern) |
| Forged POST for non-pending or non-deposit transaction | Service layer rejects; falls into the log-and-redirect path above |

## Testing

### Unit tests (template render)

In `src/admin/handlers.rs` under `#[cfg(test)] mod tests`. The template is exercised directly via `askama::Template::render()` on a constructed `TransactionDetailTemplate` — no DB, no HTTP. This isolates the data-shaping logic.

- `transaction_detail_template_renders_all_fields` — deposit-shaped struct; assert every field's display value appears in the rendered HTML.
- `transaction_detail_template_hides_actions_when_not_pending` — Posted deposit; assert no `/post` form is rendered.
- `transaction_detail_template_renders_payment_section_for_payment` — payment-shaped struct; assert merchant fields shown and Source IFSC not shown.

### UI Cucumber e2e

New scenarios in `tests/ui_features/admin_ui.feature`:

```
Scenario: Transaction detail page shows all fields
  Given a "health" account exists for holder "..." with origin IFSC "..." and account number "..."
  And the account has 5000 in self-pool and 0 in others-pool
  When I view the most recent transaction's detail page
  Then the transaction detail should show the transaction ID
  And the transaction detail should show the account ID
  And the transaction detail should show amount "50.00"
  And the transaction detail should show type "Deposit"

Scenario: Posting a pending deposit from the detail page
  Given a "health" account exists for holder "..." with origin IFSC "..." and account number "..."
  And the account has a pending deposit of 5000 in the self-pool
  When I view that pending deposit's detail page
  And I click the Post button
  Then the transaction detail status should be "posted"

Scenario: Detail page hides Post/Void for posted transactions
  Given a "health" account exists for holder "..." with origin IFSC "..." and account number "..."
  And the account has 5000 in self-pool and 0 in others-pool
  When I view the most recent transaction's detail page
  Then the Post button should not be visible
  And the Void button should not be visible
```

New step file `tests/ui_steps/transaction_steps.rs` (registered in `tests/ui_steps/mod.rs`). Uses existing helpers from `account_steps.rs` for account creation and deposits. Steps drive the all-transactions list → click the timestamp link → assert content; the pending-deposit scenario exercises the Post button.

## Implementation file map

- `crates/pba_service/src/repository/transaction_repo.rs` — add `get_transaction`.
- `crates/pba_service/src/admin/handlers.rs` — add three handlers + new template struct + unit tests.
- `crates/pba_service/src/admin.rs` — *renamed from `src/admin/mod.rs`*; register three routes.
- `crates/pba_service/templates/admin/transaction_detail.html` — new file.
- `crates/pba_service/templates/admin/transactions.html` — link timestamp cell.
- `crates/pba_service/templates/admin/transfers_fragment.html` — link timestamp cell.
- `crates/pba_service/tests/ui_features/admin_ui.feature` — three new scenarios.
- `crates/pba_service/tests/ui_steps.rs` — *renamed from `tests/ui_steps/mod.rs`*; register the new `transaction_steps` module here.
- `crates/pba_service/tests/ui_steps/transaction_steps.rs` — new file.
