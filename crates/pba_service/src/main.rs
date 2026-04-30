use dashmap::DashMap;
use std::sync::Arc;
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
    pub path_prefix: String,
}

/// Shared auth context available to all routes.
/// All endpoint URLs are populated from OIDC Discovery at startup.
#[derive(Clone)]
pub struct AuthContext {
    pub jwks: auth::jwks::JwksCache,
    pub token_cache: Arc<DashMap<String, CachedToken>>,
    pub issuer: String,
    pub auth_enabled: bool,
    // OIDC discovered endpoints
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub end_session_endpoint: Option<String>,
    // App config
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
                .unwrap_or_else(|_| "pba_service=info,tower_http=info".into()),
        )
        .init();

    let secrets = secrets::create_provider().await;
    let config = AppConfig::from_env(&*secrets).await;

    let auth_ctx = if config.auth_enabled {
        let discovery = auth::discovery::OidcDiscovery::fetch(&config.oidc_issuer_url)
            .await
            .expect("Failed to fetch OIDC discovery — is Keycloak running?");

        AuthContext {
            jwks: auth::jwks::JwksCache::new(&discovery.jwks_uri),
            token_cache: Arc::new(DashMap::new()),
            issuer: discovery.issuer,
            auth_enabled: true,
            authorization_endpoint: discovery.authorization_endpoint,
            token_endpoint: discovery.token_endpoint,
            userinfo_endpoint: discovery.userinfo_endpoint,
            end_session_endpoint: discovery.end_session_endpoint,
            oidc_client_id: config.oidc_client_id.clone(),
            callback_url: format!(
                "http://localhost:{}{}/admin/callback",
                config.port, config.path_prefix
            ),
            cookie_key: cookie::Key::derive_from(config.cookie_secret.as_bytes()),
            port: config.port,
        }
    } else {
        AuthContext {
            jwks: auth::jwks::JwksCache::new("http://localhost/_unused"),
            token_cache: Arc::new(DashMap::new()),
            issuer: String::new(),
            auth_enabled: false,
            authorization_endpoint: String::new(),
            token_endpoint: String::new(),
            userinfo_endpoint: String::new(),
            end_session_endpoint: None,
            oidc_client_id: String::new(),
            callback_url: String::new(),
            cookie_key: cookie::Key::generate(),
            port: config.port,
        }
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

    let path_prefix = config.path_prefix.clone();

    let state = AppState {
        account_service,
        deposit_service,
        payment_service,
        withdrawal_service,
        account_repo,
        ledger_repo,
        transaction_repo: Arc::clone(&transaction_repo),
        auth: auth_ctx,
        path_prefix: config.path_prefix,
    };

    // Spawn background deposit timeout poller
    tokio::spawn(service::deposit_timeout::run_deposit_timeout_poller(
        Arc::clone(&transaction_repo),
        config.deposit_poller_interval_seconds,
    ));

    use axum::Router;
    use tower_cookies::CookieManagerLayer;

    let inner = api::routes::public_router()
        .merge(
            api::routes::protected_router().layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::api_key::require_api_key,
            )),
        )
        .merge(
            admin::create_router().layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::admin_auth::require_admin_session,
            )),
        )
        .layer(CookieManagerLayer::new())
        .with_state(state);

    let app: Router = if path_prefix.is_empty() {
        inner
    } else {
        Router::new().nest(&path_prefix, inner)
    };

    use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
    let app = app.layer(
        TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
            .on_request(DefaultOnRequest::new().level(tracing::Level::INFO))
            .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
    );

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting PBA service on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");
    axum::serve(listener, app).await.expect("Server error");
}
