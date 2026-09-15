use std::sync::Arc;
use uuid::Uuid;

use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;
use crate::repository::ledger_repo::{
    LedgerRepo, SELF_FUNDING_SOURCE_TB_ID, THIRD_PARTY_FUNDING_SOURCE_TB_ID,
    TRUST_FUNDING_SOURCE_TB_ID,
};
use crate::repository::pb_account_repo::PbAccountRepo;
use crate::repository::transaction_repo::TransactionRepo;

const DEPOSIT_TRANSFER_CODE: u16 = 100;
const PENDING_DEPOSIT_TRANSFER_CODE: u16 = 101;

pub struct PbDepositService {
    pub account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
    /// When enabled, an explicit `funding_type = "self"` is honored (skipping
    /// origin matching). When disabled, `self` is rejected as an input.
    pub optional_origin_enabled: bool,
}

impl PbDepositService {
    pub fn new(
        account_repo: Arc<PbAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
        optional_origin_enabled: bool,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
            optional_origin_enabled,
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
                .find_by_idempotency_key(
                    crate::domain::account_kind::AccountKind::Pb,
                    account_id,
                    key,
                )
                .await?
            {
                return Ok(existing);
            }
        }

        if funding_type == Some("trust") {
            return Err(AppError::TrustDepositRequiresTransfer);
        }

        let account = self.account_repo.get_account(account_id).await?;

        if !account.status.is_active() {
            return Err(AppError::PbAccountNotActive(account_id.to_string()));
        }

        let is_origin_match = account.is_origin_source(source_ifsc, source_account_number);
        let (pool, resolved_funding_type, debit_sentinel) =
            resolve_funding(self.optional_origin_enabled, is_origin_match, funding_type)?;
        let is_self = pool == "self";

        let credit_tb_id = if is_self {
            account.tb_self_account_id
        } else {
            account.tb_others_account_id
        };
        let deposit_id = Uuid::now_v7();

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
                    crate::domain::account_kind::AccountKind::Pb,
                    TransactionType::Deposit,
                    TransactionStatus::Pending,
                    amount,
                    Some(pool),
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
                    None,
                    None,
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
                    crate::domain::account_kind::AccountKind::Pb,
                    TransactionType::Deposit,
                    TransactionStatus::Posted,
                    amount,
                    Some(pool),
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
                    None,
                    None,
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

/// Resolves the `(pool, funding_type, debit sentinel)` for a deposit.
///
/// An explicit `funding_type = "self"` is honored (bypassing origin matching)
/// only when `optional_origin_enabled` is on; otherwise it falls through to the
/// `FundingTypeRequired` error. A source that matches the account origin is
/// always classified as self, preserving the legacy behavior.
fn resolve_funding(
    optional_origin_enabled: bool,
    is_origin_match: bool,
    funding_type: Option<&str>,
) -> Result<(&'static str, &'static str, u128), AppError> {
    let explicit_self = optional_origin_enabled && funding_type == Some("self");
    if explicit_self || is_origin_match {
        return Ok(("self", "self", SELF_FUNDING_SOURCE_TB_ID));
    }
    match funding_type {
        Some("trust") => Ok(("others", "trust", TRUST_FUNDING_SOURCE_TB_ID)),
        Some("third_party") => Ok(("others", "third_party", THIRD_PARTY_FUNDING_SOURCE_TB_ID)),
        _ => Err(AppError::FundingTypeRequired),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_match_is_always_self() {
        // Regardless of the flag or funding_type, an origin match => self pool.
        for flag in [false, true] {
            let (pool, ft, sentinel) = resolve_funding(flag, true, None).unwrap();
            assert_eq!(
                (pool, ft, sentinel),
                ("self", "self", SELF_FUNDING_SOURCE_TB_ID)
            );
        }
    }

    #[test]
    fn explicit_self_honored_only_when_flag_on() {
        // Flag on: explicit self is trusted even without an origin match.
        let (pool, ft, sentinel) = resolve_funding(true, false, Some("self")).unwrap();
        assert_eq!(
            (pool, ft, sentinel),
            ("self", "self", SELF_FUNDING_SOURCE_TB_ID)
        );

        // Flag off: explicit self is rejected (no origin match, no valid type).
        assert!(matches!(
            resolve_funding(false, false, Some("self")),
            Err(AppError::FundingTypeRequired)
        ));
    }

    #[test]
    fn third_party_goes_to_others() {
        for flag in [false, true] {
            let (pool, ft, sentinel) = resolve_funding(flag, false, Some("third_party")).unwrap();
            assert_eq!(
                (pool, ft, sentinel),
                ("others", "third_party", THIRD_PARTY_FUNDING_SOURCE_TB_ID)
            );
        }
    }

    #[test]
    fn missing_funding_type_without_origin_is_rejected() {
        assert!(matches!(
            resolve_funding(true, false, None),
            Err(AppError::FundingTypeRequired)
        ));
    }
}
