use cucumber::{given, then, when};
use std::time::Duration;
use tokio::time::sleep;

use crate::ui_steps::account_steps::extract_balance;
use crate::UiWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Navigate to the deposit form, fill it, and submit.
/// Returns true if the deposit succeeded (redirected to detail page), false on error.
async fn do_deposit(world: &mut UiWorld, amount: i64, ifsc: &str, account_number: &str) -> bool {
    let account_id = world.account_id.clone().expect("No account ID for deposit");
    let origin_ifsc = world.origin_ifsc.clone().unwrap_or_default();
    let deposit_url = world.url(&format!("/admin/accounts/{}/deposit", account_id));

    let page = world.ensure_page().await;
    page.goto(deposit_url)
        .await
        .expect("Failed to navigate to deposit page");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;

    // Fill amount
    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

    // Fill source_ifsc
    let ifsc_input = page
        .find_element("input[name='source_ifsc']")
        .await
        .expect("Could not find source_ifsc input");
    ifsc_input
        .click()
        .await
        .expect("Failed to click source_ifsc");
    ifsc_input
        .type_str(ifsc)
        .await
        .expect("Failed to type source_ifsc");

    // Fill source_account_number
    let acct_input = page
        .find_element("input[name='source_account_number']")
        .await
        .expect("Could not find source_account_number input");
    acct_input
        .click()
        .await
        .expect("Failed to click source_account_number");
    acct_input
        .type_str(account_number)
        .await
        .expect("Failed to type source_account_number");

    // Set funding_type to "third_party" for non-origin deposits
    if ifsc != origin_ifsc {
        let js = r#"document.querySelector("select[name='funding_type']").value = "third_party";"#;
        page.evaluate(js)
            .await
            .expect("Failed to set funding_type select");
    }

    // Submit
    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    world.wait_for_redirect("/deposit").await
}

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given(regex = r"^the account has (\d+) in self-pool and (\d+) in others-pool$")]
async fn given_account_has_balances(world: &mut UiWorld, self_amount: i64, others_amount: i64) {
    let origin_ifsc = world.origin_ifsc.clone().expect("No origin IFSC recorded");
    let origin_acct = world
        .origin_account_number
        .clone()
        .expect("No origin account number recorded");

    // Deposit to self-pool from origin bank
    if self_amount > 0 {
        let ok = do_deposit(world, self_amount, &origin_ifsc, &origin_acct).await;
        assert!(ok, "Failed to deposit to self-pool in Given step");
    }

    // Deposit to others-pool from a different bank
    if others_amount > 0 {
        let ok = do_deposit(world, others_amount, "OTHER0009999", "9999999999").await;
        assert!(ok, "Failed to deposit to others-pool in Given step");
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when(regex = r#"^I deposit (\d+) from IFSC "([^"]*)" account "([^"]*)"$"#)]
async fn when_deposit(world: &mut UiWorld, amount: i64, ifsc: String, account_number: String) {
    let origin_ifsc = world.origin_ifsc.clone().unwrap_or_default();
    let ok = do_deposit(world, amount, &ifsc, &account_number).await;
    assert!(
        ok,
        "Expected deposit to succeed but it stayed on the form page"
    );

    // Determine which pool based on whether IFSC matches origin
    let pool = if ifsc == origin_ifsc {
        "self"
    } else {
        "others"
    };
    world.last_deposit_pool = Some(pool.to_string());
    world.last_error = None;
}

#[when(regex = r#"^I attempt to deposit (\d+) from IFSC "([^"]*)" account "([^"]*)"$"#)]
async fn when_attempt_deposit(
    world: &mut UiWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
) {
    let origin_ifsc = world.origin_ifsc.clone().unwrap_or_default();
    let ok = do_deposit(world, amount, &ifsc, &account_number).await;
    if ok {
        let pool = if ifsc == origin_ifsc {
            "self"
        } else {
            "others"
        };
        world.last_deposit_pool = Some(pool.to_string());
        world.last_error = None;
    } else {
        // Stayed on deposit page — error occurred
        world.last_error = Some(crate::PbaError {
            kind: "account_not_active".into(),
        });
    }
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(regex = r#"^the deposit should go to "([^"]*)" pool$"#)]
async fn then_deposit_pool(world: &mut UiWorld, expected_pool: String) {
    let pool = world
        .last_deposit_pool
        .as_ref()
        .expect("No deposit pool result recorded");
    assert_eq!(
        pool, &expected_pool,
        "Deposit pool mismatch: expected '{}' but got '{}'",
        expected_pool, pool
    );
}

#[then(regex = r"^the self contribution should be (\d+)$")]
async fn then_self_contribution(world: &mut UiWorld, expected: i64) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for balance check");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to detail for balance check");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let self_bal = extract_balance(&content, "Self Pool");

    assert_eq!(
        self_bal, expected,
        "Self contribution mismatch: expected {} but got {}",
        expected, self_bal
    );
}

#[then(regex = r"^the others contribution should be (\d+)$")]
async fn then_others_contribution(world: &mut UiWorld, expected: i64) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for balance check");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to detail for balance check");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let others_bal = extract_balance(&content, "Others Pool");

    assert_eq!(
        others_bal, expected,
        "Others contribution mismatch: expected {} but got {}",
        expected, others_bal
    );
}

#[then(regex = r"^the total balance should be (\d+)$")]
async fn then_total_balance(world: &mut UiWorld, expected: i64) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for balance check");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to detail for balance check");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let total_bal = extract_balance(&content, "Total Balance");

    assert_eq!(
        total_bal, expected,
        "Total balance mismatch: expected {} but got {}",
        expected, total_bal
    );
}

