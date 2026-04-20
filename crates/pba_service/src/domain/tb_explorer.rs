use chrono::{DateTime, Utc};
use tigerbeetle_unofficial as tb;
use tigerbeetle_unofficial::account::Flags as AccountFlags;
use tigerbeetle_unofficial::transfer::Flags as TransferFlags;
use uuid::Uuid;

use crate::repository::ledger_repo::{
    FUNDING_SOURCE_TB_ID, MERCHANT_SETTLEMENT_TB_ID, WITHDRAWAL_SETTLEMENT_TB_ID,
};

/// Account code constants as defined in LedgerRepo.
const CODE_SELF_POOL: u16 = 1;
const CODE_OTHERS_POOL: u16 = 2;
const CODE_SENTINEL: u16 = 99;

/// Transfer code constants.
const CODE_DEPOSIT: u16 = 100;
const CODE_DEPOSIT_PENDING: u16 = 101;
const CODE_PAYMENT: u16 = 200;
const CODE_WITHDRAWAL: u16 = 300;

pub fn account_code_label(code: u16) -> &'static str {
    match code {
        CODE_SELF_POOL => "Self pool",
        CODE_OTHERS_POOL => "Others pool",
        CODE_SENTINEL => "Sentinel",
        _ => "Unknown",
    }
}

pub fn transfer_code_label(code: u16) -> &'static str {
    match code {
        CODE_DEPOSIT => "Deposit",
        CODE_DEPOSIT_PENDING => "Deposit (pending)",
        CODE_PAYMENT => "Payment",
        CODE_WITHDRAWAL => "Withdrawal",
        _ => "Unknown",
    }
}

pub fn account_flag_labels(flags: AccountFlags) -> Vec<&'static str> {
    let mut out = Vec::new();
    if flags.contains(AccountFlags::LINKED) {
        out.push("LINKED");
    }
    if flags.contains(AccountFlags::DEBITS_MUST_NOT_EXCEED_CREDITS) {
        out.push("DEBITS_MUST_NOT_EXCEED_CREDITS");
    }
    if flags.contains(AccountFlags::CREDITS_MUST_NOT_EXCEED_DEBITS) {
        out.push("CREDITS_MUST_NOT_EXCEED_DEBITS");
    }
    if flags.contains(AccountFlags::HISTORY) {
        out.push("HISTORY");
    }
    if flags.contains(AccountFlags::IMPORTED) {
        out.push("IMPORTED");
    }
    if flags.contains(AccountFlags::CLOSED) {
        out.push("CLOSED");
    }
    out
}

pub fn transfer_flag_labels(flags: TransferFlags) -> Vec<&'static str> {
    let mut out = Vec::new();
    if flags.contains(TransferFlags::LINKED) {
        out.push("LINKED");
    }
    if flags.contains(TransferFlags::PENDING) {
        out.push("PENDING");
    }
    if flags.contains(TransferFlags::POST_PENDING_TRANSFER) {
        out.push("POST_PENDING_TRANSFER");
    }
    if flags.contains(TransferFlags::VOID_PENDING_TRANSFER) {
        out.push("VOID_PENDING_TRANSFER");
    }
    if flags.contains(TransferFlags::BALANCING_DEBIT) {
        out.push("BALANCING_DEBIT");
    }
    if flags.contains(TransferFlags::BALANCING_CREDIT) {
        out.push("BALANCING_CREDIT");
    }
    if flags.contains(TransferFlags::CLOSING_DEBIT) {
        out.push("CLOSING_DEBIT");
    }
    if flags.contains(TransferFlags::CLOSING_CREDIT) {
        out.push("CLOSING_CREDIT");
    }
    if flags.contains(TransferFlags::IMPORTED) {
        out.push("IMPORTED");
    }
    out
}

/// Best-effort UUID rendering — TB IDs are u128, but most IDs in this project come from UUIDs
/// so showing the UUID form is useful.
pub fn u128_as_uuid(id: u128) -> String {
    Uuid::from_bytes(id.to_be_bytes()).to_string()
}

