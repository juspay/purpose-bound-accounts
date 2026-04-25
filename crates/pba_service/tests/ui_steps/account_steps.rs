use cucumber::{given, then, when};
use std::time::Duration;
use tokio::time::sleep;

use crate::UiWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a display string like "123.45" to paisa (i64): 12345.
pub fn parse_display_to_paisa(s: &str) -> i64 {
    let s = s.trim();
    if let Some((whole, frac)) = s.split_once('.') {
        let whole: i64 = whole.parse().unwrap_or(0);
        let frac_str = format!("{:0<2}", &frac[..frac.len().min(2)]);
        let frac: i64 = frac_str.parse().unwrap_or(0);
        whole * 100 + frac
    } else {
        let whole: i64 = s.parse().unwrap_or(0);
        whole * 100
    }
}

/// Find `"Label: ₹X.XX"` in page content and return value in paisa.
pub fn extract_balance(content: &str, label: &str) -> i64 {
    // Look for pattern: label followed by optional currency symbol and number
    let search = format!("{}:", label);
    if let Some(pos) = content.find(&search) {
        let after = &content[pos + search.len()..];
        // Skip whitespace / HTML tags
        let stripped = strip_html_tags(after);
        let trimmed = stripped.trim();
        // Skip any non-digit prefix (e.g. ₹ symbol)
        let start = trimmed
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        let rest = &trimmed[start..];
        // Take up to first non-numeric (excluding '.')
        let end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let num_str = &rest[..end];
        if !num_str.is_empty() {
            return parse_display_to_paisa(num_str);
        }
    }
    0
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Extract account ID from a URL like `http://host/admin/accounts/{id}`.
fn extract_id_from_url(url: &str) -> Option<String> {
    // URL ends with /admin/accounts/{id} possibly with trailing slash
    let url = url.trim_end_matches('/');
    if let Some(pos) = url.rfind("/admin/accounts/") {
        let after = &url[pos + "/admin/accounts/".len()..];
        // after should be just the ID (no extra path segments for detail page)
        if !after.is_empty() && !after.contains('/') {
            return Some(after.to_string());
        }
    }
    None
}

/// Fill and submit the account creation form, return the created account ID.
async fn create_account_via_ui(
    world: &mut UiWorld,
    purpose: &str,
    holder_id: &str,
    ifsc: &str,
    account_number: &str,
) -> Option<String> {
    let accounts_url = world.url("/admin/accounts");
    let page = world.ensure_page().await;
    page.goto(accounts_url.clone())
        .await
        .expect("Failed to navigate to accounts page");
    sleep(Duration::from_millis(400)).await;

    // Open the <details> create form by clicking the <summary>
    let page = world.ensure_page().await;
    let summary = page
        .find_element("details summary")
        .await
        .expect("Could not find details summary");
    summary.click().await.expect("Failed to click summary");
    sleep(Duration::from_millis(200)).await;

    let page = world.ensure_page().await;

    // Fill holder_id
    let holder_input = page
        .find_element("input[name='holder_id']")
        .await
        .expect("Could not find holder_id input");
    holder_input
        .click()
        .await
        .expect("Failed to click holder_id input");
    holder_input
        .type_str(holder_id)
        .await
        .expect("Failed to type holder_id");

    // Select purpose_code
    // Use JS evaluation to set the select value
    let js = format!(
        r#"document.querySelector("select[name='purpose_code']").value = "{}";"#,
        purpose
    );
    page.evaluate(js)
        .await
        .expect("Failed to set purpose_code select");

    // Fill origin_ifsc
    let ifsc_input = page
        .find_element("input[name='origin_ifsc']")
        .await
        .expect("Could not find origin_ifsc input");
    ifsc_input
        .click()
        .await
        .expect("Failed to click origin_ifsc input");
    ifsc_input
        .type_str(ifsc)
        .await
        .expect("Failed to type origin_ifsc");

    // Fill origin_account_number
    let acct_input = page
        .find_element("input[name='origin_account_number']")
        .await
        .expect("Could not find origin_account_number input");
    acct_input
        .click()
        .await
        .expect("Failed to click origin_account_number input");
    acct_input
        .type_str(account_number)
        .await
        .expect("Failed to type origin_account_number");

    // Submit the form
    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit button");

    // Poll for redirect (up to 5 seconds) instead of a fixed sleep
    for _ in 0..10 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if let Some(id) = extract_id_from_url(&current_url) {
            return Some(id);
        }
    }

    None
}