#[then("the deposit should be rejected as account not active")]
async fn then_deposit_rejected_not_active(world: &mut UiWorld) {
    assert!(
        world.last_error.is_some(),
        "Expected deposit to be rejected as account not active, but no error was recorded"
    );
}

// ---------------------------------------------------------------------------
// Pending deposit helpers
// ---------------------------------------------------------------------------

/// Navigate to the deposit form, fill it with pending=true, and submit.
/// Captures the deposit ID from the pending deposits table on redirect.
async fn do_pending_deposit(
    world: &mut UiWorld,
    amount: i64,
    ifsc: &str,
    account_number: &str,
    gateway_ref: Option<&str>,
) -> bool {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for pending deposit");
    let origin_ifsc = world.origin_ifsc.clone().unwrap_or_default();
    let deposit_url = world.url(&format!("/admin/accounts/{}/deposit", account_id));

    let page = world.ensure_page().await;
    page.goto(deposit_url)
        .await
        .expect("Failed to navigate to deposit page");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;

    // Fill amount
    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

    // Fill source_ifsc
    let ifsc_input = page
        .find_element("input[name='source_ifsc']")
        .await
        .expect("Could not find source_ifsc input");
    ifsc_input
        .click()
        .await
        .expect("Failed to click source_ifsc");
    ifsc_input
        .type_str(ifsc)
        .await
        .expect("Failed to type source_ifsc");

    // Fill source_account_number
    let acct_input = page
        .find_element("input[name='source_account_number']")
        .await
        .expect("Could not find source_account_number input");
    acct_input
        .click()
        .await
        .expect("Failed to click source_account_number");
    acct_input
        .type_str(account_number)
        .await
        .expect("Failed to type source_account_number");

    // Set funding_type to "third_party" for non-origin deposits
    if ifsc != origin_ifsc {
        let js = r#"document.querySelector("select[name='funding_type']").value = "third_party";"#;
        page.evaluate(js)
            .await
            .expect("Failed to set funding_type select");
    }

    // Check the pending checkbox
    let pending_checkbox = page
        .find_element("input[name='pending']")
        .await
        .expect("Could not find pending checkbox");
    pending_checkbox
        .click()
        .await
        .expect("Failed to click pending checkbox");

    // Fill gateway_ref if provided
    if let Some(gw_ref) = gateway_ref {
        let gw_input = page
            .find_element("input[name='gateway_ref']")
            .await
            .expect("Could not find gateway_ref input");
        gw_input.click().await.expect("Failed to click gateway_ref");
        gw_input
            .type_str(gw_ref)
            .await
            .expect("Failed to type gateway_ref");
    }

    // Submit
    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    let success = world.wait_for_redirect("/deposit").await;

    if success {
        // Extract the deposit ID from the pending deposits table on the account detail page
        let page = world.ensure_page().await;
        let content = page.content().await.expect("Failed to get page content");
        // Find the first deposit ID in the pending deposits table (truncated to 8 chars in display,
        // but the full ID is in the Post/Void form action URLs)
        if let Some(deposit_id) = extract_pending_deposit_id(&content, &account_id) {
            world.last_deposit_id = Some(deposit_id);
        }
    }

    success
}

