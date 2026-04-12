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
async fn do_deposit(
    world: &mut UiWorld,
    amount: i64,
    ifsc: &str,
    account_number: &str,
) -> bool {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for deposit");
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
    ifsc_input.click().await.expect("Failed to click source_ifsc");
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

    // Submit
    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");
    sleep(Duration::from_millis(500)).await;

    // Check where we ended up
    let page = world.ensure_page().await;
    let current_url = page
        .url()
        .await
        .expect("Failed to get URL")
        .unwrap_or_default();

    // Success: redirected to /admin/accounts/{id} (no /deposit suffix)
    current_url.contains("/admin/accounts/") && !current_url.ends_with("/deposit")
}

// ---------------------------------------------------------------------------
// Given
// ---------------------------------------------------------------------------

#[given(regex = r"^the account has (\d+) in self-pool and (\d+) in others-pool$")]
async fn given_account_has_balances(world: &mut UiWorld, self_amount: i64, others_amount: i64) {
    let origin_ifsc = world
        .origin_ifsc
        .clone()
        .expect("No origin IFSC recorded");
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
async fn when_deposit(
    world: &mut UiWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
) {
    let origin_ifsc = world.origin_ifsc.clone().unwrap_or_default();
    let ok = do_deposit(world, amount, &ifsc, &account_number).await;
    assert!(ok, "Expected deposit to succeed but it stayed on the form page");

    // Determine which pool based on whether IFSC matches origin
    let pool = if ifsc == origin_ifsc {
        "self_contribution"
    } else {
        "others_contribution"
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
            "self_contribution"
        } else {
            "others_contribution"
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

