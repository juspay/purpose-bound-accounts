use std::sync::Arc;
use uuid::Uuid;

use crate::domain::transaction::{TransactionDirection, TransactionStatus, TransactionType};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, WITHDRAWAL_SETTLEMENT_TB_ID};
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const WITHDRAWAL_TRANSFER_CODE: u16 = 300;

pub struct PbWithdrawalService {
    pub account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}

impl PbWithdrawalService {
    pub fn new(
        account_repo: Arc<PbAccountRepo>,
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
        gateway_ref: Option<&str>,
    ) -> Result<WithdrawalResult, AppError> {
        // Idempotency check
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(
                    crate::domain::account_kind::AccountKind::Pb,
                    account_id,
                    key,
                )
                .await?
            {
                return Ok(WithdrawalResult {
                    account_id: existing.account_id,
                    amount: existing.amount,
                    gateway_ref: existing.gateway_ref,
                });
            }
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::PbAccountNotActive(account_id.to_string()));
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
                Uuid::now_v7(),
                account_id,
                crate::domain::account_kind::AccountKind::Pb,
                TransactionType::Withdrawal,
                TransactionStatus::Settled,
                amount,
                Some("self"),
                TransactionDirection::Outbound,
                None,
                None,
                gateway_ref,
                None,
                None,
                None,
                None,
                None, // funding_type
                0,
                idempotency_key,
                None,
                None,
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

        Ok(WithdrawalResult {
            account_id,
            amount,
            gateway_ref: gateway_ref.map(|s| s.to_string()),
        })
    }
}

pub struct WithdrawalResult {
    pub account_id: Uuid,
    pub amount: u64,
    pub gateway_ref: Option<String>,
}
