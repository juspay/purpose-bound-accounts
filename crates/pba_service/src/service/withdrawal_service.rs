use std::sync::Arc;
use uuid::Uuid;

use crate::domain::transaction::{TransactionDirection, TransactionStatus, TransactionType};
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::{LedgerRepo, WITHDRAWAL_SETTLEMENT_TB_ID};
use crate::repository::transaction_repo::TransactionRepo;

const WITHDRAWAL_TRANSFER_CODE: u16 = 300;

pub struct WithdrawalService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}

impl WithdrawalService {
    pub fn new(
        account_repo: Arc<AccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            transaction_repo,
        }
    }

    pub async fn withdraw(
        &self,
        account_id: Uuid,
        amount: u64,
        idempotency_key: Option<&str>,
    ) -> Result<WithdrawalResult, AppError> {
        // Idempotency check
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(account_id, key)
                .await?
            {
                return Ok(WithdrawalResult {
                    account_id: existing.account_id,
                    amount: existing.amount,
                });
            }
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

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

        let mut tx = self.transaction_repo.pool().begin().await?;

        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                Uuid::new_v4(),
                account_id,
                TransactionType::Withdrawal,
                TransactionStatus::Settled,
                amount,
                "self",
                TransactionDirection::Outbound,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                0,
                idempotency_key,
            )
            .await?;

        self.ledger_repo
            .create_transfer(
                account.tb_self_account_id,
                WITHDRAWAL_SETTLEMENT_TB_ID,
                amount,
                WITHDRAWAL_TRANSFER_CODE,
            )
            .await
            .map_err(|e| {
                tracing::error!("TB withdrawal failed, rolling back: {e}");
                e
            })?;

        tx.commit().await?;

        Ok(WithdrawalResult { account_id, amount })
    }
}

pub struct WithdrawalResult {
    pub account_id: Uuid,
    pub amount: u64,
}
