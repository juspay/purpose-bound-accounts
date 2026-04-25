use std::sync::Arc;
use dashmap::DashMap;
use std::time::Instant;

mod admin;
mod api;
mod auth;
mod config;
mod domain;
mod error;
mod repository;
pub mod secrets;
mod secrets_kms;
mod secrets_plaintext;
mod service;

use config::AppConfig;
use repository::account_repo::AccountRepo;
use repository::ledger_repo::LedgerRepo;
use repository::transaction_repo::TransactionRepo;
use service::account_service::AccountService;
use service::deposit_service::DepositService;
use service::payment_service::PaymentService;
use service::withdrawal_service::WithdrawalService;

#[derive(Clone)]
pub struct AppState {
    pub account_service: Arc<AccountService>,
    pub deposit_service: Arc<DepositService>,
    pub payment_service: Arc<PaymentService>,
    pub withdrawal_service: Arc<WithdrawalService>,
    pub account_repo: Arc<AccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub auth: AuthContext,
}

/// Shared auth context available to all routes.
#[derive(Clone)]
pub struct AuthContext {
    pub jwks: auth::jwks::JwksCache,
    pub token_cache: Arc<DashMap<String, CachedToken>>,
    pub keycloak_token_url: String,
    pub issuer: String,
    pub auth_enabled: bool,
    pub keycloak_url: String,
    pub keycloak_realm: String,
    pub oidc_client_id: String,
    pub callback_url: String,
    pub cookie_key: cookie::Key,
    pub port: u16,
}

pub struct CachedToken {
    pub access_token: String,
    pub claims: auth::claims::Claims,
    pub expires_at: Instant,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pba_service=info".into()),
        )
        .init();

    let secrets = secrets::create_provider().await;
    let config = AppConfig::from_env(&*secrets).await;

    let auth_ctx = AuthContext {
        jwks: auth::jwks::JwksCache::new(&config.keycloak_url, &config.keycloak_realm),
        token_cache: Arc::new(DashMap::new()),
        keycloak_token_url: format!(
            "{}/realms/{}/protocol/openid-connect/token",
            config.keycloak_url, config.keycloak_realm
        ),
        issuer: format!(
            "{}/realms/{}",
            config.keycloak_url, config.keycloak_realm
        ),
        auth_enabled: config.auth_enabled,
        keycloak_url: config.keycloak_url.clone(),
        keycloak_realm: config.keycloak_realm.clone(),
        oidc_client_id: config.oidc_client_id.clone(),
        callback_url: format!("http://localhost:{}/admin/callback", config.port),
        cookie_key: cookie::Key::derive_from(config.cookie_secret.as_bytes()),
        port: config.port,
    };

    // Initialize Postgres connection pool
    let pg_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    // Run migrations
    sqlx::migrate!("src/db/migrations")
        .run(&pg_pool)
        .await
        .expect("Failed to run database migrations");

    // Initialize repositories
    let account_repo = Arc::new(AccountRepo::new(pg_pool.clone()));
    let transaction_repo = Arc::new(TransactionRepo::new(pg_pool.clone()));
    let ledger_repo = Arc::new(LedgerRepo::new(
        config.tigerbeetle_cluster_id,
        config.tigerbeetle_addresses,
    ));

    // Create sentinel TB accounts (funding source, merchant settlement, withdrawal settlement)
    ledger_repo
        .init_sentinel_accounts()
        .await
        .expect("Failed to initialize sentinel TB accounts");

    // Initialize services
    let account_service = Arc::new(AccountService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
    ));
    let deposit_service = Arc::new(DepositService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
        Arc::clone(&transaction_repo),
        config.deposit_timeout_seconds,
    ));
    let payment_service = Arc::new(PaymentService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
        Arc::clone(&transaction_repo),
    ));
    let withdrawal_service = Arc::new(WithdrawalService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
        Arc::clone(&transaction_repo),
    ));

    let state = AppState {
        account_service,
        deposit_service,
        payment_service,
        withdrawal_service,
        account_repo,
        ledger_repo,
        transaction_repo: Arc::clone(&transaction_repo),
        auth: auth_ctx,
    };

    // Spawn background deposit timeout poller
    tokio::spawn(service::deposit_timeout::run_deposit_timeout_poller(
        Arc::clone(&transaction_repo),
        config.deposit_poller_interval_seconds,
    ));

    use tower_cookies::CookieManagerLayer;

    let app = api::routes::public_router()
        .merge(
            api::routes::protected_router()
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    auth::api_key::require_api_key,
                ))
        )
        .merge(
            admin::create_router()
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    auth::admin_auth::require_admin_session,
                ))
        )
        .layer(CookieManagerLayer::new())
        .with_state(state);

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting PBA service on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");
    axum::serve(listener, app).await.expect("Server error");
}
