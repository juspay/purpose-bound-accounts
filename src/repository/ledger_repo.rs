use crate::domain::pool::PoolBalance;
use crate::error::AppError;

/// Sentinel account IDs used as counterparties for deposits, payments, and withdrawals.
/// TigerBeetle disallows 0 and u128::MAX, so we use a fixed range that won't collide with UUID-derived IDs.
pub const FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 10;
pub const MERCHANT_SETTLEMENT_TB_ID: u128 = u128::MAX - 11;
pub const WITHDRAWAL_SETTLEMENT_TB_ID: u128 = u128::MAX - 12;

// ── Real TigerBeetle implementation ─────────────────────────

#[cfg(feature = "tigerbeetle")]
mod real {
    use super::*;
    use tb::account::Flags as AccountFlags;
    use tb::transfer::Flags as TransferFlags;
    use tigerbeetle_unofficial as tb;
    use uuid::Uuid;

    const LEDGER_INR: u32 = 1;
    const CODE_SELF_POOL: u16 = 1;
    const CODE_OTHERS_POOL: u16 = 2;
    const CODE_SENTINEL: u16 = 99;

    fn generate_transfer_id() -> u128 {
        u128::from_be_bytes(*Uuid::new_v4().as_bytes())
    }

    pub struct TbLedger {
        client: tb::Client,
    }

    impl TbLedger {
        pub fn new(cluster_id: u128, addresses: &[String]) -> Self {
            let address = addresses.join(",");
            let client =
                tb::Client::new(cluster_id, &address).expect("Failed to connect to TigerBeetle");
            Self { client }
        }

        /// Create sentinel accounts that serve as counterparties for deposits, payments, and withdrawals.
        /// These are idempotent — TigerBeetle returns `Exists` for already-created accounts which we ignore.
        pub async fn init_sentinel_accounts(&self) -> Result<(), AppError> {
            let funding = tb::Account::new(FUNDING_SOURCE_TB_ID, LEDGER_INR, CODE_SENTINEL)
                .with_flags(AccountFlags::LINKED);
            let merchant = tb::Account::new(MERCHANT_SETTLEMENT_TB_ID, LEDGER_INR, CODE_SENTINEL)
                .with_flags(AccountFlags::LINKED);
            let withdrawal = tb::Account::new(WITHDRAWAL_SETTLEMENT_TB_ID, LEDGER_INR, CODE_SENTINEL);

            // Best-effort: ignore errors (accounts may already exist)
            match self
                .client
                .create_accounts(vec![funding, merchant, withdrawal])
                .await
            {
                Ok(_) => {
                    tracing::info!("Created sentinel TB accounts (funding, merchant, withdrawal)");
                }
                Err(e) => {
                    // Log but don't fail — accounts likely already exist
                    tracing::warn!("Sentinel account creation returned: {e:?} (may already exist)");
                }
            }
            Ok(())
        }

        pub async fn create_account_pair(
            &self,
            self_id: u128,
            others_id: u128,
        ) -> Result<(), AppError> {
            let self_account = tb::Account::new(self_id, LEDGER_INR, CODE_SELF_POOL)
                .with_flags(AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS | AccountFlags::LINKED);

            let others_account = tb::Account::new(others_id, LEDGER_INR, CODE_OTHERS_POOL)
                .with_flags(AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS);

            self.client
                .create_accounts(vec![self_account, others_account])
                .await
                .map_err(|e| {
                    AppError::TigerBeetleError(format!("create_accounts failed: {e:?}"))
                })?;

            tracing::info!(self_id = %self_id, others_id = %others_id, "Created linked TB account pair");
            Ok(())
        }

        pub async fn get_balance(
            &self,
            self_id: u128,
            others_id: u128,
        ) -> Result<PoolBalance, AppError> {
            let accounts = self
                .client
                .lookup_accounts(vec![self_id, others_id])
                .await
                .map_err(|e| {
                    AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}"))
                })?;

            let mut self_balance: u64 = 0;
            let mut others_balance: u64 = 0;

            for account in &accounts {
                let balance = account
                    .credits_posted()
                    .saturating_sub(account.debits_posted());
                let balance_u64 = u64::try_from(balance).unwrap_or(u64::MAX);
                if account.id() == self_id {
                    self_balance = balance_u64;
                } else if account.id() == others_id {
                    others_balance = balance_u64;
                }
            }

