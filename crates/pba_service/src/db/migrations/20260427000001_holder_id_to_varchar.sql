-- Relax holder_id from UUID to VARCHAR(255) so the column can hold
-- any external IAM identifier (OIDC `sub` is up to 255 ASCII chars,
-- and Dex/Keycloak/Auth0 subjects are not always UUID-shaped).
ALTER TABLE accounts
    ALTER COLUMN holder_id TYPE VARCHAR(255) USING holder_id::text;
