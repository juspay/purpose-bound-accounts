# snake_case API Migration Design

## Goal

Change all API field names and URL path parameters from camelCase to snake_case across the entire stack, from Smithy models through to the wire format.

## Architecture

Rename Smithy model member names from camelCase to snake_case. This is the source of truth — the generated SDK and OpenAPI spec inherit field names from Smithy. Then update the service layer to stop renaming snake_case Rust fields to camelCase on the wire. Update URL path parameters to match.

## Changes by Layer

### 1. Smithy Models (`model/*.smithy`)

Rename all member names from camelCase to snake_case. Examples:

| Before | After |
|--------|-------|
| `holderId` | `holder_id` |
| `purposeCode` | `purpose_code` |
| `originIfsc` | `origin_ifsc` |
| `originAccountNumber` | `origin_account_number` |
| `kycTier` | `kyc_tier` |
| `createdAt` | `created_at` |
| `updatedAt` | `updated_at` |
| `selfContribution` | `self_contribution` |
| `othersContribution` | `others_contribution` |
| `pendingSelf` | `pending_self` |
| `pendingOthers` | `pending_others` |
| `sourceIfsc` | `source_ifsc` |
| `sourceAccountNumber` | `source_account_number` |
| `gatewayRef` | `gateway_ref` |
| `timeoutSeconds` | `timeout_seconds` |
| `depositId` | `deposit_id` |
| `accountId` | `account_id` |
| `merchantMcc` | `merchant_mcc` |
| `merchantId` | `merchant_id` |
| `fromOthers` | `from_others` |
| `fromSelf` | `from_self` |
| `allowedMccs` | `allowed_mccs` |
| `purposeType` | `purpose_type` |
| `purposeTypes` | `purpose_types` |

Affected files: `account.smithy`, `deposit.smithy`, `payment.smithy`, `purpose.smithy`, `withdrawal.smithy`

### 2. Regenerate Artifacts (`just smithy-build`)

Running `just smithy-build` regenerates:
- **SDK** (`crates/pba_client/`) — builder methods and JSON serialization will use snake_case
- **OpenAPI spec** (`crates/pba_service/src/api/openapi.json`) — schema properties will use snake_case

No manual changes needed in these generated files.

### 3. Service DTOs (`crates/pba_service/src/api/dto.rs`)

Remove all `#[serde(rename_all = "camelCase")]` attributes. Rust struct fields are already snake_case (`holder_id`, `purpose_code`, etc.), so without the rename attribute they serialize directly as snake_case on the wire.

### 4. URL Path Parameters (`crates/pba_service/src/api/routes.rs`)

Update path parameter names in route definitions:

| Before | After |
|--------|-------|
| `{accountId}` | `{account_id}` |
| `{depositId}` | `{deposit_id}` |
| `{purposeCode}` | `{purpose_code}` |

### 5. Admin Templates (`crates/pba_service/templates/*.html`)

Update any Tera template references that use camelCase JSON field names to snake_case.

### 6. Tests

Test step files may need updates if they reference camelCase field names in JSON payloads or assertions. The SDK builder method names (Rust side) are already snake_case, so those should be unaffected.

## Files Changed

| File | Change |
|------|--------|
| `model/account.smithy` | Rename members to snake_case |
| `model/deposit.smithy` | Rename members to snake_case |
| `model/payment.smithy` | Rename members to snake_case |
| `model/purpose.smithy` | Rename members to snake_case |
| `model/withdrawal.smithy` | Rename members to snake_case |
| `crates/pba_service/src/api/dto.rs` | Remove `serde(rename_all = "camelCase")` |
| `crates/pba_service/src/api/routes.rs` | Update path parameter names |
| `crates/pba_service/templates/*.html` | Update field name references |
| `crates/pba_client/` (generated) | Regenerated via `just smithy-build` |
| `crates/pba_service/src/api/openapi.json` (generated) | Regenerated via `just smithy-build` |

## Files NOT Changed

- No new Cargo dependencies
- No new modules
- Domain layer (`domain/`) already uses snake_case internally
- No changes to service logic or business rules

## Testing

- Run `just smithy-build` and verify OpenAPI spec uses snake_case field names
- Run `cargo build` to verify compilation
- Run E2E tests (`cargo test`) to verify all scenarios pass
- Start service and verify Swagger UI at `/docs` shows snake_case fields
