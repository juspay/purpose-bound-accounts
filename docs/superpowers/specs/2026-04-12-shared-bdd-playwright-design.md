# Shared BDD Tests + Browser UI Testing

## Summary

Add deposit, payment, and withdrawal pages to the admin UI, then create a browser-based BDD test suite using `cucumber` + `chromiumoxide` (all Rust) that shares the same Gherkin feature files as the existing API tests. Both test binaries execute the same 32+ business scenarios — one via Smithy SDK, one via headless Chrome.

## Goals

1. All account operations (create, deposit, pay, withdraw, status change) available in the admin UI
2. Single set of `.feature` files consumed by both test binaries
3. Step implementations in Rust — API steps (existing) and browser steps (new) interpret the same neutral business steps
4. A small UI-only feature file covers navigation, visual assertions, and HTMX behavior

## Non-goals

- Replacing the API tests with UI tests
- Visual regression / screenshot testing
- Authentication or role-based access (admin UI has none today)

---

## 1. Shared Feature Files

### Location change

Move feature files from `crates/pba_service/tests/features/` to `tests/features/` at workspace root. Both test binaries point here.

**Files moved (unchanged content, minor step wording adjustments where needed):**
- `tests/features/accounts.feature`
- `tests/features/deposits.feature`
- `tests/features/payments.feature`
- `tests/features/withdrawals.feature`
- `tests/features/purpose_types.feature`

### Step phrasing

Steps use neutral business language. No "click", "fill", "call API" — just actions and assertions:

```gherkin
Given a "health" account exists for holder "aaa..." with origin IFSC "HDFC..." and account number "123..."
And the account has 5000 in self-pool and 3000 in others-pool
When I pay 3000 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy purchase"
Then the payment should succeed
And 3000 should come from others-pool
And 0 should come from self-pool
```

The API step implementation calls the Smithy SDK. The browser step implementation navigates to the payment page, fills the form, submits, and reads the result from the account detail page.

### Rust Cucumber adaptation

Update `crates/pba_service/tests/e2e.rs` to load features from `../../tests/features` (relative to crate root). No other API test changes needed — step implementations stay in `crates/pba_service/tests/steps/`.

---

## 2. Admin UI: New Pages

### 2.1 Deposit Page — `/admin/accounts/{id}/deposit`

**GET:** Renders form with fields:
- Amount (paisa, integer input)
- Source IFSC (text)
- Source Account Number (text)

**POST:** Calls `DepositService::deposit()`. On success, redirects to account detail. On error, re-renders form with error message.

**Template:** `templates/admin/deposit.html` (extends `base.html`)

### 2.2 Payment Page — `/admin/accounts/{id}/payment`

**GET:** Renders form with fields:
- Amount (paisa, integer input)
- Merchant ID (text)
- Merchant MCC (text)
- Description (text)

**POST:** Calls `PaymentService::make_payment()`. On success, redirects to account detail. On error, re-renders form with error message.

**Template:** `templates/admin/payment.html` (extends `base.html`)

### 2.3 Withdrawal Page — `/admin/accounts/{id}/withdrawal`

**GET:** Renders form with fields:
- Amount (paisa, integer input)

**POST:** Calls `WithdrawalService::withdraw()`. On success, redirects to account detail. On error, re-renders form with error message.

**Template:** `templates/admin/withdrawal.html` (extends `base.html`)

### 2.4 Account Detail Page Updates

Add action links to the three new pages:
- "Deposit" link → `/admin/accounts/{id}/deposit`
- "Make Payment" link → `/admin/accounts/{id}/payment`
- "Withdraw" link → `/admin/accounts/{id}/withdrawal`

Links are only shown when the account status is `active`.

### 2.5 Routes

New routes in `src/admin/mod.rs`:

```
GET  /admin/accounts/{id}/deposit    → deposit form
POST /admin/accounts/{id}/deposit    → process deposit
GET  /admin/accounts/{id}/payment    → payment form
POST /admin/accounts/{id}/payment    → process payment
GET  /admin/accounts/{id}/withdrawal → withdrawal form
POST /admin/accounts/{id}/withdrawal → process withdrawal
```

