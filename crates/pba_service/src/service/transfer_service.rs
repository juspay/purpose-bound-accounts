#![allow(dead_code)]

use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::domain::transfer::TransferLegs;
use crate::error::AppError;
use crate::repository::ledger_repo::LedgerRepo;
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const MAX_RETRIES: u32 = 3;

pub struct TransferService {
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub pb_account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct TransferResult {
    pub source_txn_id: Uuid,
    pub destination_txn_id: Uuid,
    pub source_account_id: Uuid,
    pub destination_account_id: Uuid,
    pub amount: u64,
    pub status: TransactionStatus,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TransferService {
    pub fn new(
        normal_account_repo: Arc<NormalAccountRepo>,
        pb_account_repo: Arc<PbAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
    ) -> Self {
        Self {
            normal_account_repo,
            pb_account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn transfer(
        &self,
        source_normal_id: Uuid,
        destination_pb_id: Uuid,
        amount: u64,
        pending: bool,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        description: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<TransferResult, AppError> {
        // Idempotency replay
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Normal, source_normal_id, key)
                .await?
            {
                let correlation_id = existing.correlation_id.ok_or_else(|| {
                    AppError::DatabaseError(
                        "transfer source row missing correlation_id".to_string(),
                    )
                })?;
                let legs = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                if legs.len() != 2 {
                    return Err(AppError::DatabaseError(
                        "transfer correlation has != 2 legs".to_string(),
                    ));
                }
                return Ok(self.legs_to_result(&legs, source_normal_id, destination_pb_id));
            }
        }

        let source = self
            .normal_account_repo
            .get_account(source_normal_id)
            .await?;
        let destination = self.pb_account_repo.get_account(destination_pb_id).await?;

        if !source.status.is_active() {
            return Err(AppError::NormalAccountNotActive(
                source_normal_id.to_string(),
            ));
        }
        if !destination.status.is_active() {
            return Err(AppError::PbAccountNotActive(destination_pb_id.to_string()));
        }

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tracing::info!(
                    source_normal_id = %source_normal_id,
                    attempt,
                    "retrying transfer with fresh balance"
                );
            }

            let balance = self
                .ledger_repo
                .get_single_balance(source.tb_account_id)
                .await?;
            if balance.posted < amount {
                return Err(AppError::InsufficientFunds {
                    requested: amount,
                    available: balance.posted,
                });
            }

            let legs = TransferLegs::new();
            let mut tx = self.transaction_repo.pool().begin().await?;

            let source_status = if pending {
                TransactionStatus::Pending
            } else {
                TransactionStatus::Posted
            };
            let timeout = if pending {
                Some(timeout_seconds.unwrap_or(self.default_timeout_seconds))
            } else {
                None
            };

            // Source-side row (normal account, outbound transfer)
            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    legs.source_txn_id,
                    source_normal_id,
                    AccountKind::Normal,
                    TransactionType::Transfer,
                    source_status,
                    amount,
                    None,
                    TransactionDirection::Outbound,
                    None,
                    None,
                    gateway_ref,
                    timeout,
                    None,
                    None,
                    description,
                    Some("trust"),
                    0,
                    idempotency_key,
                    Some(legs.correlation_id),
                    None,
                )
                .await?;

            // Destination-side row (PB account, inbound deposit, others pool, funding_type=trust)
            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    legs.destination_txn_id,
                    destination_pb_id,
                    AccountKind::Pb,
                    TransactionType::Deposit,
                    source_status,
                    amount,
                    Some("others"),
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    timeout,
                    None,
                    None,
                    description,
                    Some("trust"),
                    0,
                    None,
                    Some(legs.correlation_id),
                    None,
                )
                .await?;

            // Execute the TB transfer
            let tb_result = if pending {
                self.ledger_repo
                    .create_pending_internal_transfer(
                        source.tb_account_id,
                        destination.tb_others_account_id,
                        amount,
                        timeout_seconds.unwrap_or(self.default_timeout_seconds),
                    )
                    .await
            } else {
                self.ledger_repo
                    .create_internal_transfer(
                        source.tb_account_id,
                        destination.tb_others_account_id,
                        amount,
                    )
                    .await
                    .map(|_| 0u128)
            };

            match tb_result {
                Ok(tb_transfer_id) => {
                    if pending && tb_transfer_id != 0 {
                        sqlx::query(
                            r#"UPDATE transactions
                               SET tb_transfer_id = $1::numeric, updated_at = now()
                               WHERE correlation_id = $2"#,
                        )
                        .bind(tb_transfer_id.to_string())
                        .bind(legs.correlation_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| AppError::DatabaseError(e.to_string()))?;
                    }
                    tx.commit().await?;
                    let updated_legs = self
                        .transaction_repo
                        .find_by_correlation_id(legs.correlation_id)
                        .await?;
                    return Ok(self.legs_to_result(
                        &updated_legs,
                        source_normal_id,
                        destination_pb_id,
                    ));
                }
                Err(AppError::ExceedsBalance) => {
                    last_err = Some(AppError::ExceedsBalance);
                    // tx rolls back on drop
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_err.unwrap_or(AppError::ExceedsBalance))
    }

    pub async fn post_transfer(
        &self,
        source_normal_id: Uuid,
        transfer_id: Uuid,
    ) -> Result<TransferResult, AppError> {
        let source_row = self
            .transaction_repo
            .get_by_id(transfer_id, source_normal_id)
            .await?;
        if source_row.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(transfer_id.to_string()));
        }
        if source_row.transaction_type != TransactionType::Transfer {
            return Err(AppError::TransactionNotPending(transfer_id.to_string()));
        }

