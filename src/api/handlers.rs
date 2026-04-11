use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use crate::api::dto::*;
use crate::domain::account::AccountStatus;
use crate::error::AppError;
use crate::AppState;

// ── Account ──

pub async fn create_account(
    State(state): State<AppState>,
    Json(req): Json<CreateAccountRequest>,
) -> Result<(axum::http::StatusCode, Json<AccountResponse>), AppError> {
    let account = state
        .account_service
        .create_account(
            req.holder_id,
            &req.purpose_code,
            &req.origin_ifsc,
            &req.origin_account_number,
        )
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(account.into())))
}

pub async fn get_account(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Result<Json<AccountResponse>, AppError> {
    let account = state.account_service.get_account(account_id).await?;
    Ok(Json(account.into()))
}

pub async fn update_account_status(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<AccountResponse>, AppError> {
    let status = AccountStatus::from_str(&req.status).ok_or_else(|| {
        AppError::DatabaseError(format!("Invalid status: {}", req.status))
    })?;
    let account = state
        .account_service
        .update_status(account_id, status)
        .await?;
    Ok(Json(account.into()))
}

// ── Balance ──

pub async fn get_balance(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Result<Json<BalanceResponse>, AppError> {
    let account = state.account_service.get_account(account_id).await?;
    let balance = state
        .ledger_repo
        .get_balance(account.tb_self_account_id, account.tb_others_account_id)
        .await?;

    Ok(Json(BalanceResponse {
        account_id,
        self_contribution: balance.self_contribution,
        others_contribution: balance.others_contribution,
        total: balance.total(),
    }))
}

// ── Deposit ──

pub async fn deposit(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<DepositRequest>,
) -> Result<(axum::http::StatusCode, Json<DepositResponse>), AppError> {
    let result = state
        .deposit_service
        .deposit(
            account_id,
            &req.source_ifsc,
            &req.source_account_number,
            req.amount,
        )
        .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(DepositResponse {
            account_id: result.account_id,
            amount: result.amount,
            pool: result.pool.to_string(),
        }),
    ))
}

// ── Payment ──

pub async fn make_payment(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<PaymentRequest>,
) -> Result<(axum::http::StatusCode, Json<PaymentResponse>), AppError> {
    let result = state
        .payment_service
        .make_payment(
            account_id,
            req.amount,
            &req.merchant_mcc,
            &req.merchant_id,
            &req.description,
        )
        .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(PaymentResponse {
            account_id: result.account_id,
            amount: result.amount,
            from_others: result.from_others,
            from_self: result.from_self,
            merchant_id: result.merchant_id,
            merchant_mcc: result.merchant_mcc,
        }),
    ))
}

// ── Withdrawal ──

pub async fn withdraw(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<WithdrawalRequest>,
) -> Result<(axum::http::StatusCode, Json<WithdrawalResponse>), AppError> {
    let result = state
        .withdrawal_service
        .withdraw(account_id, req.amount)
        .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(WithdrawalResponse {
            account_id: result.account_id,
            amount: result.amount,
        }),
    ))
}

// ── Purpose Types ──

pub async fn list_purpose_types(
    State(state): State<AppState>,
) -> Result<Json<Vec<PurposeTypeResponse>>, AppError> {
    let types = state.account_repo.list_purpose_types().await?;
    Ok(Json(types.into_iter().map(|t| t.into()).collect()))
}

pub async fn get_purpose_type(
    State(state): State<AppState>,
    Path(purpose_code): Path<String>,
) -> Result<Json<PurposeTypeResponse>, AppError> {
    let purpose = state.account_repo.get_purpose_type(&purpose_code).await?;
    Ok(Json(purpose.into()))
}
