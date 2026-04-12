mod handlers;

use axum::routing::get;
use axum::Router;

use crate::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/admin", get(handlers::dashboard))
        .route(
            "/admin/accounts",
            get(handlers::accounts_list).post(handlers::create_account),
        )
        .route(
            "/admin/accounts/{accountId}",
            get(handlers::account_detail),
        )
        .route(
            "/admin/accounts/{accountId}/status",
            axum::routing::post(handlers::update_account_status),
        )
        .route(
            "/admin/accounts/{accountId}/transfers",
            get(handlers::account_transfers_fragment),
        )
        .route(
            "/admin/accounts/{accountId}/deposit",
            get(handlers::deposit_form).post(handlers::process_deposit),
        )
        .route(
            "/admin/accounts/{accountId}/payment",
            get(handlers::payment_form).post(handlers::process_payment),
        )
        .route(
            "/admin/accounts/{accountId}/withdrawal",
            get(handlers::withdrawal_form).post(handlers::process_withdrawal),
        )
}