            Ok(PoolBalance {
                self_contribution: self_balance,
                others_contribution: others_balance,
            })
        }

        pub async fn create_transfer(
            &self,
            debit_account_id: u128,
            credit_account_id: u128,
            amount: u64,
            transfer_code: u16,
        ) -> Result<(), AppError> {
            let transfer = tb::Transfer::new(generate_transfer_id())
                .with_debit_account_id(debit_account_id)
                .with_credit_account_id(credit_account_id)
                .with_amount(amount as u128)
                .with_ledger(LEDGER_INR)
                .with_code(transfer_code);

            self.client
                .create_transfers(vec![transfer])
                .await
                .map_err(|e| {
                    AppError::TigerBeetleError(format!("create_transfers failed: {e:?}"))
                })?;

            tracing::info!(debit = %debit_account_id, credit = %credit_account_id, amount, code = transfer_code, "Created TB transfer");
            Ok(())
        }

        pub async fn create_linked_transfers(
            &self,
            others_debit_account_id: u128,
            self_debit_account_id: u128,
            credit_account_id: u128,
            others_amount: u64,
            self_amount: u64,
            transfer_code: u16,
        ) -> Result<(), AppError> {
            let transfer1 = tb::Transfer::new(generate_transfer_id())
                .with_debit_account_id(others_debit_account_id)
                .with_credit_account_id(credit_account_id)
                .with_amount(others_amount as u128)
                .with_ledger(LEDGER_INR)
                .with_code(transfer_code)
                .with_flags(TransferFlags::LINKED);

            let transfer2 = tb::Transfer::new(generate_transfer_id())
                .with_debit_account_id(self_debit_account_id)
                .with_credit_account_id(credit_account_id)
                .with_amount(self_amount as u128)
                .with_ledger(LEDGER_INR)
                .with_code(transfer_code);

            self.client
                .create_transfers(vec![transfer1, transfer2])
                .await
                .map_err(|e| {
                    AppError::TigerBeetleError(format!("create_linked_transfers failed: {e:?}"))
                })?;

            tracing::info!(
                others_debit = %others_debit_account_id,
                self_debit = %self_debit_account_id,
                credit = %credit_account_id,
                others_amount, self_amount, code = transfer_code,
                "Created linked TB transfer chain"
            );
            Ok(())
        }
    }
}

// ── In-memory ledger (default, for dev without TigerBeetle) ─

#[cfg(not(feature = "tigerbeetle"))]
mod inmem {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    pub struct InMemLedger {
        /// Track balances: account_id → credits_posted (simple credit accumulator)
        balances: Mutex<HashMap<u128, u64>>,
    }

    impl InMemLedger {
        pub fn new(_cluster_id: u128, _addresses: &[String]) -> Self {
            tracing::warn!("Using in-memory ledger (TigerBeetle feature not enabled)");
            Self {
                balances: Mutex::new(HashMap::new()),
            }
        }

        pub async fn init_sentinel_accounts(&self) -> Result<(), AppError> {
            // No-op for in-memory ledger — sentinel accounts are handled specially in create_transfer
            Ok(())
        }

        pub async fn create_account_pair(
            &self,
            self_id: u128,
            others_id: u128,
        ) -> Result<(), AppError> {
            let mut balances = self.balances.lock().unwrap();
            balances.entry(self_id).or_insert(0);
            balances.entry(others_id).or_insert(0);
            tracing::info!(self_id = %self_id, others_id = %others_id, "Created in-memory account pair");
            Ok(())
        }

        pub async fn get_balance(
            &self,
            self_id: u128,
            others_id: u128,
        ) -> Result<PoolBalance, AppError> {
            let balances = self.balances.lock().unwrap();
            Ok(PoolBalance {
                self_contribution: balances.get(&self_id).copied().unwrap_or(0),
                others_contribution: balances.get(&others_id).copied().unwrap_or(0),
            })
        }

