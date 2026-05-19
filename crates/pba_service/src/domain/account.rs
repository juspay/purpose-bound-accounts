use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::banking::{AccountNumber, Ifsc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    Frozen,
    Closed,
}

impl AccountStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Frozen => "frozen",
            Self::Closed => "closed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "active" => Some(Self::Active),
            "frozen" => Some(Self::Frozen),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        *self == Self::Active
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PurposeBoundAccount {
    pub id: Uuid,
    pub holder_id: String,
    pub purpose_code: String,
    pub origin_ifsc: Ifsc,
    pub origin_account_number: AccountNumber,
    pub vpa: Option<String>,
    pub virtual_ifsc: Option<Ifsc>,
    pub virtual_account_number: Option<AccountNumber>,
    pub tb_self_account_id: u128,
    pub tb_others_account_id: u128,
    pub kyc_tier: String,
    pub status: AccountStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PurposeBoundAccount {
    /// Check if a deposit source matches the origin bank details.
    pub fn is_origin_source(&self, source_ifsc: &str, source_account_number: &str) -> bool {
        self.origin_ifsc.as_str() == source_ifsc
            && self.origin_account_number.as_str() == source_account_number
    }
}

/// Deterministic u128 ID derivation from UUID for TigerBeetle accounts.
pub fn tb_self_id(account_id: Uuid) -> u128 {
    u128::from_be_bytes(*account_id.as_bytes())
}

/// Derives the others-contribution TB account ID by flipping the high bit.
pub fn tb_others_id(account_id: Uuid) -> u128 {
    let mut bytes = *account_id.as_bytes();
    bytes[0] ^= 0x80;
    u128::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tb_ids_are_distinct() {
        let id = Uuid::now_v7();
        assert_ne!(tb_self_id(id), tb_others_id(id));
    }

    #[test]
    fn tb_ids_are_deterministic() {
        let id = Uuid::now_v7();
        assert_eq!(tb_self_id(id), tb_self_id(id));
        assert_eq!(tb_others_id(id), tb_others_id(id));
    }

    #[test]
    fn origin_source_match() {
        let account = PurposeBoundAccount {
            id: Uuid::now_v7(),
            holder_id: "test-holder".to_string(),
            purpose_code: "health".to_string(),
            origin_ifsc: Ifsc::parse("HDFC0001234").unwrap(),
            origin_account_number: AccountNumber::parse("1234567890").unwrap(),
            vpa: None,
            virtual_ifsc: None,
            virtual_account_number: None,
            tb_self_account_id: 1,
            tb_others_account_id: 2,
            kyc_tier: "minimum".to_string(),
            status: AccountStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert!(account.is_origin_source("HDFC0001234", "1234567890"));
        assert!(!account.is_origin_source("ICIC0005678", "1234567890"));
        assert!(!account.is_origin_source("HDFC0001234", "9999999999"));
    }
}
