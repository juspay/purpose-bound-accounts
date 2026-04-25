use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use base64::Engine;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::{AppState, error::AppError};
use super::claims;

/// Axum middleware that validates the X-Api-Key header.
pub async fn require_api_key(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !state.auth.auth_enabled {
        return Ok(next.run(req).await);
    }

    let api_key = req
        .headers()
        .get("X-Api-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing X-Api-Key header".into()))?;

    let validated_claims = exchange_api_key(&state, api_key).await?;

    req.extensions_mut().insert(validated_claims);
    Ok(next.run(req).await)
}

/// Decode the API key and exchange it for a validated JWT.
async fn exchange_api_key(state: &AppState, api_key: &str) -> Result<claims::Claims, AppError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(api_key)
        .map_err(|_| AppError::Unauthorized("Invalid API key encoding".into()))?;
    let decoded_str =
        String::from_utf8(decoded).map_err(|_| AppError::Unauthorized("Invalid API key".into()))?;
    let (client_id, client_secret) = decoded_str
        .split_once(':')
        .ok_or_else(|| AppError::Unauthorized("Invalid API key format".into()))?;

    // Check token cache
    let cache_key = client_id.to_string();
    if let Some(cached) = state.auth.token_cache.get(&cache_key) {
        if cached.expires_at > Instant::now() + Duration::from_secs(30) {
            return Ok(cached.claims.clone());
        }
    }

    // Exchange client credentials at the OIDC token endpoint
    let token_response = exchange_client_credentials(
        &state.auth.token_endpoint,
        client_id,
        client_secret,
    )
    .await?;

    // Validate the JWT
    let validated_claims = claims::validate_jwt(
        &token_response.access_token,
        &state.auth.jwks,
        &state.auth.issuer,
    )
    .await
    .map_err(AppError::Unauthorized)?;

    // Cache it
    let expires_at = Instant::now() + Duration::from_secs(token_response.expires_in);
    state.auth.token_cache.insert(
        cache_key,
        crate::CachedToken {
            access_token: token_response.access_token,
            claims: validated_claims.clone(),
            expires_at,
        },
    );

    Ok(validated_claims)
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

async fn exchange_client_credentials(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<TokenResponse, AppError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(token_url)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|e| AppError::Unauthorized(format!("Token exchange failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Unauthorized("Invalid API credentials".into()));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| AppError::Unauthorized(format!("Invalid token response: {e}")))
}
