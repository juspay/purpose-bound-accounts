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

