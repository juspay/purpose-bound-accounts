use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::AppState;
use super::claims;
use super::session::{self, UserSession};

/// GET /admin/auth/keycloak — redirect to Keycloak's authorization endpoint.
pub async fn login_redirect(State(state): State<AppState>) -> Response {
    let auth_url = format!(
        "{}/realms/{}/protocol/openid-connect/auth?response_type=code&client_id={}&redirect_uri={}&scope=openid%20profile%20email",
        state.auth.keycloak_url,
        state.auth.keycloak_realm,
        state.auth.oidc_client_id,
        urlencoding::encode(&state.auth.callback_url),
    );
    Redirect::temporary(&auth_url).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: String,
}

/// GET /admin/callback — exchange auth code for tokens, create session.
pub async fn callback(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let token_url = format!(
        "{}/realms/{}/protocol/openid-connect/token",
        state.auth.keycloak_url, state.auth.keycloak_realm
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &query.code),
            ("client_id", &state.auth.oidc_client_id),
            ("redirect_uri", &state.auth.callback_url),
        ])
        .send()
        .await;

    let resp = match resp {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            tracing::error!("Token exchange failed: status={status}, body={body}");
            return Redirect::temporary("/admin/login").into_response();
        }
        Err(e) => {
            tracing::error!("Token exchange request error: {e}");
            return Redirect::temporary("/admin/login").into_response();
        }
    };

    #[derive(Deserialize)]
    struct TokenResp {
        access_token: String,
    }

    let token: TokenResp = match resp.json().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to parse token response: {e}");
            return Redirect::temporary("/admin/login").into_response();
        }
    };

    let validated = claims::validate_jwt(
        &token.access_token,
        &state.auth.jwks,
        &state.auth.issuer,
    )
    .await;

    let jwt_claims = match validated {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("JWT validation failed: {e}");
            return Redirect::temporary("/admin/login").into_response();
        }
    };

    let user_session = UserSession {
        sub: jwt_claims.sub.clone(),
        display_name: jwt_claims.display_name().to_string(),
        email: jwt_claims.email.clone(),
        roles: jwt_claims
            .realm_access
            .map(|ra| ra.roles)
            .unwrap_or_default(),
    };

    session::set_session(&cookies, &state.auth.cookie_key, &user_session);
    Redirect::temporary("/admin").into_response()
}

/// GET /admin/logout — clear session and redirect to Keycloak logout.
pub async fn logout(State(state): State<AppState>, cookies: Cookies) -> Response {
    session::clear_session(&cookies, &state.auth.cookie_key);

    let logout_url = format!(
        "{}/realms/{}/protocol/openid-connect/logout?post_logout_redirect_uri={}&client_id={}",
        state.auth.keycloak_url,
        state.auth.keycloak_realm,
        urlencoding::encode(&format!("http://localhost:{}/admin/login", state.auth.port)),
        state.auth.oidc_client_id,
    );
    Redirect::temporary(&logout_url).into_response()
}
