use std::sync::Arc;
use uuid::Uuid;

use crate::domain::pool::PaymentSplit;
use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{LedgerRepo, MERCHANT_SETTLEMENT_TB_ID};
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const PAYMENT_TRANSFER_CODE: u16 = 200;
const MAX_SPLIT_RETRIES: u32 = 3;

enum RefundResolution {
    Post,
    Void,
}

impl RefundResolution {
    fn target(&self) -> TransactionStatus {
        match self {
            Self::Post => TransactionStatus::Settled,
            Self::Void => TransactionStatus::Voided,
        }
    }

    fn target_sql(&self) -> &'static str {
        match self {
            Self::Post => "settled",
            Self::Void => "voided",
        }
    }
}

pub struct PbPaymentService {
    pub account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
}

impl PbPaymentService {
    pub fn new(
        account_repo: Arc<PbAccountRepo>,
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
        pending: bool,
        timeout_seconds: Option<u32>,
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
                        AppError::DatabaseError("refund row missing reverses_transaction_id".into())
                    })?;
                let original_row = self.transaction_repo.get_transaction(reverses_id).await?;
                let original_payment_id_resolved =
                    original_row.correlation_id.unwrap_or(original_row.id);
                let originals = self
                    .transaction_repo
                    .find_by_correlation_id(original_payment_id_resolved)
                    .await?;
                let original_amount: u64 = originals.iter().map(|r| r.amount).sum();
                // `originals.len()` is 1 or 2 for payments written by make_payment
                // (others-pool row, optional self-pool row).
                let mut total_refunded: u64 = 0;
                for o in &originals {
                    total_refunded += self.transaction_repo.sum_returns_of(o.id).await?;
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

        // Step 2: open a PG transaction up front and load the original payment
        // rows under `SELECT … FOR UPDATE`. The lock is held until commit (after
        // the TB transfer below), so a concurrent refund of the same payment
        // blocks here instead of racing the remaining-amount check.
        let mut tx = self.transaction_repo.pool().begin().await?;
        let original_rows = self
            .transaction_repo
            .find_by_correlation_id_for_update(&mut tx, original_payment_id)
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

        // Step 3: per-pool remaining (reads run inside the locked transaction so
        // they observe every committed refund against these original rows).
        let self_remaining = match p_self {
            Some(r) => r.amount.saturating_sub(
                self.transaction_repo
                    .sum_returns_of_in_tx(&mut tx, r.id)
                    .await?,
            ),
            None => 0,
        };
        let others_remaining = match p_others {
            Some(r) => r.amount.saturating_sub(
                self.transaction_repo
                    .sum_returns_of_in_tx(&mut tx, r.id)
                    .await?,
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

        // Step 2 (extended): compute row status and timeout once.
        let row_status = if pending {
            TransactionStatus::Pending
        } else {
            TransactionStatus::Settled
        };
        let timeout = if pending {
            Some(timeout_seconds.unwrap_or(self.default_timeout_seconds))
        } else {
            None
        };

        // Step 6: allocate self-first.
        let take_self = amount.min(self_remaining);
        let take_others = amount - take_self;

        let original_amount: u64 = original_rows.iter().map(|r| r.amount).sum();
        let remaining_refundable = total_remaining - amount;

        // Step 7: insert refund rows in the locked transaction.
        let refund_correlation_id = Uuid::now_v7();

        // Primary leg gets row_id == refund_correlation_id (mirrors make_payment's
        // payment_id pattern so /admin/transactions/{correlation_id} resolves to a
        // real row). Secondary leg gets a fresh id.
        if take_self > 0 {
            let row_id = refund_correlation_id;
            let p_self_row = p_self.expect("take_self>0 requires p_self");
            self.transaction_repo
                .insert_in_tx(
                    &mut tx,
                    row_id,
                    pb_account_id,
                    crate::domain::account_kind::AccountKind::Pb,
                    TransactionType::Payment,
                    row_status,
                    take_self,
                    Some("self"),
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    timeout,
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
            let row_id = if take_self > 0 {
                Uuid::now_v7()
            } else {
                refund_correlation_id
            };
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
                    row_status,
                    take_others,
                    Some("others"),
                    TransactionDirection::Inbound,
                    None,
                    None,
                    gateway_ref,
                    timeout,
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
        let (tb_self_id, tb_others_id): (Option<u128>, Option<u128>) =
            if take_self > 0 && take_others > 0 {
                if pending {
                    let (s, o) = self
                        .ledger_repo
                        .create_pending_payment_refund_split(
                            account.tb_self_account_id,
                            account.tb_others_account_id,
                            take_self,
                            take_others,
                            timeout.expect("timeout populated when pending=true"),
                        )
                        .await?;
                    (Some(s), Some(o))
                } else {
                    self.ledger_repo
                        .create_payment_refund_split(
                            account.tb_self_account_id,
                            account.tb_others_account_id,
                            take_self,
                            take_others,
                        )
                        .await?;
                    (None, None)
                }
            } else if take_self > 0 {
                if pending {
                    let id = self
                        .ledger_repo
                        .create_pending_payment_refund(
                            account.tb_self_account_id,
                            take_self,
                            timeout.expect("timeout populated when pending=true"),
                        )
                        .await?;
                    (Some(id), None)
                } else {
                    self.ledger_repo
                        .create_payment_refund(account.tb_self_account_id, take_self)
                        .await?;
                    (None, None)
                }
            } else if pending {
                let id = self
                    .ledger_repo
                    .create_pending_payment_refund(
                        account.tb_others_account_id,
                        take_others,
                        timeout.expect("timeout populated when pending=true"),
                    )
                    .await?;
                (None, Some(id))
            } else {
                self.ledger_repo
                    .create_payment_refund(account.tb_others_account_id, take_others)
                    .await?;
                (None, None)
            };

        // Step 4 (brief): persist returned TB ids when pending.
        if pending {
            if let Some(tb_id) = tb_self_id {
                sqlx::query(
                    r#"UPDATE transactions
                       SET tb_transfer_id = $1::numeric, updated_at = now()
                       WHERE correlation_id = $2 AND pool = 'self'"#,
                )
                .bind(tb_id.to_string())
                .bind(refund_correlation_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            }
            if let Some(tb_id) = tb_others_id {
                sqlx::query(
                    r#"UPDATE transactions
                       SET tb_transfer_id = $1::numeric, updated_at = now()
                       WHERE correlation_id = $2 AND pool = 'others'"#,
                )
                .bind(tb_id.to_string())
                .bind(refund_correlation_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::DatabaseError(e.to_string()))?;
            }
        }

        tx.commit().await?;

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
            status: row_status,
            correlation_id: refund_correlation_id,
            created_at: chrono::Utc::now(),
        })
    }

    pub async fn post_refund(
        &self,
        pb_account_id: Uuid,
        refund_id: Uuid,
    ) -> Result<RefundResult, AppError> {
        self.resolve_refund(pb_account_id, refund_id, RefundResolution::Post)
            .await
    }

    pub async fn void_refund(
        &self,
        pb_account_id: Uuid,
        refund_id: Uuid,
    ) -> Result<RefundResult, AppError> {
        self.resolve_refund(pb_account_id, refund_id, RefundResolution::Void)
            .await
    }

    async fn resolve_refund(
        &self,
        pb_account_id: Uuid,
        refund_id: Uuid,
        direction: RefundResolution,
    ) -> Result<RefundResult, AppError> {
        // Begin a transaction and lock the refund rows so concurrent post/void
        // calls on the same refund serialise here.
        let mut tx = self.transaction_repo.pool().begin().await?;
        let rows = self
            .transaction_repo
            .find_by_correlation_id_for_update(&mut tx, refund_id)
            .await?;
        if rows.is_empty() {
            return Err(AppError::TransactionNotFound(refund_id.to_string()));
        }
        for r in &rows {
            if r.account_kind != crate::domain::account_kind::AccountKind::Pb
                || r.account_id != pb_account_id
                || r.transaction_type != TransactionType::Payment
                || r.reverses_transaction_id.is_none()
            {
                return Err(AppError::TransactionNotFound(refund_id.to_string()));
            }
        }

        // Idempotent same-direction no-op
        if rows.iter().all(|r| r.status == direction.target()) {
            // Commit to release the lock, then rebuild result from pool.
            tx.commit().await?;
            let updated = self
                .transaction_repo
                .find_by_correlation_id(refund_id)
                .await?;
            return self
                .build_refund_result_from_rows(pb_account_id, refund_id, &updated)
                .await;
        }
        if rows.iter().any(|r| r.status != TransactionStatus::Pending) {
            return Err(AppError::TransactionNotPending(refund_id.to_string()));
        }

        // Per-leg TB resolution. Tolerate already-resolved errors from the TB
        // layer in case a LINKED chain auto-resolves the tail.
        for r in &rows {
            if r.tb_transfer_id != 0 {
                let res = match direction {
                    RefundResolution::Post => {
                        self.ledger_repo
                            .post_pending_transfer(r.tb_transfer_id)
                            .await
                    }
                    RefundResolution::Void => {
                        self.ledger_repo
                            .void_pending_transfer(r.tb_transfer_id)
                            .await
                    }
                };
                match res {
                    Ok(()) => {}
                    Err(AppError::TbPendingAlreadyResolved) => {} // tolerate; row will be UPDATEd anyway
                    Err(e) => return Err(e),
                }
            }
        }

        sqlx::query(
            r#"UPDATE transactions
               SET status = $1, updated_at = now()
               WHERE correlation_id = $2 AND status = 'pending'"#,
        )
        .bind(direction.target_sql())
        .bind(refund_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        tx.commit().await?;

        // Post-commit read to build the result (no lock needed).
        let updated = self
            .transaction_repo
            .find_by_correlation_id(refund_id)
            .await?;
        self.build_refund_result_from_rows(pb_account_id, refund_id, &updated)
            .await
    }

    async fn build_refund_result_from_rows(
        &self,
        pb_account_id: Uuid,
        refund_id: Uuid,
        rows: &[TransactionRecord],
    ) -> Result<RefundResult, AppError> {
        let amount_to_self: u64 = rows
            .iter()
            .filter(|r| r.pool.as_deref() == Some("self"))
            .map(|r| r.amount)
            .sum();
        let amount_to_others: u64 = rows
            .iter()
            .filter(|r| r.pool.as_deref() == Some("others"))
            .map(|r| r.amount)
            .sum();
        let total_amount = amount_to_self + amount_to_others;

        // Derive original_payment_id and original_amount via reverses_transaction_id.
        let reverses_id = rows
            .first()
            .and_then(|r| r.reverses_transaction_id)
            .ok_or_else(|| {
                AppError::DatabaseError("refund row missing reverses_transaction_id".into())
            })?;
        let original_row = self.transaction_repo.get_transaction(reverses_id).await?;
        let original_payment_id = original_row.correlation_id.unwrap_or(original_row.id);
        let originals = self
            .transaction_repo
            .find_by_correlation_id(original_payment_id)
            .await?;
        let original_amount: u64 = originals.iter().map(|r| r.amount).sum();

        let mut total_refunded: u64 = 0;
        for o in &originals {
            total_refunded += self.transaction_repo.sum_returns_of(o.id).await?;
        }
        let remaining_refundable = original_amount.saturating_sub(total_refunded);

        let status = rows
            .first()
            .map(|r| r.status)
            .unwrap_or(TransactionStatus::Settled);
        let created_at = rows
            .first()
            .map(|r| r.created_at)
            .unwrap_or_else(chrono::Utc::now);

        Ok(RefundResult {
            refund_id,
            original_payment_id,
            account_id: pb_account_id,
            amount: total_amount,
            amount_to_self,
            amount_to_others,
            original_amount,
            remaining_refundable,
            status,
            correlation_id: refund_id,
            created_at,
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
