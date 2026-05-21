use axum::extract::State;
use axum::Json;

use crate::api::dto::{ListTransactionsQuery, ListTransactionsResponse};
use crate::error::AppError;
use crate::AppState;

pub async fn list_all_transactions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListTransactionsQuery>,
) -> Result<Json<ListTransactionsResponse>, AppError> {
    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let transactions = state
        .transaction_repo
        .list_all(offset, limit, query.from_date, query.to_date, None)
        .await?;
    let total = state
        .transaction_repo
        .count_all(query.from_date, query.to_date, None)
        .await?;

    Ok(Json(ListTransactionsResponse {
        transactions: transactions.into_iter().map(|t| t.into()).collect(),
        total,
        offset,
        limit,
    }))
}
