use crate::domain::pool::PoolBalance;
use crate::error::AppError;
use tb::account::Flags as AccountFlags;
use tb::error::CreateTransferErrorKind;
use tb::transfer::Flags as TransferFlags;
use tigerbeetle_unofficial as tb;
use uuid::Uuid;

/// Sentinel account IDs used as counterparties for deposits, payments, and withdrawals.
/// TigerBeetle disallows 0 and u128::MAX, so we use a fixed range that won't collide with UUID-derived IDs.
pub const SELF_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 10;
pub const MERCHANT_SETTLEMENT_TB_ID: u128 = u128::MAX - 11;
pub const WITHDRAWAL_SETTLEMENT_TB_ID: u128 = u128::MAX - 12;
pub const TRUST_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 13;
pub const THIRD_PARTY_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 14;

/// Ledger ID for Indian Rupee. All amounts are in paisa (1 INR = 100 paisa).
const LEDGER_INR_PAISA: u32 = 1;
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
        let self_funding =
            tb::Account::new(SELF_FUNDING_SOURCE_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
                .with_flags(AccountFlags::LINKED);
        let merchant = tb::Account::new(MERCHANT_SETTLEMENT_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
        let withdrawal =
            tb::Account::new(WITHDRAWAL_SETTLEMENT_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
                .with_flags(AccountFlags::LINKED);
        let trust = tb::Account::new(TRUST_FUNDING_SOURCE_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
        let third_party = tb::Account::new(
            THIRD_PARTY_FUNDING_SOURCE_TB_ID,
            LEDGER_INR_PAISA,
            CODE_SENTINEL,
        );

        match self
            .client
            .create_accounts(vec![self_funding, merchant, withdrawal, trust, third_party])
            .await
        {
            Ok(_) => {
                tracing::info!(
                    "Created sentinel TB accounts (self_funding, merchant, withdrawal, trust, third_party)"
                );
            }
            Err(e) => {
                tracing::warn!("Sentinel account creation returned: {e:?} (may already exist)");
            }
        }
        Ok(())
    }

    /// Look up all sentinel accounts and return their balances.
    /// Returns a Vec of (name, credits_posted, debits_posted, credits_pending, debits_pending).
    pub async fn lookup_sentinel_accounts(
        &self,
    ) -> Result<Vec<(String, u64, u64, u64, u64)>, AppError> {
        let ids = vec![
            SELF_FUNDING_SOURCE_TB_ID,
            TRUST_FUNDING_SOURCE_TB_ID,
            THIRD_PARTY_FUNDING_SOURCE_TB_ID,
            MERCHANT_SETTLEMENT_TB_ID,
            WITHDRAWAL_SETTLEMENT_TB_ID,
        ];
        let names = [
            "Self Funding Source",
            "Trust Funding Source",
            "Third Party Funding Source",
            "Merchant Settlement",
            "Withdrawal Settlement",
        ];

        let accounts =
            self.client.lookup_accounts(ids).await.map_err(|e| {
                AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}"))
            })?;

        let mut results: Vec<(String, u64, u64, u64, u64)> =
            names.iter().map(|n| (n.to_string(), 0, 0, 0, 0)).collect();

        for account in &accounts {
            let idx = match account.id() {
                id if id == SELF_FUNDING_SOURCE_TB_ID => 0,
                id if id == TRUST_FUNDING_SOURCE_TB_ID => 1,
                id if id == THIRD_PARTY_FUNDING_SOURCE_TB_ID => 2,
                id if id == MERCHANT_SETTLEMENT_TB_ID => 3,
                id if id == WITHDRAWAL_SETTLEMENT_TB_ID => 4,
                _ => continue,
            };
            results[idx] = (
                results[idx].0.clone(),
                u64::try_from(account.credits_posted()).unwrap_or(u64::MAX),
                u64::try_from(account.debits_posted()).unwrap_or(u64::MAX),
                u64::try_from(account.credits_pending()).unwrap_or(u64::MAX),
                u64::try_from(account.debits_pending()).unwrap_or(u64::MAX),
            );
        }

        Ok(results)
    }

    pub async fn create_account_pair(
        &self,
        self_id: u128,
        others_id: u128,
    ) -> Result<(), AppError> {
        let self_account = tb::Account::new(self_id, LEDGER_INR_PAISA, CODE_SELF_POOL).with_flags(
            AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS
                | AccountFlags::HISTORY
                | AccountFlags::LINKED,
        );

        let others_account = tb::Account::new(others_id, LEDGER_INR_PAISA, CODE_OTHERS_POOL)
            .with_flags(AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS | AccountFlags::HISTORY);

        self.client
            .create_accounts(vec![self_account, others_account])
            .await
            .map_err(|e| AppError::TigerBeetleError(format!("create_accounts failed: {e:?}")))?;

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
            .map_err(|e| AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}")))?;

        let mut self_balance: u64 = 0;
        let mut others_balance: u64 = 0;
        let mut pending_self: u64 = 0;
        let mut pending_others: u64 = 0;

        for account in &accounts {
            let posted = account
                .credits_posted()
                .saturating_sub(account.debits_posted());
            let posted_u64 = u64::try_from(posted).unwrap_or(u64::MAX);
            let pending = u64::try_from(account.credits_pending()).unwrap_or(u64::MAX);

            if account.id() == self_id {
                self_balance = posted_u64;
                pending_self = pending;
            } else if account.id() == others_id {
                others_balance = posted_u64;
                pending_others = pending;
            }
        }

        Ok(PoolBalance {
            self_contribution: self_balance,
            others_contribution: others_balance,
            pending_self,
            pending_others,
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
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| classify_transfer_error(e, "create_transfer"))?;

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
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code)
            .with_flags(TransferFlags::LINKED);

        let transfer2 = tb::Transfer::new(generate_transfer_id())
            .with_debit_account_id(self_debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(self_amount as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code);

        self.client
            .create_transfers(vec![transfer1, transfer2])
            .await
            .map_err(|e| classify_transfer_error(e, "create_linked_transfers"))?;

        tracing::info!(
            others_debit = %others_debit_account_id,
            self_debit = %self_debit_account_id,
            credit = %credit_account_id,
            others_amount, self_amount, code = transfer_code,
            "Created linked TB transfer chain"
        );
        Ok(())
    }

    /// Create a pending transfer. Returns the TB transfer ID (needed for post/void).
    pub async fn create_pending_transfer(
        &self,
        debit_account_id: u128,
        credit_account_id: u128,
        amount: u64,
        transfer_code: u16,
        timeout_seconds: u32,
    ) -> Result<u128, AppError> {
        let transfer_id = generate_transfer_id();
        let transfer = tb::Transfer::new(transfer_id)
            .with_debit_account_id(debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(amount as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code)
            .with_flags(TransferFlags::PENDING)
            .with_timeout(timeout_seconds);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| classify_transfer_error(e, "create_pending_transfer"))?;

        tracing::info!(
            debit = %debit_account_id, credit = %credit_account_id,
            amount, code = transfer_code, timeout = timeout_seconds,
            transfer_id = %transfer_id,
            "Created pending TB transfer"
        );
        Ok(transfer_id)
    }

    /// Post (confirm) a pending transfer.
    pub async fn post_pending_transfer(&self, pending_id: u128) -> Result<(), AppError> {
        let transfer = tb::Transfer::new(generate_transfer_id())
            .with_pending_id(pending_id)
            .with_amount(u128::MAX)
            .with_flags(TransferFlags::POST_PENDING_TRANSFER);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| {
                AppError::TigerBeetleError(format!("post_pending_transfer failed: {e:?}"))
            })?;

        tracing::info!(pending_id = %pending_id, "Posted pending TB transfer");
        Ok(())
    }

    /// Void (cancel) a pending transfer.
    pub async fn void_pending_transfer(&self, pending_id: u128) -> Result<(), AppError> {
        let transfer = tb::Transfer::new(generate_transfer_id())
            .with_pending_id(pending_id)
            .with_flags(TransferFlags::VOID_PENDING_TRANSFER);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| {
                AppError::TigerBeetleError(format!("void_pending_transfer failed: {e:?}"))
            })?;

        tracing::info!(pending_id = %pending_id, "Voided pending TB transfer");
        Ok(())
    }
}

/// Classify a TigerBeetle transfer error into an appropriate AppError.
/// Returns `ExceedsBalance` for overdraft rejections (retryable), generic error otherwise.
fn classify_transfer_error(e: tb::error::CreateTransfersError, context: &str) -> AppError {
    if let tb::error::CreateTransfersError::Api(ref api_err) = e {
        let is_balance_exceeded = api_err.as_slice().iter().any(|individual| {
            matches!(
                individual.kind(),
                CreateTransferErrorKind::ExceedsCredits
                    | CreateTransferErrorKind::ExceedsDebits
                    | CreateTransferErrorKind::LinkedEventFailed
            )
        });
        if is_balance_exceeded {
            return AppError::ExceedsBalance;
        }
    }
    AppError::TigerBeetleError(format!("{context} failed: {e:?}"))
}