        pub async fn create_transfer(
            &self,
            debit_account_id: u128,
            credit_account_id: u128,
            amount: u64,
            _transfer_code: u16,
        ) -> Result<(), AppError> {
            let mut balances = self.balances.lock().unwrap();

            // Debit side: reduce balance (skip for sentinel accounts like funding source)
            let is_sentinel = debit_account_id == FUNDING_SOURCE_TB_ID
                || debit_account_id == MERCHANT_SETTLEMENT_TB_ID
                || debit_account_id == WITHDRAWAL_SETTLEMENT_TB_ID;
            if !is_sentinel {
                let debit_bal = balances.get(&debit_account_id).copied().unwrap_or(0);
                if debit_bal < amount {
                    return Err(AppError::TigerBeetleError(format!(
                        "debit would exceed credits: available {debit_bal}, requested {amount}"
                    )));
                }
                balances.insert(debit_account_id, debit_bal - amount);
            }

            // Credit side: increase balance
            let credit_bal = balances.get(&credit_account_id).copied().unwrap_or(0);
            balances.insert(credit_account_id, credit_bal + amount);

            tracing::debug!(
                debit = %debit_account_id, credit = %credit_account_id,
                amount, "In-memory transfer"
            );
            Ok(())
        }

        pub async fn create_linked_transfers(
            &self,
            others_debit_account_id: u128,
            self_debit_account_id: u128,
            credit_account_id: u128,
            others_amount: u64,
            self_amount: u64,
            _transfer_code: u16,
        ) -> Result<(), AppError> {
            let mut balances = self.balances.lock().unwrap();

            // Validate both debits before applying
            let others_bal = balances.get(&others_debit_account_id).copied().unwrap_or(0);
            let self_bal = balances.get(&self_debit_account_id).copied().unwrap_or(0);

            if others_bal < others_amount {
                return Err(AppError::TigerBeetleError(format!(
                    "others debit would exceed credits: available {others_bal}, requested {others_amount}"
                )));
            }
            if self_bal < self_amount {
                return Err(AppError::TigerBeetleError(format!(
                    "self debit would exceed credits: available {self_bal}, requested {self_amount}"
                )));
            }

            // Apply atomically
            balances.insert(others_debit_account_id, others_bal - others_amount);
            balances.insert(self_debit_account_id, self_bal - self_amount);
            let credit_bal = balances.get(&credit_account_id).copied().unwrap_or(0);
            balances.insert(credit_account_id, credit_bal + others_amount + self_amount);

            tracing::debug!(
                others_debit = %others_debit_account_id,
                self_debit = %self_debit_account_id,
                credit = %credit_account_id,
                others_amount, self_amount,
                "In-memory linked transfer"
            );
            Ok(())
        }
    }
}

// ── Unified LedgerRepo facade ───────────────────────────────

pub struct LedgerRepo {
    #[cfg(feature = "tigerbeetle")]
    inner: real::TbLedger,
    #[cfg(not(feature = "tigerbeetle"))]
    inner: inmem::InMemLedger,
}

impl LedgerRepo {
    pub fn new(cluster_id: u128, addresses: Vec<String>) -> Self {
        Self {
            #[cfg(feature = "tigerbeetle")]
            inner: real::TbLedger::new(cluster_id, &addresses),
            #[cfg(not(feature = "tigerbeetle"))]
            inner: inmem::InMemLedger::new(cluster_id, &addresses),
        }
    }

    pub async fn init_sentinel_accounts(&self) -> Result<(), AppError> {
        self.inner.init_sentinel_accounts().await
    }

    pub async fn create_account_pair(
        &self,
        self_id: u128,
        others_id: u128,
    ) -> Result<(), AppError> {
        self.inner.create_account_pair(self_id, others_id).await
    }

    pub async fn get_balance(
        &self,
        self_id: u128,
        others_id: u128,
    ) -> Result<PoolBalance, AppError> {
        self.inner.get_balance(self_id, others_id).await
    }

    pub async fn create_transfer(
        &self,
        debit_account_id: u128,
        credit_account_id: u128,
        amount: u64,
        transfer_code: u16,
    ) -> Result<(), AppError> {
        self.inner
            .create_transfer(debit_account_id, credit_account_id, amount, transfer_code)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_linked_transfers(
        &self,
        others_debit_account_id: u128,
        self_debit_account_id: u128,
        credit_account_id: u128,
        others_amount: u64,
        self_amount: u64,
        transfer_code: u16,
    ) -> Result<(), AppError> {
        self.inner
            .create_linked_transfers(
                others_debit_account_id,
                self_debit_account_id,
                credit_account_id,
                others_amount,
                self_amount,
                transfer_code,
            )
            .await
    }
}
