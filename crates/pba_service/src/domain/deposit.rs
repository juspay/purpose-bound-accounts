use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepositStatus {
    Created,
    Pending,
    Posted,
    Voided,
}

impl DepositStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Pending => "pending",
            Self::Posted => "posted",
            Self::Voided => "voided",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "created" => Some(Self::Created),
            "pending" => Some(Self::Pending),
            "posted" => Some(Self::Posted),
            "voided" => Some(Self::Voided),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DepositRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub pool: String,
    pub source_ifsc: String,
    pub source_account: String,
    pub status: DepositStatus,
    pub tb_transfer_id: u128,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
