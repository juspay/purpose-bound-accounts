use std::sync::Arc;

mod admin;
mod api;
mod config;
mod domain;
mod error;
mod repository;
mod service;

use config::AppConfig;
use repository::account_repo::AccountRepo;
use repository::ledger_repo::LedgerRepo;
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
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pba_service=info".into()),
        )
        .init();

    let config = AppConfig::from_env();

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
    let account_repo = Arc::new(AccountRepo::new(pg_pool));
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
    ));
    let payment_service = Arc::new(PaymentService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
    ));
    let withdrawal_service = Arc::new(WithdrawalService::new(
        Arc::clone(&account_repo),
        Arc::clone(&ledger_repo),
    ));

    let state = AppState {
        account_service,
        deposit_service,
        payment_service,
        withdrawal_service,
        account_repo,
        ledger_repo,
    };

    let app = api::routes::create_router()
        .merge(admin::create_router())
        .with_state(state);

    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting PBA service on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");
    axum::serve(listener, app).await.expect("Server error");
}