/// Extract the first pending deposit ID from the account detail page content.
/// Looks for the Post form action URL pattern: /admin/accounts/{acct_id}/deposits/{deposit_id}/post
fn extract_pending_deposit_id(content: &str, account_id: &str) -> Option<String> {
    let pattern = format!("/admin/accounts/{}/deposits/", account_id);
    if let Some(pos) = content.find(&pattern) {
        let after = &content[pos + pattern.len()..];
        if let Some(end) = after.find('/') {
            let id = &after[..end];
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// Extract pending balance from page content.
/// Pending amounts appear as: `+ ₹X.XX pending` inside a span after the pool label.
fn extract_pending_balance(content: &str, label: &str) -> i64 {
    use crate::ui_steps::account_steps::parse_display_to_paisa;

    let search = format!("{}:", label);
    if let Some(pos) = content.find(&search) {
        let after = &content[pos + search.len()..];
        // Look for "pending" keyword after the label
        if let Some(pending_pos) = after.find("pending") {
            let before_pending = &after[..pending_pos];
            // Find the last ₹ before "pending"
            if let Some(currency_pos) = before_pending.rfind('₹') {
                let num_str = &before_pending[currency_pos + '₹'.len_utf8()..];
                let trimmed = num_str.trim();
                if !trimmed.is_empty() {
                    return parse_display_to_paisa(trimmed);
                }
            }
        }
    }
    0
}

// ---------------------------------------------------------------------------
// When steps — pending deposits
// ---------------------------------------------------------------------------

#[when(regex = r#"^I create a pending deposit of (\d+) from IFSC "([^"]*)" account "([^"]*)"$"#)]
async fn when_create_pending_deposit(
    world: &mut UiWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
) {
    let origin_ifsc = world.origin_ifsc.clone().unwrap_or_default();
    let ok = do_pending_deposit(world, amount, &ifsc, &account_number, None).await;
    assert!(
        ok,
        "Expected pending deposit to succeed but it stayed on the form page"
    );

    let pool = if ifsc == origin_ifsc {
        "self"
    } else {
        "others"
    };
    world.last_deposit_pool = Some(pool.to_string());
    world.last_error = None;
}

#[when(
    regex = r#"^I create a pending deposit of (\d+) from IFSC "([^"]*)" account "([^"]*)" with gateway ref "([^"]*)"$"#
)]
async fn when_create_pending_deposit_with_ref(
    world: &mut UiWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    gateway_ref: String,
) {
    let origin_ifsc = world.origin_ifsc.clone().unwrap_or_default();
    let ok = do_pending_deposit(world, amount, &ifsc, &account_number, Some(&gateway_ref)).await;
    assert!(
        ok,
        "Expected pending deposit to succeed but it stayed on the form page"
    );

    let pool = if ifsc == origin_ifsc {
        "self"
    } else {
        "others"
    };
    world.last_deposit_pool = Some(pool.to_string());
    world.last_error = None;
}

#[when("I post the pending deposit")]
async fn when_post_pending_deposit(world: &mut UiWorld) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for post deposit");
    let deposit_id = world
        .last_deposit_id
        .clone()
        .expect("No deposit ID to post");

    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));

    // Navigate to account detail to find the Post button
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to account detail");
    sleep(Duration::from_millis(400)).await;

    // Find and submit the Post form for this deposit
    let page = world.ensure_page().await;
    let js = format!(
        r#"document.querySelector("form[action*='/deposits/{}/post']").submit();"#,
        deposit_id
    );
    page.evaluate(js).await.expect("Failed to submit post form");
    sleep(Duration::from_millis(500)).await;
}

#[when("I void the pending deposit")]
async fn when_void_pending_deposit(world: &mut UiWorld) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for void deposit");
    let deposit_id = world
        .last_deposit_id
        .clone()
        .expect("No deposit ID to void");

    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to account detail");
    sleep(Duration::from_millis(400)).await;

    // Find and submit the Void form for this deposit
    let page = world.ensure_page().await;
    let js = format!(
        r#"document.querySelector("form[action*='/deposits/{}/void']").submit();"#,
        deposit_id
    );
    page.evaluate(js).await.expect("Failed to submit void form");
    sleep(Duration::from_millis(500)).await;
}

// ---------------------------------------------------------------------------
// Then steps — pending balances
// ---------------------------------------------------------------------------

#[then(regex = r"^the pending self should be (\d+)$")]
async fn then_pending_self(world: &mut UiWorld, expected: i64) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for pending balance check");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));

    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to detail for pending balance check");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let pending = extract_pending_balance(&content, "Self Pool");

    assert_eq!(
        pending, expected,
        "Pending self mismatch: expected {} but got {}",
        expected, pending
    );
}

#[then(regex = r"^the pending others should be (\d+)$")]
async fn then_pending_others(world: &mut UiWorld, expected: i64) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for pending balance check");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));

    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to detail for pending balance check");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let pending = extract_pending_balance(&content, "Others Pool");

    assert_eq!(
        pending, expected,
        "Pending others mismatch: expected {} but got {}",
        expected, pending
    );
}