/// Identify a well-known sentinel account ID.
pub fn sentinel_label(id: u128) -> Option<&'static str> {
    match id {
        FUNDING_SOURCE_TB_ID => Some("Funding source"),
        MERCHANT_SETTLEMENT_TB_ID => Some("Merchant settlement"),
        WITHDRAWAL_SETTLEMENT_TB_ID => Some("Withdrawal settlement"),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct TbAccountView {
    pub id: u128,
    pub id_str: String,
    pub id_uuid: String,
    pub id_sentinel: Option<&'static str>,
    pub ledger: u32,
    pub code: u16,
    pub code_label: &'static str,
    pub flags_bits: u16,
    pub flags_labels: Vec<&'static str>,
    pub user_data_128: u128,
    pub user_data_128_uuid: String,
    pub user_data_64: u64,
    pub user_data_32: u32,
    pub debits_pending: u128,
    pub debits_posted: u128,
    pub credits_pending: u128,
    pub credits_posted: u128,
    /// credits_posted - debits_posted (may be negative for credit-restricted accounts)
    pub balance_posted: i128,
    pub timestamp: DateTime<Utc>,
}

impl TbAccountView {
    pub fn from_account(a: &tb::Account) -> Self {
        let id = a.id();
        let credits_posted = a.credits_posted();
        let debits_posted = a.debits_posted();
        let balance_posted = credits_posted as i128 - debits_posted as i128;
        Self {
            id,
            id_str: id.to_string(),
            id_uuid: u128_as_uuid(id),
            id_sentinel: sentinel_label(id),
            ledger: a.ledger(),
            code: a.code(),
            code_label: account_code_label(a.code()),
            flags_bits: a.flags().bits(),
            flags_labels: account_flag_labels(a.flags()),
            user_data_128: a.user_data_128(),
            user_data_128_uuid: u128_as_uuid(a.user_data_128()),
            user_data_64: a.user_data_64(),
            user_data_32: a.user_data_32(),
            debits_pending: a.debits_pending(),
            debits_posted,
            credits_pending: a.credits_pending(),
            credits_posted,
            balance_posted,
            timestamp: a.timestamp().into(),
        }
    }

    pub fn balance_display(&self) -> String {
        let abs = self.balance_posted.unsigned_abs();
        let sign = if self.balance_posted < 0 { "-" } else { "" };
        format!("{sign}{}.{:02}", abs / 100, abs % 100)
    }

    pub fn amount_display(amount: u128) -> String {
        let whole = amount / 100;
        let frac = (amount % 100) as u32;
        format!("{whole}.{frac:02}")
    }
}

#[derive(Debug, Clone)]
pub struct TbTransferView {
    pub id: u128,
    pub id_str: String,
    pub id_uuid: String,
    pub debit_account_id: u128,
    pub debit_account_str: String,
    pub debit_account_sentinel: Option<&'static str>,
    pub credit_account_id: u128,
    pub credit_account_str: String,
    pub credit_account_sentinel: Option<&'static str>,
    pub amount: u128,
    pub amount_display: String,
    pub ledger: u32,
    pub code: u16,
    pub code_label: &'static str,
    pub flags_bits: u16,
    pub flags_labels: Vec<&'static str>,
    pub pending_id: u128,
    pub pending_id_str: String,
    pub timeout: u32,
    pub user_data_128: u128,
    pub user_data_64: u64,
    pub user_data_32: u32,
    pub timestamp: DateTime<Utc>,
    pub is_pending: bool,
}

impl TbTransferView {
    pub fn from_transfer(t: &tb::Transfer) -> Self {
        let id = t.id();
        let debit = t.debit_account_id();
        let credit = t.credit_account_id();
        let pending_id = t.pending_id();
        let amount = t.amount();
        let amount_display = TbAccountView::amount_display(amount);
        let flags = t.flags();
        Self {
            id,
            id_str: id.to_string(),
            id_uuid: u128_as_uuid(id),
            debit_account_id: debit,
            debit_account_str: debit.to_string(),
            debit_account_sentinel: sentinel_label(debit),
            credit_account_id: credit,
            credit_account_str: credit.to_string(),
            credit_account_sentinel: sentinel_label(credit),
            amount,
            amount_display,
            ledger: t.ledger(),
            code: t.code(),
            code_label: transfer_code_label(t.code()),
            flags_bits: flags.bits(),
            flags_labels: transfer_flag_labels(flags),
            pending_id,
            pending_id_str: if pending_id == 0 {
                String::new()
            } else {
                pending_id.to_string()
            },
            timeout: t.timeout(),
            user_data_128: t.user_data_128(),
            user_data_64: t.user_data_64(),
            user_data_32: t.user_data_32(),
            timestamp: t.timestamp().into(),
            is_pending: flags.contains(TransferFlags::PENDING),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TbBalanceView {
    pub debits_pending: u128,
    pub debits_posted: u128,
    pub credits_pending: u128,
    pub credits_posted: u128,
    pub balance_posted: i128,
    pub balance_display: String,
    pub timestamp: DateTime<Utc>,
}

impl TbBalanceView {
    pub fn from_balance(b: &tb::account::Balance) -> Self {
        let credits_posted = b.credits_posted();
        let debits_posted = b.debits_posted();
        let balance = credits_posted as i128 - debits_posted as i128;
        let abs = balance.unsigned_abs();
        let sign = if balance < 0 { "-" } else { "" };
        let balance_display = format!("{sign}{}.{:02}", abs / 100, abs % 100);
        Self {
            debits_pending: b.debits_pending(),
            debits_posted,
            credits_pending: b.credits_pending(),
            credits_posted,
            balance_posted: balance,
            balance_display,
            timestamp: b.timestamp().into(),
        }
    }
}
