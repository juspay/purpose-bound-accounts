use std::sync::Arc;

use cucumber::World as _;
use pba_client::Client;

mod steps;

#[derive(Debug, cucumber::World)]
pub struct PbaWorld {
    client: Arc<Client>,
    /// Current account ID being operated on
    account_id: Option<String>,
    /// Last deposit result
    last_deposit_pool: Option<String>,
    /// Last payment result
    last_payment: Option<PaymentResult>,
    /// Last error (for negative test cases)
    last_error: Option<PbaError>,
    /// Purpose types from list operation
    purpose_types_count: Option<usize>,
    /// Last purpose type fetched
    last_purpose_code: Option<String>,
    last_purpose_mccs_count: Option<usize>,
    /// Last balance
    last_balance: Option<BalanceResult>,
    /// Last account status
    last_account_status: Option<String>,
    /// Whether a duplicate was rejected
    duplicate_rejected: bool,
    /// Last withdrawal amount
    last_withdrawal_amount: Option<i64>,
    /// Results from concurrent payment tests
    concurrent_successes: Option<usize>,
    concurrent_failures: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct PaymentResult {
    pub amount: i64,
    pub from_others: i64,
    pub from_self: i64,
}

#[derive(Debug, Clone)]
pub struct BalanceResult {
    pub self_contribution: i64,
    pub others_contribution: i64,
    pub total: i64,
}

#[derive(Debug, Clone)]
pub struct PbaError {
    pub kind: String,
}

impl Default for PbaWorld {
    fn default() -> Self {
        let base_url =
            std::env::var("PBA_SERVICE_URL").unwrap_or_else(|_| "http://127.0.0.1:3030".into());

        let config = pba_client::Config::builder()
            .endpoint_url(&base_url)
            .behavior_version_latest()
            .build();
        let client = Client::from_conf(config);

        Self {
            client: Arc::new(client),
            account_id: None,
            last_deposit_pool: None,
            last_payment: None,
            last_error: None,
            purpose_types_count: None,
            last_purpose_code: None,
            last_purpose_mccs_count: None,
            last_balance: None,
            last_account_status: None,
            duplicate_rejected: false,
            last_withdrawal_amount: None,
            concurrent_successes: None,
            concurrent_failures: None,
        }
    }
}

#[tokio::main]
async fn main() {
    PbaWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run("tests/features")
        .await;
}
