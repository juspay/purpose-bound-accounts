use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum PoolType {
    SelfContribution,
    OthersContribution,
}

/// Represents the balance of both pools in an account.
#[derive(Debug, Clone, Serialize)]
pub struct PoolBalance {
    pub self_contribution: u64,
    pub others_contribution: u64,
    pub pending_self: u64,
    pub pending_others: u64,
}

impl PoolBalance {
    pub fn total(&self) -> u64 {
        self.self_contribution + self.others_contribution
    }
}

/// Determines the debit split across pools for a payment.
/// Others-contribution is used first, then self-contribution.
#[derive(Debug, Clone)]
pub struct PaymentSplit {
    pub from_others: u64,
    pub from_self: u64,
}

impl PaymentSplit {
    /// Calculate the payment split given pool balances and the requested amount.
    /// Returns `None` if total balance is insufficient.
    pub fn calculate(balance: &PoolBalance, amount: u64) -> Option<Self> {
        if balance.total() < amount {
            return None;
        }

        if balance.others_contribution >= amount {
            Some(Self {
                from_others: amount,
                from_self: 0,
            })
        } else {
            Some(Self {
                from_others: balance.others_contribution,
                from_self: amount - balance.others_contribution,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_split_others_sufficient() {
        let balance = PoolBalance {
            self_contribution: 1000,
            others_contribution: 5000,
            pending_self: 0,
            pending_others: 0,
        };
        let split = PaymentSplit::calculate(&balance, 3000).unwrap();
        assert_eq!(split.from_others, 3000);
        assert_eq!(split.from_self, 0);
    }

    #[test]
    fn payment_split_mixed() {
        let balance = PoolBalance {
            self_contribution: 3000,
            others_contribution: 2000,
            pending_self: 0,
            pending_others: 0,
        };
        let split = PaymentSplit::calculate(&balance, 4000).unwrap();
        assert_eq!(split.from_others, 2000);
        assert_eq!(split.from_self, 2000);
    }

    #[test]
    fn payment_split_self_only() {
        let balance = PoolBalance {
            self_contribution: 5000,
            others_contribution: 0,
            pending_self: 0,
            pending_others: 0,
        };
        let split = PaymentSplit::calculate(&balance, 3000).unwrap();
        assert_eq!(split.from_others, 0);
        assert_eq!(split.from_self, 3000);
    }

    #[test]
    fn payment_split_insufficient() {
        let balance = PoolBalance {
            self_contribution: 1000,
            others_contribution: 1000,
            pending_self: 0,
            pending_others: 0,
        };
        assert!(PaymentSplit::calculate(&balance, 3000).is_none());
    }
}
