use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};

use crate::domain::pool::PoolBalance;
use crate::domain::tb_explorer::{TbAccountView, TbBalanceView, TbTransferView};
use crate::error::AppError;
use tb::account::Flags as AccountFlags;
use tb::error::CreateTransferErrorKind;
use tb::transfer::Flags as TransferFlags;
use tigerbeetle_unofficial as tb;
use uuid::Uuid;

/// Sentinel account IDs used as counterparties for deposits, payments, and withdrawals.
/// TigerBeetle disallows 0 and u128::MAX, so we use a fixed range that won't collide with UUID-derived IDs.
pub const SELF_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 10;
pub const MERCHANT_SETTLEMENT_TB_ID: u128 = u128::MAX - 11;
pub const WITHDRAWAL_SETTLEMENT_TB_ID: u128 = u128::MAX - 12;
pub const TRUST_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 13;
pub const THIRD_PARTY_FUNDING_SOURCE_TB_ID: u128 = u128::MAX - 14;

/// Ledger ID for Indian Rupee. All amounts are in paisa (1 INR = 100 paisa).
pub const LEDGER_INR_PAISA: u32 = 1;
const CODE_SELF_POOL: u16 = 1;
const CODE_OTHERS_POOL: u16 = 2;
#[allow(dead_code)]
const CODE_NORMAL_POOL: u16 = 3;
const CODE_SENTINEL: u16 = 99;

const INTERNAL_TRANSFER_CODE: u16 = 400;
const PENDING_INTERNAL_TRANSFER_CODE: u16 = 401;
pub const INTERNAL_TRANSFER_REVERSAL_CODE: u16 = 410;
pub const PAYMENT_REFUND_CODE: u16 = 210;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct SingleBalance {
    pub posted: u64,
    pub pending: u64,
}

fn generate_transfer_id() -> u128 {
    u128::from_be_bytes(*Uuid::now_v7().as_bytes())
}

pub struct LedgerRepo {
    client: tb::Client,
}

impl LedgerRepo {
    pub fn new(cluster_id: u128, addresses: Vec<String>) -> Self {
        let address = addresses.join(",");
        let client =
            tb::Client::new(cluster_id, &address).expect("Failed to connect to TigerBeetle");
        Self { client }
    }

