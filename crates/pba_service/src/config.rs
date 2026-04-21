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
        let raw_database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let database_url = secrets
            .decrypt(&raw_database_url)
            .await
            .expect("Failed to decrypt DATABASE_URL");

        let raw_tb_addresses =
            std::env::var("TIGERBEETLE_ADDRESSES").unwrap_or_else(|_| "3000".to_string());
        let tb_addresses = secrets
            .decrypt(&raw_tb_addresses)
            .await
            .expect("Failed to decrypt TIGERBEETLE_ADDRESSES");

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
        let deposit_poller_interval_seconds: u64 =
            std::env::var("DEPOSIT_POLLER_INTERVAL_SECONDS")
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