/// Submit a status change form on the account detail page.
/// `status_value` is the value of the hidden input, e.g. "frozen", "closed", "active".
async fn submit_status_change(world: &mut UiWorld, status_value: &str) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for status change");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to account detail");
    sleep(Duration::from_millis(400)).await;

    // Find the form that has a hidden input with the target status value
    let selector = format!("input[name='status'][value='{}']", status_value);
    let page = world.ensure_page().await;
    let hidden_input = page
        .find_element(&selector)
        .await
        .unwrap_or_else(|_| panic!("Could not find status input with value={}", status_value));

    // Click the submit button within the same form
    // Use JS to submit the parent form
    let js = format!(
        r#"document.querySelector("input[name='status'][value='{}']").closest('form').submit();"#,
        status_value
    );
    page.evaluate(js)
        .await
        .expect("Failed to submit status form");
    drop(hidden_input);

    // Poll for status change (up to 5 seconds) instead of a fixed sleep
    for _ in 0..10 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let content = page.content().await.expect("Failed to get page content");
        let status = extract_status_from_content(&content);
        if status.as_deref() == Some(status_value) {
            world.last_account_status = status;
            return;
        }
    }

    // Fallback: read whatever status is there
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let status = extract_status_from_content(&content);
    world.last_account_status = status;
}

fn extract_status_from_content(content: &str) -> Option<String> {
    // Look for: <strong>Status:</strong> <span ...>STATUS</span>
    let marker = "Status:</strong>";
    if let Some(pos) = content.find(marker) {
        let after = &content[pos + marker.len()..];
        // Find the span content
        if let Some(span_start) = after.find("<span") {
            let span_content = &after[span_start..];
            if let Some(gt) = span_content.find('>') {
                let inner = &span_content[gt + 1..];
                if let Some(end) = inner.find("</span>") {
                    return Some(inner[..end].trim().to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Given steps
// ---------------------------------------------------------------------------

#[given(
    regex = r#"^a "([^"]*)" account exists for holder "([^"]*)" with origin IFSC "([^"]*)" and account number "([^"]*)"$"#
)]
async fn given_account_exists(
    world: &mut UiWorld,
    purpose: String,
    holder_id: String,
    ifsc: String,
    account_number: String,
) {
    world.origin_ifsc = Some(ifsc.clone());
    world.origin_account_number = Some(account_number.clone());

    let id = create_account_via_ui(world, &purpose, &holder_id, &ifsc, &account_number).await;
    match id {
        Some(account_id) => {
            world.account_id = Some(account_id);
            world.last_account_status = Some("active".to_string());
        }
        None => {
            // We stayed on the accounts page — likely a duplicate. Panic with context.
            let page = world.ensure_page().await;
            let content = page.content().await.unwrap_or_default();
            panic!(
                "Given step: failed to create account (no redirect to detail page). \
                 Page content snippet: {}",
                &content[..content.len().min(500)]
            );
        }
    }
}

#[given("the account is frozen")]
async fn given_account_frozen(world: &mut UiWorld) {
    submit_status_change(world, "frozen").await;
}

#[given("the account is closed")]
async fn given_account_closed(world: &mut UiWorld) {
    submit_status_change(world, "closed").await;
}

// ---------------------------------------------------------------------------
// When steps
// ---------------------------------------------------------------------------

#[when(
    regex = r#"^I create a "([^"]*)" account for holder "([^"]*)" with origin IFSC "([^"]*)" and account number "([^"]*)"$"#
)]
async fn when_create_account(
    world: &mut UiWorld,
    purpose: String,
    holder_id: String,
    ifsc: String,
    account_number: String,
) {
    world.origin_ifsc = Some(ifsc.clone());
    world.origin_account_number = Some(account_number.clone());
    world.last_error = None;

    let id = create_account_via_ui(world, &purpose, &holder_id, &ifsc, &account_number).await;
    match id {
        Some(account_id) => {
            world.account_id = Some(account_id);
            world.last_account_status = Some("active".to_string());
        }
        None => {
            panic!("When step: expected account creation to succeed but stayed on accounts page");
        }
    }
}

#[when(
    regex = r#"^I create a duplicate "([^"]*)" account for holder "([^"]*)" with origin IFSC "([^"]*)" and account number "([^"]*)"$"#
)]
async fn when_create_duplicate_account(
    world: &mut UiWorld,
    purpose: String,
    holder_id: String,
    ifsc: String,
    account_number: String,
) {
    let id = create_account_via_ui(world, &purpose, &holder_id, &ifsc, &account_number).await;
    // If we got redirected to a new detail page, the account was created (not rejected as duplicate)
    // If we stayed on the accounts page (id is None), the duplicate was rejected
    world.duplicate_rejected = id.is_none();
}

