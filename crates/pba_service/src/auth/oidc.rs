use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::AppState;
use super::session::{self, UserSession};

/// GET /admin/auth/keycloak — redirect to the OIDC authorization endpoint.
pub async fn login_redirect(State(state): State<AppState>) -> Response {
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope=openid%20profile%20email",
        state.auth.authorization_endpoint,
        state.auth.oidc_client_id,
        urlencoding::encode(&state.auth.callback_url),
    );
    Redirect::temporary(&auth_url).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: String,
}

/// GET /admin/callback — exchange auth code for tokens, fetch UserInfo, create session.
pub async fn callback(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let client = reqwest::Client::new();

    // Exchange authorization code for tokens
    let resp = client
        .post(&state.auth.token_endpoint)
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

    // Fetch standardized claims from the UserInfo endpoint.
    // This works across OIDC providers (not just Keycloak) and avoids
    // relying on provider-specific access-token claims.
    let userinfo = client
        .get(&state.auth.userinfo_endpoint)
        .bearer_auth(&token.access_token)
        .send()
        .await;

    #[derive(Deserialize)]
    struct UserInfoResp {
        sub: Option<String>,
        preferred_username: Option<String>,
        email: Option<String>,
        // Keycloak includes realm_access here; other providers won't.
        // We fall back to an empty role list for non-Keycloak providers.
        realm_access: Option<RealmAccessResp>,
    }
    #[derive(Deserialize)]
    struct RealmAccessResp {
        roles: Vec<String>,
    }

    let userinfo: UserInfoResp = match userinfo {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("Failed to parse UserInfo response: {e}");
                return Redirect::temporary("/admin/login").into_response();
            }
        },
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            tracing::error!("UserInfo request failed: status={status}, body={body}");
            return Redirect::temporary("/admin/login").into_response();
        }
        Err(e) => {
            tracing::error!("UserInfo request error: {e}");
            return Redirect::temporary("/admin/login").into_response();
        }
    };

    let user_session = UserSession {
        sub: userinfo.sub.unwrap_or_else(|| "unknown".into()),
        display_name: userinfo
            .preferred_username
            .as_deref()
            .or(userinfo.email.as_deref())
            .unwrap_or("unknown")
            .to_string(),
        email: userinfo.email,
        roles: userinfo
            .realm_access
            .map(|ra| ra.roles)
            .unwrap_or_default(),
    };

    session::set_session(&cookies, &state.auth.cookie_key, &user_session);
    Redirect::temporary("/admin").into_response()
}

/// GET /admin/logout — clear session and redirect to OIDC end-session endpoint.
pub async fn logout(State(state): State<AppState>, cookies: Cookies) -> Response {
    session::clear_session(&cookies, &state.auth.cookie_key);

    let post_logout_uri = format!("http://localhost:{}/admin/login", state.auth.port);

    if let Some(ref end_session_url) = state.auth.end_session_endpoint {
        let logout_url = format!(
            "{}?post_logout_redirect_uri={}&client_id={}",
            end_session_url,
            urlencoding::encode(&post_logout_uri),
            state.auth.oidc_client_id,
        );
        Redirect::temporary(&logout_url).into_response()
    } else {
        // Provider doesn't support end_session_endpoint — just redirect to login
        Redirect::temporary(&post_logout_uri).into_response()
    }
}
