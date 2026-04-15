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
    let status = AccountStatus::from_str(&req.status)
        .ok_or_else(|| AppError::DatabaseError(format!("Invalid status: {}", req.status)))?;
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
        pending_self: balance.pending_self,
        pending_others: balance.pending_others,
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
            req.pending,
            req.gateway_ref.as_deref(),
            req.timeout_seconds,
        )
        .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(DepositResponse {
            deposit_id: result.deposit_id,
            account_id: result.account_id,
            amount: result.amount,
            pool: result.pool.to_string(),
            status: result.status.as_str().to_string(),
            gateway_ref: result.gateway_ref,
            timeout_seconds: result.timeout_seconds,
        }),
    ))
}

pub async fn post_deposit(
    State(state): State<AppState>,
    Path((account_id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DepositResponse>, AppError> {
    let result = state
        .deposit_service
        .post_deposit(account_id, deposit_id)
        .await?;

    Ok(Json(DepositResponse {
        deposit_id: result.deposit_id,
        account_id: result.account_id,
        amount: result.amount,
        pool: result.pool.to_string(),
        status: result.status.as_str().to_string(),
        gateway_ref: result.gateway_ref,
        timeout_seconds: result.timeout_seconds,
    }))
}

pub async fn void_deposit(
    State(state): State<AppState>,
    Path((account_id, deposit_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<VoidDepositRequest>,
) -> Result<Json<DepositResponse>, AppError> {
    let result = state
        .deposit_service
        .void_deposit(account_id, deposit_id, req.reason.as_deref())
        .await?;

    Ok(Json(DepositResponse {
        deposit_id: result.deposit_id,
        account_id: result.account_id,
        amount: result.amount,
        pool: result.pool.to_string(),
        status: result.status.as_str().to_string(),
        gateway_ref: result.gateway_ref,
        timeout_seconds: result.timeout_seconds,
    }))
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
) -> Result<Json<ListPurposeTypesResponse>, AppError> {
    let types = state.account_repo.list_purpose_types().await?;
    Ok(Json(ListPurposeTypesResponse {
        purpose_types: types.into_iter().map(|t| t.into()).collect(),
    }))
}

pub async fn get_purpose_type(
    State(state): State<AppState>,
    Path(purpose_code): Path<String>,
) -> Result<Json<PurposeTypeResponse>, AppError> {
    let purpose = state.account_repo.get_purpose_type(&purpose_code).await?;
    Ok(Json(purpose.into()))
}

// ── API Docs ──

const OPENAPI_SPEC: &str = include_str!("openapi.json");

pub async fn openapi_json() -> (axum::http::StatusCode, [(&'static str, &'static str); 1], &'static str) {
    (
        axum::http::StatusCode::OK,
        [("content-type", "application/json")],
        OPENAPI_SPEC,
    )
}

pub async fn swagger_ui() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>PBA Service — API Docs</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
        SwaggerUIBundle({
            url: "/docs/openapi.json",
            dom_id: "#swagger-ui",
            deepLinking: true,
            presets: [SwaggerUIBundle.presets.apis],
            layout: "BaseLayout"
        });
    </script>
</body>
</html>"##,
    )
}
