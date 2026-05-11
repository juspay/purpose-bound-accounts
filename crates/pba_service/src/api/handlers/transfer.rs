use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::api::dto::{TransferResponse, TransferToPBAccountRequest};
use crate::error::AppError;
use crate::AppState;

pub async fn initiate_transfer(
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
    Json(req): Json<TransferToPBAccountRequest>,
) -> Result<(StatusCode, Json<TransferResponse>), AppError> {
    if req.amount == 0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    if !req.pending && req.timeout_seconds.is_some() {
        return Err(AppError::Validation(
            "timeout_seconds is only valid when pending=true".into(),
        ));
    }
    if let Some(d) = req.description.as_deref() {
        if d.len() > 256 {
            return Err(AppError::Validation(
                "description must be \u{2264} 256 chars".into(),
            ));
        }
    }

    let result = state
        .transfer_service
        .transfer(
            source_id,
            req.destination_pb_account_id,
            req.amount,
            req.pending,
            req.gateway_ref.as_deref(),
            req.timeout_seconds,
            req.description.as_deref(),
            req.idempotency_key.as_deref(),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(result.into())))
}

pub async fn post_transfer(
    State(state): State<AppState>,
    Path((source_id, transfer_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TransferResponse>, AppError> {
    let result = state
        .transfer_service
        .post_transfer(source_id, transfer_id)
        .await?;
    Ok(Json(result.into()))
}

pub async fn void_transfer(
    State(state): State<AppState>,
    Path((source_id, transfer_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<TransferResponse>, AppError> {
    let result = state
        .transfer_service
        .void_transfer(source_id, transfer_id)
        .await?;
    Ok(Json(result.into()))
}
