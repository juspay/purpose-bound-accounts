use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, WITHDRAWAL_SETTLEMENT_TB_ID};
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const NORMAL_WITHDRAWAL_TRANSFER_CODE: u16 = 310;

pub struct NormalWithdrawalService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}

impl NormalWithdrawalService {
    pub fn new(
        normal_account_repo: Arc<NormalAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
    ) -> Self {
        Self {
            normal_account_repo,
            ledger_repo,
            transaction_repo,
        }
    }

    pub async fn withdraw(
        &self,
        account_id: Uuid,
        amount: u64,
        idempotency_key: Option<&str>,
        gateway_ref: Option<&str>,
        description: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Normal, account_id, key)
                .await?
            {
                return Ok(existing);
            }
        }

        let account = self.normal_account_repo.get_account(account_id).await?;
        if !account.status.is_active() {
            return Err(AppError::NormalAccountNotActive(account_id.to_string()));
        }

        let balance = self
            .ledger_repo
            .get_single_balance(account.tb_account_id)
            .await?;
        if balance.posted < amount {
            return Err(AppError::InsufficientFunds {
                requested: amount,
                available: balance.posted,
            });
        }

        let withdrawal_id = Uuid::now_v7();
        let mut tx = self.transaction_repo.pool().begin().await?;

        let record = self
            .transaction_repo
            .insert_in_tx(
                &mut tx,
                withdrawal_id,
                account_id,
                AccountKind::Normal,
                TransactionType::Withdrawal,
                TransactionStatus::Settled,
                amount,
                None,
                TransactionDirection::Outbound,
                None,
                None,
                gateway_ref,
                None,
                None,
                None,
                description,
                None,
                0,
                idempotency_key,
                None,
                None,
            )
            .await?;

        self.ledger_repo
            .create_transfer(
                account.tb_account_id,
                WITHDRAWAL_SETTLEMENT_TB_ID,
                amount,
                NORMAL_WITHDRAWAL_TRANSFER_CODE,
            )
            .await
            .map_err(|e| {
                tracing::error!("TB withdrawal failed, rolling back: {e}");
                e
            })?;

        tx.commit().await?;
        Ok(record)
    }
}
