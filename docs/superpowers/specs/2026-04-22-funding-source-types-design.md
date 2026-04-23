# Funding Source Types Design

## Context

The PBA service currently uses a single `FUNDING_SOURCE_TB_ID` sentinel account as the counterparty for all deposits, regardless of who is depositing. The pool (self/others) is determined algorithmically by comparing the deposit source IFSC/account with the PBA account's origin details, but the funding source itself is undifferentiated in TigerBeetle.

We need three distinct funding source types to better categorize deposit origins:

- **Self** — the account holder depositing from their own bank account (IFSC/account matches PBA origin)
- **Trust** — big-ticket contributions from organizations (e.g., employer, government)
- **Third-party** — individual small-ticket contributions from other people

## Design

### TigerBeetle Sentinel Accounts

Replace the single `FUNDING_SOURCE_TB_ID` with three sentinel accounts:

| Constant | TB ID | Role |
|----------|-------|------|
| `SELF_FUNDING_SOURCE_TB_ID` | `u128::MAX - 10` | Counterparty for self-deposits (reuses existing slot) |
| `TRUST_FUNDING_SOURCE_TB_ID` | `u128::MAX - 13` | Counterparty for trust deposits |
| `THIRD_PARTY_FUNDING_SOURCE_TB_ID` | `u128::MAX - 14` | Counterparty for third-party deposits |
| `MERCHANT_SETTLEMENT_TB_ID` | `u128::MAX - 11` | Counterparty for payments (unchanged) |
| `WITHDRAWAL_SETTLEMENT_TB_ID` | `u128::MAX - 12` | Counterparty for withdrawals (unchanged) |

All five are created at startup in `init_sentinel_accounts()` with `CODE_SENTINEL = 99` and `LEDGER_INR_PAISA = 1`.

The deposit flow selects the sentinel based on:

- IFSC/account matches origin → `SELF_FUNDING_SOURCE_TB_ID`
- Non-match + `funding_type = "trust"` → `TRUST_FUNDING_SOURCE_TB_ID`
- Non-match + `funding_type = "third_party"` → `THIRD_PARTY_FUNDING_SOURCE_TB_ID`

Withdrawal flow remains unchanged — debits self-pool to `WITHDRAWAL_SETTLEMENT_TB_ID`.

### Fund Flow Diagram

```
SELF_FUNDING_SOURCE  →  PBA self-pool   →  WITHDRAWAL_SETTLEMENT
TRUST_FUNDING_SOURCE →  PBA others-pool
THIRD_PARTY_FUNDING  →  PBA others-pool
                        PBA (either)    →  MERCHANT_SETTLEMENT
```

Each sentinel's TB balance is a clean aggregate of one flow type.

### Postgres Schema Change

Add a `funding_type` column to the `transactions` table via a new migration:

```sql
ALTER TABLE transactions ADD COLUMN funding_type TEXT;
```

- Nullable — payments and withdrawals will have `NULL`
- New deposits always populate with `"self"`, `"trust"`, or `"third_party"`
- No backfill of existing rows needed

### Deposit API Changes

**Smithy model** — add optional `funding_type` to deposit input:

```
funding_type: FundingType

@enum([
    { value: "trust", name: "TRUST" },
    { value: "third_party", name: "THIRD_PARTY" }
])
string FundingType
```

The field is part of the POST body (not a query parameter), consistent with existing deposit input fields.

No `"self"` variant — self is auto-detected, never caller-supplied.

**Deposit service logic:**

```
if IFSC/account matches origin:
    funding_type = "self"
    sentinel = SELF_FUNDING_SOURCE_TB_ID
    pool = "self"
    (ignore any funding_type input)
else:
    if funding_type is None → reject 400 "funding_type required for non-origin deposits"
    if funding_type == "trust":
        sentinel = TRUST_FUNDING_SOURCE_TB_ID
    else:
        sentinel = THIRD_PARTY_FUNDING_SOURCE_TB_ID
    pool = "others"
```

**Deposit response** — add `funding_type` field (always present: "self", "trust", or "third_party").

**TransactionSummary** — add `funding_type` as optional string (null for payments/withdrawals).

### Admin UI Changes

#### Transactions Page (`/admin/transactions`)

Add a "Funding Type" column to the existing transactions table, showing "self", "trust", "third_party", or "—" for payments/withdrawals.

#### System Accounts Page (`/admin/system-accounts`)

New admin page with two sections using consistent columns.

**Section 1: Sentinel Accounts** (from TigerBeetle `lookup_accounts`)

| Account | Credits Posted | Debits Posted | Credits Pending | Debits Pending |
|---------|---------------|---------------|-----------------|----------------|
| Self Funding Source | ... | ... | ... | ... |
| Trust Funding Source | ... | ... | ... | ... |
| Third Party Funding Source | ... | ... | ... | ... |
| Merchant Settlement | ... | ... | ... | ... |
| Withdrawal Settlement | ... | ... | ... | ... |

**Section 2: PBA Pool Balances** (aggregated from Postgres transactions table)

| Account | Credits Posted | Debits Posted | Credits Pending | Debits Pending |
|---------|---------------|---------------|-----------------|----------------|
| Self Pool (all accounts) | ... | ... | ... | ... |
| Others Pool (all accounts) | ... | ... | ... | ... |

Column mapping from Postgres:
- Credits Posted = SUM(amount) WHERE direction='inbound' AND status='posted'
- Debits Posted = SUM(amount) WHERE direction='outbound' AND status='posted'
- Credits Pending = SUM(amount) WHERE direction='inbound' AND status='pending'
- Debits Pending = SUM(amount) WHERE direction='outbound' AND status='pending'

Grouped by pool ('self', 'others').

This enables visual reconciliation — e.g., Self Funding Source debits posted should roughly equal Self Pool credits posted.

- Sentinel data from single `lookup_accounts` call to TB for 5 IDs
- Pool data from existing `pool_summary()` Postgres query (extended for pending breakdown)
- Amounts displayed in rupees (paisa / 100)
- "System Accounts" link added to the nav bar
- No pagination — fixed number of rows

### E2E Tests

#### API Tests (`funding_types.feature`)

1. Self-deposit auto-detection — deposit from origin IFSC/account, verify `funding_type = "self"` in response
2. Trust deposit — deposit from non-matching source with `funding_type = "trust"`, verify pool = "others" and `funding_type = "trust"`
3. Third-party deposit — same pattern, verify `funding_type = "third_party"`
4. Missing funding_type for non-origin deposit — expect 400 error
5. Transactions listing shows funding_type

#### UI Tests (added to `admin_ui.feature`)

1. System accounts page loads and shows all 5 sentinel accounts and PBA pool balances
2. Transactions page shows funding type column

## What Stays the Same

- Pool determination logic (self IFSC match → self-pool, non-match → others-pool)
- Two-phase deposit flow (pending/post/void)
- Withdrawal only from self-pool, routed to `WITHDRAWAL_SETTLEMENT_TB_ID`
- Payment flow unchanged
- Merchant settlement unchanged
