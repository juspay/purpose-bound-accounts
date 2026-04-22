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
            "/admin/accounts/{account_id}",
            get(handlers::account_detail),
        )
        .route(
            "/admin/accounts/{account_id}/status",
            axum::routing::post(handlers::update_account_status),
        )
        .route(
            "/admin/accounts/{account_id}/transfers",
            get(handlers::account_transfers_fragment),
        )
        .route(
            "/admin/accounts/{account_id}/deposit",
            get(handlers::deposit_form).post(handlers::process_deposit),
        )
        .route(
            "/admin/accounts/{account_id}/deposits/{deposit_id}/post",
            axum::routing::post(handlers::post_deposit),
        )
        .route(
            "/admin/accounts/{account_id}/deposits/{deposit_id}/void",
            axum::routing::post(handlers::void_deposit),
        )
        .route(
            "/admin/accounts/{account_id}/payment",
            get(handlers::payment_form).post(handlers::process_payment),
        )
        .route(
            "/admin/accounts/{account_id}/withdrawal",
            get(handlers::withdrawal_form).post(handlers::process_withdrawal),
        )
        .route("/admin/transactions", get(handlers::transactions_page))
        .route(
            "/admin/system-accounts",
            get(handlers::system_accounts_page),
        )
        .route("/admin/purpose-types", get(handlers::purpose_types_page))
}
