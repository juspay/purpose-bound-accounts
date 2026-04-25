use jsonwebtoken::{decode, Algorithm, Validation};
use serde::Deserialize;

use super::jwks::JwksCache;

/// JWT claims from OIDC provider tokens.
#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: Option<String>,
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
            .or(self.sub.as_deref())
            .unwrap_or("unknown")
    }

    /// The subject identifier: sub, preferred_username, or azp as fallback.
    pub fn subject(&self) -> &str {
        self.sub
            .as_deref()
            .or(self.preferred_username.as_deref())
            .or(self.azp.as_deref())
            .unwrap_or("unknown")
    }

    /// Human-readable identity for audit trails (e.g. `updated_by`).
    /// Returns "admin@pba.local" for humans, "pba-api" for service accounts.
    pub fn actor_name(&self) -> &str {
        self.preferred_username
            .as_deref()
            .or(self.azp.as_deref())
            .unwrap_or("unknown")
    }
}

/// Validate a JWT string against the OIDC provider's JWKS.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: Keycloak-style human user — all fields populated.
    fn keycloak_human() -> Claims {
        Claims {
            sub: Some("user-uuid-123".into()),
            preferred_username: Some("admin@pba.local".into()),
            email: Some("admin@pba.local".into()),
            realm_access: Some(RealmAccess {
                roles: vec!["admin".into(), "user".into()],
            }),
            azp: Some("pba-admin".into()),
            exp: 9999999999,
        }
    }

    /// Helper: Keycloak-style service account — no username/email, has azp.
    fn keycloak_service_account() -> Claims {
        Claims {
            sub: Some("service-uuid-456".into()),
            preferred_username: None,
            email: None,
            realm_access: Some(RealmAccess {
                roles: vec!["uma_authorization".into()],
            }),
            azp: Some("pba-api".into()),
            exp: 9999999999,
        }
    }

    /// Helper: Dex/standard OIDC — sub always present, no realm_access, no azp.
    fn standard_oidc_user() -> Claims {
        Claims {
            sub: Some("CiQwOGE4Njg0Yi1kYjg4LTRiNzM".into()),
            preferred_username: Some("admin@pba.local".into()),
            email: Some("admin@pba.local".into()),
            realm_access: None,
            azp: None,
            exp: 9999999999,
        }
    }

    /// Helper: Minimal OIDC — only sub present.
    fn minimal_oidc_user() -> Claims {
        Claims {
            sub: Some("minimal-sub".into()),
            preferred_username: None,
            email: None,
            realm_access: None,
            azp: None,
            exp: 9999999999,
        }
    }

    /// Helper: Worst case — nothing populated (Keycloak 26 access token edge case).
    fn empty_claims() -> Claims {
        Claims {
            sub: None,
            preferred_username: None,
            email: None,
            realm_access: None,
            azp: None,
            exp: 9999999999,
        }
    }

    // ── display_name ──────────────────────────────────────────

    #[test]
    fn display_name_prefers_username() {
        assert_eq!(keycloak_human().display_name(), "admin@pba.local");
    }

    #[test]
    fn display_name_falls_back_to_email() {
        let mut c = keycloak_human();
        c.preferred_username = None;
        assert_eq!(c.display_name(), "admin@pba.local"); // email
    }

    #[test]
    fn display_name_falls_back_to_sub() {
        assert_eq!(minimal_oidc_user().display_name(), "minimal-sub");
    }

    #[test]
    fn display_name_returns_unknown_when_empty() {
        assert_eq!(empty_claims().display_name(), "unknown");
    }

    // ── subject ───────────────────────────────────────────────

    #[test]
    fn subject_prefers_sub() {
        assert_eq!(keycloak_human().subject(), "user-uuid-123");
    }

    #[test]
    fn subject_falls_back_to_username() {
        let mut c = keycloak_human();
        c.sub = None;
        assert_eq!(c.subject(), "admin@pba.local");
    }

    #[test]
    fn subject_falls_back_to_azp() {
        assert_eq!(keycloak_service_account().subject(), "service-uuid-456");
        // When sub is missing, azp is the last resort
        let mut c = keycloak_service_account();
        c.sub = None;
        assert_eq!(c.subject(), "pba-api");
    }

    #[test]
    fn subject_returns_unknown_when_empty() {
        assert_eq!(empty_claims().subject(), "unknown");
    }

    // ── actor_name ────────────────────────────────────────────

    #[test]
    fn actor_name_returns_username_for_humans() {
        assert_eq!(keycloak_human().actor_name(), "admin@pba.local");
    }

    #[test]
    fn actor_name_returns_azp_for_service_accounts() {
        assert_eq!(keycloak_service_account().actor_name(), "pba-api");
    }

    #[test]
    fn actor_name_returns_username_for_standard_oidc() {
        assert_eq!(standard_oidc_user().actor_name(), "admin@pba.local");
    }

    #[test]
    fn actor_name_returns_unknown_when_no_identity() {
        assert_eq!(minimal_oidc_user().actor_name(), "unknown");
        assert_eq!(empty_claims().actor_name(), "unknown");
    }

    // ── has_role ──────────────────────────────────────────────

    #[test]
    fn has_role_finds_existing_role() {
        assert!(keycloak_human().has_role("admin"));
        assert!(keycloak_human().has_role("user"));
    }

    #[test]
    fn has_role_rejects_missing_role() {
        assert!(!keycloak_human().has_role("superadmin"));
    }

    #[test]
    fn has_role_returns_false_when_no_realm_access() {
        // Standard OIDC providers don't include realm_access
        assert!(!standard_oidc_user().has_role("admin"));
        assert!(!minimal_oidc_user().has_role("anything"));
    }
}
