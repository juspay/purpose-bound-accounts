use axum::routing::{get, patch, post};
use axum::Router;

use crate::api::handlers;
use crate::AppState;

/// Public API routes — no authentication required.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/purpose-types", get(handlers::list_purpose_types))
        .route(
            "/purpose-types/{purpose_code}",
            get(handlers::get_purpose_type),
        )
        .route("/docs", get(handlers::swagger_ui))
        .route("/docs/openapi.json", get(handlers::openapi_json))
}

/// Protected API routes — require API key authentication.
pub fn protected_router() -> Router<AppState> {
    Router::new()
        // Account operations
        .route("/accounts", post(handlers::create_account))
        .route("/accounts/{account_id}", get(handlers::get_account))
        .route(
            "/accounts/{account_id}/status",
            patch(handlers::update_account_status),
        )
        // Balance
        .route("/accounts/{account_id}/balance", get(handlers::get_balance))
        // Deposit
        .route("/accounts/{account_id}/deposits", post(handlers::deposit))
        .route(
            "/accounts/{account_id}/deposits/{deposit_id}/post",
            post(handlers::post_deposit),
        )
        .route(
            "/accounts/{account_id}/deposits/{deposit_id}/void",
            post(handlers::void_deposit),
        )
        // Payment
        .route(
            "/accounts/{account_id}/payments",
            post(handlers::make_payment),
        )
        // Withdrawal
        .route(
            "/accounts/{account_id}/withdrawals",
            post(handlers::withdraw),
        )
        // Transactions
        .route("/transactions", get(handlers::list_all_transactions))
        .route(
            "/accounts/{account_id}/transactions",
            get(handlers::list_transactions),
        )
}
