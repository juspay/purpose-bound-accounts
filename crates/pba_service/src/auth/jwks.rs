use dashmap::DashMap;
use jsonwebtoken::{jwk::JwkSet, DecodingKey};
use std::sync::Arc;
use std::time::Instant;

/// Caches OIDC provider's JWKS (public keys) with a TTL.
#[derive(Clone)]
pub struct JwksCache {
    http: reqwest::Client,
    jwks_uri: String,
    /// kid -> (DecodingKey, fetched_at)
    keys: Arc<DashMap<String, (DecodingKey, Instant)>>,
    ttl_secs: u64,
}

impl JwksCache {
    /// Create a new JWKS cache from a discovered `jwks_uri`.
    pub fn new(jwks_uri: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            jwks_uri: jwks_uri.to_string(),
            keys: Arc::new(DashMap::new()),
            ttl_secs: 300,
        }
    }

    /// Get a decoding key by key ID, refreshing from the provider if expired or missing.
    pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, String> {
        if let Some(entry) = self.keys.get(kid) {
            let (key, fetched_at) = entry.value();
            if fetched_at.elapsed().as_secs() < self.ttl_secs {
                return Ok(key.clone());
            }
        }

        self.refresh().await?;

        self.keys
            .get(kid)
            .map(|e| e.value().0.clone())
            .ok_or_else(|| format!("Key ID '{}' not found in JWKS", kid))
    }

    async fn refresh(&self) -> Result<(), String> {
        let jwks: JwkSet = self
            .http
            .get(&self.jwks_uri)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch JWKS: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse JWKS: {e}"))?;

        let now = Instant::now();
        for jwk in &jwks.keys {
            if let Some(kid) = &jwk.common.key_id {
                if let Ok(key) = DecodingKey::from_jwk(jwk) {
                    self.keys.insert(kid.clone(), (key, now));
                }
            }
        }

        Ok(())
    }
}
