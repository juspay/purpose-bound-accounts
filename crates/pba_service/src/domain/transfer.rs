use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct TransferRecord {
    pub id: u128,
    pub debit_account_id: u128,
    pub credit_account_id: u128,
    pub amount: u64,
    pub code: u16,
    pub timestamp: DateTime<Utc>,
    pub direction: TransferDirection,
    pub transfer_type: TransferType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Inbound,
    Outbound,
}

impl TransferDirection {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Deposit,
    PendingDeposit,
    Payment,
    Withdrawal,
    Unknown,
}

impl TransferType {
    pub fn from_code(code: u16) -> Self {
        match code {
            100 => Self::Deposit,
            101 => Self::PendingDeposit,
            200 => Self::Payment,
            300 => Self::Withdrawal,
            _ => Self::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Deposit => "Deposit",
            Self::PendingDeposit => "Deposit (Pending)",
            Self::Payment => "Payment",
            Self::Withdrawal => "Withdrawal",
            Self::Unknown => "Unknown",
        }
    }
}

impl TransferRecord {
    pub fn from_tb_transfer(
        transfer: &tigerbeetle_unofficial::Transfer,
        self_tb_id: u128,
        others_tb_id: u128,
    ) -> Self {
        let is_credit = transfer.credit_account_id() == self_tb_id
            || transfer.credit_account_id() == others_tb_id;
        let direction = if is_credit {
            TransferDirection::Inbound
        } else {
            TransferDirection::Outbound
        };

        let amount = u64::try_from(transfer.amount()).unwrap_or(u64::MAX);

        let timestamp: DateTime<Utc> = transfer.timestamp().into();

        Self {
            id: transfer.id(),
            debit_account_id: transfer.debit_account_id(),
            credit_account_id: transfer.credit_account_id(),
            amount,
            code: transfer.code(),
            timestamp,
            direction,
            transfer_type: TransferType::from_code(transfer.code()),
        }
    }

    pub fn amount_display(&self) -> String {
        format!("{}.{:02}", self.amount / 100, self.amount % 100)
    }
}
