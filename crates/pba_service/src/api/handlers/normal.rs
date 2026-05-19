use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::api::dto::*;
use crate::domain::account::AccountStatus;
use crate::domain::account_kind::AccountKind;
use crate::domain::banking::{AccountNumber, Ifsc};
use crate::error::AppError;
use crate::AppState;

pub async fn create_normal_account(
    State(state): State<AppState>,
    Json(req): Json<CreateNormalAccountRequest>,
) -> Result<(StatusCode, Json<NormalAccountResponse>), AppError> {
    let ifsc = req
        .origin_ifsc
        .as_deref()
        .map(Ifsc::parse)
        .transpose()
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let acc_num = req
        .origin_account_number
        .as_deref()
        .map(AccountNumber::parse)
        .transpose()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let account = state
        .normal_account_service
        .create_account(&req.holder_id, ifsc.as_ref(), acc_num.as_ref())
        .await?;

    Ok((StatusCode::CREATED, Json(account.into())))
}

pub async fn get_normal_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<NormalAccountResponse>, AppError> {
    let account = state.normal_account_service.get_account(id).await?;
    Ok(Json(account.into()))
}

pub async fn list_normal_accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<NormalAccountResponse>>, AppError> {
    let accounts = state.normal_account_repo.list_accounts().await?;
    Ok(Json(accounts.into_iter().map(Into::into).collect()))
}

pub async fn update_normal_account_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<NormalAccountResponse>, AppError> {
    let status = AccountStatus::from_str(&req.status)
        .ok_or_else(|| AppError::Validation(format!("invalid status: {}", req.status)))?;
    let account = state
        .normal_account_service
        .update_status(id, status)
        .await?;
    Ok(Json(account.into()))
}

pub async fn get_normal_account_balance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<NormalAccountBalanceResponse>, AppError> {
    let account = state.normal_account_service.get_account(id).await?;
    let balance = state
        .ledger_repo
        .get_single_balance(account.tb_account_id)
        .await?;
    Ok(Json(NormalAccountBalanceResponse {
        account_id: id,
        balance: balance.posted,
        pending: balance.pending,
    }))
}

pub async fn deposit_to_normal_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<DepositToNormalAccountRequest>,
) -> Result<(StatusCode, Json<NormalDepositResponse>), AppError> {
    if req.amount == 0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    if !req.pending && req.timeout_seconds.is_some() {
        return Err(AppError::Validation(
            "timeout_seconds is only valid when pending=true".into(),
        ));
    }
    let record = state
        .normal_deposit_service
        .deposit(
            id,
            req.amount,
            req.pending,
            req.gateway_ref.as_deref(),
            req.timeout_seconds,
            req.idempotency_key.as_deref(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(NormalDepositResponse {
            deposit_id: record.id,
            account_id: id,
            amount: record.amount,
            status: record.status.as_str().to_string(),
            gateway_ref: record.gateway_ref,
            timeout_seconds: record.timeout_seconds,
        }),
    ))
}

pub async fn post_normal_account_deposit(
    State(state): State<AppState>,
    Path((id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<NormalDepositResponse>, AppError> {
    let record = state
        .normal_deposit_service
        .post_deposit(id, deposit_id)
        .await?;
    Ok(Json(NormalDepositResponse {
        deposit_id: record.id,
        account_id: id,
        amount: record.amount,
        status: record.status.as_str().to_string(),
        gateway_ref: record.gateway_ref,
        timeout_seconds: record.timeout_seconds,
    }))
}

pub async fn void_normal_account_deposit(
    State(state): State<AppState>,
    Path((id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<NormalDepositResponse>, AppError> {
    let record = state
        .normal_deposit_service
        .void_deposit(id, deposit_id)
        .await?;
    Ok(Json(NormalDepositResponse {
        deposit_id: record.id,
        account_id: id,
        amount: record.amount,
        status: record.status.as_str().to_string(),
        gateway_ref: record.gateway_ref,
        timeout_seconds: record.timeout_seconds,
    }))
}

pub async fn withdraw_from_normal_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<WithdrawFromNormalAccountRequest>,
) -> Result<(StatusCode, Json<NormalWithdrawalResponse>), AppError> {
    if req.amount == 0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    let record = state
        .normal_withdrawal_service
        .withdraw(
            id,
            req.amount,
            req.idempotency_key.as_deref(),
            req.gateway_ref.as_deref(),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(NormalWithdrawalResponse {
            account_id: id,
            amount: record.amount,
            gateway_ref: record.gateway_ref,
        }),
    ))
}

pub async fn list_normal_account_transactions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<ListTransactionsQuery>,
) -> Result<Json<ListTransactionsResponse>, AppError> {
    let offset = q.offset.unwrap_or(0);
    let limit = q.limit.unwrap_or(50);
    let txns = state
        .transaction_repo
        .list_by_account(
            AccountKind::Normal,
            id,
            offset,
            limit,
            q.from_date,
            q.to_date,
        )
        .await?;
    let total = state
        .transaction_repo
        .count_by_account(id, q.from_date, q.to_date)
        .await?;
    Ok(Json(ListTransactionsResponse {
        transactions: txns.into_iter().map(Into::into).collect(),
        total,
        offset,
        limit,
    }))
}
