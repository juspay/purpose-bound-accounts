use cucumber::{then, when};
use std::time::Duration;
use tokio::time::sleep;

use crate::UiWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Navigate to the withdrawal form, fill it, and submit.
/// Returns Ok(amount) if redirected (success), Err(kind) if stayed on form.
async fn do_withdrawal(world: &mut UiWorld, amount: i64) -> Result<i64, String> {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for withdrawal");
    let withdrawal_url = world.url(&format!("/admin/accounts/{}/withdrawal", account_id));

    let page = world.ensure_page().await;
    page.goto(withdrawal_url)
        .await
        .expect("Failed to navigate to withdrawal page");
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

    // Submit
    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    let succeeded = world.wait_for_redirect("/withdrawal").await;

    if succeeded {
        Ok(amount)
    } else {
        // Parse the error message from page content
        let page = world.ensure_page().await;
        let content = page
            .content()
            .await
            .expect("Failed to get page content for error");
        let kind = classify_withdrawal_error(&content);
        Err(kind)
    }
}

fn classify_withdrawal_error(content: &str) -> String {
    // Extract just the error message from the form-error element for precise matching
    if let Some(pos) = content.find(r#"id="form-error""#) {
        let snippet = &content[pos..content.len().min(pos + 500)];
        let lower = snippet.to_lowercase();
        if lower.contains("not active") || lower.contains("accountnotactive") {
            return "account_not_active".to_string();
        }
        if lower.contains("insufficient") || lower.contains("exceeds") {
            return "insufficient_funds".to_string();
        }
    }
    // Fallback
    let lower = content.to_lowercase();
    if lower.contains("not active") {
        "account_not_active".to_string()
    } else {
        "insufficient_funds".to_string()
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when(regex = r"^I withdraw (\d+)$")]
async fn when_withdraw(world: &mut UiWorld, amount: i64) {
    match do_withdrawal(world, amount).await {
        Ok(withdrawn) => {
            world.last_withdrawal_amount = Some(withdrawn);
            world.last_error = None;
        }
        Err(e) => panic!("Withdrawal failed unexpectedly: {}", e),
    }
}

#[when(regex = r"^I attempt to withdraw (\d+)$")]
async fn when_attempt_withdraw(world: &mut UiWorld, amount: i64) {
    match do_withdrawal(world, amount).await {
        Ok(withdrawn) => {
            world.last_withdrawal_amount = Some(withdrawn);
            world.last_error = None;
        }
        Err(kind) => {
            world.last_withdrawal_amount = None;
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

#[when(regex = r#"^I withdraw (\d+) from the admin UI with gateway ref "([^"]*)"$"#)]
async fn when_withdraw_with_gateway_ref(world: &mut UiWorld, amount: i64, gateway_ref: String) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID for withdrawal");
    let url = world.url(&format!("/admin/accounts/{}/withdrawal", account_id));

    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to withdrawal page");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;

    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

    let gw_input = page
        .find_element("input[name='gateway_ref']")
        .await
        .expect("Could not find gateway_ref input");
    gw_input.click().await.expect("Failed to click gateway_ref");
    gw_input
        .type_str(&gateway_ref)
        .await
        .expect("Failed to type gateway_ref");

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    let succeeded = world.wait_for_redirect("/withdrawal").await;
    assert!(
        succeeded,
        "Withdrawal form did not redirect — likely a server error"
    );
    world.last_withdrawal_amount = Some(amount);
    world.last_error = None;
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(regex = r"^the withdrawal should succeed with amount (\d+)$")]
async fn then_withdrawal_succeed(world: &mut UiWorld, expected: i64) {
    let amount = world
        .last_withdrawal_amount
        .expect("No withdrawal amount recorded");
    assert_eq!(
        amount, expected,
        "Withdrawal amount mismatch: expected {} but got {}",
        expected, amount
    );
}

#[then("the withdrawal should be rejected as insufficient funds")]
async fn then_withdrawal_rejected_insufficient(world: &mut UiWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected withdrawal to be rejected as insufficient funds, but no error recorded");
    assert_eq!(
        err.kind, "insufficient_funds",
        "Expected insufficient_funds but got: {}",
        err.kind
    );
}

#[then("the withdrawal should be rejected as account not active")]
async fn then_withdrawal_rejected_not_active(world: &mut UiWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected withdrawal to be rejected as account not active, but no error recorded");
    assert_eq!(
        err.kind, "account_not_active",
        "Expected account_not_active but got: {}",
        err.kind
    );
}
