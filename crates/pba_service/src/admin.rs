mod handlers;
mod normal_handlers;
mod tb;
mod transfer_handlers;

use axum::routing::{get, post};
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
            "/admin/transactions/{txn_id}/returns.json",
            get(handlers::admin_returns_list_json),
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
        // Transfer routes
        .route(
            "/admin/normal-accounts/{account_id}/transfer",
            get(transfer_handlers::normal_transfer_form),
        )
        .route(
            "/admin/normal-accounts/{account_id}/transfers",
            get(normal_handlers::normal_transfers_fragment)
                .post(transfer_handlers::process_normal_transfer),
        )
        .route(
            "/admin/transfers/{transfer_id}",
            get(transfer_handlers::transfer_detail),
        )
        .route(
            "/admin/transfers/{transfer_id}/post",
            axum::routing::post(transfer_handlers::post_transfer),
        )
        .route(
            "/admin/transfers/{transfer_id}/void",
            axum::routing::post(transfer_handlers::void_transfer),
        )
        .route(
            "/admin/transfers/{transfer_id}/reverse",
            get(transfer_handlers::reverse_transfer_form)
                .post(transfer_handlers::process_reverse_transfer),
        )
        .route(
            "/admin/accounts/{account_id}/payments/{payment_id}/refund",
            get(handlers::refund_payment_form).post(handlers::process_refund_payment),
        )
        .route(
            "/admin/accounts/{account_id}/contribution-returns/new",
            get(handlers::contribution_return_form),
        )
        .route(
            "/admin/accounts/{account_id}/contribution-returns",
            post(handlers::process_contribution_return),
        )
        .route(
            "/admin/accounts/{account_id}/refunds/{refund_id}/post",
            post(handlers::admin_post_refund),
        )
        .route(
            "/admin/accounts/{account_id}/refunds/{refund_id}/void",
            post(handlers::admin_void_refund),
        )
        .route(
            "/admin/accounts/{account_id}/contribution-returns/{return_id}/post",
            post(handlers::admin_post_contribution_return),
        )
        .route(
            "/admin/accounts/{account_id}/contribution-returns/{return_id}/void",
            post(handlers::admin_void_contribution_return),
        )
        // Static assets (self-hosted so admin UI works offline / on locked-down networks).
        .route("/admin/static/htmx.min.js", get(serve_htmx))
        // TigerBeetle explorer
        .route("/admin/tb", get(tb::overview))
        .route("/admin/tb/accounts", get(tb::accounts_page))
        .route("/admin/tb/accounts/{id}", get(tb::account_detail))
        .route("/admin/tb/transfers", get(tb::transfers_page))
        .route("/admin/tb/transfers/{id}", get(tb::transfer_detail))
        .route("/admin/tb/pending", get(tb::pending_page))
        .route("/admin/tb/pending/{id}/post", post(tb::pending_post))
        .route("/admin/tb/pending/{id}/void", post(tb::pending_void))
        .route("/admin/tb/decoder", get(tb::decoder))
}

const HTMX_JS: &str = include_str!("../static/htmx.min.js");

async fn serve_htmx() -> axum::response::Response {
    use axum::http::header;
    use axum::response::IntoResponse;
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        HTMX_JS,
    )
        .into_response()
}