### 2.6 Error Display

Each form page shows errors inline (same pattern as create account form). The error message comes from the service layer (insufficient funds, invalid MCC, account not active, etc.).

For successful operations that return result details (e.g., payment split amounts), the redirect to account detail is sufficient — the balance and transaction history on that page show the outcome.

### 2.7 Purpose Types Page — `/admin/purpose-types`

Simple page listing all purpose types with their allowed MCCs. Added to admin nav.

**GET:** Calls `AccountRepo::list_purpose_types()`, renders a table per purpose showing MCC code and description.

**Template:** `templates/admin/purpose_types.html` (extends `base.html`)

---

## 3. Browser Test Crate

### Approach

A new crate `pba_ui_tests` in the workspace using `cucumber` (same BDD crate as the API tests) + `chromiumoxide` for headless Chrome automation. This keeps the entire test stack in Rust.

### Structure

```
crates/pba_ui_tests/
  Cargo.toml
  tests/
    ui_e2e.rs            ← test harness (UiWorld, cucumber main)
    steps/
      mod.rs
      account_steps.rs
      deposit_steps.rs
      payment_steps.rs
      withdrawal_steps.rs
      purpose_steps.rs
    features/
      admin_ui.feature   ← UI-only visual/navigation tests

tests/
  features/              ← shared Gherkin (moved here)
    accounts.feature
    deposits.feature
    payments.feature
    withdrawals.feature
    purpose_types.feature
```

### Cargo.toml

```toml
[package]
name = "pba-ui-tests"
version = "0.1.0"
edition = "2021"

[dev-dependencies]
cucumber = "0.22"
chromiumoxide = { version = "0.7", features = ["tokio-runtime"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures = "0.3"

[[test]]
name = "ui_e2e"
harness = false
```

### UiWorld

```rust
pub struct UiWorld {
    browser: Browser,
    page: Page,
    base_url: String,
    account_id: Option<String>,
    origin_ifsc: Option<String>,
    origin_account_number: Option<String>,
    last_error: Option<String>,
    last_payment_from_others: Option<i64>,
    last_payment_from_self: Option<i64>,
    last_withdrawal_amount: Option<i64>,
    purpose_types_count: Option<usize>,
    last_purpose_code: Option<String>,
    last_purpose_mccs_count: Option<usize>,
    last_balance: Option<BalanceResult>,
    last_account_status: Option<String>,
    duplicate_rejected: bool,
}
```

The `UiWorld` launches a headless Chrome browser via `chromiumoxide` on init and shares a single `Page` across steps within each scenario.

### Step Implementation Patterns

**Given "a {purpose} account exists...":**
1. Navigate to `/admin/accounts`
2. Click the "Create New Account" summary to expand the form
3. Fill holder ID, select purpose from dropdown, fill IFSC and account number
4. Click "Create Account" button
5. Wait for navigation to account detail page
6. Extract account ID from URL path
7. Store account_id, origin_ifsc, origin_account_number in world

**Given "the account has N in self-pool and M in others-pool":**
1. Navigate to `/admin/accounts/{id}/deposit`
2. Fill amount=N, source IFSC=origin IFSC (routes to self-pool), source account=origin account
3. Submit, wait for redirect to account detail
4. Navigate to `/admin/accounts/{id}/deposit` again
5. Fill amount=M, source IFSC=different value (routes to others-pool), source account=different
6. Submit, wait for redirect

**When "I pay N to merchant ... with MCC ... described as ...":**
1. Navigate to `/admin/accounts/{id}/payment`
2. Fill amount, merchant ID, MCC, description
3. Submit form
4. If redirected to account detail → success, read balance from page
5. If error displayed on form → store error text in last_error

**Then "the self contribution should be N":**
1. Navigate to `/admin/accounts/{id}` (or already there from redirect)
2. Find the "Self Pool:" text element, extract the numeric value
3. Convert display format (e.g., "50.00") back to paisa (5000)
4. Assert equals N

