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
    /// Last deposit ID (for two-phase deposit tracking)
    last_deposit_id: Option<String>,
    /// Last payment result
    last_payment: Option<PaymentResult>,
    last_payment_gateway_ref: Option<String>,
    /// Payment ID stashed for cross-call comparison (e.g. idempotency replay)
    remembered_payment_id: Option<String>,
    /// Per-account transactions listing (separate from the all-transactions view)
    last_account_transactions_correlation_ids: Option<Vec<Option<String>>>,
    last_account_transactions_types: Option<Vec<String>>,
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
    /// Last withdrawal amount
    last_withdrawal_amount: Option<i64>,
    last_withdrawal_gateway_ref: Option<String>,
    /// Results from concurrent payment tests
    concurrent_successes: Option<usize>,
    concurrent_failures: Option<usize>,
    /// All-transactions results
    all_transactions_total: Option<i64>,
    all_transactions_count: Option<usize>,
    all_transactions_types: Option<Vec<String>>,
    all_transactions_account_ids: Option<Vec<String>>,
    /// Last deposit funding type
    last_funding_type: Option<String>,
    /// All-transactions funding types
    all_transactions_funding_types: Option<Vec<Option<String>>>,
    /// Last normal-account ID
    last_normal_account_id: Option<String>,
    /// Last normal-account holder_id
    last_normal_holder_id: Option<String>,
    /// Last normal-account origin_ifsc
    last_normal_origin_ifsc: Option<String>,
    /// Last normal-account deposit ID
    last_normal_deposit_id: Option<String>,
    /// Last normal-account deposit status
    last_normal_deposit_status: Option<String>,
    /// Last normal-account balance
    last_normal_balance: Option<i64>,
    /// Last normal-account deposit ID set (for idempotency comparison)
    last_normal_deposit_ids: Option<Vec<String>>,
    /// Last transfer ID
    last_transfer_id: Option<String>,
    /// Last transfer status
    last_transfer_status: Option<String>,
    /// Last transfer correlation_id
    last_transfer_correlation_id: Option<String>,
    /// Last set of transfer IDs (for idempotency replay)
    last_transfer_ids: Option<Vec<String>>,
    /// Source-side transaction fields (populated by correlation_id assertion step)
    last_source_txn_type: Option<String>,
    last_source_txn_direction: Option<String>,
    /// Destination-side transaction fields (populated by correlation_id assertion step)
    last_dest_txn_type: Option<String>,
    last_dest_txn_pool: Option<String>,
    last_dest_txn_funding_type: Option<String>,
    /// Last reversal ID
    last_reversal_id: Option<String>,
    /// Last reversal status
    last_reversal_status: Option<String>,
    /// Last reversal correlation_id
    last_reversal_correlation_id: Option<String>,
    /// Last reversal's original_amount (echoes the original transfer's amount)
    last_reversal_original_amount: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PaymentResult {
    pub payment_id: String,
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
    pub message: Option<String>,
}

impl Default for PbaWorld {
    fn default() -> Self {
        let base_url =
            std::env::var("PBA_SERVICE_URL").unwrap_or_else(|_| "http://127.0.0.1:3030".into());

        let config = pba_client::Config::builder()
            .endpoint_url(&base_url)
            .behavior_version_latest()
            .api_key(pba_client::config::Token::new("dGVzdDp0ZXN0", None))
            .build();
        let client = Client::from_conf(config);

        Self {
            client: Arc::new(client),
            account_id: None,
            last_deposit_pool: None,
            last_deposit_id: None,
            last_payment: None,
            last_payment_gateway_ref: None,
            remembered_payment_id: None,
            last_account_transactions_correlation_ids: None,
            last_account_transactions_types: None,
            last_error: None,
            purpose_types_count: None,
            last_purpose_code: None,
            last_purpose_mccs_count: None,
            last_balance: None,
            last_account_status: None,
            last_withdrawal_amount: None,
            last_withdrawal_gateway_ref: None,
            concurrent_successes: None,
            concurrent_failures: None,
            all_transactions_total: None,
            all_transactions_count: None,
            all_transactions_types: None,
            all_transactions_account_ids: None,
            last_funding_type: None,
            all_transactions_funding_types: None,
            last_normal_account_id: None,
            last_normal_holder_id: None,
            last_normal_origin_ifsc: None,
            last_normal_deposit_id: None,
            last_normal_deposit_status: None,
            last_normal_balance: None,
            last_normal_deposit_ids: None,
            last_transfer_id: None,
            last_transfer_status: None,
            last_transfer_correlation_id: None,
            last_transfer_ids: None,
            last_source_txn_type: None,
            last_source_txn_direction: None,
            last_dest_txn_type: None,
            last_dest_txn_pool: None,
            last_dest_txn_funding_type: None,
            last_reversal_id: None,
            last_reversal_status: None,
            last_reversal_correlation_id: None,
            last_reversal_original_amount: None,
        }
    }
}

#[tokio::main]
async fn main() {
    use cucumber::StatsWriter as _;

    // Phase 1: Run @empty-db scenarios first (clean DB, nothing else has run)
    let w1 = PbaWorld::cucumber()
        .max_concurrent_scenarios(1)
        .filter_run("tests/features", |_feature, _rule, scenario| {
            scenario.tags.iter().any(|t| t == "empty-db")
        })
        .await;

    // Phase 2: Run everything else
    let w2 = PbaWorld::cucumber()
        .max_concurrent_scenarios(1)
        .filter_run("tests/features", |_feature, _rule, scenario| {
            !scenario.tags.iter().any(|t| t == "empty-db")
        })
        .await;

    if w1.execution_has_failed() || w2.execution_has_failed() {
        std::process::exit(1);
    }
}
