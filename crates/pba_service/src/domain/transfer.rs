#![allow(dead_code)]

use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TransferLegs {
    pub source_txn_id: Uuid,
    pub destination_txn_id: Uuid,
    pub correlation_id: Uuid,
}

impl TransferLegs {
    pub fn new() -> Self {
        Self {
            source_txn_id: Uuid::now_v7(),
            destination_txn_id: Uuid::now_v7(),
            correlation_id: Uuid::now_v7(),
        }
    }
}

impl Default for TransferLegs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legs_have_distinct_ids() {
        let legs = TransferLegs::new();
        assert_ne!(legs.source_txn_id, legs.destination_txn_id);
        assert_ne!(legs.source_txn_id, legs.correlation_id);
        assert_ne!(legs.destination_txn_id, legs.correlation_id);
    }
}