#[when("I get the account")]
async fn when_get_account(world: &mut UiWorld) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID to get account");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to account detail");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    world.last_account_status = extract_status_from_content(&content);
}

#[when("I get the account balance")]
async fn when_get_account_balance(world: &mut UiWorld) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID to get balance");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to account detail for balance");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");

    let self_bal = extract_balance(&content, "Self Pool");
    let others_bal = extract_balance(&content, "Others Pool");
    let total_bal = extract_balance(&content, "Total Balance");

    world.last_balance = Some(crate::BalanceResult {
        self_contribution: self_bal,
        others_contribution: others_bal,
        total: total_bal,
    });
}

#[when("I freeze the account")]
async fn when_freeze_account(world: &mut UiWorld) {
    submit_status_change(world, "frozen").await;
}

#[when("I reactivate the account")]
async fn when_reactivate_account(world: &mut UiWorld) {
    submit_status_change(world, "active").await;
}

#[when("I close the account")]
async fn when_close_account(world: &mut UiWorld) {
    submit_status_change(world, "closed").await;
}

// ---------------------------------------------------------------------------
// Then steps
// ---------------------------------------------------------------------------

#[then("the account should be created successfully")]
async fn then_account_created(world: &mut UiWorld) {
    assert!(
        world.account_id.is_some(),
        "Account should have been created but account_id is None"
    );
}

#[then(regex = r#"^the account purpose should be "([^"]*)"$"#)]
async fn then_account_purpose(world: &mut UiWorld, expected: String) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID to check purpose");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to account detail for purpose check");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");

    // Look for: <strong>Purpose:</strong> {purpose_code}
    let marker = "Purpose:</strong>";
    let found = if let Some(pos) = content.find(marker) {
        let after = &content[pos + marker.len()..];
        let stripped = strip_html_tags(after);
        let trimmed = stripped.trim();
        let end = trimmed
            .find(|c: char| c == '<' || c == '\n')
            .unwrap_or(trimmed.len());
        trimmed[..end].trim().to_string()
    } else {
        String::new()
    };

    assert_eq!(
        found, expected,
        "Account purpose mismatch: expected '{}' but found '{}'",
        expected, found
    );
}

#[then(regex = r#"^the account status should be "([^"]*)"$"#)]
async fn then_account_status(world: &mut UiWorld, expected: String) {
    let status = world
        .last_account_status
        .as_ref()
        .expect("No account status recorded");
    assert_eq!(
        status, &expected,
        "Account status mismatch: expected '{}' but found '{}'",
        expected, status
    );
}

#[then("the duplicate should be rejected")]
async fn then_duplicate_rejected(world: &mut UiWorld) {
    assert!(
        world.duplicate_rejected,
        "Expected duplicate account to be rejected, but it was created successfully"
    );
}

// ---------------------------------------------------------------------------
// UI-only steps (admin_ui.feature)
// ---------------------------------------------------------------------------

#[when(regex = r#"^I visit the dashboard$"#)]
async fn visit_dashboard(world: &mut UiWorld) {
    let url = world.url("/admin");
    let page = world.ensure_page().await;
    page.goto(url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}

#[then(regex = r#"^the dashboard should show at least (\d+) total accounts$"#)]
async fn dashboard_total(world: &mut UiWorld, min: i64) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    // Find the total accounts stat card — first <h2> in the stat cards
    let re = regex::Regex::new(r"<h2>(\d+)</h2>").unwrap();
    if let Some(cap) = re.captures(&content) {
        let total: i64 = cap[1].parse().unwrap_or(0);
        assert!(
            total >= min,
            "Expected at least {min} total accounts, got {total}"
        );
    } else {
        panic!("Could not find total accounts on dashboard");
    }
}

