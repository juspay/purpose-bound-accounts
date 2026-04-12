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
    sleep(Duration::from_millis(500)).await;

    // Check where we ended up
    let page = world.ensure_page().await;
    let current_url = page
        .url()
        .await
        .expect("Failed to get URL")
        .unwrap_or_default();

    let succeeded =
        current_url.contains("/admin/accounts/") && !current_url.ends_with("/withdrawal");

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
    let lower = content.to_lowercase();
    if lower.contains("accountnotactive")
        || lower.contains("account not active")
        || lower.contains("account is not active")
        || lower.contains("frozen")
        || lower.contains("closed")
    {
        "account_not_active".to_string()
    } else if lower.contains("insufficientfunds")
        || lower.contains("insufficient funds")
        || lower.contains("insufficient")
        || lower.contains("exceed")
    {
        "insufficient_funds".to_string()
    } else {
        // Check for form-error element
        if let Some(pos) = content.find(r#"id="form-error""#) {
            let snippet = &content[pos..content.len().min(pos + 200)];
            let snippet_lower = snippet.to_lowercase();
            if snippet_lower.contains("active") || snippet_lower.contains("frozen") || snippet_lower.contains("closed") {
                return "account_not_active".to_string();
            }
            if snippet_lower.contains("fund") || snippet_lower.contains("balance") || snippet_lower.contains("insufficient") {
                return "insufficient_funds".to_string();
            }
        }
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