**Error assertions ("rejected as insufficient funds"):**
1. Check that the current page shows an error message element
2. Match error text to determine kind (InsufficientFunds, InvalidMcc, AccountNotActive)

### chromiumoxide specifics

- Launch with `BrowserConfig::builder().no_sandbox().build()`
- Use `page.goto(url).await` for navigation
- Use `page.find_element("css selector").await` + `.click().await` / `.type_str().await` for form interaction
- Use `page.wait_for_navigation().await` after form submissions
- Use `page.content().await` or element text extraction for assertions
- Headless by default; can be toggled for debugging

---

## 4. UI-Only Feature File

`crates/pba_ui_tests/tests/features/admin_ui.feature`:

Covers aspects only testable in the browser:
- Dashboard shows correct stat cards after creating accounts
- Account list table renders with correct columns
- Account detail page shows balance breakdown
- Transaction history loads via HTMX (appears after page load)
- Status buttons change based on current status (active → freeze/close, frozen → activate/close, closed → no buttons)
- Navigation between pages works
- Form validation (empty required fields, invalid UUID format)
- Action links hidden on frozen/closed accounts
- Purpose types page lists all purposes with MCCs

---

## 5. Justfile Targets

### New targets

```just
# Run browser UI tests (full cycle)
ui-e2e: e2e-start ui-e2e-run e2e-stop
    @echo "UI E2E tests complete"

# Run browser UI tests only (service must be running)
ui-e2e-run:
    PBA_SERVICE_URL="http://127.0.0.1:{{E2E_APP_PORT}}" cargo test -p pba-ui-tests --test ui_e2e
```

### Updated targets

```just
# Full E2E: both API and UI tests
e2e-all: e2e-start e2e-run ui-e2e-run e2e-stop
    @echo "All E2E tests complete"
```

### Rust Cucumber path update

In `crates/pba_service/tests/e2e.rs`, change feature path from `"tests/features"` to `"../../tests/features"`.

In `crates/pba_ui_tests/tests/ui_e2e.rs`, load shared features from `"../../tests/features"` and UI-only features from `"tests/features"`.

---

## 6. Workspace Updates

Add the new crate to the workspace `Cargo.toml`:

```toml
[workspace]
members = ["crates/pba_service", "crates/pba_client", "crates/pba_ui_tests"]
resolver = "2"
```

---

## 7. Test Infrastructure

Both test binaries share the same test infrastructure:
- PostgreSQL: `pba_service_test` on port 5432
- TigerBeetle: port 3001 with `.tb_data/test/`
- pba-service: port 3031

Test database is reset before each test run (`just e2e-reset-db`). Tests run sequentially (both binaries: `max_concurrent_scenarios(1)`).

The TB data file for tests is recreated each run (accounts need HISTORY flag for transfer queries).

### Chrome requirement

`chromiumoxide` requires a Chrome or Chromium binary. In the Nix flake, add `chromium` to the dev inputs. Outside Nix, document that Chrome/Chromium must be installed.

---

## 8. Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Browser step timing (HTMX loads, redirects) | Use `page.wait_for_navigation()` and element wait selectors |
| Form error text matching across API/UI | Error messages come from same service layer |
| Feature file wording too API-specific | Review and neutralize before moving |
| chromiumoxide API stability | Pin to specific version, wrap in helper functions |
| Chrome not available in CI/dev | Add to Nix flake; document requirement |
| Headless Chrome startup time | Reuse browser instance across scenarios in same feature |

---

## 9. Implementation Order

1. Move feature files to `tests/features/`, update Rust Cucumber path, verify `just e2e` still passes
2. Add deposit/payment/withdrawal pages to admin UI
3. Add purpose types page to admin UI
4. Add action links to account detail page
5. Create `crates/pba_ui_tests/` crate with `cucumber` + `chromiumoxide`
6. Implement browser step definitions for all shared features
7. Implement UI-only feature file and steps
8. Add justfile targets (`ui-e2e`, `ui-e2e-run`, `e2e-all`)
9. Verify both `just e2e` and `just ui-e2e` pass
