mod handlers;
mod normal_handlers;

use axum::routing::get;
use axum::Router;

use crate::auth::oidc;
use crate::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        // Auth routes
        .route("/admin/login", get(handlers::login_page))
        .route("/admin/auth/keycloak", get(oidc::login_redirect))
        .route("/admin/callback", get(oidc::callback))
        .route("/admin/logout", get(oidc::logout))
        // Existing routes
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
            "/admin/transactions/{transaction_id}",
            get(handlers::transaction_detail),
        )
        .route(
            "/admin/transactions/{transaction_id}/post",
            axum::routing::post(handlers::post_transaction),
        )
        .route(
            "/admin/transactions/{transaction_id}/void",
            axum::routing::post(handlers::void_transaction),
        )
        .route(
            "/admin/system-accounts",
            get(handlers::system_accounts_page),
        )
        .route("/admin/purpose-types", get(handlers::purpose_types_page))
        // Normal account routes
        .route(
            "/admin/normal-accounts",
            get(normal_handlers::normal_accounts_list).post(normal_handlers::create_normal_account),
        )
        .route(
            "/admin/normal-accounts/{account_id}",
            get(normal_handlers::normal_account_detail),
        )
        .route(
            "/admin/normal-accounts/{account_id}/freeze",
            axum::routing::post(normal_handlers::freeze_normal_account),
        )
        .route(
            "/admin/normal-accounts/{account_id}/reactivate",
            axum::routing::post(normal_handlers::reactivate_normal_account),
        )
        .route(
            "/admin/normal-accounts/{account_id}/deposit",
            get(normal_handlers::normal_deposit_form),
        )
        .route(
            "/admin/normal-accounts/{account_id}/deposits",
            axum::routing::post(normal_handlers::process_normal_deposit),
        )
        .route(
            "/admin/normal-accounts/{account_id}/withdrawal",
            get(normal_handlers::normal_withdrawal_form),
        )
        .route(
            "/admin/normal-accounts/{account_id}/withdrawals",
            axum::routing::post(normal_handlers::process_normal_withdrawal),
        )
}
