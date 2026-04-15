# snake_case API Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change all API field names and URL path parameters from camelCase to snake_case.

**Architecture:** Rename Smithy model member names to snake_case (source of truth), regenerate SDK and OpenAPI spec, remove camelCase serde renaming from service DTOs, and update route path parameters. The Smithy codegen produces the same Rust field names regardless of Smithy member casing, so only the JSON wire format changes.

**Tech Stack:** Smithy 1.55.0, Serde, Axum

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `model/account.smithy` | Modify | Rename members to snake_case |
| `model/deposit.smithy` | Modify | Rename members to snake_case |
| `model/payment.smithy` | Modify | Rename members to snake_case |
| `model/purpose.smithy` | Modify | Rename members to snake_case |
| `model/withdrawal.smithy` | Modify | Rename members to snake_case |
| `crates/pba_service/src/api/dto.rs` | Modify | Remove `serde(rename_all = "camelCase")` |
| `crates/pba_service/src/api/routes.rs` | Modify | Update path parameter names |
| `crates/pba_client/` (generated) | Regenerate | `just smithy-build` |
| `crates/pba_service/src/api/openapi.json` (generated) | Regenerate | `just smithy-build` |

---

### Task 1: Rename Smithy model members to snake_case

**Files:**
- Modify: `model/account.smithy`
- Modify: `model/deposit.smithy`
- Modify: `model/payment.smithy`
- Modify: `model/purpose.smithy`
- Modify: `model/withdrawal.smithy`

- [ ] **Step 1: Update account.smithy**

Replace the full contents of `model/account.smithy` with:

```smithy
$version: "2"
namespace com.ppi.pba

/// Create a new purpose-bound account.
@http(method: "POST", uri: "/accounts", code: 201)
operation CreateAccount {
    input := {
        @required
        holder_id: String

        @required
        purpose_code: String

        @required
        origin_ifsc: String

        @required
        origin_account_number: String
    }
    output := with [AccountMixin] {}
    errors: [PurposeTypeNotFoundError, DuplicateAccountError]
}

/// Get account metadata.
@readonly
@http(method: "GET", uri: "/accounts/{account_id}")
operation GetAccount {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := with [AccountMixin] {}
    errors: [AccountNotFoundError]
}

/// Get pool balances for an account.
@readonly
@http(method: "GET", uri: "/accounts/{account_id}/balance")
operation GetBalance {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := {
        @required
        account_id: String

        @required
        self_contribution: Money

        @required
        others_contribution: Money

        @required
        total: Money

        @required
        pending_self: Money

        @required
        pending_others: Money
    }
    errors: [AccountNotFoundError]
}

/// Update account status (freeze, close, reactivate).
@http(method: "PATCH", uri: "/accounts/{account_id}/status")
operation UpdateAccountStatus {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        status: Status
    }
    output := with [AccountMixin] {}
    errors: [AccountNotFoundError]
}

/// Shared account fields.
@mixin
structure AccountMixin {
    @required
    id: String

    @required
    holder_id: String

    @required
    purpose_code: String

    @required
    origin_ifsc: String

    @required
    origin_account_number: String

    vpa: String
    virtual_ifsc: String
    virtual_account_number: String

    @required
    kyc_tier: String

    @required
    status: String

    @required
    created_at: String

    @required
    updated_at: String
}

@error("client")
@httpError(404)
structure AccountNotFoundError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(409)
structure AccountNotActiveError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(409)
structure DuplicateAccountError {
    @required
    error: String
    @required
    message: String
}
```

- [ ] **Step 2: Update deposit.smithy**

Replace the full contents of `model/deposit.smithy` with:

