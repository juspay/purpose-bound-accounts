use std::sync::Arc;
use uuid::Uuid;

use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::{
    LedgerRepo, SELF_FUNDING_SOURCE_TB_ID, THIRD_PARTY_FUNDING_SOURCE_TB_ID,
    TRUST_FUNDING_SOURCE_TB_ID,
};
use crate::repository::transaction_repo::TransactionRepo;

const DEPOSIT_TRANSFER_CODE: u16 = 100;
const PENDING_DEPOSIT_TRANSFER_CODE: u16 = 101;

pub struct DepositService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
}

impl DepositService {
    pub fn new(
        account_repo: Arc<AccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn deposit(
        &self,
        account_id: Uuid,
        source_ifsc: &str,
        source_account_number: &str,
        funding_type: Option<&str>,
        amount: u64,
        pending: bool,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        idempotency_key: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        // Idempotency check
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(account_id, key)
                .await?
            {
                return Ok(existing);
            }
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        let is_self = account.is_origin_source(source_ifsc, source_account_number);

        let (pool, resolved_funding_type, debit_sentinel) = if is_self {
            ("self", "self", SELF_FUNDING_SOURCE_TB_ID)
        } else {
            match funding_type {
                Some("trust") => ("others", "trust", TRUST_FUNDING_SOURCE_TB_ID),
                Some("third_party") => ("others", "third_party", THIRD_PARTY_FUNDING_SOURCE_TB_ID),
                _ => return Err(AppError::FundingTypeRequired),
            }
        };

        let credit_tb_id = if is_self {
            account.tb_self_account_id
        } else {
            account.tb_others_account_id
        };
        let deposit_id = Uuid::new_v4();

        let mut tx = self.transaction_repo.pool().begin().await?;

        if pending {
            let timeout = timeout_seconds.unwrap_or(self.default_timeout_seconds);

            // Insert PG row (status=pending, tb_transfer_id=0)
            let record = self
                .transaction_repo
                .insert_in_tx(
                    &mut tx,
                    deposit_id,
                    account_id,
                    TransactionType::Deposit,
                    TransactionStatus::Pending,
                    amount,
                    pool,
                    TransactionDirection::Inbound,
                    Some(source_ifsc),
                    Some(source_account_number),
                    gateway_ref,
                    Some(timeout),
                    None,
                    None,
                    None,
                    Some(resolved_funding_type),
                    0,
                    idempotency_key,
                )
                .await?;

            // Create pending transfer in TigerBeetle
            let tb_transfer_id = self
                .ledger_repo
                .create_pending_transfer(
                    debit_sentinel,
                    credit_tb_id,
                    amount,
                    PENDING_DEPOSIT_TRANSFER_CODE,
                    timeout,
                )
                .await
                .map_err(|e| {
                    tracing::error!("TB pending transfer failed, rolling back: {e}");
                    e
                })?;

            // Update with real TB transfer ID
            self.transaction_repo
                .update_tb_transfer_id_in_tx(&mut tx, deposit_id, tb_transfer_id)
                .await?;

            tx.commit().await?;
            // Return record with updated tb_transfer_id
            Ok(TransactionRecord {
                tb_transfer_id,
                ..record
            })
        } else {
            // Insert PG row (status=posted)
            let record = self
                .transaction_repo
                .insert_in_tx(
                    &mut tx,
                    deposit_id,
                    account_id,
                    TransactionType::Deposit,
                    TransactionStatus::Posted,
                    amount,
                    pool,
                    TransactionDirection::Inbound,
                    Some(source_ifsc),
                    Some(source_account_number),
                    gateway_ref,
                    None,
                    None,
                    None,
                    None,
                    Some(resolved_funding_type),
                    0,
                    idempotency_key,
                )
                .await?;

            // Execute TB transfer
            self.ledger_repo
                .create_transfer(debit_sentinel, credit_tb_id, amount, DEPOSIT_TRANSFER_CODE)
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

        // Post in TigerBeetle
        self.ledger_repo
            .post_pending_transfer(txn.tb_transfer_id)
            .await?;

        // Update PG
        let updated = self
            .transaction_repo
            .update_status(deposit_id, TransactionStatus::Posted)
            .await?;

        tracing::info!(deposit_id = %deposit_id, account_id = %account_id, amount = txn.amount, "Pending deposit posted");
        Ok(updated)
    }

    pub async fn void_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
        _reason: Option<&str>,
    ) -> Result<TransactionRecord, AppError> {
        let txn = self
            .transaction_repo
            .get_by_id(deposit_id, account_id)
            .await?;

        if txn.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(deposit_id.to_string()));
        }

        // Void in TigerBeetle
        self.ledger_repo
            .void_pending_transfer(txn.tb_transfer_id)
            .await?;

        // Update PG
        let updated = self
            .transaction_repo
            .update_status(deposit_id, TransactionStatus::Voided)
            .await?;

        tracing::info!(deposit_id = %deposit_id, account_id = %account_id, amount = txn.amount, "Pending deposit voided");
        Ok(updated)
    }
}
