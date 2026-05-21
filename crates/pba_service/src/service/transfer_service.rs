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

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReversalResult {
    pub reversal_id: Uuid,            // normal-side reversal row id
    pub original_transfer_id: Uuid,   // T_src.id of the original
    pub source_account_id: Uuid,      // the normal account being credited
    pub destination_account_id: Uuid, // the PB account being debited
    pub amount: u64,
    pub original_amount: u64,
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

    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    pub async fn reverse_transfer(
        &self,
        source_normal_id: Uuid,
        original_transfer_id: Uuid,
        amount: u64,
        gateway_ref: Option<&str>,
        description: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<ReversalResult, AppError> {
        // Step 1: Idempotency replay.
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Normal, source_normal_id, key)
                .await?
            {
                let correlation_id = existing.correlation_id.ok_or_else(|| {
                    AppError::DatabaseError(
                        "reversal source row missing correlation_id".to_string(),
                    )
                })?;
                let legs = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                if legs.len() != 2 {
                    return Err(AppError::DatabaseError(
                        "reversal correlation has != 2 legs".to_string(),
                    ));
                }
                let mut result = self.reversal_legs_to_result(&legs, source_normal_id);
                // Best-effort lookup of the original amount via the reverses_transaction_id link.
                if let Some(orig_id) = legs.iter().find_map(|l| l.reverses_transaction_id) {
                    if let Ok(orig) = self.transaction_repo.get_transaction(orig_id).await {
                        result.original_amount = orig.amount;
                    }
                }
                return Ok(result);
            }
        }

        // Step 2: Load and validate the original source row.
        let original = self
            .transaction_repo
            .get_by_id(original_transfer_id, source_normal_id)
            .await?;

        if original.account_kind != AccountKind::Normal
            || original.transaction_type != TransactionType::Transfer
            || original.direction != TransactionDirection::Outbound
        {
            return Err(AppError::TransferNotReversible(
                original_transfer_id.to_string(),
                "wrong_type".to_string(),
            ));
        }
        if original.status != TransactionStatus::Posted {
            return Err(AppError::TransferNotReversible(
                original_transfer_id.to_string(),
                "not_posted".to_string(),
            ));
        }
        if original.reverses_transaction_id.is_some() {
            return Err(AppError::TransferNotReversible(
                original_transfer_id.to_string(),
                "is_itself_a_reversal".to_string(),
            ));
        }

        // Step 3: Reject if already reversed.
        if self
            .transaction_repo
            .find_reversal_of(original_transfer_id)
            .await?
            .is_some()
        {
            return Err(AppError::TransferAlreadyReversed(
                original_transfer_id.to_string(),
            ));
        }

        // Step 4: Validate amount.
        if amount == 0 || amount > original.amount {
            return Err(AppError::ReversalAmountInvalid {
                requested: amount,
                original: original.amount,
            });
        }

        // Step 5: Resolve destination PB account from the original's correlation pair.
        let original_corr = original.correlation_id.ok_or_else(|| {
            AppError::DatabaseError("original transfer row missing correlation_id".to_string())
        })?;
        let original_legs = self
            .transaction_repo
            .find_by_correlation_id(original_corr)
            .await?;
        let dst_leg = original_legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .ok_or_else(|| {
                AppError::DatabaseError("original transfer missing pb leg".to_string())
            })?;
        let destination_pb_id = dst_leg.account_id;
        let destination = self.pb_account_repo.get_account(destination_pb_id).await?;

        // Step 6: Active checks on both sides.
        let source = self
            .normal_account_repo
            .get_account(source_normal_id)
            .await?;
        if !source.status.is_active() {
            return Err(AppError::NormalAccountNotActive(
                source_normal_id.to_string(),
            ));
        }
        if !destination.status.is_active() {
            return Err(AppError::PbAccountNotActive(destination_pb_id.to_string()));
        }

        // Step 7: Insert the two reversal rows under a fresh correlation_id.
        let legs = TransferLegs::new();
        let pb_side_id = legs.source_txn_id;
        let normal_side_id = legs.destination_txn_id;
        let correlation_id = legs.correlation_id;

        let mut tx = self.transaction_repo.pool().begin().await?;

        // PB-side debit row.
        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                pb_side_id,
                destination_pb_id,
                AccountKind::Pb,
                TransactionType::Transfer,
                TransactionStatus::Posted,
                amount,
                Some("others"),
                TransactionDirection::Outbound,
                None,
                None,
                gateway_ref,
                None, // no timeout — reversal is immediate
                None,
                None,
                description,
                Some("trust"),
                0,    // tb_transfer_id (immediate transfers leave this 0, matching transfer())
                None, // no idempotency key on the pb-side row
                Some(correlation_id),
                None, // reverses_transaction_id NULL on pb-side
            )
            .await?;

        // Normal-side credit row — carries the link back to the original.
        self.transaction_repo
            .insert_in_tx(
                &mut tx,
                normal_side_id,
                source_normal_id,
                AccountKind::Normal,
                TransactionType::Transfer,
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
                description,
                Some("trust"),
                0,
                idempotency_key, // idempotency lives here, mirroring transfer()
                Some(correlation_id),
                Some(original_transfer_id), // <-- the link
            )
            .await?;

        // Step 8: Execute the TB transfer (code 410).
        let tb_result = self
            .ledger_repo
            .create_internal_transfer_reversal(
                destination.tb_others_account_id,
                source.tb_account_id,
                amount,
            )
            .await;

        match tb_result {
            Ok(()) => {
                tx.commit().await?;
                let updated_legs = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                let mut result = self.reversal_legs_to_result(&updated_legs, source_normal_id);
                result.original_amount = original.amount;
                Ok(result)
            }
            Err(AppError::ExceedsBalance) => {
                // Roll back PG, fetch fresh balance, surface InsufficientFunds.
                drop(tx);
                let balance = self
                    .ledger_repo
                    .get_single_balance(destination.tb_others_account_id)
                    .await
                    .unwrap_or(crate::repository::ledger_repo::SingleBalance {
                        posted: 0,
                        pending: 0,
                    });
                Err(AppError::InsufficientFunds {
                    requested: amount,
                    available: balance.posted,
                })
            }
            Err(e) => Err(e),
        }
    }

    fn reversal_legs_to_result(
        &self,
        legs: &[TransactionRecord],
        source_normal_id: Uuid,
    ) -> ReversalResult {
        let normal_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Normal)
            .expect("reversal correlation has a normal leg");
        let pb_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Pb)
            .expect("reversal correlation has a pb leg");
        ReversalResult {
            reversal_id: normal_leg.id,
            original_transfer_id: normal_leg
                .reverses_transaction_id
                .expect("normal-side reversal row carries reverses_transaction_id"),
            source_account_id: source_normal_id,
            destination_account_id: pb_leg.account_id,
            amount: normal_leg.amount,
            original_amount: normal_leg.amount, // overwritten by caller with original.amount
            status: normal_leg.status,
            correlation_id: normal_leg
                .correlation_id
                .expect("reversal leg has correlation_id"),
            created_at: normal_leg.created_at,
        }
    }
}
