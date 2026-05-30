use std::sync::Arc;
use uuid::Uuid;

use crate::domain::pool::PaymentSplit;
use crate::domain::transaction::{TransactionDirection, TransactionStatus, TransactionType};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, MERCHANT_SETTLEMENT_TB_ID};
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const PAYMENT_TRANSFER_CODE: u16 = 200;
const MAX_SPLIT_RETRIES: u32 = 3;

pub struct PbPaymentService {
    pub account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
}

impl PbPaymentService {
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

    #[allow(clippy::too_many_arguments)]
    pub async fn make_payment(
        &self,
        account_id: Uuid,
        amount: u64,
        merchant_mcc: &str,
        merchant_id: &str,
        description: &str,
        idempotency_key: Option<&str>,
        gateway_ref: Option<&str>,
    ) -> Result<PaymentResult, AppError> {
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
                // payment_id is correlation_id (also = primary row's id for payments
                // written by this service). Fall back to id for legacy rows.
                let payment_id = existing.correlation_id.unwrap_or(existing.id);
                return Ok(PaymentResult {
                    payment_id,
                    account_id: existing.account_id,
                    amount: existing.amount,
                    from_others: if existing.pool.as_deref() == Some("others") {
                        existing.amount
                    } else {
                        0
                    },
                    from_self: if existing.pool.as_deref() == Some("self") {
                        existing.amount
                    } else {
                        0
                    },
                    merchant_id: existing.merchant_id.unwrap_or_default(),
                    merchant_mcc: existing.merchant_mcc.unwrap_or_default(),
                    gateway_ref: existing.gateway_ref,
                });
            }
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::PbAccountNotActive(account_id.to_string()));
        }

        // Validate MCC
        let mcc_allowed = self
            .account_repo
            .is_mcc_allowed(&account.purpose_code, merchant_mcc)
            .await?;
        if !mcc_allowed {
            return Err(AppError::InvalidMcc {
                mcc: merchant_mcc.to_string(),
                purpose_code: account.purpose_code.clone(),
            });
        }

        // Retry loop for stale balance
        let mut last_err = None;
        for attempt in 0..MAX_SPLIT_RETRIES {
            if attempt > 0 {
                tracing::info!(account_id = %account_id, attempt, "Retrying payment with fresh balance");
            }

            let balance = self
                .ledger_repo
                .get_balance(account.tb_self_account_id, account.tb_others_account_id)
                .await?;

            let split = match PaymentSplit::calculate(&balance, amount) {
                Some(s) => s,
                None => {
                    return Err(AppError::InsufficientFunds {
                        requested: amount,
                        available: balance.total(),
                    });
                }
            };

            // Begin PG transaction
            let mut tx = self.transaction_repo.pool().begin().await?;

            // Single payment_id, used as both the primary row's id and the
            // correlation_id linking split legs. Lookup-by-payment_id can use
            // either column.
            let payment_id = Uuid::now_v7();

            // Insert transaction row(s). The "primary" row (idempotency-key holder)
            // gets id=payment_id; the secondary split leg gets a fresh id but shares
            // correlation_id=payment_id.
            if split.from_others > 0 {
                self.transaction_repo
                    .insert_in_tx(
                        &mut tx,
                        payment_id,
                        account_id,
                        crate::domain::account_kind::AccountKind::Pb,
                        TransactionType::Payment,
                        TransactionStatus::Settled,
                        split.from_others,
                        Some("others"),
                        TransactionDirection::Outbound,
                        None,
                        None,
                        gateway_ref,
                        None,
                        Some(merchant_id),
                        Some(merchant_mcc),
                        Some(description),
                        None, // funding_type
                        0,
                        idempotency_key,
                        Some(payment_id),
                        None,
                    )
                    .await?;
            }
            if split.from_self > 0 {
                // For split payments, only the first row gets the idempotency key
                // (unique constraint) and the payment_id-as-row-id.
                let (row_id, idem_key) = if split.from_others > 0 {
                    (Uuid::now_v7(), None)
                } else {
                    (payment_id, idempotency_key)
                };
                self.transaction_repo
                    .insert_in_tx(
                        &mut tx,
                        row_id,
                        account_id,
                        crate::domain::account_kind::AccountKind::Pb,
                        TransactionType::Payment,
                        TransactionStatus::Settled,
                        split.from_self,
                        Some("self"),
                        TransactionDirection::Outbound,
                        None,
                        None,
                        gateway_ref,
                        None,
                        Some(merchant_id),
                        Some(merchant_mcc),
                        Some(description),
                        None, // funding_type
                        0,
                        idem_key,
                        Some(payment_id),
                        None,
                    )
                    .await?;
            }

            // Execute TB transfer(s)
            let tb_result = self.execute_transfer(&account, &split).await;

            match tb_result {
                Ok(()) => {
                    tx.commit().await?;
                    tracing::info!(
                        payment_id = %payment_id, account_id = %account_id,
                        merchant_id, merchant_mcc,
                        amount, from_others = split.from_others, from_self = split.from_self,
                        "Payment processed"
                    );
                    return Ok(PaymentResult {
                        payment_id,
                        account_id,
                        amount,
                        from_others: split.from_others,
                        from_self: split.from_self,
                        merchant_id: merchant_id.to_string(),
                        merchant_mcc: merchant_mcc.to_string(),
                        gateway_ref: gateway_ref.map(|s| s.to_string()),
                    });
                }
                Err(AppError::ExceedsBalance) => {
                    // Rollback happens automatically when tx is dropped
                    last_err = Some(AppError::ExceedsBalance);
                }
                Err(e) => return Err(e),
            }
        }

        let balance = self
            .ledger_repo
            .get_balance(account.tb_self_account_id, account.tb_others_account_id)
            .await?;

        Err(last_err.unwrap_or(AppError::InsufficientFunds {
            requested: amount,
            available: balance.total(),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn refund_payment(
        &self,
        pb_account_id: Uuid,
        original_payment_id: Uuid,
        amount: u64,
        description: Option<&str>,
        gateway_ref: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<RefundResult, AppError> {
        // Step 1: idempotency replay.
        if let Some(key) = idempotency_key {
            if let Some(existing) = self
                .transaction_repo
                .find_by_idempotency_key(
                    crate::domain::account_kind::AccountKind::Pb,
                    pb_account_id,
                    key,
                )
                .await?
            {
                let correlation_id = existing.correlation_id.unwrap_or(existing.id);
                let refund_rows = self
                    .transaction_repo
                    .find_by_correlation_id(correlation_id)
                    .await?;
                let amount_to_self: u64 = refund_rows
                    .iter()
                    .filter(|r| r.pool.as_deref() == Some("self"))
                    .map(|r| r.amount)
                    .sum();
                let amount_to_others: u64 = refund_rows
                    .iter()
                    .filter(|r| r.pool.as_deref() == Some("others"))
                    .map(|r| r.amount)
                    .sum();
                let total_amount = amount_to_self + amount_to_others;

                // Recover original_payment_id from a refund row's reverses_transaction_id.
                let reverses_id = refund_rows
                    .first()
                    .and_then(|r| r.reverses_transaction_id)
                    .ok_or_else(|| {
                        AppError::DatabaseError(
                            "refund row missing reverses_transaction_id".into(),
                        )
                    })?;
                let original_row = self
                    .transaction_repo
                    .get_transaction(reverses_id)
                    .await?;
                let original_payment_id_resolved = original_row
                    .correlation_id
                    .unwrap_or(original_row.id);
                let originals = self
                    .transaction_repo
                    .find_by_correlation_id(original_payment_id_resolved)
                    .await?;
                let original_amount: u64 = originals.iter().map(|r| r.amount).sum();
                let mut total_refunded: u64 = 0;
                for o in &originals {
                    total_refunded += self.transaction_repo.sum_refunds_of(o.id).await?;
                }
                let remaining_refundable = original_amount.saturating_sub(total_refunded);

                return Ok(RefundResult {
                    refund_id: correlation_id,
                    original_payment_id: original_payment_id_resolved,
                    account_id: pb_account_id,
                    amount: total_amount,
                    amount_to_self,
                    amount_to_others,
                    original_amount,
                    remaining_refundable,
                    status: existing.status,
                    correlation_id,
                    created_at: existing.created_at,
                });
            }
        }

        // Step 2: load original payment rows.
        let original_rows = self
            .transaction_repo
            .find_by_correlation_id(original_payment_id)
            .await?;
        if original_rows.is_empty() {
            return Err(AppError::TransactionNotFound(
                original_payment_id.to_string(),
            ));
        }
        for row in &original_rows {
            if row.account_id != pb_account_id {
                return Err(AppError::RefundNotRefundable(
                    original_payment_id.to_string(),
                    "wrong_account".into(),
                ));
            }
            if row.transaction_type != TransactionType::Payment {
                return Err(AppError::RefundNotRefundable(
                    original_payment_id.to_string(),
                    "wrong_type".into(),
                ));
            }
            if row.status != TransactionStatus::Settled {
                return Err(AppError::RefundNotRefundable(
                    original_payment_id.to_string(),
                    "not_settled".into(),
                ));
            }
            if row.reverses_transaction_id.is_some() {
                return Err(AppError::RefundNotRefundable(
                    original_payment_id.to_string(),
                    "is_itself_a_refund".into(),
                ));
            }
        }

        let p_self = original_rows
            .iter()
            .find(|r| r.pool.as_deref() == Some("self"));
        let p_others = original_rows
            .iter()
            .find(|r| r.pool.as_deref() == Some("others"));

        // Step 3: per-pool remaining.
        let self_remaining = match p_self {
            Some(r) => r.amount.saturating_sub(
                self.transaction_repo.sum_refunds_of(r.id).await?,
            ),
            None => 0,
        };
        let others_remaining = match p_others {
            Some(r) => r.amount.saturating_sub(
                self.transaction_repo.sum_refunds_of(r.id).await?,
            ),
            None => 0,
        };
        let total_remaining = self_remaining + others_remaining;

        // Step 4: amount validation.
        if amount == 0 || amount > total_remaining {
            if total_remaining == 0 {
                return Err(AppError::PaymentFullyRefunded(
                    original_payment_id.to_string(),
                ));
            }
            return Err(AppError::RefundAmountInvalid {
                requested: amount,
                remaining: total_remaining,
            });
        }

        // Step 5: PB account active check.
        let account = self.account_repo.get_account(pb_account_id).await?;
        if !account.status.is_active() {
            return Err(AppError::PbAccountNotActive(pb_account_id.to_string()));
        }

        // Step 6: allocate self-first.
        let take_self = amount.min(self_remaining);
        let take_others = amount - take_self;

        // Step 7: insert refund rows in one PG transaction.
        let refund_correlation_id = Uuid::now_v7();
        let mut tx = self.transaction_repo.pool().begin().await?;

        if take_self > 0 {
            let row_id = Uuid::now_v7();
            let p_self_row = p_self.expect("take_self>0 requires p_self");
            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    row_id,
                    pb_account_id,
                    crate::domain::account_kind::AccountKind::Pb,
                    TransactionType::Payment,
                    TransactionStatus::Settled,
                    take_self,
                    Some("self"),
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    None,
                    p_self_row.merchant_id.as_deref(),
                    p_self_row.merchant_mcc.as_deref(),
                    description,
                    p_self_row.funding_type.as_deref(),
                    0,
                    idempotency_key,
                    Some(refund_correlation_id),
                    Some(p_self_row.id),
                )
                .await?;
        }

        if take_others > 0 {
            let row_id = Uuid::now_v7();
            let p_others_row = p_others.expect("take_others>0 requires p_others");
            // idempotency_key lives on the primary row (self if present, else others).
            let idem = if take_self > 0 { None } else { idempotency_key };
            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    row_id,
                    pb_account_id,
                    crate::domain::account_kind::AccountKind::Pb,
                    TransactionType::Payment,
                    TransactionStatus::Settled,
                    take_others,
                    Some("others"),
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    None,
                    p_others_row.merchant_id.as_deref(),
                    p_others_row.merchant_mcc.as_deref(),
                    description,
                    p_others_row.funding_type.as_deref(),
                    0,
                    idem,
                    Some(refund_correlation_id),
                    Some(p_others_row.id),
                )
                .await?;
        }

        // Step 8: TB transfer(s).
        if take_self > 0 && take_others > 0 {
            self.ledger_repo
                .create_payment_refund_split(
                    account.tb_self_account_id,
                    account.tb_others_account_id,
                    take_self,
                    take_others,
                )
                .await?;
        } else if take_self > 0 {
            self.ledger_repo
                .create_payment_refund(account.tb_self_account_id, take_self)
                .await?;
        } else {
            self.ledger_repo
                .create_payment_refund(account.tb_others_account_id, take_others)
                .await?;
        }

        // Step 9 deferred: tb_transfer_id propagation matches make_payment's posture
        // (rows carry tb_transfer_id=0; the existing helpers don't surface TB ids).
        // See plan Task 6b note.

        tx.commit().await?;

        let original_amount: u64 = original_rows.iter().map(|r| r.amount).sum();
        let remaining_refundable = total_remaining - amount;

        tracing::info!(
            refund_id = %refund_correlation_id,
            original_payment_id = %original_payment_id,
            pb_account_id = %pb_account_id,
            amount,
            take_self,
            take_others,
            remaining_refundable,
            "Payment refund processed"
        );

        Ok(RefundResult {
            refund_id: refund_correlation_id,
            original_payment_id,
            account_id: pb_account_id,
            amount,
            amount_to_self: take_self,
            amount_to_others: take_others,
            original_amount,
            remaining_refundable,
            status: TransactionStatus::Settled,
            correlation_id: refund_correlation_id,
            created_at: chrono::Utc::now(),
        })
    }

    async fn execute_transfer(
        &self,
        account: &crate::domain::account::PurposeBoundAccount,
        split: &PaymentSplit,
    ) -> Result<(), AppError> {
        if split.from_others > 0 && split.from_self > 0 {
            self.ledger_repo
                .create_linked_transfers(
                    account.tb_others_account_id,
                    account.tb_self_account_id,
                    MERCHANT_SETTLEMENT_TB_ID,
                    split.from_others,
                    split.from_self,
                    PAYMENT_TRANSFER_CODE,
                )
                .await
        } else if split.from_others > 0 {
            self.ledger_repo
                .create_transfer(
                    account.tb_others_account_id,
                    MERCHANT_SETTLEMENT_TB_ID,
                    split.from_others,
                    PAYMENT_TRANSFER_CODE,
                )
                .await
        } else {
            self.ledger_repo
                .create_transfer(
                    account.tb_self_account_id,
                    MERCHANT_SETTLEMENT_TB_ID,
                    split.from_self,
                    PAYMENT_TRANSFER_CODE,
                )
                .await
        }
    }
}

pub struct PaymentResult {
    pub payment_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub from_others: u64,
    pub from_self: u64,
    pub merchant_id: String,
    pub merchant_mcc: String,
    pub gateway_ref: Option<String>,
}

pub struct RefundResult {
    pub refund_id: Uuid,
    pub original_payment_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub amount_to_self: u64,
    pub amount_to_others: u64,
    pub original_amount: u64,
    pub remaining_refundable: u64,
    pub status: crate::domain::transaction::TransactionStatus,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
