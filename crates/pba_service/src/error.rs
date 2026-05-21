use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
    message: String,
}

#[derive(Debug)]
pub enum AppError {
    PbAccountNotFound(String),
    PbAccountNotActive(String),
    #[allow(dead_code)]
    NormalAccountNotFound(String),
    #[allow(dead_code)]
    NormalAccountNotActive(String),
    PurposeTypeNotFound(String),
    InsufficientFunds {
        requested: u64,
        available: u64,
    },
    InvalidMcc {
        mcc: String,
        purpose_code: String,
    },
    TransactionNotFound(String),
    TransactionNotPending(String),
    FundingTypeRequired,
    TrustDepositRequiresTransfer,
    /// The transfer cannot be reversed in its current state.
    /// `reason` is one of: "not_posted", "is_itself_a_reversal", "wrong_type".
    TransferNotReversible(String, String),
    /// A reversal already exists for this original transfer.
    TransferAlreadyReversed(String),
    /// Requested reversal amount is zero or exceeds the original transfer amount.
    ReversalAmountInvalid { requested: u64, original: u64 },
    /// Transfer rejected by TigerBeetle because debit would exceed credits (overdraft).
    /// This is retryable with a fresh balance read.
    ExceedsBalance,
    TigerBeetleError(String),
    DatabaseError(String),
    Unauthorized(String),
    Forbidden(String),
    Validation(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PbAccountNotFound(id) => write!(f, "PB account not found: {id}"),
            Self::PbAccountNotActive(id) => write!(f, "PB account not active: {id}"),
            Self::NormalAccountNotFound(id) => write!(f, "Normal account not found: {id}"),
            Self::NormalAccountNotActive(id) => write!(f, "Normal account not active: {id}"),
            Self::PurposeTypeNotFound(code) => {
                write!(f, "Purpose type not found: {code}")
            }
            Self::InsufficientFunds {
                requested,
                available,
            } => write!(
                f,
                "Insufficient funds: requested {requested}, available {available}"
            ),
            Self::InvalidMcc { mcc, purpose_code } => {
                write!(f, "MCC {mcc} not allowed for purpose {purpose_code}")
            }
            Self::TransactionNotFound(id) => write!(f, "Transaction not found: {id}"),
            Self::TransactionNotPending(id) => {
                write!(f, "Transaction is not in pending state: {id}")
            }
            Self::FundingTypeRequired => write!(f, "funding_type is required for non-origin deposits (must be 'trust' or 'third_party')"),
            Self::TrustDepositRequiresTransfer => write!(
                f,
                "Trust-funded deposits to PB accounts have been removed. Use POST /normal-accounts/{{id}}/transfers instead."
            ),
            Self::TransferNotReversible(id, reason) => {
                write!(f, "Transfer {id} cannot be reversed: {reason}")
            }
            Self::TransferAlreadyReversed(id) => {
                write!(f, "Transfer {id} has already been reversed")
            }
            Self::ReversalAmountInvalid { requested, original } => write!(
                f,
                "Reversal amount {requested} is invalid for original transfer of {original}"
            ),
            Self::ExceedsBalance => write!(f, "Transfer exceeds available balance"),
            Self::TigerBeetleError(msg) => write!(f, "TigerBeetle error: {msg}"),
            Self::DatabaseError(msg) => write!(f, "Database error: {msg}"),
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {msg}"),
            Self::Forbidden(msg) => write!(f, "Forbidden: {msg}"),
            Self::Validation(msg) => write!(f, "{msg}"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            AppError::PbAccountNotFound(_) => (StatusCode::NOT_FOUND, "PbAccountNotFound"),
            AppError::PbAccountNotActive(_) => (StatusCode::CONFLICT, "PbAccountNotActive"),
            AppError::NormalAccountNotFound(_) => (StatusCode::NOT_FOUND, "NormalAccountNotFound"),
            AppError::NormalAccountNotActive(_) => (StatusCode::CONFLICT, "NormalAccountNotActive"),
            AppError::PurposeTypeNotFound(_) => (StatusCode::NOT_FOUND, "PurposeTypeNotFound"),
            AppError::InsufficientFunds { .. } => {
                (StatusCode::UNPROCESSABLE_ENTITY, "InsufficientFunds")
            }
            AppError::InvalidMcc { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "InvalidMcc"),
            AppError::TransactionNotFound(_) => (StatusCode::NOT_FOUND, "TransactionNotFound"),
            AppError::TransactionNotPending(_) => (StatusCode::CONFLICT, "TransactionNotPending"),
            AppError::FundingTypeRequired => (StatusCode::BAD_REQUEST, "FundingTypeRequired"),
            AppError::TrustDepositRequiresTransfer => {
                (StatusCode::BAD_REQUEST, "TrustDepositRequiresTransfer")
            }
            AppError::TransferNotReversible(_, _) => (StatusCode::CONFLICT, "TransferNotReversible"),
            AppError::TransferAlreadyReversed(_) => (StatusCode::CONFLICT, "TransferAlreadyReversed"),
            AppError::ReversalAmountInvalid { .. } => (StatusCode::BAD_REQUEST, "ReversalAmountInvalid"),
            AppError::ExceedsBalance => (StatusCode::UNPROCESSABLE_ENTITY, "InsufficientFunds"),
            AppError::TigerBeetleError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "TigerBeetleError")
            }
            AppError::DatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DatabaseError"),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "Forbidden"),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, "ValidationError"),
        };

        let body = ErrorBody {
            error: error_type.to_string(),
            message: self.to_string(),
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::DatabaseError(err.to_string())
    }
}

impl From<crate::domain::banking::BankingValidationError> for AppError {
    fn from(err: crate::domain::banking::BankingValidationError) -> Self {
        AppError::Validation(err.to_string())
    }
}

#[cfg(test)]
mod reversal_error_tests {
    use super::*;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let (_parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn transfer_not_reversible_returns_409_with_pascal_case_kind() {
        let err = AppError::TransferNotReversible("abc".into(), "not_posted".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "TransferNotReversible");
        assert!(body["message"].as_str().unwrap().contains("not_posted"));
    }

    #[tokio::test]
    async fn transfer_already_reversed_returns_409() {
        let err = AppError::TransferAlreadyReversed("xyz".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "TransferAlreadyReversed");
    }

    #[tokio::test]
    async fn reversal_amount_invalid_returns_400_with_amounts_in_message() {
        let err = AppError::ReversalAmountInvalid {
            requested: 1500,
            original: 1000,
        };
        let resp = err.into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["error"], "ReversalAmountInvalid");
        let msg = body["message"].as_str().unwrap();
        assert!(msg.contains("1500"));
        assert!(msg.contains("1000"));
    }
}
