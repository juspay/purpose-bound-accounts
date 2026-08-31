use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use cucumber::World as _;
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

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
    /// Saved normal account ID so it survives after a PB account step overwrites `account_id`.
    last_normal_account_id: Option<String>,
    /// Transfer ID from the most recent transfer initiation.
    last_transfer_id: Option<String>,
    /// Reversal transaction ID captured from the "Reversed by" link.
    last_reversal_id: Option<String>,
    /// Last payment id captured by 'I visit the transaction detail page for the last payment'
    last_payment_id: Option<String>,
    /// Last refund correlation_id captured after submitting the refund form
    last_refund_id: Option<String>,
    /// Last contribution return correlation_id captured after submitting the return form
    last_return_id: Option<String>,
    #[allow(dead_code)]
    /// Last contribution return status captured after the return detail page is read
    last_return_status: Option<String>,
}

impl std::fmt::Debug for UiWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiWorld")
            .field("base_url", &self.base_url)
            .field("account_id", &self.account_id)
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
            last_normal_account_id: None,
            last_transfer_id: None,
            last_reversal_id: None,
            last_payment_id: None,
            last_refund_id: None,
            last_return_id: None,
            last_return_status: None,
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

    /// Poll until the current URL no longer ends with `form_suffix`
    /// (i.e. the server has redirected away from the form page).
    /// Returns `true` if the redirect happened within ~5 seconds.
    pub async fn wait_for_redirect(&mut self, form_suffix: &str) -> bool {
        for _ in 0..10 {
            sleep(Duration::from_millis(500)).await;
            let page = self.ensure_page().await;
            let url = page
                .url()
                .await
                .expect("Failed to get URL")
                .unwrap_or_default();
            if url.contains("/admin/accounts/") && !url.ends_with(form_suffix) {
                return true;
            }
        }
        false
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
        .filter_run("tests/ui_features", |_feature, _rule, scenario| {
            !scenario.tags.iter().any(|t| t == "todo")
        })
        .await;

    if w1.execution_has_failed() || w2.execution_has_failed() {
        std::process::exit(1);
    }
}
