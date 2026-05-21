HSA account is an account that holds INR (Indian Rupee) with the following characteristics:

1.  It is setup with an origin bank account i.e. a combination of IFSC code and bank account
2.  It has two internal pools
  1. self-contribution
  2. others-contribution
3.  But to the external world - the HSA account looks like one account with its own VPA / IFSC code + account number
4.  When remittances come from the origin bank account the money is deposited in the self-contribution pool
5.  When remittances come from any other bank account the money is deposited in the others-contribution pool
6.  When payments are made - the others-contribution pool funds are first depleted after which the self-contribution pool are utilized
7.  When withdrawals are made - only the funds in the self-contribution pool are utilized
8.  When payments are made - the merchant MCCs are restricted to the ones that provide medical services / goods.

We would like to create a Axum based service that models the above HSA domain model but uses TigerBeetle as the transaction engine leveraging native TigerBeetle's linked accounts mechanisms.

The HSA account could be modelled something on the following lines:

hsa_wallets:
  id (UUID) ─── maps to TB account IDs
  holder_id (FK to identity/IAM)
  origin_ifsc
  origin_account_number
  vpa
  virtual_ifsc
  virtual_account_number
  kyc_tier
  status (active/frozen/closed)
  created_at

hsa_tb_accounts:
  hsa_wallet_id (FK)
  tb_account_id (u128)
  pool_type (self_contribution | others_contribution)


## Normal accounts and reversing a sponsor transfer

Normal accounts (introduced in the Phase 2/3 normal-accounts work) hold
trust-sourced funds and act as the inbound funding container for purpose-bound
accounts. A sponsor's bank deposit lands in a normal account, then moves into
a PB account's others-pool via an internal transfer.

If, after a posted transfer, the PB account holder turns out not to meet the
sponsor's matching requirements, an admin can reverse the transfer. The
reversal is recorded as a new compensating transaction pair (debit on the PB
others-pool, credit back to the normal account) plus a TigerBeetle transfer
in the opposite direction. The original transfer rows are not mutated; the
reversal links back via `reverses_transaction_id`.

Constraints:

- Only `posted` transfers are reversible. Pending transfers continue to be
  cancelled via `VoidNormalAccountTransfer`.
- At most one reversal per original transfer.
- Both the source normal account and destination PB account must be Active.
- The reversal amount must be greater than zero and at most the original
  amount. The PB others-pool must have sufficient balance; if it does not
  (because earlier payments spent the pool down), the reversal is rejected
  with `InsufficientFunds` and the admin can retry with a smaller amount.
- After reversal, the funds sit in the source normal account. The admin can
  separately call `WithdrawFromNormalAccount` to return them to the sponsor's
  bank.