```smithy
$version: "2"
namespace com.ppi.pba

/// Deposit funds into a purpose-bound account.
/// Automatically routes to self-contribution or others-contribution pool
/// based on whether the source matches the account's origin bank.
/// Set `pending` to true for two-phase deposits (pending → post/void).
@http(method: "POST", uri: "/accounts/{account_id}/deposits", code: 201)
operation Deposit {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        source_ifsc: String

        @required
        source_account_number: String

        @required
        amount: Money

        pending: Boolean

        gateway_ref: String

        timeout_seconds: Integer
    }
    output := {
        @required
        deposit_id: String

        @required
        account_id: String

        @required
        amount: Money

        @required
        pool: String

        @required
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, AccountNotActiveError]
}

/// Confirm a pending deposit (post the held funds).
@http(method: "POST", uri: "/accounts/{account_id}/deposits/{deposit_id}/post")
operation PostDeposit {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        deposit_id: String
    }
    output := {
        @required
        deposit_id: String

        @required
        account_id: String

        @required
        amount: Money

        @required
        pool: String

        @required
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, DepositNotFoundError, DepositNotPendingError]
}

/// Cancel a pending deposit (void the held funds).
@http(method: "POST", uri: "/accounts/{account_id}/deposits/{deposit_id}/void")
operation VoidDeposit {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        deposit_id: String

        reason: String
    }
    output := {
        @required
        deposit_id: String

        @required
        account_id: String

        @required
        amount: Money

        @required
        pool: String

        @required
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, DepositNotFoundError, DepositNotPendingError]
}

@error("client")
@httpError(404)
structure DepositNotFoundError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(409)
structure DepositNotPendingError {
    @required
    error: String
    @required
    message: String
}
```

- [ ] **Step 3: Update payment.smithy**

Replace the full contents of `model/payment.smithy` with:

```smithy
$version: "2"
namespace com.ppi.pba

/// Make a payment from a purpose-bound account.
/// Validates the merchant's MCC against the account's purpose type.
/// Uses others-contribution pool first, then self-contribution.
@http(method: "POST", uri: "/accounts/{account_id}/payments", code: 201)
operation MakePayment {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money

        @required
        merchant_mcc: String

        @required
        merchant_id: String

        @required
        description: String
    }
    output := {
        @required
        account_id: String

        @required
        amount: Money

        @required
        from_others: Money

        @required
        from_self: Money

        @required
        merchant_id: String

        @required
        merchant_mcc: String
    }
    errors: [
        AccountNotFoundError
        AccountNotActiveError
        InvalidMccError
        InsufficientFundsError
    ]
}

@error("client")
@httpError(422)
structure InvalidMccError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(422)
structure InsufficientFundsError {
    @required
    error: String
    @required
    message: String
}
```

- [ ] **Step 4: Update purpose.smithy**

Replace the full contents of `model/purpose.smithy` with:

```smithy
$version: "2"
namespace com.ppi.pba

/// List all available purpose types.
@readonly
@http(method: "GET", uri: "/purpose-types")
operation ListPurposeTypes {
    output := {
        @required
        purpose_types: PurposeTypeList
    }
}

/// Get a specific purpose type and its allowed MCCs.
@readonly
@http(method: "GET", uri: "/purpose-types/{purpose_code}")
operation GetPurposeType {
    input := {
        @required
        @httpLabel
        purpose_code: String
    }
    output := {
        @required
        purpose_code: String

        @required
        allowed_mccs: MccEntryList
    }
    errors: [PurposeTypeNotFoundError]
}

list PurposeTypeList {
    member: PurposeTypeSummary
}

structure PurposeTypeSummary {
    @required
    purpose_code: String

    @required
    allowed_mccs: MccEntryList
}

list MccEntryList {
    member: MccEntry
}

structure MccEntry {
    @required
    mcc: String

    description: String
}

@error("client")
@httpError(404)
structure PurposeTypeNotFoundError {
    @required
    error: String
    @required
    message: String
}
```

- [ ] **Step 5: Update withdrawal.smithy**

Replace the full contents of `model/withdrawal.smithy` with:

```smithy
$version: "2"
namespace com.ppi.pba

/// Withdraw funds from the self-contribution pool only.
/// Cannot withdraw from the others-contribution pool.
@http(method: "POST", uri: "/accounts/{account_id}/withdrawals", code: 201)
operation Withdraw {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money
    }
    output := {
        @required
        account_id: String

        @required
        amount: Money
    }
    errors: [
        AccountNotFoundError
        AccountNotActiveError
        InsufficientFundsError
    ]
}
```

- [ ] **Step 6: Validate Smithy models**

Run: `smithy validate model/`

Expected: Validation passes (may show info messages, no errors).

- [ ] **Step 7: Commit Smithy changes**

```bash
git add model/account.smithy model/deposit.smithy model/payment.smithy model/purpose.smithy model/withdrawal.smithy
git commit -m "refactor: rename Smithy model members from camelCase to snake_case"
```

---

### Task 2: Regenerate SDK and OpenAPI spec

**Files:**
- Regenerate: `crates/pba_client/` (entire directory)
- Regenerate: `crates/pba_service/src/api/openapi.json`

- [ ] **Step 1: Run smithy-build to regenerate artifacts**

