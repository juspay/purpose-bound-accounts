use std::sync::Arc;
use uuid::Uuid;

use crate::domain::deposit::DepositStatus;
use crate::error::AppError;
use crate::repository::account_repo::AccountRepo;
use crate::repository::deposit_repo::DepositRepo;
use crate::repository::ledger_repo::{LedgerRepo, FUNDING_SOURCE_TB_ID};

const DEPOSIT_TRANSFER_CODE: u16 = 100;
const PENDING_DEPOSIT_TRANSFER_CODE: u16 = 101;

pub struct DepositService {
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub deposit_repo: Arc<DepositRepo>,
    pub default_timeout_seconds: u32,
}

impl DepositService {
    pub fn new(
        account_repo: Arc<AccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        deposit_repo: Arc<DepositRepo>,
        default_timeout_seconds: u32,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            deposit_repo,
            default_timeout_seconds,
        }
    }

    pub async fn deposit(
        &self,
        account_id: Uuid,
        source_ifsc: &str,
        source_account_number: &str,
        amount: u64,
        pending: bool,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
    ) -> Result<DepositResult, AppError> {
        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::AccountNotActive(account_id.to_string()));
        }

        // Route deposit based on origin match
        let is_self = account.is_origin_source(source_ifsc, source_account_number);
        let credit_tb_id = if is_self {
            account.tb_self_account_id
        } else {
            account.tb_others_account_id
        };
        let pool = if is_self {
            "self"
        } else {
            "others"
        };

        let deposit_id = Uuid::new_v4();

        if pending {
            let timeout = timeout_seconds.unwrap_or(self.default_timeout_seconds);

            // Create pending transfer in TigerBeetle
            let tb_transfer_id = self
                .ledger_repo
                .create_pending_transfer(
                    FUNDING_SOURCE_TB_ID,
                    credit_tb_id,
                    amount,
                    PENDING_DEPOSIT_TRANSFER_CODE,
                    timeout,
                )
                .await?;

            // Record in PostgreSQL
            let record = self
                .deposit_repo
                .insert(
                    deposit_id,
                    account_id,
                    amount,
                    pool,
                    source_ifsc,
                    source_account_number,
                    DepositStatus::Pending,
                    tb_transfer_id,
                    gateway_ref,
                    Some(timeout),
                )
                .await?;

            Ok(DepositResult {
                deposit_id: record.id,
                account_id,
                amount,
                pool: if is_self { "self_contribution" } else { "others_contribution" },
                status: DepositStatus::Pending,
                gateway_ref: record.gateway_ref,
                timeout_seconds: record.timeout_seconds,
            })
        } else {
            // Immediate deposit — same as before, but now also recorded in PG
            self.ledger_repo
                .create_transfer(
                    FUNDING_SOURCE_TB_ID,
                    credit_tb_id,
                    amount,
                    DEPOSIT_TRANSFER_CODE,
                )
                .await?;

            let tb_transfer_id = 0_u128; // Immediate deposits don't need TB ID tracking
            let record = self
                .deposit_repo
                .insert(
                    deposit_id,
                    account_id,
                    amount,
                    pool,
                    source_ifsc,
                    source_account_number,
                    DepositStatus::Posted,
                    tb_transfer_id,
                    gateway_ref,
                    None,
                )
                .await?;

            Ok(DepositResult {
                deposit_id: record.id,
                account_id,
                amount,
                pool: if is_self { "self_contribution" } else { "others_contribution" },
                status: DepositStatus::Posted,
                gateway_ref: record.gateway_ref,
                timeout_seconds: None,
            })
        }
    }

    pub async fn post_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
    ) -> Result<DepositResult, AppError> {
        let deposit = self.deposit_repo.get_by_id(deposit_id, account_id).await?;

        if deposit.status != DepositStatus::Pending {
            return Err(AppError::DepositNotPending(deposit_id.to_string()));
        }

        // Post the pending transfer in TigerBeetle
        self.ledger_repo
            .post_pending_transfer(deposit.tb_transfer_id)
            .await?;

        // Update PG status
        let updated = self
            .deposit_repo
            .update_status(deposit_id, DepositStatus::Posted)
            .await?;

        tracing::info!(
            deposit_id = %deposit_id,
            account_id = %account_id,
            amount = deposit.amount,
            "Pending deposit posted"
        );

        Ok(DepositResult {
            deposit_id: updated.id,
            account_id: updated.account_id,
            amount: updated.amount,
            pool: pool_display(&updated.pool),
            status: DepositStatus::Posted,
            gateway_ref: updated.gateway_ref,
            timeout_seconds: updated.timeout_seconds,
        })
    }

    pub async fn void_deposit(
        &self,
        account_id: Uuid,
        deposit_id: Uuid,
        reason: Option<&str>,
    ) -> Result<DepositResult, AppError> {
        let deposit = self.deposit_repo.get_by_id(deposit_id, account_id).await?;

        if deposit.status != DepositStatus::Pending {
            return Err(AppError::DepositNotPending(deposit_id.to_string()));
        }

        // Void the pending transfer in TigerBeetle
        self.ledger_repo
            .void_pending_transfer(deposit.tb_transfer_id)
            .await?;

        // Update PG status
        let updated = self
            .deposit_repo
            .update_status(deposit_id, DepositStatus::Voided)
            .await?;

        tracing::info!(
            deposit_id = %deposit_id,
            account_id = %account_id,
            amount = deposit.amount,
            reason = reason.unwrap_or("none"),
            "Pending deposit voided"
        );

        Ok(DepositResult {
            deposit_id: updated.id,
            account_id: updated.account_id,
            amount: updated.amount,
            pool: pool_display(&updated.pool),
            status: DepositStatus::Voided,
            gateway_ref: updated.gateway_ref,
            timeout_seconds: updated.timeout_seconds,
        })
    }
}

fn pool_display(pool: &str) -> &'static str {
    match pool {
        "self" => "self_contribution",
        "others" => "others_contribution",
        _ => "unknown",
    }
}

pub struct DepositResult {
    pub deposit_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub pool: &'static str,
    pub status: DepositStatus,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
}
