use axum::routing::{get, patch, post};
use axum::Router;

use crate::api::handlers;
use crate::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        // Account operations
        .route("/accounts", post(handlers::create_account))
        .route("/accounts/{accountId}", get(handlers::get_account))
        .route(
            "/accounts/{accountId}/status",
            patch(handlers::update_account_status),
        )
        // Balance
        .route(
            "/accounts/{accountId}/balance",
            get(handlers::get_balance),
        )
        // Deposit
        .route(
            "/accounts/{accountId}/deposits",
            post(handlers::deposit),
        )
        // Payment
        .route(
            "/accounts/{accountId}/payments",
            post(handlers::make_payment),
        )
        // Withdrawal
        .route(
            "/accounts/{accountId}/withdrawals",
            post(handlers::withdraw),
        )
        // Purpose types
        .route("/purpose-types", get(handlers::list_purpose_types))
        .route(
            "/purpose-types/{purposeCode}",
            get(handlers::get_purpose_type),
        )
}