        self.ledger_repo
            .post_pending_transfer(source_row.tb_transfer_id)
            .await?;

        let correlation_id = source_row.correlation_id.ok_or_else(|| {
            AppError::DatabaseError("transfer source row missing correlation_id".to_string())
        })?;
        sqlx::query(
            r#"UPDATE transactions
               SET status = $1, updated_at = now()
               WHERE correlation_id = $2"#,
        )
        .bind(TransactionStatus::Posted.as_str())
        .bind(correlation_id)
        .execute(self.transaction_repo.pool())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let legs = self
            .transaction_repo
            .find_by_correlation_id(correlation_id)
            .await?;
        let dest_id = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .map(|l| l.account_id)
            .ok_or_else(|| AppError::DatabaseError("transfer missing pb leg".to_string()))?;
        Ok(self.legs_to_result(&legs, source_normal_id, dest_id))
    }

    pub async fn void_transfer(
        &self,
        source_normal_id: Uuid,
        transfer_id: Uuid,
    ) -> Result<TransferResult, AppError> {
        let source_row = self
            .transaction_repo
            .get_by_id(transfer_id, source_normal_id)
            .await?;
        if source_row.status != TransactionStatus::Pending {
            return Err(AppError::TransactionNotPending(transfer_id.to_string()));
        }
        if source_row.transaction_type != TransactionType::Transfer {
            return Err(AppError::TransactionNotPending(transfer_id.to_string()));
        }

        self.ledger_repo
            .void_pending_transfer(source_row.tb_transfer_id)
            .await?;

        let correlation_id = source_row.correlation_id.ok_or_else(|| {
            AppError::DatabaseError("transfer source row missing correlation_id".to_string())
        })?;
        sqlx::query(
            r#"UPDATE transactions
               SET status = $1, updated_at = now()
               WHERE correlation_id = $2"#,
        )
        .bind(TransactionStatus::Voided.as_str())
        .bind(correlation_id)
        .execute(self.transaction_repo.pool())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        let legs = self
            .transaction_repo
            .find_by_correlation_id(correlation_id)
            .await?;
        let dest_id = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .map(|l| l.account_id)
            .ok_or_else(|| AppError::DatabaseError("transfer missing pb leg".to_string()))?;
        Ok(self.legs_to_result(&legs, source_normal_id, dest_id))
    }

    fn legs_to_result(
        &self,
        legs: &[TransactionRecord],
        source_normal_id: Uuid,
        destination_pb_id: Uuid,
    ) -> TransferResult {
        let source_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Normal)
            .expect("transfer correlation has a normal leg");
        let dest_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .expect("transfer correlation has a pb leg");
        TransferResult {
            source_txn_id: source_leg.id,
            destination_txn_id: dest_leg.id,
            source_account_id: source_normal_id,
            destination_account_id: destination_pb_id,
            amount: source_leg.amount,
            status: source_leg.status,
            correlation_id: source_leg
                .correlation_id
                .expect("source leg has correlation_id"),
            created_at: source_leg.created_at,
        }
    }
}
