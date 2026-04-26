use serde::Deserialize;

/// OIDC Discovery document (subset of fields we need).
#[derive(Debug, Clone, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub end_session_endpoint: Option<String>,
}

impl OidcDiscovery {
    /// Fetch the OIDC discovery document from the provider's well-known endpoint.
    pub async fn fetch(issuer_url: &str) -> Result<Self, String> {
        let url = format!(
            "{}/.well-known/openid-configuration",
            issuer_url.trim_end_matches('/')
        );

        let discovery: OidcDiscovery = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch OIDC discovery from {url}: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse OIDC discovery: {e}"))?;

        tracing::info!(
            issuer = %discovery.issuer,
            "OIDC discovery loaded"
        );

        Ok(discovery)
    }
}