    /// Create sentinel accounts that serve as counterparties for deposits, payments, and withdrawals.
    /// These are idempotent — TigerBeetle returns `Exists` for already-created accounts which we ignore.
    pub async fn init_sentinel_accounts(&self) -> Result<(), AppError> {
        let self_funding =
            tb::Account::new(SELF_FUNDING_SOURCE_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
                .with_flags(AccountFlags::LINKED);
        let merchant = tb::Account::new(MERCHANT_SETTLEMENT_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
        let withdrawal =
            tb::Account::new(WITHDRAWAL_SETTLEMENT_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
                .with_flags(AccountFlags::LINKED);
        let trust = tb::Account::new(TRUST_FUNDING_SOURCE_TB_ID, LEDGER_INR_PAISA, CODE_SENTINEL)
            .with_flags(AccountFlags::LINKED);
        let third_party = tb::Account::new(
            THIRD_PARTY_FUNDING_SOURCE_TB_ID,
            LEDGER_INR_PAISA,
            CODE_SENTINEL,
        );

        match self
            .client
            .create_accounts(vec![self_funding, merchant, withdrawal, trust, third_party])
            .await
        {
            Ok(_) => {
                tracing::info!(
                    "Created sentinel TB accounts (self_funding, merchant, withdrawal, trust, third_party)"
                );
            }
            Err(e) => {
                tracing::warn!("Sentinel account creation returned: {e:?} (may already exist)");
            }
        }
        Ok(())
    }

    /// Look up all sentinel accounts and return their balances.
    /// Returns a Vec of (name, credits_posted, debits_posted, credits_pending, debits_pending).
    pub async fn lookup_sentinel_accounts(
        &self,
    ) -> Result<Vec<(String, u64, u64, u64, u64)>, AppError> {
        let ids = vec![
            SELF_FUNDING_SOURCE_TB_ID,
            TRUST_FUNDING_SOURCE_TB_ID,
            THIRD_PARTY_FUNDING_SOURCE_TB_ID,
            MERCHANT_SETTLEMENT_TB_ID,
            WITHDRAWAL_SETTLEMENT_TB_ID,
        ];
        let names = [
            "Self Funding Source",
            "Trust Funding Source",
            "Third Party Funding Source",
            "Merchant Settlement",
            "Withdrawal Settlement",
        ];

        let accounts =
            self.client.lookup_accounts(ids).await.map_err(|e| {
                AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}"))
            })?;

        let mut results: Vec<(String, u64, u64, u64, u64)> =
            names.iter().map(|n| (n.to_string(), 0, 0, 0, 0)).collect();

        for account in &accounts {
            let idx = match account.id() {
                id if id == SELF_FUNDING_SOURCE_TB_ID => 0,
                id if id == TRUST_FUNDING_SOURCE_TB_ID => 1,
                id if id == THIRD_PARTY_FUNDING_SOURCE_TB_ID => 2,
                id if id == MERCHANT_SETTLEMENT_TB_ID => 3,
                id if id == WITHDRAWAL_SETTLEMENT_TB_ID => 4,
                _ => continue,
            };
            results[idx] = (
                results[idx].0.clone(),
                u64::try_from(account.credits_posted()).unwrap_or(u64::MAX),
                u64::try_from(account.debits_posted()).unwrap_or(u64::MAX),
                u64::try_from(account.credits_pending()).unwrap_or(u64::MAX),
                u64::try_from(account.debits_pending()).unwrap_or(u64::MAX),
            );
        }

        Ok(results)
    }

    pub async fn create_account_pair(
        &self,
        self_id: u128,
        others_id: u128,
    ) -> Result<(), AppError> {
        let self_account = tb::Account::new(self_id, LEDGER_INR_PAISA, CODE_SELF_POOL).with_flags(
            AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS
                | AccountFlags::HISTORY
                | AccountFlags::LINKED,
        );

        let others_account = tb::Account::new(others_id, LEDGER_INR_PAISA, CODE_OTHERS_POOL)
            .with_flags(AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS | AccountFlags::HISTORY);

        self.client
            .create_accounts(vec![self_account, others_account])
            .await
            .map_err(|e| AppError::TigerBeetleError(format!("create_accounts failed: {e:?}")))?;

        tracing::info!(self_id = %self_id, others_id = %others_id, "Created linked TB account pair");
        Ok(())
    }

    pub async fn get_balance(
        &self,
        self_id: u128,
        others_id: u128,
    ) -> Result<PoolBalance, AppError> {
        let accounts = self
            .client
            .lookup_accounts(vec![self_id, others_id])
            .await
            .map_err(|e| AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}")))?;

        let mut self_balance: u64 = 0;
        let mut others_balance: u64 = 0;
        let mut pending_self: u64 = 0;
        let mut pending_others: u64 = 0;

        for account in &accounts {
            let posted = account
                .credits_posted()
                .saturating_sub(account.debits_posted());
            let posted_u64 = u64::try_from(posted).unwrap_or(u64::MAX);
            let pending = u64::try_from(account.credits_pending()).unwrap_or(u64::MAX);

            if account.id() == self_id {
                self_balance = posted_u64;
                pending_self = pending;
            } else if account.id() == others_id {
                others_balance = posted_u64;
                pending_others = pending;
            }
        }

        Ok(PoolBalance {
            self_contribution: self_balance,
            others_contribution: others_balance,
            pending_self,
            pending_others,
        })
    }

    pub async fn create_transfer(
        &self,
        debit_account_id: u128,
        credit_account_id: u128,
        amount: u64,
        transfer_code: u16,
    ) -> Result<(), AppError> {
        let transfer = tb::Transfer::new(generate_transfer_id())
            .with_debit_account_id(debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(amount as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| classify_transfer_error(e, "create_transfer"))?;

        tracing::info!(debit = %debit_account_id, credit = %credit_account_id, amount, code = transfer_code, "Created TB transfer");
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_linked_transfers(
        &self,
        others_debit_account_id: u128,
        self_debit_account_id: u128,
        credit_account_id: u128,
        others_amount: u64,
        self_amount: u64,
        transfer_code: u16,
    ) -> Result<(), AppError> {
        let transfer1 = tb::Transfer::new(generate_transfer_id())
            .with_debit_account_id(others_debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(others_amount as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code)
            .with_flags(TransferFlags::LINKED);

        let transfer2 = tb::Transfer::new(generate_transfer_id())
            .with_debit_account_id(self_debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(self_amount as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code);

        self.client
            .create_transfers(vec![transfer1, transfer2])
            .await
            .map_err(|e| classify_transfer_error(e, "create_linked_transfers"))?;

        tracing::info!(
            others_debit = %others_debit_account_id,
            self_debit = %self_debit_account_id,
            credit = %credit_account_id,
            others_amount, self_amount, code = transfer_code,
            "Created linked TB transfer chain"
        );
        Ok(())
    }

    /// Create a pending transfer. Returns the TB transfer ID (needed for post/void).
    pub async fn create_pending_transfer(
        &self,
        debit_account_id: u128,
        credit_account_id: u128,
        amount: u64,
        transfer_code: u16,
        timeout_seconds: u32,
    ) -> Result<u128, AppError> {
        let transfer_id = generate_transfer_id();
        let transfer = tb::Transfer::new(transfer_id)
            .with_debit_account_id(debit_account_id)
            .with_credit_account_id(credit_account_id)
            .with_amount(amount as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(transfer_code)
            .with_flags(TransferFlags::PENDING)
            .with_timeout(timeout_seconds);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| classify_transfer_error(e, "create_pending_transfer"))?;

        tracing::info!(
            debit = %debit_account_id, credit = %credit_account_id,
            amount, code = transfer_code, timeout = timeout_seconds,
            transfer_id = %transfer_id,
            "Created pending TB transfer"
        );
        Ok(transfer_id)
    }

    /// Post (confirm) a pending transfer.
    pub async fn post_pending_transfer(&self, pending_id: u128) -> Result<(), AppError> {
        let transfer = tb::Transfer::new(generate_transfer_id())
            .with_pending_id(pending_id)
            .with_amount(u128::MAX)
            .with_flags(TransferFlags::POST_PENDING_TRANSFER);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| classify_pending_resolution_error(e, "post_pending_transfer"))?;

        tracing::info!(pending_id = %pending_id, "Posted pending TB transfer");
        Ok(())
    }

    /// Void (cancel) a pending transfer.
    pub async fn void_pending_transfer(&self, pending_id: u128) -> Result<(), AppError> {
        let transfer = tb::Transfer::new(generate_transfer_id())
            .with_pending_id(pending_id)
            .with_flags(TransferFlags::VOID_PENDING_TRANSFER);

        self.client
            .create_transfers(vec![transfer])
            .await
            .map_err(|e| classify_pending_resolution_error(e, "void_pending_transfer"))?;

        tracing::info!(pending_id = %pending_id, "Voided pending TB transfer");
        Ok(())
    }

    // ----- Normal account methods -----

    #[allow(dead_code)]
    pub async fn create_normal_account(&self, tb_account_id: u128) -> Result<(), AppError> {
        let account = tb::Account::new(tb_account_id, LEDGER_INR_PAISA, CODE_NORMAL_POOL)
            .with_flags(AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS | AccountFlags::HISTORY);

        self.client
            .create_accounts(vec![account])
            .await
            .map_err(|e| {
                AppError::TigerBeetleError(format!("create_normal_account failed: {e:?}"))
            })?;

        tracing::info!(tb_account_id = %tb_account_id, "Created TB normal account");
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn create_internal_transfer(
        &self,
        debit_account_id: u128,
        credit_account_id: u128,
        amount: u64,
    ) -> Result<(), AppError> {
        self.create_transfer(
            debit_account_id,
            credit_account_id,
            amount,
            INTERNAL_TRANSFER_CODE,
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn create_pending_internal_transfer(
        &self,
        debit_account_id: u128,
        credit_account_id: u128,
        amount: u64,
        timeout_seconds: u32,
    ) -> Result<u128, AppError> {
        self.create_pending_transfer(
            debit_account_id,
            credit_account_id,
            amount,
            PENDING_INTERNAL_TRANSFER_CODE,
            timeout_seconds,
        )
        .await
    }

    /// Immediate reversal of an internal transfer.
    ///
    /// Debits the PB others-pool, credits the source normal account. TigerBeetle's
    /// `DEBITS_MUST_NOT_EXCEED_CREDITS` flag on the others-pool enforces that the
    /// reversal cannot debit below zero; the caller maps `AppError::ExceedsBalance`
    /// to `AppError::InsufficientFunds` with the observed balance.
    #[allow(dead_code)]
    pub async fn create_internal_transfer_reversal(
        &self,
        debit_pb_others_tb_id: u128,
        credit_normal_tb_id: u128,
        amount: u64,
    ) -> Result<(), AppError> {
        self.create_transfer(
            debit_pb_others_tb_id,
            credit_normal_tb_id,
            amount,
            INTERNAL_TRANSFER_REVERSAL_CODE,
        )
        .await
    }

    /// Single-leg payment refund — debit MERCHANT_SETTLEMENT_TB_ID, credit one
    /// pool of the PB account. Used when only one of `take_self` / `take_others`
    /// is non-zero.
    ///
    /// The merchant sentinel has no balance constraint (only `LINKED` flag), so
    /// debiting it is unconstrained — over-amount cases are caught upstream in
    /// `pb_payment_service::refund_payment` step 4 as `RefundAmountInvalid`.
    pub async fn create_payment_refund(
        &self,
        credit_pb_pool_tb_id: u128,
        amount: u64,
    ) -> Result<(), AppError> {
        self.create_transfer(
            MERCHANT_SETTLEMENT_TB_ID,
            credit_pb_pool_tb_id,
            amount,
            PAYMENT_REFUND_CODE,
        )
        .await
    }

    /// Linked two-leg payment refund — debit MERCHANT_SETTLEMENT_TB_ID twice,
    /// credit the self-pool and the others-pool. Both transfers land atomically
    /// via TB's LINKED flag. Used when both `take_self` and `take_others` are
    /// non-zero.
    pub async fn create_payment_refund_split(
        &self,
        credit_pb_self_tb_id: u128,
        credit_pb_others_tb_id: u128,
        amount_self: u64,
        amount_others: u64,
    ) -> Result<(), AppError> {
        let transfer1 = tb::Transfer::new(generate_transfer_id())
            .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
            .with_credit_account_id(credit_pb_self_tb_id)
            .with_amount(amount_self as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(PAYMENT_REFUND_CODE)
            .with_flags(TransferFlags::LINKED);

        let transfer2 = tb::Transfer::new(generate_transfer_id())
            .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
            .with_credit_account_id(credit_pb_others_tb_id)
            .with_amount(amount_others as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(PAYMENT_REFUND_CODE);

        self.client
            .create_transfers(vec![transfer1, transfer2])
            .await
            .map_err(|e| classify_transfer_error(e, "create_payment_refund_split"))?;

        tracing::info!(
            credit_self = %credit_pb_self_tb_id,
            credit_others = %credit_pb_others_tb_id,
            amount_self, amount_others, code = PAYMENT_REFUND_CODE,
            "Created linked payment-refund TB transfers"
        );
        Ok(())
    }

    /// Pending single-leg payment refund — pending TB transfer debiting the
    /// merchant sentinel, crediting one PB pool. Returns the TB transfer ID for
    /// later post/void.
    pub async fn create_pending_payment_refund(
        &self,
        credit_pb_pool_tb_id: u128,
        amount: u64,
        timeout_seconds: u32,
    ) -> Result<u128, AppError> {
        self.create_pending_transfer(
            MERCHANT_SETTLEMENT_TB_ID,
            credit_pb_pool_tb_id,
            amount,
            PAYMENT_REFUND_CODE,
            timeout_seconds,
        )
        .await
    }

    /// Pending two-leg payment refund — two LINKED pending TB transfers debiting
    /// the merchant sentinel, crediting self-pool and others-pool. Returns
    /// (tb_transfer_id_self, tb_transfer_id_others) so the service can persist
    /// both ids on their corresponding rows.
    pub async fn create_pending_payment_refund_split(
        &self,
        credit_pb_self_tb_id: u128,
        credit_pb_others_tb_id: u128,
        amount_self: u64,
        amount_others: u64,
        timeout_seconds: u32,
    ) -> Result<(u128, u128), AppError> {
        let id_self = generate_transfer_id();
        let id_others = generate_transfer_id();

        let transfer1 = tb::Transfer::new(id_self)
            .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
            .with_credit_account_id(credit_pb_self_tb_id)
            .with_amount(amount_self as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(PAYMENT_REFUND_CODE)
            .with_flags(TransferFlags::PENDING | TransferFlags::LINKED)
            .with_timeout(timeout_seconds);

        let transfer2 = tb::Transfer::new(id_others)
            .with_debit_account_id(MERCHANT_SETTLEMENT_TB_ID)
            .with_credit_account_id(credit_pb_others_tb_id)
            .with_amount(amount_others as u128)
            .with_ledger(LEDGER_INR_PAISA)
            .with_code(PAYMENT_REFUND_CODE)
            .with_flags(TransferFlags::PENDING)
            .with_timeout(timeout_seconds);

        self.client
            .create_transfers(vec![transfer1, transfer2])
            .await
            .map_err(|e| classify_transfer_error(e, "create_pending_payment_refund_split"))?;

        tracing::info!(
            credit_self = %credit_pb_self_tb_id,
            credit_others = %credit_pb_others_tb_id,
            amount_self, amount_others, code = PAYMENT_REFUND_CODE,
            timeout = timeout_seconds,
            id_self = %id_self,
            id_others = %id_others,
            "Created pending LINKED payment-refund TB transfers"
        );
        Ok((id_self, id_others))
    }

    #[allow(dead_code)]
    pub async fn get_single_balance(&self, tb_account_id: u128) -> Result<SingleBalance, AppError> {
        let accounts = self
            .client
            .lookup_accounts(vec![tb_account_id])
            .await
            .map_err(|e| AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}")))?;

        let mut posted: u64 = 0;
        let mut pending: u64 = 0;

        for account in &accounts {
            if account.id() == tb_account_id {
                let net = account
                    .credits_posted()
                    .saturating_sub(account.debits_posted());
                posted = u64::try_from(net).unwrap_or(u64::MAX);
                pending = u64::try_from(account.credits_pending()).unwrap_or(u64::MAX);
            }
        }

        Ok(SingleBalance { posted, pending })
    }

    // ----- Explorer methods (read-only queries for /admin/tb) -----

    pub async fn explorer_lookup_accounts(
        &self,
        ids: Vec<u128>,
    ) -> Result<Vec<TbAccountView>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let accounts =
            self.client.lookup_accounts(ids).await.map_err(|e| {
                AppError::TigerBeetleError(format!("lookup_accounts failed: {e:?}"))
            })?;
        Ok(accounts.iter().map(TbAccountView::from_account).collect())
    }

    pub async fn explorer_lookup_transfers(
        &self,
        ids: Vec<u128>,
    ) -> Result<Vec<TbTransferView>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let transfers =
            self.client.lookup_transfers(ids).await.map_err(|e| {
                AppError::TigerBeetleError(format!("lookup_transfers failed: {e:?}"))
            })?;
        Ok(transfers
            .iter()
            .map(TbTransferView::from_transfer)
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn explorer_query_accounts(
        &self,
        ledger: u32,
        code: u16,
        timestamp_min: Option<DateTime<Utc>>,
        timestamp_max: Option<DateTime<Utc>>,
        limit: u32,
        reversed: bool,
    ) -> Result<Vec<TbAccountView>, AppError> {
        let limit = limit.clamp(1, 8190);
        let mut filter = tb::core::query_filter::QueryFilter::new(limit)
            .with_ledger(ledger)
            .with_code(code);
        if let Some(ts) = timestamp_min.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_min(ts);
        }
        if let Some(ts) = timestamp_max.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_max(ts);
        }
        if reversed {
            filter = filter.with_flags(tb::core::query_filter::Flags::REVERSED);
        }
        let accounts = self
            .client
            .query_accounts(Box::new(filter))
            .await
            .map_err(|e| AppError::TigerBeetleError(format!("query_accounts failed: {e:?}")))?;
        Ok(accounts.iter().map(TbAccountView::from_account).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn explorer_query_transfers(
        &self,
        ledger: u32,
        code: u16,
        timestamp_min: Option<DateTime<Utc>>,
        timestamp_max: Option<DateTime<Utc>>,
        limit: u32,
        reversed: bool,
    ) -> Result<Vec<TbTransferView>, AppError> {
        let limit = limit.clamp(1, 8190);
        let mut filter = tb::core::query_filter::QueryFilter::new(limit)
            .with_ledger(ledger)
            .with_code(code);
        if let Some(ts) = timestamp_min.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_min(ts);
        }
        if let Some(ts) = timestamp_max.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_max(ts);
        }
        if reversed {
            filter = filter.with_flags(tb::core::query_filter::Flags::REVERSED);
        }
        let transfers = self
            .client
            .query_transfers(Box::new(filter))
            .await
            .map_err(|e| AppError::TigerBeetleError(format!("query_transfers failed: {e:?}")))?;
        Ok(transfers
            .iter()
            .map(TbTransferView::from_transfer)
            .collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn explorer_account_transfers(
        &self,
        account_id: u128,
        include_debits: bool,
        include_credits: bool,
        reversed: bool,
        timestamp_min: Option<DateTime<Utc>>,
        timestamp_max: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<TbTransferView>, AppError> {
        let limit = limit.clamp(1, 8190);
        let mut flags = tb::account::FilterFlags::empty();
        if include_debits {
            flags |= tb::account::FilterFlags::DEBITS;
        }
        if include_credits {
            flags |= tb::account::FilterFlags::CREDITS;
        }
        if reversed {
            flags |= tb::account::FilterFlags::REVERSED;
        }
        let mut filter = tb::account::Filter::new(account_id, limit).with_flags(flags);
        if let Some(ts) = timestamp_min.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_min(ts);
        }
        if let Some(ts) = timestamp_max.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_max(ts);
        }
        let transfers = self
            .client
            .get_account_transfers(Box::new(filter))
            .await
            .map_err(|e| {
                AppError::TigerBeetleError(format!("get_account_transfers failed: {e:?}"))
            })?;
        Ok(transfers
            .iter()
            .map(TbTransferView::from_transfer)
            .collect())
    }

    pub async fn explorer_account_balances(
        &self,
        account_id: u128,
        reversed: bool,
        timestamp_min: Option<DateTime<Utc>>,
        timestamp_max: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<TbBalanceView>, AppError> {
        let limit = limit.clamp(1, 8190);
        let mut flags = tb::account::FilterFlags::DEBITS | tb::account::FilterFlags::CREDITS;
        if reversed {
            flags |= tb::account::FilterFlags::REVERSED;
        }
        let mut filter = tb::account::Filter::new(account_id, limit).with_flags(flags);
        if let Some(ts) = timestamp_min.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_min(ts);
        }
        if let Some(ts) = timestamp_max.and_then(datetime_to_system_time) {
            filter = filter.with_timestamp_max(ts);
        }
        let balances = self
            .client
            .get_account_balances(Box::new(filter))
            .await
            .map_err(|e| {
                AppError::TigerBeetleError(format!("get_account_balances failed: {e:?}"))
            })?;
        Ok(balances.iter().map(TbBalanceView::from_balance).collect())
    }
}

fn datetime_to_system_time(dt: DateTime<Utc>) -> Option<SystemTime> {
    let nanos: u128 = (dt.timestamp() as i128 * 1_000_000_000
        + dt.timestamp_subsec_nanos() as i128)
        .try_into()
        .ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_nanos(nanos.try_into().ok()?))
}

/// Classify a TigerBeetle error from post/void pending into an appropriate AppError.
/// Returns `TbPendingAlreadyResolved` when the pending transfer was already posted
/// or voided (safe to tolerate on idempotent retries). All other errors become the
/// generic `TigerBeetleError`.
fn classify_pending_resolution_error(
    e: tb::error::CreateTransfersError,
    context: &str,
) -> AppError {
    if let tb::error::CreateTransfersError::Api(ref api_err) = e {
        let already_resolved = api_err.as_slice().iter().any(|individual| {
            matches!(
                individual.kind(),
                CreateTransferErrorKind::PendingTransferAlreadyPosted
                    | CreateTransferErrorKind::PendingTransferAlreadyVoided
            )
        });
        if already_resolved {
            return AppError::TbPendingAlreadyResolved;
        }
    }
    AppError::TigerBeetleError(format!("{context} failed: {e:?}"))
}

/// Classify a TigerBeetle transfer error into an appropriate AppError.
/// Returns `ExceedsBalance` for overdraft rejections (retryable), generic error otherwise.
fn classify_transfer_error(e: tb::error::CreateTransfersError, context: &str) -> AppError {
    if let tb::error::CreateTransfersError::Api(ref api_err) = e {
        let is_balance_exceeded = api_err.as_slice().iter().any(|individual| {
            matches!(
                individual.kind(),
                CreateTransferErrorKind::ExceedsCredits
                    | CreateTransferErrorKind::ExceedsDebits
                    | CreateTransferErrorKind::LinkedEventFailed
            )
        });
        if is_balance_exceeded {
            return AppError::ExceedsBalance;
        }
    }
    AppError::TigerBeetleError(format!("{context} failed: {e:?}"))
}
