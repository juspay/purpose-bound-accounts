use cucumber::{given, then, when};
use regex::Regex;
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

    let succeeded = world.wait_for_redirect("/payment").await;

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

async fn do_payment_with_gateway_ref(
    world: &mut UiWorld,
    amount: i64,
    merchant_id: &str,
    merchant_mcc: &str,
    description: &str,
    gateway_ref: &str,
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

    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

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

    let gw_input = page
        .find_element("input[name='gateway_ref']")
        .await
        .expect("Could not find gateway_ref input");
    gw_input.click().await.expect("Failed to click gateway_ref");
    gw_input
        .type_str(gateway_ref)
        .await
        .expect("Failed to type gateway_ref");

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    let succeeded = world.wait_for_redirect("/payment").await;

    if succeeded {
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
        let page = world.ensure_page().await;
        let content = page
            .content()
            .await
            .expect("Failed to get page content for error");
        Err(classify_error(&content))
    }
}

#[when(
    regex = r#"^I pay (\d+) to merchant "([^"]*)" with MCC "([^"]*)" described as "([^"]*)" with gateway ref "([^"]*)"$"#
)]
async fn when_pay_with_gateway_ref(
    world: &mut UiWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
    description: String,
    gateway_ref: String,
) {
    match do_payment_with_gateway_ref(
        world,
        amount,
        &merchant_id,
        &mcc,
        &description,
        &gateway_ref,
    )
    .await
    {
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

// ---------------------------------------------------------------------------
// Refund UI helpers
// ---------------------------------------------------------------------------

/// Navigate to the account's PB-account transfers fragment and extract the most
/// recent payment txn id from the rendered table. Stores it in
/// world.last_payment_id and navigates to /admin/transactions/{id}.
///
/// We fetch /admin/accounts/{id}/transfers directly (the HTMX fragment endpoint
/// returns real HTML with /admin/transactions/{uuid} links ordered newest-first)
/// rather than the main account detail page, which lazy-loads the table via HTMX
/// and would have no transaction links in its initial HTML.
async fn goto_last_payment_detail(world: &mut UiWorld) {
    // If we've already captured the payment id earlier in the scenario, reuse it.
    // After a refund, the most-recent transaction on the account is the refund row,
    // not the original payment — re-discovery would clobber the payment id with a
    // refund-row id.
    let payment_id = match world.last_payment_id.clone() {
        Some(id) => id,
        None => {
            let account_id = world.account_id.clone().expect("No account ID");
            let fragment_url = world.url(&format!("/admin/accounts/{}/transfers", account_id));
            let page = world.ensure_page().await;
            page.goto(fragment_url)
                .await
                .expect("Failed to navigate to transfers fragment");
            sleep(Duration::from_millis(400)).await;
            let content = world
                .ensure_page()
                .await
                .content()
                .await
                .expect("Failed to read transfers fragment content");
            let re = Regex::new(r"/admin/transactions/([0-9a-f-]{36})").unwrap();
            let id = re
                .captures(&content)
                .map(|c| c[1].to_string())
                .expect("No transaction link found on account detail page");
            world.last_payment_id = Some(id.clone());
            id
        }
    };

    let detail_url = world.url(&format!("/admin/transactions/{}", payment_id));
    let page = world.ensure_page().await;
    page.goto(detail_url)
        .await
        .expect("Failed to navigate to txn detail");
    sleep(Duration::from_millis(400)).await;
}

async fn goto_last_refund_detail(world: &mut UiWorld) {
    let refund_id = world.last_refund_id.clone().expect("No refund ID captured");
    let url = world.url(&format!("/admin/transactions/{}", refund_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to refund detail");
    sleep(Duration::from_millis(400)).await;
}

// ---------------------------------------------------------------------------
// Refund when steps
// ---------------------------------------------------------------------------

#[when("I visit the transaction detail page for the last payment")]
async fn when_visit_last_payment_detail(world: &mut UiWorld) {
    goto_last_payment_detail(world).await;
}

#[when("I visit the transaction detail page for the last refund")]
async fn when_visit_last_refund_detail(world: &mut UiWorld) {
    goto_last_refund_detail(world).await;
}

#[when(regex = r#"^I click the Refund button and submit the refund form with amount (\d+)$"#)]
async fn when_click_refund_and_submit(world: &mut UiWorld, amount_paisa: u64) {
    let account_id = world.account_id.clone().expect("No account ID");
    let payment_id = world.last_payment_id.clone().expect(
        "No payment ID — call 'I visit the transaction detail page for the last payment' first",
    );

    let url = world.url(&format!(
        "/admin/accounts/{}/payments/{}/refund",
        account_id, payment_id
    ));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to refund form");
    sleep(Duration::from_millis(400)).await;

    // Clear pre-filled amount and remove optional timeout_seconds to avoid
    // "cannot parse integer from empty string" deserialization error.
    let page = world.ensure_page().await;
    let prep_js = r#"
    var inp = document.querySelector("input[name='amount_paisa']");
    if (inp) {
        inp.removeAttribute('max');
        inp.value = '';
    }
    var ts = document.querySelector("input[name='timeout_seconds']");
    if (ts) ts.remove();
"#;
    let _ = page.evaluate(prep_js.to_string()).await;

    let amount_input = page
        .find_element("input[name='amount_paisa']")
        .await
        .expect("Could not find amount_paisa input on refund form");
    amount_input
        .click()
        .await
        .expect("Failed to click amount_paisa");
    amount_input
        .type_str(&amount_paisa.to_string())
        .await
        .expect("Failed to type amount_paisa");

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button on refund form");
    submit.click().await.expect("Failed to click submit");

    sleep(Duration::from_millis(800)).await;

    // On success, the server redirects to /admin/transactions/{payment_id} and the
    // refund-history block on that page contains a link to the new refund correlation_id.
    // Capture the refund correlation_id from any /admin/transactions/{uuid} link on the
    // post-submit page that isn't the payment id itself.
    let page = world.ensure_page().await;
    let content = page
        .content()
        .await
        .expect("Failed to read content after refund submit");
    let re = Regex::new(r"/admin/transactions/([0-9a-f-]{36})").unwrap();
    for cap in re.captures_iter(&content) {
        let candidate = cap[1].to_string();
        if Some(candidate.clone()) != world.last_payment_id {
            world.last_refund_id = Some(candidate);
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Refund then steps
// ---------------------------------------------------------------------------

#[then("the page shows a Refund button")]
async fn then_refund_button_visible(world: &mut UiWorld) {
    let account_id = world.account_id.clone().expect("No account ID");
    let payment_id = world.last_payment_id.clone().expect("No payment ID");
    let expected_href = format!(
        "/admin/accounts/{}/payments/{}/refund",
        account_id, payment_id
    );
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains(&expected_href),
        "Expected Refund button (href containing '{}'). Page snippet: {}",
        expected_href,
        &content[..content.len().min(2000)]
    );
}

#[then("the page does not show a Refund button")]
async fn then_refund_button_absent(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    // Look for any /refund href under /admin/accounts/.../payments/.../refund
    let re = Regex::new(r"/admin/accounts/[0-9a-f-]{36}/payments/[0-9a-f-]{36}/refund").unwrap();
    assert!(
        !re.is_match(&content),
        "Expected Refund button to be ABSENT, but found one. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}

#[then(regex = r#"^the page shows "([^"]*)"$"#)]
async fn then_page_shows_text(world: &mut UiWorld, text: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains(&text),
        "Expected page to contain '{}'. Page snippet: {}",
        text,
        &content[..content.len().min(2000)]
    );
}

#[then(regex = r#"^the page shows a refund history entry for (\d+) paisa total$"#)]
async fn then_refund_history_entry(world: &mut UiWorld, paisa: i64) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains("Refund history"),
        "Expected 'Refund history' section. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
    let display = format!("{}.{:02}", paisa / 100, paisa % 100);
    let needle = format!("<td>₹{}</td>", display);
    assert!(
        content.contains(&needle),
        "Expected refund history to show <td>₹{}</td> total. Page snippet: {}",
        display,
        &content[..content.len().min(2000)]
    );
}

#[then(regex = r#"^the refund form shows an error containing "([^"]*)"$"#)]
async fn then_refund_form_shows_error(world: &mut UiWorld, needle: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains("Refund failed") && content.contains(&needle),
        "Expected refund form error containing '{}'. Page snippet: {}",
        needle,
        &content[..content.len().min(3000)]
    );
}

#[then(regex = r#"^the remaining refundable on the payment page shows (\d+) paisa$"#)]
async fn then_remaining_refundable_display(world: &mut UiWorld, paisa: i64) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    let display = format!("₹{}.{:02} remaining", paisa / 100, paisa % 100);
    assert!(
        content.contains(&display),
        "Expected payment detail to show '{}'. Page snippet: {}",
        display,
        &content[..content.len().min(2000)]
    );
}

// ---------------------------------------------------------------------------
// Two-phase refund UI steps
// ---------------------------------------------------------------------------

/// Open the refund form for the currently active payment page (reuses the
/// cached `last_payment_id`).
#[when("I open the refund form for that payment")]
async fn when_open_refund_form(world: &mut UiWorld) {
    let account_id = world.account_id.clone().expect("No account ID");
    let payment_id = world
        .last_payment_id
        .clone()
        .expect("No payment ID — visit the payment detail page first");
    let url = world.url(&format!(
        "/admin/accounts/{}/payments/{}/refund",
        account_id, payment_id
    ));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to refund form");
    sleep(Duration::from_millis(400)).await;
}

/// Click the "Hold as pending" radio on the refund/reverse form.
#[when(regex = r#"^I select "Hold as pending" mode$"#)]
async fn when_select_hold_as_pending(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let js = "document.querySelector(\"input[name='mode'][value='pending']\").click();";
    page.evaluate(js.to_string())
        .await
        .expect("Failed to select 'Hold as pending' radio");
}

/// Clear and fill the amount_paisa input with the given value.
#[when(regex = r#"^I enter (\d+) as the refund amount paisa$"#)]
async fn when_enter_refund_amount(world: &mut UiWorld, amount: u64) {
    let page = world.ensure_page().await;
    let clear_js = "document.querySelector(\"input[name='amount_paisa']\").value = '';";
    page.evaluate(clear_js.to_string()).await.ok();
    let input = page
        .find_element("input[name='amount_paisa']")
        .await
        .expect("Could not find amount_paisa input");
    input.click().await.expect("Failed to click amount_paisa");
    input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount_paisa");
}

/// Submit the currently open refund form and wait for the redirect to the
/// refund detail page. Captures `last_refund_id` from the redirected page URL.
#[when("I submit the refund form")]
async fn when_submit_refund_form(world: &mut UiWorld) {
    // Remove the optional timeout_seconds input to avoid a
    // "cannot parse integer from empty string" deserialization error.
    let page = world.ensure_page().await;
    let clear_js =
        "var ts = document.querySelector(\"input[name='timeout_seconds']\"); if (ts) ts.remove();";
    page.evaluate(clear_js.to_string()).await.ok();

    let page = world.ensure_page().await;
    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button on refund form");
    submit.click().await.expect("Failed to click submit");
    sleep(Duration::from_millis(1000)).await;

    // Capture the refund transaction ID from the post-redirect URL.
    let current_url = world
        .ensure_page()
        .await
        .url()
        .await
        .expect("Failed to get URL")
        .unwrap_or_default();
    let saved_payment_id = world.last_payment_id.clone();
    // After a pending-mode refund the server redirects to
    // /admin/transactions/{refund_correlation_id}.
    let re = Regex::new(r"/admin/transactions/([0-9a-f-]{36})").unwrap();
    if let Some(cap) = re.captures(&current_url) {
        let id = cap[1].to_string();
        if Some(id.clone()) != saved_payment_id {
            world.last_refund_id = Some(id);
        }
    }
    // Also try to capture from page content links
    if world.last_refund_id.is_none() {
        let content = world
            .ensure_page()
            .await
            .content()
            .await
            .expect("Failed to read page content after refund submit");
        let re_content = Regex::new(r"/admin/transactions/([0-9a-f-]{36})").unwrap();
        for cap in re_content.captures_iter(&content) {
            let candidate = cap[1].to_string();
            if Some(candidate.clone()) != saved_payment_id {
                world.last_refund_id = Some(candidate);
                break;
            }
        }
    }
}

/// Navigate to the refund transaction detail page and assert the status badge.
#[then(regex = r#"^the refund detail page shows status "([^"]*)"$"#)]
async fn then_refund_detail_shows_status(world: &mut UiWorld, expected: String) {
    let refund_id = world
        .last_refund_id
        .clone()
        .expect("No refund ID captured — submit the refund form first");
    let url = world.url(&format!("/admin/transactions/{}", refund_id));
    let page = world.ensure_page().await;
    page.goto(url.clone())
        .await
        .expect("Failed to navigate to refund detail");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    // The template renders the status in a stat-value span: `{{ status }}`
    // and in the kv grid. A simple contains check on the page is sufficient.
    assert!(
        content
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase()),
        "Expected refund detail page to show status '{}'. Page snippet: {}",
        expected,
        &content[..content.len().min(2000)]
    );
}

/// Assert that the "Post refund" button (a submit button in a form ending in
/// `/refunds/{id}/post`) is visible on the current page.
#[then("the Post refund button is visible")]
async fn then_post_refund_button_visible(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains("/refunds/") && content.contains("/post"),
        "Expected Post refund button to be visible. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}

/// Assert that the "Void refund" button is visible.
#[then("the Void refund button is visible")]
async fn then_void_refund_button_visible(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains("/refunds/") && content.contains("/void"),
        "Expected Void refund button to be visible. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}

/// Click the "Post refund" button on the refund detail page.
/// Navigates to the refund detail first to ensure we are on the right page.
#[when("I click the Post refund button on the refund detail page")]
async fn when_click_post_refund(world: &mut UiWorld) {
    let refund_id = world
        .last_refund_id
        .clone()
        .expect("No refund ID — submit the refund form first");
    let url = world.url(&format!("/admin/transactions/{}", refund_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to refund detail for Post");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let js = r#"
        const forms = Array.from(document.querySelectorAll('form'));
        const f = forms.find(f => f.action && f.action.includes('/refunds/') && f.action.endsWith('/post'));
        if (f) { f.submit(); } else { throw new Error('Post refund form not found'); }
    "#;
    page.evaluate(js.to_string())
        .await
        .expect("Failed to submit Post refund form");
    sleep(Duration::from_millis(800)).await;
    // Wait for redirect back to the refund detail
    for _ in 0..20 {
        sleep(Duration::from_millis(300)).await;
        let page = world.ensure_page().await;
        let curr = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if curr.contains("/admin/transactions/") {
            return;
        }
    }
}

/// Click the "Void refund" button on the refund detail page.
/// Navigates to the refund detail first to ensure we are on the right page.
#[when("I click the Void refund button on the refund detail page")]
async fn when_click_void_refund(world: &mut UiWorld) {
    let refund_id = world
        .last_refund_id
        .clone()
        .expect("No refund ID — submit the refund form first");
    let url = world.url(&format!("/admin/transactions/{}", refund_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to refund detail for Void");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let js = r#"
        const forms = Array.from(document.querySelectorAll('form'));
        const f = forms.find(f => f.action && f.action.includes('/refunds/') && f.action.endsWith('/void'));
        if (f) { f.submit(); } else { throw new Error('Void refund form not found'); }
    "#;
    page.evaluate(js.to_string())
        .await
        .expect("Failed to submit Void refund form");
    sleep(Duration::from_millis(800)).await;
    for _ in 0..20 {
        sleep(Duration::from_millis(300)).await;
        let page = world.ensure_page().await;
        let curr = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if curr.contains("/admin/transactions/") {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Two-phase refund Given steps for scenario setup
// ---------------------------------------------------------------------------

/// "a posted PB->merchant payment exists" — full setup: create accounts,
/// fund, and make a payment. Uses unique holder ids baked into the scenario.
/// The feature scenarios provide individual account setup steps, so this
/// compound Given is not needed; the individual steps are used instead.

// ---------------------------------------------------------------------------
// History assertions (for @todo scenario)
// ---------------------------------------------------------------------------

#[then("the refund history shows two entries")]
async fn then_refund_history_two_entries(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains("Refund history"),
        "Expected 'Refund history' section on page"
    );
    // Count <tr> rows inside the refund history table (subtract 1 for header)
    let section_start = content.find("Refund history").unwrap_or(0);
    let section = &content[section_start..];
    let rows = section.matches("<tr>").count().saturating_sub(1);
    assert_eq!(rows, 2, "Expected 2 refund history rows but found {}", rows);
}

#[then("the voided entry is rendered with strike-through")]
async fn then_voided_entry_strike_through(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    // Template renders voided cells as <s>₹X.XX</s>
    assert!(
        content.contains("<s>₹"),
        "Expected strike-through <s> tag for voided refund entry. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}

// ---------------------------------------------------------------------------
// Unused given stub (keeps compiler happy for @todo scenario)
// ---------------------------------------------------------------------------

#[given("a payment has a voided pending refund and a settled refund")]
async fn given_payment_with_voided_and_settled_refund(_world: &mut UiWorld) {
    // @todo: implement complex setup for history scenario
    panic!("@todo scenario not yet implemented — tag with @todo to skip");
}