#[when(regex = r#"^I view the account detail$"#)]
async fn view_detail(world: &mut UiWorld) {
    let id = world.account_id.clone().expect("No account_id set");
    let url = world.url(&format!("/admin/accounts/{id}"));
    let page = world.ensure_page().await;
    page.goto(url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}

#[then(regex = r#"^the page should show self pool as "([^"]*)"$"#)]
async fn page_self_pool(world: &mut UiWorld, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        content.contains(&format!("₹{expected}")),
        "Expected self pool '₹{expected}' on page"
    );
}

#[then(regex = r#"^the page should show others pool as "([^"]*)"$"#)]
async fn page_others_pool(world: &mut UiWorld, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        content.contains(&format!("₹{expected}")),
        "Expected others pool '₹{expected}' on page"
    );
}

#[then(regex = r#"^the page should show total balance as "([^"]*)"$"#)]
async fn page_total_balance(world: &mut UiWorld, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        content.contains(&format!("₹{expected}")),
        "Expected total '₹{expected}' on page"
    );
}

#[then(regex = r#"^the transaction history should load$"#)]
async fn tx_history_loads(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    // Wait for HTMX to load the transfers fragment
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let content = page.content().await.unwrap();
    assert!(
        !content.contains("Loading transfers..."),
        "Transaction history should have finished loading"
    );
}

#[then(regex = r#"^the transaction history should show at least (\d+) entry$"#)]
async fn tx_history_count(world: &mut UiWorld, min: usize) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    if let Some(pos) = content.find("id=\"transfers\"") {
        let section = &content[pos..];
        let rows = section.matches("<tr>").count().saturating_sub(1);
        assert!(
            rows >= min,
            "Expected at least {min} transfer entries, got {rows}"
        );
    } else {
        panic!("Transfers section not found");
    }
}

#[then(regex = r#"^the deposit link should not be visible$"#)]
async fn no_deposit_link(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        !content.contains("/deposit\" role=\"button\""),
        "Deposit link should not be visible on frozen/closed account"
    );
}

#[then(regex = r#"^the payment link should not be visible$"#)]
async fn no_payment_link(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        !content.contains("/payment\" role=\"button\""),
        "Payment link should not be visible on frozen/closed account"
    );
}

#[then(regex = r#"^the withdrawal link should not be visible$"#)]
async fn no_withdrawal_link(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        !content.contains("/withdrawal\" role=\"button\""),
        "Withdrawal link should not be visible on frozen/closed account"
    );
}

#[when(regex = r#"^I visit the all transactions page$"#)]
async fn visit_all_transactions(world: &mut UiWorld) {
    let url = world.url("/admin/transactions");
    let page = world.ensure_page().await;
    page.goto(url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
}

#[then(regex = r#"^the all transactions page should show at least (\d+) transactions$"#)]
async fn all_transactions_count(world: &mut UiWorld, min: usize) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    // Count <tr> rows in the table body (subtract 1 for header row)
    let rows = content.matches("<tr>").count().saturating_sub(1);
    assert!(
        rows >= min,
        "Expected at least {min} transaction rows, got {rows}"
    );
}

#[then(regex = r#"^the all transactions page should show pool balance summary$"#)]
async fn all_transactions_pool_summary(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        content.contains("Total Pool Balance"),
        "Expected 'Total Pool Balance' on the page"
    );
    assert!(
        content.contains("Self Pool"),
        "Expected 'Self Pool' on the page"
    );
    assert!(
        content.contains("Others Pool"),
        "Expected 'Others Pool' on the page"
    );
}

#[when(regex = r#"^I visit the purpose types page$"#)]
async fn visit_purpose_types(world: &mut UiWorld) {
    let url = world.url("/admin/purpose-types");
    let page = world.ensure_page().await;
    page.goto(url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}

#[then(regex = r#"^I should see at least (\d+) purpose types listed$"#)]
async fn purpose_types_listed(world: &mut UiWorld, min: usize) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    let count = content.matches("id=\"purpose-").count();
    assert!(
        count >= min,
        "Expected at least {min} purpose types listed, got {count}"
    );
}

#[when(regex = r#"^I visit the system accounts page$"#)]
async fn visit_system_accounts(world: &mut UiWorld) {
    let url = world.url("/admin/system-accounts");
    let page = world.ensure_page().await;
    page.goto(url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
}

#[then(regex = r#"^I should see "([^"]*)" on the page$"#)]
async fn should_see_text(world: &mut UiWorld, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.unwrap();
    assert!(
        content.contains(&expected),
        "Expected to see '{}' on the page, but it was not found",
        expected
    );
}
