use std::sync::Arc;
use uuid::Uuid;

use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::{LedgerRepo, FUNDING_SOURCE_TB_ID};

const DEPOSIT_TRANSFER_CODE: u16 = 100;

pub struct DepositService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
}

impl DepositService {
    pub fn new(account_repo: Arc<AccountRepo>, ledger_repo: Arc<LedgerRepo>) -> Self {
        Self {
            account_repo,
            ledger_repo,
        }
    }

    pub async fn deposit(
        &self,
        account_id: Uuid,
        source_ifsc: &str,
        source_account_number: &str,
        amount: u64,
    ) -> Result<DepositResult, AppError> {
        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        // Route deposit based on origin match
        let is_self = account.is_origin_source(source_ifsc, source_account_number);
        let credit_tb_id = if is_self {
            account.tb_self_account_id
        } else {
            account.tb_others_account_id
        };

        self.ledger_repo
            .create_transfer(
                FUNDING_SOURCE_TB_ID,
                credit_tb_id,
                amount,
                DEPOSIT_TRANSFER_CODE,
            )
            .await?;

        Ok(DepositResult {
            account_id,
            amount,
            pool: if is_self {
                "self_contribution"
            } else {
                "others_contribution"
            },
        })
    }
}

pub struct DepositResult {
    pub account_id: Uuid,
    pub amount: u64,
    pub pool: &'static str,
}

impl DepositResult {
    // Convenience accessor to avoid needing to import AccountStatus in handlers
    pub fn is_self_pool(&self) -> bool {
        self.pool == "self_contribution"
    }
}
