use jsonwebtoken::{decode, Algorithm, Validation};
use serde::Deserialize;

use super::jwks::JwksCache;

/// JWT claims from Keycloak tokens.
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub preferred_username: Option<String>,
    pub email: Option<String>,
    pub realm_access: Option<RealmAccess>,
    pub azp: Option<String>,
    pub exp: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RealmAccess {
    pub roles: Vec<String>,
}

impl Claims {
    pub fn has_role(&self, role: &str) -> bool {
        self.realm_access
            .as_ref()
            .is_some_and(|ra| ra.roles.iter().any(|r| r == role))
    }

    /// The display name: preferred_username, email, or sub as fallback.
    pub fn display_name(&self) -> &str {
        self.preferred_username
            .as_deref()
            .or(self.email.as_deref())
            .unwrap_or(&self.sub)
    }
}

/// Validate a JWT string against Keycloak's JWKS.
pub async fn validate_jwt(token: &str, jwks: &JwksCache, issuer: &str) -> Result<Claims, String> {
    let header =
        jsonwebtoken::decode_header(token).map_err(|e| format!("Invalid JWT header: {e}"))?;
    let kid = header.kid.ok_or("JWT missing kid header")?;

    let key = jwks.get_key(&kid).await?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[issuer]);
    validation.validate_aud = false;

    let token_data = decode::<Claims>(token, &key, &validation)
        .map_err(|e| format!("JWT validation failed: {e}"))?;

    Ok(token_data.claims)
}
