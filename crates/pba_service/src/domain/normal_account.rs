use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::account::AccountStatus;
use crate::domain::banking::{AccountNumber, Ifsc};

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct NormalAccount {
    pub id: Uuid,
    pub holder_id: String,
    pub origin_ifsc: Option<Ifsc>,
    pub origin_account_number: Option<AccountNumber>,
    pub vpa: Option<String>,
    pub virtual_ifsc: Option<Ifsc>,
    pub virtual_account_number: Option<AccountNumber>,
    pub tb_account_id: u128,
    pub kyc_tier: String,
    pub status: AccountStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Deterministic u128 ID derivation from UUID for the single TigerBeetle
/// account behind a normal account. Same byte layout as `tb_self_id`; collisions
/// across UUIDs are bounded by UUID v4 collision probability (~ 2^-122).
#[allow(dead_code)]
pub fn tb_normal_id(account_id: Uuid) -> u128 {
    u128::from_be_bytes(*account_id.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tb_normal_id_is_deterministic() {
        let id = Uuid::now_v7();
        assert_eq!(tb_normal_id(id), tb_normal_id(id));
    }

    #[test]
    fn tb_normal_id_distinguishes_uuids() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        assert_ne!(tb_normal_id(a), tb_normal_id(b));
    }
}
