# Shared BDD Tests + Playwright UI Testing

## Summary

Add deposit, payment, and withdrawal pages to the admin UI, then create a Playwright BDD test suite that shares the same Gherkin feature files as the existing Rust Cucumber tests. Both test runners execute the same 32+ business scenarios — one via API, one via browser.

## Goals

1. All account operations (create, deposit, pay, withdraw, status change) available in the admin UI
2. Single set of `.feature` files consumed by both Rust Cucumber and Playwright BDD
3. Step implementations in Rust (API) and TypeScript (UI) interpret the same neutral business steps
4. A small UI-only feature file covers navigation, visual assertions, and HTMX behavior

## Non-goals

- Replacing the API tests with UI tests
- Visual regression / screenshot testing
- Authentication or role-based access (admin UI has none today)

---

## 1. Shared Feature Files

### Location change

Move feature files from `crates/pba_service/tests/features/` to `tests/features/` at workspace root. Both test runners point here.

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

The Rust step implementation calls the API. The Playwright step implementation navigates to the payment page, fills the form, submits, and reads the result from the account detail page.

### Rust Cucumber adaptation

Update `crates/pba_service/tests/e2e.rs` to load features from `../../tests/features` (relative to crate root). No other Rust test changes needed — step implementations stay in `crates/pba_service/tests/steps/`.

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

---

## 3. Playwright Project

### Structure

```
tests/
  features/                    ← shared Gherkin (moved here)
    accounts.feature
    deposits.feature
    payments.feature
    withdrawals.feature
    purpose_types.feature
  playwright/
    package.json
    playwright.config.ts
    tsconfig.json
    steps/
      account_steps.ts
      deposit_steps.ts
      payment_steps.ts
      withdrawal_steps.ts
      purpose_steps.ts
    features/
      admin_ui.feature         ← UI-only visual/navigation tests
```

### Dependencies

```json
{
  "devDependencies": {
    "@playwright/test": "^1.50",
    "playwright-bdd": "^8",
    "@cucumber/cucumber": "^11"
  }
}
```

### Configuration

`playwright.config.ts`:
- Base URL: `http://127.0.0.1:3031` (test service)
- Browser: chromium (headless)
- Feature paths: `../features/**/*.feature` + `features/**/*.feature`
- Step paths: `steps/**/*.ts`

### Step Implementations

Each TypeScript step file mirrors the Rust step file. Key patterns:

**Given "a {purpose} account exists...":**
1. Navigate to `/admin/accounts`
2. Open the create form (click details/summary)
3. Fill holder ID, select purpose, fill IFSC and account number
4. Submit form
5. Extract account ID from redirect URL
6. Store in world context

**Given "the account has N in self-pool and M in others-pool":**
1. Navigate to `/admin/accounts/{id}/deposit`
2. Fill amount with N, source IFSC matching origin (self-pool), submit
3. Navigate to deposit page again
4. Fill amount with M, source IFSC different from origin (others-pool), submit

**When "I pay N to merchant ... with MCC ... described as ...":**
1. Navigate to `/admin/accounts/{id}/payment`
2. Fill amount, merchant ID, MCC, description
3. Submit form
4. Check result: if redirected to detail → success; if error shown → capture error

**Then "the self contribution should be N":**
1. Navigate to `/admin/accounts/{id}` (or already there)
2. Read self pool balance text from the page
3. Assert value matches N (converted from paisa to display format)

**Error assertions ("rejected as insufficient funds"):**
1. After form submission, check that the page shows an error message
2. Match error text against expected error type

### World / Context

Playwright BDD uses a shared test context (similar to Cucumber World):
- `accountId: string` — current account under test
- `originIfsc: string` — origin bank IFSC for self-pool routing
- `originAccountNumber: string` — origin bank account number
- `lastError: string | null` — captured error message from form

---

## 4. UI-Only Feature File

`tests/playwright/features/admin_ui.feature`:

Covers aspects only testable in the browser:
- Dashboard shows correct stat cards after creating accounts
- Account list table renders with correct columns
- Account detail page shows balance breakdown
- Transaction history loads via HTMX (appears after page load)
- Status buttons change based on current status (active → freeze/close, frozen → activate/close, closed → no buttons)
- Navigation between pages works (breadcrumb-style links)
- Form validation (empty required fields, invalid UUID format)
- Action links hidden on frozen/closed accounts

---

## 5. Justfile Targets

### New targets

```just
# Install Playwright dependencies
playwright-install:
    cd tests/playwright && npm install && npx playwright install chromium

# Run Playwright BDD tests (full cycle)
playwright: e2e-start
    cd tests/playwright && npx bddgen && npx playwright test
    just e2e-stop

# Run Playwright tests only (service must be running)
playwright-run:
    cd tests/playwright && npx bddgen && npx playwright test
```

### Updated targets

```just
# Rust Cucumber now reads from workspace-root tests/features/
e2e-run:
    PBA_SERVICE_URL="http://127.0.0.1:3031" cargo test -p pba-service --test e2e

# Full E2E: both API and UI tests
e2e-all: e2e-start e2e-run playwright-run e2e-stop
```

### Rust Cucumber path update

In `crates/pba_service/tests/e2e.rs`, change feature path from `"tests/features"` to `"../../tests/features"`.

---

## 6. Test Infrastructure

Both Rust Cucumber and Playwright share the same test infrastructure:
- PostgreSQL: `pba_service_test` on port 5432
- TigerBeetle: port 3001 with `.tb_data/test/`
- pba-service: port 3031

Test database is reset before each test run (`just e2e-reset-db`). Tests run sequentially (Cucumber: `max_concurrent_scenarios(1)`, Playwright: `workers: 1`).

The TB data file for tests is recreated each run (accounts need HISTORY flag for transfer queries).

---

## 7. Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Playwright step timing (HTMX loads) | Use `page.waitForSelector` for async content |
| Form error text matching across API/UI | Error messages come from same service layer |
| Feature file wording too API-specific | Review and neutralize before moving |
| Flaky browser tests | Single worker, explicit waits, retry config |
| Purpose types step ("I list all purpose types") has no UI equivalent | Add purpose types to admin nav or skip in Playwright |

### Purpose types handling

The `purpose_types.feature` scenarios (list, get, not-found) test metadata endpoints. The admin UI doesn't have a dedicated purpose types page. Options:
- Add a simple `/admin/purpose-types` page listing purposes and their MCCs
- Or mark these scenarios with a `@api-only` tag and skip in Playwright

Recommendation: Add a simple purpose types page — it's useful for the admin anyway and keeps full scenario coverage.

---

## 8. Implementation Order

1. Move feature files to `tests/features/`, update Rust Cucumber path, verify `just e2e` still passes
2. Add deposit/payment/withdrawal pages to admin UI
3. Add purpose types page to admin UI
4. Add action links to account detail page
5. Scaffold Playwright project (`tests/playwright/`)
6. Implement Playwright step definitions
7. Implement UI-only feature file
8. Add justfile targets
9. Verify both `just e2e` and `just playwright` pass
