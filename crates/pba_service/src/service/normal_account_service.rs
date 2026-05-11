use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account::AccountStatus;
use crate::domain::banking::{AccountNumber, Ifsc};
use crate::domain::normal_account::{tb_normal_id, NormalAccount};
use crate::error::AppError;
use crate::repository::ledger_repo::LedgerRepo;
use crate::repository::normal_account_repo::NormalAccountRepo;

pub struct NormalAccountService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
}

impl NormalAccountService {
    pub fn new(normal_account_repo: Arc<NormalAccountRepo>, ledger_repo: Arc<LedgerRepo>) -> Self {
        Self {
            normal_account_repo,
            ledger_repo,
        }
    }

    pub async fn create_account(
        &self,
        holder_id: &str,
        origin_ifsc: Option<&Ifsc>,
        origin_account_number: Option<&AccountNumber>,
    ) -> Result<NormalAccount, AppError> {
        let account_id = Uuid::new_v4();
        let tb_id = tb_normal_id(account_id);

        self.ledger_repo.create_normal_account(tb_id).await?;

        let account = self
            .normal_account_repo
            .create_account(
                account_id,
                holder_id,
                origin_ifsc,
                origin_account_number,
                tb_id,
            )
            .await?;

        Ok(account)
    }

    pub async fn get_account(&self, id: Uuid) -> Result<NormalAccount, AppError> {
        self.normal_account_repo.get_account(id).await
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AccountStatus,
    ) -> Result<NormalAccount, AppError> {
        self.normal_account_repo.get_account(id).await?;
        self.normal_account_repo.update_status(id, status).await
    }
}
