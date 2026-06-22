use axum::body::Body;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::Router;

use crate::api::handlers;
use crate::AppState;

/// Public API routes — no authentication required.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::pb::health))
        .route("/purpose-types", get(handlers::pb::list_purpose_types))
        .route(
            "/purpose-types/{purpose_code}",
            get(handlers::pb::get_purpose_type),
        )
        .route("/docs", get(handlers::pb::swagger_ui))
        .route("/docs/openapi.json", get(handlers::pb::openapi_json))
}

/// Protected API routes — require API key authentication.
pub fn protected_router() -> Router<AppState> {
    let pb = Router::new()
        .route("/pb-accounts", post(handlers::pb::create_account))
        .route("/pb-accounts/{account_id}", get(handlers::pb::get_account))
        .route(
            "/pb-accounts/{account_id}/status",
            patch(handlers::pb::update_account_status),
        )
        .route(
            "/pb-accounts/{account_id}/balance",
            get(handlers::pb::get_balance),
        )
        .route(
            "/pb-accounts/{account_id}/deposits",
            post(handlers::pb::deposit),
        )
        .route(
            "/pb-accounts/{account_id}/deposits/{deposit_id}/post",
            post(handlers::pb::post_deposit),
        )
        .route(
            "/pb-accounts/{account_id}/deposits/{deposit_id}/void",
            post(handlers::pb::void_deposit),
        )
        .route(
            "/pb-accounts/{account_id}/payments",
            post(handlers::pb::make_payment),
        )
        .route(
            "/pb-accounts/{account_id}/payments/{payment_id}/refund",
            post(handlers::pb::refund_payment),
        )
        .route(
            "/pb-accounts/{account_id}/withdrawals",
            post(handlers::pb::withdraw),
        )
        .route(
            "/pb-accounts/{account_id}/transactions",
            get(handlers::pb::list_transactions),
        );

    let normal = Router::new()
        .route(
            "/normal-accounts",
            post(handlers::normal::create_normal_account)
                .get(handlers::normal::list_normal_accounts),
        )
        .route(
            "/normal-accounts/{account_id}",
            get(handlers::normal::get_normal_account),
        )
        .route(
            "/normal-accounts/{account_id}/status",
            patch(handlers::normal::update_normal_account_status),
        )
        .route(
            "/normal-accounts/{account_id}/balance",
            get(handlers::normal::get_normal_account_balance),
        )
        .route(
            "/normal-accounts/{account_id}/deposits",
            post(handlers::normal::deposit_to_normal_account),
        )
        .route(
            "/normal-accounts/{account_id}/deposits/{deposit_id}/post",
            post(handlers::normal::post_normal_account_deposit),
        )
        .route(
            "/normal-accounts/{account_id}/deposits/{deposit_id}/void",
            post(handlers::normal::void_normal_account_deposit),
        )
        .route(
            "/normal-accounts/{account_id}/transfers",
            post(handlers::transfer::initiate_transfer),
        )
        .route(
            "/normal-accounts/{account_id}/transfers/{transfer_id}/post",
            post(handlers::transfer::post_transfer),
        )
        .route(
            "/normal-accounts/{account_id}/transfers/{transfer_id}/void",
            post(handlers::transfer::void_transfer),
        )
        .route(
            "/normal-accounts/{account_id}/transfers/{transfer_id}/reverse",
            post(handlers::transfer::reverse_transfer),
        )
        .route(
            "/normal-accounts/{account_id}/withdrawals",
            post(handlers::normal::withdraw_from_normal_account),
        )
        .route(
            "/normal-accounts/{account_id}/transactions",
            get(handlers::normal::list_normal_account_transactions),
        );

    // Legacy /accounts/* aliases — same handlers as /pb-accounts/* but with
    // Deprecation/Sunset response headers attached via middleware. New endpoints
    // (e.g. refund_payment) are intentionally not mirrored here: the legacy
    // surface is shrinking, not growing.
    let legacy = Router::new()
        .route("/accounts", post(handlers::pb::create_account))
        .route("/accounts/{account_id}", get(handlers::pb::get_account))
        .route(
            "/accounts/{account_id}/status",
            patch(handlers::pb::update_account_status),
        )
        .route(
            "/accounts/{account_id}/balance",
            get(handlers::pb::get_balance),
        )
        .route(
            "/accounts/{account_id}/deposits",
            post(handlers::pb::deposit),
        )
        .route(
            "/accounts/{account_id}/deposits/{deposit_id}/post",
            post(handlers::pb::post_deposit),
        )
        .route(
            "/accounts/{account_id}/deposits/{deposit_id}/void",
            post(handlers::pb::void_deposit),
        )
        .route(
            "/accounts/{account_id}/payments",
            post(handlers::pb::make_payment),
        )
        .route(
            "/accounts/{account_id}/withdrawals",
            post(handlers::pb::withdraw),
        )
        .route(
            "/accounts/{account_id}/transactions",
            get(handlers::pb::list_transactions),
        )
        .layer(axum::middleware::from_fn(deprecation_headers));

    Router::new().merge(pb).merge(normal).merge(legacy).route(
        "/transactions",
        get(handlers::transactions::list_all_transactions),
    )
}

/// Attach Deprecation and Sunset headers to legacy /accounts/* responses.
/// Sunset is 90 days from PR 2 merge: 2026-05-08 + 90 = 2026-08-06.
async fn deprecation_headers(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("Deprecation", "true".parse().unwrap());
    response
        .headers_mut()
        .insert("Sunset", "2026-08-06".parse().unwrap());
    response.headers_mut().insert(
        "Link",
        "</docs#deprecation>; rel=\"deprecation\"".parse().unwrap(),
    );
    response
}
