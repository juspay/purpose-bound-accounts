use std::sync::Arc;
use uuid::Uuid;

use crate::domain::pool::PaymentSplit;
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::LedgerRepo;

use crate::repository::ledger_repo::MERCHANT_SETTLEMENT_TB_ID;

const PAYMENT_TRANSFER_CODE: u16 = 200;
const MAX_SPLIT_RETRIES: u32 = 3;

pub struct PaymentService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
}

impl PaymentService {
    pub fn new(account_repo: Arc<AccountRepo>, ledger_repo: Arc<LedgerRepo>) -> Self {
        Self {
            account_repo,
            ledger_repo,
        }
    }

    pub async fn make_payment(
        &self,
        account_id: Uuid,
        amount: u64,
        merchant_mcc: &str,
        merchant_id: &str,
        description: &str,
    ) -> Result<PaymentResult, AppError> {
        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        // Validate MCC against account's purpose
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

        // Retry loop: re-read balance and recompute split if TB rejects due to stale balance
        let mut last_err = None;
        for attempt in 0..MAX_SPLIT_RETRIES {
            if attempt > 0 {
                tracing::info!(
                    account_id = %account_id,
                    attempt,
                    "Retrying payment with fresh balance after stale split"
                );
            }

            // Get pool balances
            let balance = self
                .ledger_repo
                .get_balance(account.tb_self_account_id, account.tb_others_account_id)
                .await?;

            // Calculate payment split (others-first priority)
            let split = match PaymentSplit::calculate(&balance, amount) {
                Some(s) => s,
                None => {
                    return Err(AppError::InsufficientFunds {
                        requested: amount,
                        available: balance.total(),
                    });
                }
            };

            // Execute transfer(s)
            let transfer_result = self
                .execute_transfer(&account, &split)
                .await;

            match transfer_result {
                Ok(()) => {
                    tracing::info!(
                        account_id = %account_id,
                        merchant_id = merchant_id,
                        merchant_mcc = merchant_mcc,
                        description = description,
                        amount = amount,
                        from_others = split.from_others,
                        from_self = split.from_self,
                        "Payment processed"
                    );

                    return Ok(PaymentResult {
                        account_id,
                        amount,
                        from_others: split.from_others,
                        from_self: split.from_self,
                        merchant_id: merchant_id.to_string(),
                        merchant_mcc: merchant_mcc.to_string(),
                    });
                }
                Err(AppError::ExceedsBalance) => {
                    last_err = Some(AppError::ExceedsBalance);
                    // Continue to next retry iteration with fresh balance
                }
                Err(e) => return Err(e),
            }
        }

        // All retries exhausted — re-read balance for accurate error message
        let balance = self
            .ledger_repo
            .get_balance(account.tb_self_account_id, account.tb_others_account_id)
            .await?;

        tracing::warn!(
            account_id = %account_id,
            amount,
            retries = MAX_SPLIT_RETRIES,
            available = balance.total(),
            "Payment failed after all retries"
        );

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
    pub account_id: Uuid,
    pub amount: u64,
    pub from_others: u64,
    pub from_self: u64,
    pub merchant_id: String,
    pub merchant_mcc: String,
}
