use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Deposit,
    Payment,
    Withdrawal,
    Transfer,
}

impl TransactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deposit => "deposit",
            Self::Payment => "payment",
            Self::Withdrawal => "withdrawal",
            Self::Transfer => "transfer",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "deposit" => Some(Self::Deposit),
            "payment" => Some(Self::Payment),
            "withdrawal" => Some(Self::Withdrawal),
            "transfer" => Some(Self::Transfer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Posted,
    Voided,
    Settled,
}

impl TransactionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Posted => "posted",
            Self::Voided => "voided",
            Self::Settled => "settled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "posted" => Some(Self::Posted),
            "voided" => Some(Self::Voided),
            "settled" => Some(Self::Settled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionDirection {
    Inbound,
    Outbound,
}

impl TransactionDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "inbound" => Some(Self::Inbound),
            "outbound" => Some(Self::Outbound),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Inbound => "Inbound",
            Self::Outbound => "Outbound",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub account_kind: crate::domain::account_kind::AccountKind,
    pub transaction_type: TransactionType,
    pub status: TransactionStatus,
    pub amount: u64,
    pub pool: Option<String>,
    pub direction: TransactionDirection,
    pub source_ifsc: Option<String>,
    pub source_account: Option<String>,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub merchant_id: Option<String>,
    pub merchant_mcc: Option<String>,
    pub description: Option<String>,
    pub funding_type: Option<String>,
    pub tb_transfer_id: u128,
    pub idempotency_key: Option<String>,
    pub correlation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TransactionRecord {
    pub fn amount_display(&self) -> String {
        format!("{}.{:02}", self.amount / 100, self.amount % 100)
    }

    pub fn type_label(&self) -> &'static str {
        match (self.transaction_type, self.status) {
            (TransactionType::Deposit, TransactionStatus::Pending) => "Deposit (Pending)",
            (TransactionType::Deposit, TransactionStatus::Posted) => "Deposit",
            (TransactionType::Deposit, TransactionStatus::Voided) => "Deposit (Voided)",
            (TransactionType::Payment, _) => "Payment",
            (TransactionType::Withdrawal, _) => "Withdrawal",
            (TransactionType::Transfer, TransactionStatus::Pending) => "Transfer (Pending)",
            (TransactionType::Transfer, TransactionStatus::Posted)
            | (TransactionType::Transfer, TransactionStatus::Settled) => "Transfer",
            (TransactionType::Transfer, TransactionStatus::Voided) => "Transfer (Voided)",
            _ => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_round_trips() {
        assert_eq!(TransactionType::Transfer.as_str(), "transfer");
        assert_eq!(
            TransactionType::from_str("transfer"),
            Some(TransactionType::Transfer)
        );
    }
}
