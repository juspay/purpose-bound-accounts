use crate::secrets::SecretsProvider;

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
        }
    }
}
