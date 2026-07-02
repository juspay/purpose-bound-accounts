use std::sync::Arc;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, THIRD_PARTY_FUNDING_SOURCE_TB_ID};
use crate::repository::normal_account_repo::NormalAccountRepo;
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

pub struct PbContributionReturnService {
    pub pb_account_repo: Arc<PbAccountRepo>,
    pub normal_account_repo: Arc<NormalAccountRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub default_pending_timeout_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct AllocationEntry {
    pub original_transaction_id: Uuid,
    pub amount: u64,
}

#[derive(Debug, Clone)]
pub struct ContributionReturnResult {
    pub return_id: Uuid,
    pub account_id: Uuid,
    pub funding_type: String,
    pub amount: u64,
    pub allocations: Vec<AllocationEntry>,
    pub remaining_returnable_after: u64,
    pub status: TransactionStatus,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl PbContributionReturnService {
    pub fn new(
        pb_account_repo: Arc<PbAccountRepo>,
        normal_account_repo: Arc<NormalAccountRepo>,
        transaction_repo: Arc<TransactionRepo>,
        ledger_repo: Arc<LedgerRepo>,
        default_pending_timeout_seconds: u32,
    ) -> Self {
        Self {
            pb_account_repo,
            normal_account_repo,
            transaction_repo,
            ledger_repo,
            default_pending_timeout_seconds,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn return_contribution(
        &self,
        pb_account_id: Uuid,
        amount: u64,
        funding_type: &str,
        pending: bool,
        timeout_seconds: Option<u32>,
        gateway_ref: Option<&str>,
        description: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<ContributionReturnResult, AppError> {
        // Step 1: idempotency replay.
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(AccountKind::Pb, pb_account_id, key)
                .await?
            {
                let correlation_id = existing.correlation_id.unwrap_or(existing.id);
                let rows = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                let total_amount: u64 = rows.iter().map(|r| r.amount).sum();
                let allocations = rows
                    .iter()
                    .map(|r| AllocationEntry {
                        original_transaction_id: r
                            .reverses_transaction_id
                            .expect("return row missing reverses_transaction_id"),
                        amount: r.amount,
                    })
                    .collect();
                let remaining_returnable_after =
                    self.compute_remaining(pb_account_id, funding_type).await?;
                return Ok(ContributionReturnResult {
                    return_id: correlation_id,
                    account_id: pb_account_id,
                    funding_type: funding_type.to_string(),
                    amount: total_amount,
                    allocations,
                    remaining_returnable_after,
                    status: existing.status,
                    correlation_id,
                    created_at: existing.created_at,
                });
            }
        }

        self.execute_return(
            pb_account_id,
            amount,
            funding_type,
            pending,
            timeout_seconds,
            gateway_ref,
            description,
            idempotency_key,
        )
        .await
    }

    async fn compute_remaining(
        &self,
        pb_account_id: Uuid,
        funding_type: &str,
    ) -> Result<u64, AppError> {
        let contributed = self
            .transaction_repo
            .sum_others_contributions(pb_account_id, funding_type)
            .await?;
        let returned = self
            .transaction_repo
            .sum_others_returns(pb_account_id, funding_type)
            .await?;
        Ok(contributed.saturating_sub(returned))
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_return(
        &self,
        pb_account_id: Uuid,
        amount: u64,
        funding_type: &str,
        pending: bool,
        timeout_seconds: Option<u32>,
        gateway_ref: Option<&str>,
        description: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<ContributionReturnResult, AppError> {
        if funding_type != "trust" && funding_type != "third_party" {
            return Err(AppError::Validation(format!(
                "funding_type must be 'trust' or 'third_party', got {funding_type:?}"
            )));
        }

        // Step 2: begin DB tx.
        let mut tx = self.transaction_repo.pool().begin().await?;

        // Step 3: find candidate originals under a row lock.
        let originals = self
            .transaction_repo
            .find_returnable_originals_for_update(&mut tx, pb_account_id, funding_type)
            .await?;

        // Step 4: compute per-original remaining.
        let mut candidates: Vec<(TransactionRecord, u64)> = Vec::with_capacity(originals.len());
        for o in originals.into_iter() {
            let already_returned = self
                .transaction_repo
                .sum_returns_of_in_tx(&mut tx, o.id)
                .await?;
            let remaining = o.amount.saturating_sub(already_returned);
            if remaining > 0 {
                candidates.push((o, remaining));
            }
        }
        let total_available: u64 = candidates.iter().map(|(_, r)| *r).sum();

        // Step 5: validate.
        if total_available == 0 {
            return Err(AppError::ContributionFullyReturned(
                pb_account_id.to_string(),
            ));
        }
        if amount == 0 || amount > total_available {
            return Err(AppError::ContributionAmountInvalid {
                requested: amount,
                remaining: total_available,
            });
        }

        // Step 6: PB account active check.
        let pb_account = self.pb_account_repo.get_account(pb_account_id).await?;
        if !pb_account.status.is_active() {
            return Err(AppError::PbAccountNotActive(pb_account_id.to_string()));
        }

        // Step 7: FIFO allocation.
        let mut allocations_raw: Vec<(TransactionRecord, u64)> = Vec::new();
        let mut amount_left = amount;
        for (original, remaining) in candidates.into_iter() {
            if amount_left == 0 {
                break;
            }
            let take = amount_left.min(remaining);
            allocations_raw.push((original, take));
            amount_left -= take;
        }
        debug_assert_eq!(amount_left, 0);

        let row_status = if pending {
            TransactionStatus::Pending
        } else {
            TransactionStatus::Settled
        };
        let timeout = if pending {
            Some(timeout_seconds.unwrap_or(self.default_pending_timeout_seconds))
        } else {
            None
        };

        let return_correlation_id = Uuid::now_v7();

        // Step 8: insert one Withdrawal row per allocation.
        let mut row_ids: Vec<Uuid> = Vec::with_capacity(allocations_raw.len());
        for (idx, (original, take)) in allocations_raw.iter().enumerate() {
            // First row's id == correlation_id (mirrors make_payment / refund pattern).
            let row_id = if idx == 0 {
                return_correlation_id
            } else {
                Uuid::now_v7()
            };
            row_ids.push(row_id);
            let idem = if idx == 0 { idempotency_key } else { None };
            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    row_id,
                    pb_account_id,
                    AccountKind::Pb,
                    TransactionType::Withdrawal,
                    row_status,
                    *take,
                    Some("others"),
                    TransactionDirection::Outbound,
                    None,
                    None,
                    gateway_ref,
                    timeout,
                    None,
                    None,
                    description,
                    Some(funding_type),
                    0,
                    idem,
                    Some(return_correlation_id),
                    Some(original.id),
                )
                .await?;
        }

        // Step 9: TB transfers, one per allocation. Persist returned tb_transfer_id when pending.
        for (idx, (original, take)) in allocations_raw.iter().enumerate() {
            let credit_destination_tb_id = self
                .resolve_credit_destination(original, funding_type)
                .await?;
            if pending {
                let tb_id = self
                    .ledger_repo
                    .create_pending_contribution_return(
                        pb_account.tb_others_account_id,
                        credit_destination_tb_id,
                        *take,
                        timeout.expect("timeout populated when pending=true"),
                    )
                    .await?;
                let row_id = row_ids[idx];
                sqlx::query(
                    r#"UPDATE transactions
                       SET tb_transfer_id = $1::numeric, updated_at = now()
                       WHERE id = $2"#,
                )
                .bind(tb_id.to_string())
                .bind(row_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            } else {
                self.ledger_repo
                    .create_contribution_return(
                        pb_account.tb_others_account_id,
                        credit_destination_tb_id,
                        *take,
                    )
                    .await?;
            }
        }

        // Step 10: commit.
        tx.commit().await?;

        // Step 11: build result.
        let allocations = allocations_raw
            .iter()
            .map(|(o, take)| AllocationEntry {
                original_transaction_id: o.id,
                amount: *take,
            })
            .collect();
        let remaining_returnable_after = total_available - amount;

        Ok(ContributionReturnResult {
            return_id: return_correlation_id,
            account_id: pb_account_id,
            funding_type: funding_type.to_string(),
            amount,
            allocations,
            remaining_returnable_after,
            status: row_status,
            correlation_id: return_correlation_id,
            created_at: chrono::Utc::now(),
        })
    }

    async fn resolve_credit_destination(
        &self,
        original: &TransactionRecord,
        funding_type: &str,
    ) -> Result<u128, AppError> {
        if funding_type == "third_party" {
            return Ok(THIRD_PARTY_FUNDING_SOURCE_TB_ID);
        }
        // 'trust': look up the normal-side leg of the original transfer.
        let correlation_id = original.correlation_id.ok_or_else(|| {
            AppError::DatabaseError(
                "trust contribution original missing correlation_id".to_string(),
            )
        })?;
        let legs = self
            .transaction_repo
            .find_by_correlation_id(correlation_id)
            .await?;
        let normal_leg = legs
            .iter()
            .find(|l| l.account_kind == AccountKind::Normal)
            .ok_or_else(|| {
                AppError::DatabaseError(
                    "trust contribution original missing normal leg".to_string(),
                )
            })?;
        let normal_account = self
            .normal_account_repo
            .get_account(normal_leg.account_id)
            .await?;
        Ok(normal_account.tb_account_id)
    }
}
