# `gateway_ref` Symmetry Across Transaction APIs — Design

**Date:** 2026-05-05
**Status:** Approved

## Goal

Make `gateway_ref` a first-class optional reconciliation reference on payments and withdrawals, mirroring its existing role on deposits. End-to-end coverage: API input + output + admin UI form + transaction detail page + tests.

## Background

`gateway_ref` is a caller-supplied opaque string that correlates a PBA transaction with the originating upstream system (payment gateway txn id, switch ref, card-network auth code, etc.). Today only `Deposit` accepts and echoes it; `MakePayment` and `Withdraw` do not, despite `TransactionRecord.gateway_ref` already existing on the unified domain record (`crates/pba_service/src/domain/transaction.rs:107`). This is an inconsistency, not a domain-layer limitation.

## Non-goals

- No changes to the `Deposit` API or its templates (already correct).
- No new fields beyond `gateway_ref` on payment / withdrawal.
- No domain-layer changes — the storage column and Rust field already exist.
- No retroactive backfill or migration of historical data.

## Scope

| Area | Change |
|---|---|
| Smithy | Add optional `gateway_ref: String` to `MakePayment` and `Withdraw` inputs and outputs. |
| SDK | Regenerate `pba_client` from updated Smithy. |
| Service | `payment_service::make_payment` and `withdrawal_service::withdraw` gain `gateway_ref: Option<&str>` parameter; thread into existing `TransactionRecord` write site (the slot is presently `None`). |
| API DTOs | Add `gateway_ref: Option<String>` to `PaymentInput`, `WithdrawInput`, `PaymentOutput`, `WithdrawOutput` in `crates/pba_service/src/api/dto.rs`. |
| API handlers | `crates/pba_service/src/api/handlers.rs` — pass `gateway_ref` from request into service; populate response from returned `TransactionRecord`. |
| Admin UI forms | `templates/admin/payment.html` and `templates/admin/withdrawal.html` — add the Gateway Reference input block, mirroring `deposit.html:44-47`. |
| Admin form structs | `crates/pba_service/src/admin/handlers.rs` — extend `PaymentForm` and `WithdrawalForm` with `gateway_ref: Option<String>`; pass into service call. |
| Transaction detail page | `templates/admin/transaction_detail.html` — payment branch: append Gateway Ref line to the Merchant card; withdrawal branch: replace the "no source/merchant" placeholder with a Gateway Ref field and rename the card header to `Reference`. |
| Tests | Extend payment + withdrawal API e2e and UI e2e scenarios; extend / add transaction-detail unit tests. |

## Architecture

The change is purely additive plumbing — no new components, no new data flow.

```
POST /accounts/{id}/payments      ──► PaymentInput.gateway_ref
                                  ──► payment_service.make_payment(..., gateway_ref)
                                  ──► TransactionRecord { gateway_ref, .. }    [Postgres]
                                  ──► PaymentOutput.gateway_ref

POST /accounts/{id}/withdrawals   ──► WithdrawInput.gateway_ref
                                  ──► withdrawal_service.withdraw(..., gateway_ref)
                                  ──► TransactionRecord { gateway_ref, .. }    [Postgres]
                                  ──► WithdrawOutput.gateway_ref
```

Smithy additions are optional fields; existing callers compile and run unchanged. SDK regen produces new optional setter methods. The Postgres column and the Rust `TransactionRecord` field already exist from the deposit implementation.

## UI changes

### Admin form pages

`payment.html` and `withdrawal.html` each gain one new block immediately above the submit button, copied verbatim from `deposit.html:44-47`:

```html
<label>
    Gateway Reference (optional)
    <input type="text" name="gateway_ref" placeholder="e.g. gw-txn-12345">
</label>
```

### Transaction detail page

In `templates/admin/transaction_detail.html`:

- **Payment branch** (`{% else if is_payment %}` block) — append a fourth `<p>` to the existing Merchant card showing `Gateway Ref: {{ gateway_ref }}` (using the `—` fallback the template already renders for absent values).
- **Withdrawal branch** (`{% else if is_withdrawal %}` block) — replace the current single-line placeholder with a Gateway Ref field. Rename the card header from `Source / Merchant` to `Reference`.

The `gateway_ref` template variable is already populated on the `TransactionDetailTemplate` struct from the existing deposit-detail work — no template-struct changes needed.

## Error handling

No new error paths. `gateway_ref` is opaque, optional, and untouched by business logic. Validation is bounded only by Postgres column length on the existing column.

## Testing

### Unit (in `src/admin/handlers.rs`)

- Extend `transaction_detail_template_renders_payment_section_for_payment` — assert `gateway_ref` appears in the rendered Merchant card when set.
- Add `transaction_detail_template_renders_gateway_ref_for_withdrawal` — withdrawal-shaped struct with a `gateway_ref` value; assert the Reference card renders it.
- Add `transaction_detail_template_renders_dash_for_absent_gateway_ref_on_withdrawal` — same shape with `gateway_ref = None`; assert `—` fallback.

### API e2e Cucumber

Extend `tests/features/payments.feature` and `tests/features/withdrawals.feature`:

- One new scenario per flow: caller supplies `gateway_ref` in the request, response echoes the same value, the resulting transaction's `gateway_ref` is queryable via the transaction list / get endpoints.

### UI e2e Cucumber

Extend `tests/ui_features/admin_ui.feature`:

- Payment scenario: fill the Gateway Reference field in the payment form; navigate to the resulting transaction detail page; assert Gateway Ref is shown in the Merchant card.
- Withdrawal scenario: same shape — fill the field on withdrawal form; assert it appears in the Reference card.

Reuse existing UI step helpers; no new step modules needed.

## Implementation file map

- `model/payment.smithy` — add `gateway_ref` to input + output.
- `model/withdrawal.smithy` — add `gateway_ref` to input + output.
- `crates/pba_client/**` — regenerated SDK.
- `crates/pba_service/src/api/dto.rs` — extend four DTOs.
- `crates/pba_service/src/api/handlers.rs` — thread `gateway_ref` through payment + withdrawal handlers.
- `crates/pba_service/src/service/payment_service.rs` — add parameter, populate `TransactionRecord`.
- `crates/pba_service/src/service/withdrawal_service.rs` — same.
- `crates/pba_service/src/admin/handlers.rs` — extend `PaymentForm` / `WithdrawalForm`; pass into services; extend / add unit tests.
- `crates/pba_service/templates/admin/payment.html` — new input block.
- `crates/pba_service/templates/admin/withdrawal.html` — new input block.
- `crates/pba_service/templates/admin/transaction_detail.html` — payment Merchant card, withdrawal Reference card.
- `crates/pba_service/tests/features/payments.feature` — extend with a gateway_ref roundtrip scenario.
- `crates/pba_service/tests/features/withdrawals.feature` — same.
- `crates/pba_service/tests/ui_features/admin_ui.feature` — extend payment + withdrawal UI scenarios.
