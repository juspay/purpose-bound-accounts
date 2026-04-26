use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use tower_cookies::Cookies;

use super::session;
use crate::AppState;

/// Middleware that checks for a valid session cookie on admin routes.
/// Redirects to /admin/login if no session found.
pub async fn require_admin_session(
    State(state): State<AppState>,
    cookies: Cookies,
    req: Request,
    next: Next,
) -> Response {
    if !state.auth.auth_enabled {
        return next.run(req).await;
    }

    // Skip auth for login/callback/logout routes
    let path = req.uri().path();
    if path == "/admin/login"
        || path == "/admin/auth/keycloak"
        || path == "/admin/callback"
        || path == "/admin/logout"
    {
        return next.run(req).await;
    }

    match session::get_session(&cookies, &state.auth.cookie_key) {
        Some(_session) => next.run(req).await,
        None => Redirect::temporary(&format!("{}/admin/login", state.path_prefix)).into_response(),
    }
}
