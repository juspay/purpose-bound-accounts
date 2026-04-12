use std::sync::Arc;
use uuid::Uuid;

use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::LedgerRepo;

use crate::repository::ledger_repo::WITHDRAWAL_SETTLEMENT_TB_ID;

const WITHDRAWAL_TRANSFER_CODE: u16 = 300;

pub struct WithdrawalService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
}

impl WithdrawalService {
    pub fn new(account_repo: Arc<AccountRepo>, ledger_repo: Arc<LedgerRepo>) -> Self {
        Self {
            account_repo,
            ledger_repo,
        }
    }

    pub async fn withdraw(
        &self,
        account_id: Uuid,
        amount: u64,
    ) -> Result<WithdrawalResult, AppError> {
        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        // Get balance — withdrawal only from self-pool
        let balance = self
            .ledger_repo
            .get_balance(account.tb_self_account_id, account.tb_others_account_id)
            .await?;

        if balance.self_contribution < amount {
            return Err(AppError::InsufficientFunds {
                requested: amount,
                available: balance.self_contribution,
            });
        }

        self.ledger_repo
            .create_transfer(
                account.tb_self_account_id,
                WITHDRAWAL_SETTLEMENT_TB_ID,
                amount,
                WITHDRAWAL_TRANSFER_CODE,
            )
            .await?;

        Ok(WithdrawalResult { account_id, amount })
    }
}

pub struct WithdrawalResult {
    pub account_id: Uuid,
    pub amount: u64,
}
