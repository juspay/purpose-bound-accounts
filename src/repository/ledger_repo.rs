use crate::domain::pool::PoolBalance;
use crate::error::AppError;
use tb::account::Flags as AccountFlags;
use tb::transfer::Flags as TransferFlags;
use tigerbeetle_unofficial as tb;
use uuid::Uuid;

/// Sentinel account IDs used as counterparties for deposits, payments, and withdrawals.
/// TigerBeetle disallows 0 and u128::MAX, so we use a fixed range that won't collide with UUID-derived IDs.
pub const FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 10;
pub const MERCHANT_SETTLEMENT_TB_ID: u128 = u128::MAX - 11;
pub const WITHDRAWAL_SETTLEMENT_TB_ID: u128 = u128::MAX - 12;

const LEDGER_INR: u32 = 1;
const CODE_SELF_POOL: u16 = 1;
const CODE_OTHERS_POOL: u16 = 2;
const CODE_SENTINEL: u16 = 99;

fn generate_transfer_id() -> u128 {
    u128::from_be_bytes(*Uuid::new_v4().as_bytes())
}

pub struct LedgerRepo {
    client: tb::Client,
}

impl LedgerRepo {
    pub fn new(cluster_id: u128, addresses: Vec<String>) -> Self {
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
