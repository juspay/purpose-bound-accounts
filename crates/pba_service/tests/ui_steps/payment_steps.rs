use cucumber::{then, when};
use std::time::Duration;
use tokio::time::sleep;

use crate::ui_steps::account_steps::extract_balance;
use crate::UiWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_balances_from_content(content: &str) -> (i64, i64, i64) {
    let self_bal = extract_balance(content, "Self Pool");
    let others_bal = extract_balance(content, "Others Pool");
    let total_bal = extract_balance(content, "Total Balance");
    (self_bal, others_bal, total_bal)
}

async fn get_current_balances(world: &mut UiWorld) -> (i64, i64, i64) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for balance read");
    let detail_url = world.url(&format!("/admin/accounts/{}", account_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to detail page for balance");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    read_balances_from_content(&content)
}

/// Navigate to the payment form, fill it, and submit.
/// Returns Ok((from_others, from_self)) if redirected (success), Err(kind) if stayed on form.
async fn do_payment(
    world: &mut UiWorld,
    amount: i64,
    merchant_id: &str,
    merchant_mcc: &str,
    description: &str,
) -> Result<(i64, i64), String> {
    // Read balances before payment
    let (self_before, others_before, _) = get_current_balances(world).await;

    let account_id = world.account_id.clone().expect("No account ID for payment");
    let payment_url = world.url(&format!("/admin/accounts/{}/payment", account_id));

    let page = world.ensure_page().await;
    page.goto(payment_url)
        .await
        .expect("Failed to navigate to payment page");
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

    // Fill merchant_id
    let mid_input = page
        .find_element("input[name='merchant_id']")
        .await
        .expect("Could not find merchant_id input");
    mid_input
        .click()
        .await
        .expect("Failed to click merchant_id");
    mid_input
        .type_str(merchant_id)
        .await
        .expect("Failed to type merchant_id");

    // Fill merchant_mcc
    let mcc_input = page
        .find_element("input[name='merchant_mcc']")
        .await
        .expect("Could not find merchant_mcc input");
    mcc_input
        .click()
        .await
        .expect("Failed to click merchant_mcc");
    mcc_input
        .type_str(merchant_mcc)
        .await
        .expect("Failed to type merchant_mcc");

    // Fill description
    let desc_input = page
        .find_element("input[name='description']")
        .await
        .expect("Could not find description input");
    desc_input
        .click()
        .await
        .expect("Failed to click description");
    desc_input
        .type_str(description)
        .await
        .expect("Failed to type description");

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

    let succeeded = current_url.contains("/admin/accounts/") && !current_url.ends_with("/payment");

    if succeeded {
        // Read balances after payment and compute split
        let page = world.ensure_page().await;
        let content = page
            .content()
            .await
            .expect("Failed to get page content after payment");
        let (self_after, others_after, _) = read_balances_from_content(&content);

        let from_others = (others_before - others_after).max(0);
        let from_self = (self_before - self_after).max(0);

        Ok((from_others, from_self))
    } else {
        // Parse the error message from page content
        let page = world.ensure_page().await;
        let content = page
            .content()
            .await
            .expect("Failed to get page content for error");
        let kind = classify_error(&content);
        Err(kind)
    }
}

fn classify_error(content: &str) -> String {
    // Extract just the error message from the form-error element for precise matching
    if let Some(pos) = content.find(r#"id="form-error""#) {
        let snippet = &content[pos..content.len().min(pos + 500)];
        let lower = snippet.to_lowercase();
        // Check MCC error first — "not allowed for purpose" is unique to InvalidMcc
        if lower.contains("not allowed for purpose") || lower.contains("invalidmcc") {
            return "invalid_mcc".to_string();
        }
        if lower.contains("insufficient") || lower.contains("exceeds") {
            return "insufficient_funds".to_string();
        }
        if lower.contains("not active") || lower.contains("accountnotactive") {
            return "account_not_active".to_string();
        }
    }
    // Fallback: check full page content
    let lower = content.to_lowercase();
    if lower.contains("not allowed for purpose") {
        "invalid_mcc".to_string()
    } else if lower.contains("insufficient") {
        "insufficient_funds".to_string()
    } else if lower.contains("not active") {
        "account_not_active".to_string()
    } else {
        "unknown".to_string()
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when(regex = r#"^I pay (\d+) to merchant "([^"]*)" with MCC "([^"]*)" described as "([^"]*)"$"#)]
async fn when_pay(
    world: &mut UiWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
    description: String,
) {
    match do_payment(world, amount, &merchant_id, &mcc, &description).await {
        Ok((from_others, from_self)) => {
            world.last_payment = Some(crate::PaymentInfo {
                amount,
                from_others,
                from_self,
            });
            world.last_error = None;
        }
        Err(e) => panic!("Payment failed unexpectedly: {}", e),
    }
}

#[when(
    regex = r#"^I attempt to pay (\d+) to merchant "([^"]*)" with MCC "([^"]*)" described as "([^"]*)"$"#
)]
async fn when_attempt_pay(
    world: &mut UiWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
    description: String,
) {
    match do_payment(world, amount, &merchant_id, &mcc, &description).await {
        Ok((from_others, from_self)) => {
            world.last_payment = Some(crate::PaymentInfo {
                amount,
                from_others,
                from_self,
            });
            world.last_error = None;
        }
        Err(kind) => {
            world.last_payment = None;
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then("the payment should succeed")]
async fn then_payment_succeed(world: &mut UiWorld) {
    assert!(
        world.last_payment.is_some(),
        "Expected payment to succeed but no payment info recorded. Error: {:?}",
        world.last_error
    );
}

#[then(regex = r"^(\d+) should come from others-pool$")]
async fn then_from_others(world: &mut UiWorld, expected: i64) {
    let payment = world
        .last_payment
        .as_ref()
        .expect("No payment result recorded");
    assert_eq!(
        payment.from_others, expected,
        "Others-pool contribution mismatch: expected {} but got {}",
        expected, payment.from_others
    );
}

#[then(regex = r"^(\d+) should come from self-pool$")]
async fn then_from_self(world: &mut UiWorld, expected: i64) {
    let payment = world
        .last_payment
        .as_ref()
        .expect("No payment result recorded");
    assert_eq!(
        payment.from_self, expected,
        "Self-pool contribution mismatch: expected {} but got {}",
        expected, payment.from_self
    );
}

#[then("the payment should be rejected as insufficient funds")]
async fn then_payment_rejected_insufficient(world: &mut UiWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected payment to be rejected as insufficient funds, but no error recorded");
    assert_eq!(
        err.kind, "insufficient_funds",
        "Expected insufficient_funds but got: {}",
        err.kind
    );
}

#[then("the payment should be rejected as invalid MCC")]
async fn then_payment_rejected_invalid_mcc(world: &mut UiWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected payment to be rejected as invalid MCC, but no error recorded");
    assert_eq!(
        err.kind, "invalid_mcc",
        "Expected invalid_mcc but got: {}",
        err.kind
    );
}

#[then("the payment should be rejected as account not active")]
async fn then_payment_rejected_not_active(world: &mut UiWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected payment to be rejected as account not active, but no error recorded");
    assert_eq!(
        err.kind, "account_not_active",
        "Expected account_not_active but got: {}",
        err.kind
    );
}
