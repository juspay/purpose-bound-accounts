use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── CreateAccount ──

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub holder_id: String,
    pub purpose_code: String,
    pub origin_ifsc: String,
    pub origin_account_number: String,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub id: Uuid,
    pub holder_id: String,
    pub purpose_code: String,
    pub origin_ifsc: String,
    pub origin_account_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_account_number: Option<String>,
    pub kyc_tier: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::domain::account::PurposeBoundAccount> for AccountResponse {
    fn from(a: crate::domain::account::PurposeBoundAccount) -> Self {
        Self {
            id: a.id,
            holder_id: a.holder_id,
            purpose_code: a.purpose_code,
            origin_ifsc: a.origin_ifsc.to_string(),
            origin_account_number: a.origin_account_number.to_string(),
            vpa: a.vpa,
            virtual_ifsc: a.virtual_ifsc.map(|v| v.to_string()),
            virtual_account_number: a.virtual_account_number.map(|v| v.to_string()),
            kyc_tier: a.kyc_tier,
            status: a.status.as_str().to_string(),
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

// ── Balance ──

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub account_id: Uuid,
    pub self_contribution: u64,
    pub others_contribution: u64,
    pub total: u64,
    pub pending_self: u64,
    pub pending_others: u64,
}

// ── Deposit ──

#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    pub source_ifsc: String,
    pub source_account_number: String,
    pub funding_type: Option<String>,
    pub amount: u64,
    #[serde(default)]
    pub pending: bool,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DepositResponse {
    pub deposit_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    pub funding_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct VoidDepositRequest {
    pub reason: Option<String>,
}

// ── Payment ──

#[derive(Debug, Deserialize)]
pub struct PaymentRequest {
    pub amount: u64,
    pub merchant_mcc: String,
    pub merchant_id: String,
    pub description: String,
    pub idempotency_key: Option<String>,
    pub gateway_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    pub payment_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub from_others: u64,
    pub from_self: u64,
    pub merchant_id: String,
    pub merchant_mcc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
}

// ── Withdrawal ──

#[derive(Debug, Deserialize)]
pub struct WithdrawalRequest {
    pub amount: u64,
    pub idempotency_key: Option<String>,
    pub gateway_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WithdrawalResponse {
    pub account_id: Uuid,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
}

// ── Status Update ──

#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

// ── Purpose Types ──

#[derive(Debug, Serialize)]
pub struct ListPurposeTypesResponse {
    pub purpose_types: Vec<PurposeTypeResponse>,
}

#[derive(Debug, Serialize)]
pub struct PurposeTypeResponse {
    pub purpose_code: String,
    pub allowed_mccs: Vec<MccEntryResponse>,
}

#[derive(Debug, Serialize)]
pub struct MccEntryResponse {
    pub mcc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<crate::domain::purpose::PurposeType> for PurposeTypeResponse {
    fn from(p: crate::domain::purpose::PurposeType) -> Self {
        Self {
            purpose_code: p.purpose_code,
            allowed_mccs: p
                .allowed_mccs
                .into_iter()
                .map(|m| MccEntryResponse {
                    mcc: m.mcc,
                    description: m.description,
                })
                .collect(),
        }
    }
}

// ── Transactions ──

#[derive(Debug, Deserialize)]
pub struct ListTransactionsQuery {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    pub from_date: Option<chrono::DateTime<chrono::Utc>>,
    pub to_date: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ListTransactionsResponse {
    pub transactions: Vec<TransactionSummaryDto>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct TransactionSummaryDto {
    pub id: Uuid,
    pub account_id: Uuid,
    pub account_kind: String,
    #[serde(rename = "type")]
    pub transaction_type: String,
    pub status: String,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    pub direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merchant_mcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funding_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_transaction_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::domain::transaction::TransactionRecord> for TransactionSummaryDto {
    fn from(t: crate::domain::transaction::TransactionRecord) -> Self {
        Self {
            id: t.id,
            account_id: t.account_id,
            account_kind: t.account_kind.as_str().to_string(),
            transaction_type: t.transaction_type.as_str().to_string(),
            status: t.status.as_str().to_string(),
            amount: t.amount,
            pool: t.pool,
            direction: t.direction.as_str().to_string(),
            description: t.description,
            merchant_id: t.merchant_id,
            merchant_mcc: t.merchant_mcc,
            source_ifsc: t.source_ifsc,
            source_account: t.source_account,
            gateway_ref: t.gateway_ref,
            funding_type: t.funding_type,
            correlation_id: t.correlation_id,
            reverses_transaction_id: t.reverses_transaction_id,
            created_at: t.created_at,
        }
    }
}

// ── Normal Account ──

#[derive(Debug, Deserialize)]
pub struct CreateNormalAccountRequest {
    pub holder_id: String,
    pub origin_ifsc: Option<String>,
    pub origin_account_number: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NormalAccountResponse {
    pub id: Uuid,
    pub holder_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_account_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_ifsc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_account_number: Option<String>,
    pub kyc_tier: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::domain::normal_account::NormalAccount> for NormalAccountResponse {
    fn from(a: crate::domain::normal_account::NormalAccount) -> Self {
        Self {
            id: a.id,
            holder_id: a.holder_id,
            origin_ifsc: a.origin_ifsc.map(|v| v.to_string()),
            origin_account_number: a.origin_account_number.map(|v| v.to_string()),
            vpa: a.vpa,
            virtual_ifsc: a.virtual_ifsc.map(|v| v.to_string()),
            virtual_account_number: a.virtual_account_number.map(|v| v.to_string()),
            kyc_tier: a.kyc_tier,
            status: a.status.as_str().to_string(),
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NormalAccountBalanceResponse {
    pub account_id: Uuid,
    pub balance: u64,
    pub pending: u64,
}

#[derive(Debug, Deserialize)]
pub struct DepositToNormalAccountRequest {
    pub amount: u64,
    #[serde(default)]
    pub pending: bool,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NormalDepositResponse {
    pub deposit_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct WithdrawFromNormalAccountRequest {
    pub amount: u64,
    pub idempotency_key: Option<String>,
    pub gateway_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NormalWithdrawalResponse {
    pub account_id: Uuid,
    pub amount: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway_ref: Option<String>,
}

// ── Transfer ──

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct TransferToPBAccountRequest {
    pub destination_pb_account_id: Uuid,
    pub amount: u64,
    #[serde(default)]
    pub pending: bool,
    pub gateway_ref: Option<String>,
    pub timeout_seconds: Option<u32>,
    pub description: Option<String>,
    pub idempotency_key: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct TransferResponse {
    pub transfer_id: Uuid,
    pub source_account_id: Uuid,
    pub destination_account_id: Uuid,
    pub amount: u64,
    pub status: String,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::transfer_service::TransferResult> for TransferResponse {
    fn from(r: crate::service::transfer_service::TransferResult) -> Self {
        Self {
            transfer_id: r.source_txn_id,
            source_account_id: r.source_account_id,
            destination_account_id: r.destination_account_id,
            amount: r.amount,
            status: r.status.as_str().to_string(),
            correlation_id: r.correlation_id,
            created_at: r.created_at,
        }
    }
}

// ── Transfer Reversal ──

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ReverseTransferRequest {
    pub amount: u64,
    pub gateway_ref: Option<String>,
    pub description: Option<String>,
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub timeout_seconds: Option<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct ReversalResponse {
    pub reversal_id: Uuid,
    pub original_transfer_id: Uuid,
    pub source_account_id: Uuid,
    pub destination_account_id: Uuid,
    pub amount: u64,
    pub original_amount: u64,
    pub status: String,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::transfer_service::ReversalResult> for ReversalResponse {
    fn from(r: crate::service::transfer_service::ReversalResult) -> Self {
        Self {
            reversal_id: r.reversal_id,
            original_transfer_id: r.original_transfer_id,
            source_account_id: r.source_account_id,
            destination_account_id: r.destination_account_id,
            amount: r.amount,
            original_amount: r.original_amount,
            status: r.status.as_str().to_string(),
            correlation_id: r.correlation_id,
            created_at: r.created_at,
        }
    }
}

// ── Payment Refund ──

#[derive(Debug, Deserialize)]
pub struct RefundPaymentRequest {
    pub amount: u64,
    pub description: Option<String>,
    pub gateway_ref: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RefundResponse {
    pub refund_id: Uuid,
    pub original_payment_id: Uuid,
    pub account_id: Uuid,
    pub amount: u64,
    pub amount_to_self: u64,
    pub amount_to_others: u64,
    pub original_amount: u64,
    pub remaining_refundable: u64,
    pub status: String,
    pub correlation_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::service::pb_payment_service::RefundResult> for RefundResponse {
    fn from(r: crate::service::pb_payment_service::RefundResult) -> Self {
        Self {
            refund_id: r.refund_id,
            original_payment_id: r.original_payment_id,
            account_id: r.account_id,
            amount: r.amount,
            amount_to_self: r.amount_to_self,
            amount_to_others: r.amount_to_others,
            original_amount: r.original_amount,
            remaining_refundable: r.remaining_refundable,
            status: r.status.as_str().to_string(),
            correlation_id: r.correlation_id,
            created_at: r.created_at,
        }
    }
}
