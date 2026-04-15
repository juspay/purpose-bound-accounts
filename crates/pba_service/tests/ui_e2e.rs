use std::sync::Arc;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use cucumber::World as _;
use futures::StreamExt;

mod ui_steps;

#[derive(Debug)]
pub struct BalanceResult {
    pub self_contribution: i64,
    pub others_contribution: i64,
    pub total: i64,
}

#[derive(Debug)]
pub struct PbaError {
    pub kind: String,
}

#[derive(Debug)]
pub struct PaymentInfo {
    pub amount: i64,
    pub from_others: i64,
    pub from_self: i64,
}

#[derive(cucumber::World)]
pub struct UiWorld {
    #[world(init)]
    page: Option<Arc<Page>>,
    /// Keep the browser alive so the page connection doesn't drop
    #[world(init)]
    _browser: Option<Browser>,
    base_url: String,
    account_id: Option<String>,
    origin_ifsc: Option<String>,
    origin_account_number: Option<String>,
    last_error: Option<PbaError>,
    last_deposit_pool: Option<String>,
    last_deposit_id: Option<String>,
    last_payment: Option<PaymentInfo>,
    last_withdrawal_amount: Option<i64>,
    purpose_types_count: Option<usize>,
    last_purpose_code: Option<String>,
    last_purpose_mccs_count: Option<usize>,
    last_balance: Option<BalanceResult>,
    last_account_status: Option<String>,
    duplicate_rejected: bool,
}

impl std::fmt::Debug for UiWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiWorld")
            .field("base_url", &self.base_url)
            .field("account_id", &self.account_id)
            .field("duplicate_rejected", &self.duplicate_rejected)
            .finish_non_exhaustive()
    }
}

impl Default for UiWorld {
    fn default() -> Self {
        let base_url =
            std::env::var("PBA_SERVICE_URL").unwrap_or_else(|_| "http://127.0.0.1:3030".into());
        Self {
            page: None,
            _browser: None,
            base_url,
            account_id: None,
            origin_ifsc: None,
            origin_account_number: None,
            last_error: None,
            last_deposit_pool: None,
            last_deposit_id: None,
            last_payment: None,
            last_withdrawal_amount: None,
            purpose_types_count: None,
            last_purpose_code: None,
            last_purpose_mccs_count: None,
            last_balance: None,
            last_account_status: None,
            duplicate_rejected: false,
        }
    }
}

impl UiWorld {
    pub async fn ensure_page(&mut self) -> &Page {
        if self.page.is_none() {
            let (browser, mut handler) = Browser::launch(
                {
                    let mut builder = BrowserConfig::builder();
                    builder = builder.no_sandbox();
                    if std::env::var("UI_HEAD").is_ok() {
                        builder = builder.with_head();
                    }
                    builder.build()
                }
                    .expect("Failed to build browser config"),
            )
            .await
            .expect("Failed to launch browser");

            tokio::spawn(async move {
                while let Some(event) = handler.next().await {
                    let _ = event;
                }
            });

            let page = browser
                .new_page("about:blank")
                .await
                .expect("Failed to create page");
            self._browser = Some(browser);
            self.page = Some(Arc::new(page));
        }
        self.page.as_ref().unwrap()
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[tokio::main]
async fn main() {
    use cucumber::StatsWriter as _;

    let w1 = UiWorld::cucumber()
        .max_concurrent_scenarios(1)
        .filter_run("tests/features", |_feature, _rule, scenario| {
            !scenario.tags.iter().any(|t| t == "api")
        })
        .await;

    let w2 = UiWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run("tests/ui_features")
        .await;

    if w1.execution_has_failed() || w2.execution_has_failed() {
        std::process::exit(1);
    }
}
