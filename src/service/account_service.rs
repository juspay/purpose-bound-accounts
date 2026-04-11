use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account::{
    tb_others_id, tb_self_id, AccountStatus, PurposeBoundAccount,
};
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::LedgerRepo;

pub struct AccountService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
}

impl AccountService {
    pub fn new(account_repo: Arc<AccountRepo>, ledger_repo: Arc<LedgerRepo>) -> Self {
        Self {
            account_repo,
            ledger_repo,
        }
    }

    pub async fn create_account(
        &self,
        holder_id: Uuid,
        purpose_code: &str,
        origin_ifsc: &str,
        origin_account_number: &str,
    ) -> Result<PurposeBoundAccount, AppError> {
        // Validate purpose code exists
        self.account_repo.get_purpose_type(purpose_code).await?;

        let account_id = Uuid::new_v4();
        let self_tb_id = tb_self_id(account_id);
        let others_tb_id = tb_others_id(account_id);

        // Create linked TB account pair atomically
        self.ledger_repo
            .create_account_pair(self_tb_id, others_tb_id)
            .await?;

        // Persist account metadata in Postgres
        let account = self
            .account_repo
            .create_account(
                account_id,
                holder_id,
                purpose_code,
                origin_ifsc,
                origin_account_number,
                self_tb_id,
                others_tb_id,
            )
            .await?;

        Ok(account)
    }

    pub async fn get_account(&self, id: Uuid) -> Result<PurposeBoundAccount, AppError> {
        self.account_repo.get_account(id).await
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AccountStatus,
    ) -> Result<PurposeBoundAccount, AppError> {
        // Verify account exists first
        self.account_repo.get_account(id).await?;
        self.account_repo.update_status(id, status).await
    }
}
