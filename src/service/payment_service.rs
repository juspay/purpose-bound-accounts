use std::sync::Arc;
use uuid::Uuid;

use crate::domain::pool::PaymentSplit;
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::ledger_repo::LedgerRepo;

const PAYMENT_TRANSFER_CODE: u16 = 200;
/// A designated "merchant settlement" account in TB that payments flow to.
/// In production this would be resolved per-merchant.
const MERCHANT_SETTLEMENT_TB_ID: u128 = 1;

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

        // Get pool balances
        let balance = self
            .ledger_repo
            .get_balance(account.tb_self_account_id, account.tb_others_account_id)
            .await?;

        // Calculate payment split (others-first priority)
        let split = PaymentSplit::calculate(&balance, amount).ok_or(
            AppError::InsufficientFunds {
                requested: amount,
                available: balance.total(),
            },
        )?;

        // Execute transfer(s)
        if split.from_others > 0 && split.from_self > 0 {
            // Linked transfer chain: others + self
            self.ledger_repo
                .create_linked_transfers(
                    account.tb_others_account_id,
                    account.tb_self_account_id,
                    MERCHANT_SETTLEMENT_TB_ID,
                    split.from_others,
                    split.from_self,
                    PAYMENT_TRANSFER_CODE,
                )
                .await?;
        } else if split.from_others > 0 {
            self.ledger_repo
                .create_transfer(
                    account.tb_others_account_id,
                    MERCHANT_SETTLEMENT_TB_ID,
                    split.from_others,
                    PAYMENT_TRANSFER_CODE,
                )
                .await?;
        } else {
            self.ledger_repo
                .create_transfer(
                    account.tb_self_account_id,
                    MERCHANT_SETTLEMENT_TB_ID,
                    split.from_self,
                    PAYMENT_TRANSFER_CODE,
                )
                .await?;
        }

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

        Ok(PaymentResult {
            account_id,
            amount,
            from_others: split.from_others,
            from_self: split.from_self,
            merchant_id: merchant_id.to_string(),
            merchant_mcc: merchant_mcc.to_string(),
        })
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
