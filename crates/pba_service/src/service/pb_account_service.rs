use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account::{tb_others_id, tb_self_id, AccountStatus, PurposeBoundAccount};
use crate::domain::banking::{AccountNumber, Ifsc};
use crate::error::AppError;
use crate::repository::ledger_repo::LedgerRepo;
use crate::repository::pb_account_repo::PbAccountRepo;

pub struct PbAccountService {
    pub account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
}

impl PbAccountService {
    pub fn new(account_repo: Arc<PbAccountRepo>, ledger_repo: Arc<LedgerRepo>) -> Self {
        Self {
            account_repo,
            ledger_repo,
        }
    }

    pub async fn create_account(
        &self,
        holder_id: &str,
        purpose_code: &str,
        origin_ifsc: &Ifsc,
        origin_account_number: &AccountNumber,
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
