use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, TRUST_FUNDING_SOURCE_TB_ID};
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const NORMAL_DEPOSIT_TRANSFER_CODE: u16 = 110;
const PENDING_NORMAL_DEPOSIT_TRANSFER_CODE: u16 = 111;

pub struct NormalDepositService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
}

impl NormalDepositService {
    pub fn new(
        normal_account_repo: Arc<NormalAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
    ) -> Self {
        Self {
            normal_account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit(
        &self,
        account_id: Uuid,
        amount: u64,
        pending: bool,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        idempotency_key: Option<&str>,
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

        let deposit_id = Uuid::now_v7();
        let mut tx = self.transaction_repo.pool().begin().await?;

        if pending {
            let timeout = timeout_seconds.unwrap_or(self.default_timeout_seconds);

            let record = self
                .transaction_repo
                .insert_in_tx(
                    &mut tx,
                    deposit_id,
                    account_id,
                    AccountKind::Normal,
                    TransactionType::Deposit,
                    TransactionStatus::Pending,
                    amount,
                    None,
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    Some(timeout),
                    None,
                    None,
                    None,
                    Some("trust"),
                    0,
                    idempotency_key,
                    None,
                )
                .await?;

            let tb_transfer_id = self
                .ledger_repo
                .create_pending_transfer(
                    TRUST_FUNDING_SOURCE_TB_ID,
                    account.tb_account_id,
                    amount,
                    PENDING_NORMAL_DEPOSIT_TRANSFER_CODE,
                    timeout,
                )
                .await
                .map_err(|e| {
                    tracing::error!("TB pending transfer failed, rolling back: {e}");
                    e
                })?;

            self.transaction_repo
                .update_tb_transfer_id_in_tx(&mut tx, deposit_id, tb_transfer_id)
                .await?;

            tx.commit().await?;
            Ok(TransactionRecord {
                tb_transfer_id,
                ..record
            })
        } else {
            let record = self
                .transaction_repo
                .insert_in_tx(
                    &mut tx,
                    deposit_id,
                    account_id,
                    AccountKind::Normal,
                    TransactionType::Deposit,
                    TransactionStatus::Posted,
                    amount,
                    None,
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    None,
                    None,
                    None,
                    None,
                    Some("trust"),
                    0,
                    idempotency_key,
                    None,
                )
                .await?;

            self.ledger_repo
                .create_transfer(
                    TRUST_FUNDING_SOURCE_TB_ID,
                    account.tb_account_id,
                    amount,
                    NORMAL_DEPOSIT_TRANSFER_CODE,
                )
                .await
                .map_err(|e| {
                    tracing::error!("TB transfer failed, rolling back: {e}");
                    e
                })?;

            tx.commit().await?;
            Ok(record)
        }
    }

    pub async fn post_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
    ) -> Result<TransactionRecord, AppError> {
        let txn = self
            .transaction_repo
            .get_by_id(deposit_id, account_id)
            .await?;
        if txn.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(deposit_id.to_string()));
        }
        self.ledger_repo
            .post_pending_transfer(txn.tb_transfer_id)
            .await?;
        self.transaction_repo
            .update_status(deposit_id, TransactionStatus::Posted)
            .await
    }

    pub async fn void_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
    ) -> Result<TransactionRecord, AppError> {
        let txn = self
            .transaction_repo
            .get_by_id(deposit_id, account_id)
            .await?;
        if txn.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(deposit_id.to_string()));
        }
        self.ledger_repo
            .void_pending_transfer(txn.tb_transfer_id)
            .await?;
        self.transaction_repo
            .update_status(deposit_id, TransactionStatus::Voided)
            .await
    }
}