Run: `just smithy-build`

Expected: Build succeeds. Both SDK and OpenAPI spec are regenerated with snake_case field names.

- [ ] **Step 2: Verify OpenAPI spec uses snake_case**

Run: `grep -o '"[a-z_]*Id"' crates/pba_service/src/api/openapi.json | head -5`

Expected: Output shows `"account_id"`, `"holder_id"`, `"deposit_id"` (snake_case, not camelCase).

- [ ] **Step 3: Commit regenerated artifacts**

```bash
git add crates/pba_client/ crates/pba_service/src/api/openapi.json
git commit -m "chore: regenerate SDK and OpenAPI spec with snake_case field names"
```

---

### Task 3: Update service DTOs and routes

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`
- Modify: `crates/pba_service/src/api/routes.rs`

- [ ] **Step 1: Remove camelCase serde renaming from dto.rs**

In `crates/pba_service/src/api/dto.rs`, remove every occurrence of `#[serde(rename_all = "camelCase")]`. There are 10 occurrences on these structs:

- `CreateAccountRequest`
- `AccountResponse`
- `BalanceResponse`
- `DepositRequest`
- `DepositResponse`
- `VoidDepositRequest`
- `PaymentRequest`
- `PaymentResponse`
- `WithdrawalRequest`
- `WithdrawalResponse`
- `UpdateStatusRequest`
- `ListPurposeTypesResponse`
- `PurposeTypeResponse`
- `MccEntryResponse`

Delete every line containing `#[serde(rename_all = "camelCase")]` in the file. The Rust field names are already snake_case, so they will now serialize as snake_case on the wire.

- [ ] **Step 2: Update route path parameters in routes.rs**

In `crates/pba_service/src/api/routes.rs`, replace all camelCase path parameters:

Replace `{accountId}` with `{account_id}` (7 occurrences).
Replace `{depositId}` with `{deposit_id}` (2 occurrences).
Replace `{purposeCode}` with `{purpose_code}` (1 occurrence).

The full `create_router` function should become:

```rust
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Account operations
        .route("/accounts", post(handlers::create_account))
        .route("/accounts/{account_id}", get(handlers::get_account))
        .route(
            "/accounts/{account_id}/status",
            patch(handlers::update_account_status),
        )
        // Balance
        .route("/accounts/{account_id}/balance", get(handlers::get_balance))
        // Deposit
        .route("/accounts/{account_id}/deposits", post(handlers::deposit))
        .route(
            "/accounts/{account_id}/deposits/{deposit_id}/post",
            post(handlers::post_deposit),
        )
        .route(
            "/accounts/{account_id}/deposits/{deposit_id}/void",
            post(handlers::void_deposit),
        )
        // Payment
        .route(
            "/accounts/{account_id}/payments",
            post(handlers::make_payment),
        )
        // Withdrawal
        .route(
            "/accounts/{account_id}/withdrawals",
            post(handlers::withdraw),
        )
        // Purpose types
        .route("/purpose-types", get(handlers::list_purpose_types))
        .route(
            "/purpose-types/{purpose_code}",
            get(handlers::get_purpose_type),
        )
        // API Docs
        .route("/docs", get(handlers::swagger_ui))
        .route("/docs/openapi.json", get(handlers::openapi_json))
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build`

Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/api/dto.rs crates/pba_service/src/api/routes.rs
git commit -m "refactor: remove camelCase serde renaming and update route path params to snake_case"
```

---

### Task 4: Run tests and verify

**Files:** None (verification only)

- [ ] **Step 1: Start infrastructure**

Run: `just services-start`

Expected: PostgreSQL and TigerBeetle start.

- [ ] **Step 2: Run E2E tests**

Run: `cargo test --test e2e -- --retry 0`

Expected: All scenarios pass. The SDK and service now both use snake_case on the wire, so they agree.

- [ ] **Step 3: Run UI E2E tests**

Run: `cargo test --test ui_e2e -- --retry 0`

Expected: All UI scenarios pass. Templates use snake_case form field names which already match the Rust struct field names.

- [ ] **Step 4: Stop infrastructure**

Run: `just services-stop`

- [ ] **Step 5: Verify Swagger UI shows snake_case (manual)**

Run: `just run-all`

Open `http://localhost:3030/docs` in a browser.

Expected: All schema properties show snake_case names (e.g., `holder_id`, `purpose_code`, `account_id`, `self_contribution`).

Run: `just stop-all`
