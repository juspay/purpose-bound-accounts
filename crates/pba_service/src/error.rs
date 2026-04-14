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
    AccountNotFound(String),
    AccountNotActive(String),
    PurposeTypeNotFound(String),
    InsufficientFunds { requested: u64, available: u64 },
    InvalidMcc { mcc: String, purpose_code: String },
    DuplicateAccount(String),
    DepositNotFound(String),
    DepositNotPending(String),
    /// Transfer rejected by TigerBeetle because debit would exceed credits (overdraft).
    /// This is retryable with a fresh balance read.
    ExceedsBalance,
    TigerBeetleError(String),
    DatabaseError(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountNotFound(id) => write!(f, "Account not found: {id}"),
            Self::AccountNotActive(id) => write!(f, "Account not active: {id}"),
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
            Self::DuplicateAccount(msg) => write!(f, "Duplicate account: {msg}"),
            Self::DepositNotFound(id) => write!(f, "Deposit not found: {id}"),
            Self::DepositNotPending(id) => write!(f, "Deposit is not in pending state: {id}"),
            Self::ExceedsBalance => write!(f, "Transfer exceeds available balance"),
            Self::TigerBeetleError(msg) => write!(f, "TigerBeetle error: {msg}"),
            Self::DatabaseError(msg) => write!(f, "Database error: {msg}"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            AppError::AccountNotFound(_) => (StatusCode::NOT_FOUND, "AccountNotFound"),
            AppError::AccountNotActive(_) => (StatusCode::CONFLICT, "AccountNotActive"),
            AppError::PurposeTypeNotFound(_) => (StatusCode::NOT_FOUND, "PurposeTypeNotFound"),
            AppError::InsufficientFunds { .. } => {
                (StatusCode::UNPROCESSABLE_ENTITY, "InsufficientFunds")
            }
            AppError::InvalidMcc { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "InvalidMcc"),
            AppError::DuplicateAccount(_) => (StatusCode::CONFLICT, "DuplicateAccount"),
            AppError::DepositNotFound(_) => (StatusCode::NOT_FOUND, "DepositNotFound"),
            AppError::DepositNotPending(_) => (StatusCode::CONFLICT, "DepositNotPending"),
            AppError::ExceedsBalance => {
                (StatusCode::UNPROCESSABLE_ENTITY, "InsufficientFunds")
            }
            AppError::TigerBeetleError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "TigerBeetleError")
            }
            AppError::DatabaseError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DatabaseError"),
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
