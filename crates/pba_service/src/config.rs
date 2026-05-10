use crate::secrets::SecretsProvider;

/// Controls how database migrations are handled at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationMode {
    /// Skip migrations entirely.
    None,
    /// Apply any pending migrations (default).
    Run,
    /// Print SQL for any pending migrations to stdout, then exit without serving.
    DryRun,
}

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub tigerbeetle_addresses: Vec<String>,
    pub tigerbeetle_cluster_id: u128,
    pub host: String,
    pub port: u16,
    pub deposit_timeout_seconds: u32,
    pub deposit_poller_interval_seconds: u64,
    pub oidc_issuer_url: String,
    pub oidc_client_id: String,
    pub cookie_secret: String,
    pub auth_enabled: bool,
    pub path_prefix: String,
    pub db_migration_mode: MigrationMode,
}

impl AppConfig {
    pub async fn from_env(secrets: &dyn SecretsProvider) -> Self {
        let db_host = std::env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string());
        let db_port = std::env::var("DB_PORT").unwrap_or_else(|_| "5432".to_string());
        let db_name = std::env::var("DB_NAME").unwrap_or_else(|_| "pba_service".to_string());
        let db_user = std::env::var("DB_USER").ok();
        let raw_db_password = std::env::var("DB_PASSWORD").unwrap_or_default();
        let db_password = if raw_db_password.is_empty() {
            raw_db_password
        } else {
            secrets
                .decrypt(&raw_db_password)
                .await
                .expect("Failed to decrypt DB_PASSWORD")
        };

        // Unix socket: fall back to OS user (trust auth); TCP: require explicit DB_USER
        let database_url = if db_host.starts_with('/') {
            let user = db_user
                .or_else(|| std::env::var("USER").ok())
                .expect("DB_USER or USER must be set");
            format!("postgres://{user}:{db_password}@localhost:{db_port}/{db_name}?host={db_host}")
        } else {
            let user = db_user.expect("DB_USER must be set for TCP connections");
            format!("postgres://{user}:{db_password}@{db_host}:{db_port}/{db_name}")
        };

        let tb_addresses =
            std::env::var("TIGERBEETLE_ADDRESSES").unwrap_or_else(|_| "3000".to_string());
        let tb_cluster_id: u128 = std::env::var("TIGERBEETLE_CLUSTER_ID")
            .unwrap_or_else(|_| "0".to_string())
            .parse()
            .expect("TIGERBEETLE_CLUSTER_ID must be a valid u128");
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .expect("PORT must be a valid u16");
        let deposit_timeout_seconds: u32 = std::env::var("DEPOSIT_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "1800".to_string())
            .parse()
            .expect("DEPOSIT_TIMEOUT_SECONDS must be a valid u32");
        let deposit_poller_interval_seconds: u64 = std::env::var("DEPOSIT_POLLER_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .expect("DEPOSIT_POLLER_INTERVAL_SECONDS must be a valid u64");

        let oidc_issuer_url = std::env::var("OIDC_ISSUER_URL")
            .unwrap_or_else(|_| "http://localhost:8180/realms/pba".to_string());
        let oidc_client_id =
            std::env::var("OIDC_CLIENT_ID").unwrap_or_else(|_| "pba-admin".to_string());
        let raw_cookie_secret = std::env::var("COOKIE_SECRET")
            .unwrap_or_else(|_| "change-me-in-production-32-bytes!".to_string());
        let cookie_secret = secrets
            .decrypt(&raw_cookie_secret)
            .await
            .expect("Failed to decrypt COOKIE_SECRET");
        let auth_enabled = std::env::var("AUTH_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        let db_migration_mode = match std::env::var("DB_MIGRATION_MODE")
            .unwrap_or_else(|_| "run".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "none" | "skip" | "off" => MigrationMode::None,
            "run" | "apply" | "auto" => MigrationMode::Run,
            "dry-run" | "dry_run" | "dryrun" | "print" => MigrationMode::DryRun,
            other => panic!(
                "DB_MIGRATION_MODE must be one of: none, run, dry-run (got: {other})"
            ),
        };

        let path_prefix = std::env::var("PATH_PREFIX").unwrap_or_default();
        let path_prefix = path_prefix.trim_end_matches('/').to_string();
        let path_prefix = if !path_prefix.is_empty() && !path_prefix.starts_with('/') {
            format!("/{path_prefix}")
        } else {
            path_prefix
        };

        Self {
            database_url,
            tigerbeetle_addresses: tb_addresses
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            tigerbeetle_cluster_id: tb_cluster_id,
            host,
            port,
            deposit_timeout_seconds,
            deposit_poller_interval_seconds,
            oidc_issuer_url,
            oidc_client_id,
            cookie_secret,
            auth_enabled,
            path_prefix,
            db_migration_mode,
        }
    }
}
