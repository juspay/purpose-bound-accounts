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
