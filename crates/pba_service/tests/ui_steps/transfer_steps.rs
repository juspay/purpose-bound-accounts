use cucumber::{given, then, when};
use regex::Regex;
use std::time::Duration;
use tokio::time::sleep;

use crate::UiWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a transfer ID from a URL like `http://host/admin/transfers/{id}`.
fn extract_transfer_id_from_url(url: &str) -> Option<String> {
    let url = url.trim_end_matches('/');
    if let Some(pos) = url.rfind("/admin/transfers/") {
        let after = &url[pos + "/admin/transfers/".len()..];
        // Must be just the ID — no extra path segments (e.g. not /post or /void)
        if !after.is_empty() && !after.contains('/') {
            return Some(after.to_string());
        }
    }
    None
}

/// Extract the status text from inside `<span id="transfer-status" ...>STATUS</span>`.
fn extract_transfer_status(content: &str) -> Option<String> {
    let marker = "id=\"transfer-status\"";
    if let Some(pos) = content.find(marker) {
        let after = &content[pos + marker.len()..];
        if let Some(gt) = after.find('>') {
            let inner = &after[gt + 1..];
            if let Some(end) = inner.find("</span>") {
                return Some(inner[..end].trim().to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Given steps
// ---------------------------------------------------------------------------

#[given(regex = r#"^the normal account has balance (\d+)$"#)]
async fn given_normal_account_has_balance(world: &mut UiWorld, amount: u64) {
    let account_id = world
        .last_normal_account_id
        .clone()
        .or_else(|| world.account_id.clone())
        .expect("No normal account ID set before 'has balance' step");

    let url = world.url(&format!("/admin/normal-accounts/{}/deposit", account_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to normal deposit page");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input on normal deposit form");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button on normal deposit form");
    submit.click().await.expect("Failed to click submit");

    // Wait for redirect away from /deposit
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if current_url.contains("/admin/normal-accounts/") && !current_url.contains("/deposit") {
            return;
        }
    }
    panic!("Normal deposit did not redirect after submission");
}

// ---------------------------------------------------------------------------
// When steps
// ---------------------------------------------------------------------------

#[when("I navigate to the transfer form for the normal account")]
async fn when_navigate_to_transfer_form(world: &mut UiWorld) {
    let account_id = world
        .last_normal_account_id
        .clone()
        .expect("No normal account ID for transfer form navigation");
    let url = world.url(&format!("/admin/normal-accounts/{}/transfer", account_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transfer form");
    sleep(Duration::from_millis(400)).await;
}

#[when(regex = r#"^I select the PB account as destination and submit a transfer of (\d+) paisa$"#)]
async fn when_submit_immediate_transfer(world: &mut UiWorld, amount: u64) {
    let page = world.ensure_page().await;

    // Select the first (only) real PB account option in the dropdown using JS
    let js = r#"
        const sel = document.querySelector("select[name='destination_pb_account_id']");
        const opts = Array.from(sel.options).filter(o => o.value !== '');
        if (opts.length > 0) { sel.value = opts[0].value; }
    "#;
    page.evaluate(js.to_string())
        .await
        .expect("Failed to select PB account in dropdown");

    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    // Wait for redirect to /admin/transfers/{id}
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if let Some(transfer_id) = extract_transfer_id_from_url(&current_url) {
            world.last_transfer_id = Some(transfer_id);
            return;
        }
    }

    let page = world.ensure_page().await;
    let current_url = page.url().await.unwrap_or_default().unwrap_or_default();
    let content = page.content().await.unwrap_or_default();
    panic!(
        "Transfer did not redirect to detail page. URL: {}. Content: {}",
        current_url,
        &content[..content.len().min(800)]
    );
}

#[when(
    regex = r#"^I select the PB account as destination, set amount (\d+), mark as pending, and submit$"#
)]
async fn when_submit_pending_transfer(world: &mut UiWorld, amount: u64) {
    let page = world.ensure_page().await;

    // Select the first (only) real PB account option
    let js = r#"
        const sel = document.querySelector("select[name='destination_pb_account_id']");
        const opts = Array.from(sel.options).filter(o => o.value !== '');
        if (opts.length > 0) { sel.value = opts[0].value; }
    "#;
    page.evaluate(js.to_string())
        .await
        .expect("Failed to select PB account in dropdown");

    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

    // Check the pending checkbox
    let pending_cb = page
        .find_element("input[name='pending']")
        .await
        .expect("Could not find pending checkbox");
    pending_cb.click().await.expect("Failed to click pending");

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    // Wait for redirect to /admin/transfers/{id}
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if let Some(transfer_id) = extract_transfer_id_from_url(&current_url) {
            world.last_transfer_id = Some(transfer_id);
            return;
        }
    }

    let page = world.ensure_page().await;
    let current_url = page.url().await.unwrap_or_default().unwrap_or_default();
    let content = page.content().await.unwrap_or_default();
    panic!(
        "Pending transfer did not redirect to detail page. URL: {}. Content: {}",
        current_url,
        &content[..content.len().min(800)]
    );
}

#[when("I click the post button on the transfer detail page")]
async fn when_click_post_button(world: &mut UiWorld) {
    let transfer_id = world
        .last_transfer_id
        .clone()
        .expect("No transfer ID set before clicking Post");

    let url = world.url(&format!("/admin/transfers/{}", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transfer detail");
    sleep(Duration::from_millis(400)).await;

    // Find and click the Post button
    let js = r#"
        const forms = Array.from(document.querySelectorAll('form'));
        const postForm = forms.find(f => f.action && f.action.endsWith('/post'));
        if (postForm) { postForm.submit(); } else { throw new Error('Post form not found'); }
    "#;
    let page = world.ensure_page().await;
    page.evaluate(js.to_string())
        .await
        .expect("Failed to click post button");

    // Wait for the page to reload and status to change
    sleep(Duration::from_millis(800)).await;
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if extract_transfer_id_from_url(&current_url).is_some() {
            return;
        }
    }
}

#[when("I click the void button on the transfer detail page")]
async fn when_click_void_button(world: &mut UiWorld) {
    let transfer_id = world
        .last_transfer_id
        .clone()
        .expect("No transfer ID set before clicking Void");

    let url = world.url(&format!("/admin/transfers/{}", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transfer detail");
    sleep(Duration::from_millis(400)).await;

    // Find and click the Void button
    let js = r#"
        const forms = Array.from(document.querySelectorAll('form'));
        const voidForm = forms.find(f => f.action && f.action.endsWith('/void'));
        if (voidForm) { voidForm.submit(); } else { throw new Error('Void form not found'); }
    "#;
    let page = world.ensure_page().await;
    page.evaluate(js.to_string())
        .await
        .expect("Failed to click void button");

    // Wait for the page to reload and status to change
    sleep(Duration::from_millis(800)).await;
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if extract_transfer_id_from_url(&current_url).is_some() {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Then steps
// ---------------------------------------------------------------------------

#[then("I land on the transfer detail page")]
async fn then_land_on_transfer_detail(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let current_url = page
        .url()
        .await
        .expect("Failed to get URL")
        .unwrap_or_default();
    let transfer_id = extract_transfer_id_from_url(&current_url);
    assert!(
        transfer_id.is_some(),
        "Expected to be on /admin/transfers/{{uuid}} but URL is: {}",
        current_url
    );
    if let Some(id) = transfer_id {
        world.last_transfer_id = Some(id);
    }
}

#[then(regex = r#"^the transfer detail page shows source account holder "([^"]*)"$"#)]
async fn then_transfer_shows_source_holder(world: &mut UiWorld, expected: String) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    assert!(
        content.contains(&expected),
        "Expected source holder '{}' to appear on transfer detail page. Content snippet: {}",
        expected,
        &content[..content.len().min(2000)]
    );
}

#[then(regex = r#"^the transfer detail page shows status "([^"]*)"$"#)]
async fn then_transfer_shows_status(world: &mut UiWorld, expected: String) {
    // Reload the page to get fresh status
    let transfer_id = world.last_transfer_id.clone().expect("No transfer ID set");
    let url = world.url(&format!("/admin/transfers/{}", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to reload transfer detail");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let status = extract_transfer_status(&content);
    assert_eq!(
        status.as_deref(),
        Some(expected.as_str()),
        "Expected transfer status '{}' but found '{:?}'. Page snippet: {}",
        expected,
        status,
        &content[..content.len().min(2000)]
    );
}

// ── Reversal UI steps ────────────────────────────────────────────────────────

#[then("the Reverse button is visible on the transfer detail page")]
async fn then_reverse_button_visible(world: &mut UiWorld) {
    let transfer_id = world.last_transfer_id.clone().expect("No transfer ID set");
    let url = world.url(&format!("/admin/transfers/{}", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transfer detail");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let expected_href = format!("/admin/transfers/{}/reverse", transfer_id);
    assert!(
        content.contains(&expected_href),
        "Expected Reverse button (href containing '{}') on transfer detail. Page snippet: {}",
        expected_href,
        &content[..content.len().min(2000)]
    );
}

#[then("the Reverse button is not visible on the transfer detail page")]
async fn then_reverse_button_not_visible(world: &mut UiWorld) {
    let transfer_id = world.last_transfer_id.clone().expect("No transfer ID set");
    let url = world.url(&format!("/admin/transfers/{}", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transfer detail");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let unexpected_href = format!("/admin/transfers/{}/reverse", transfer_id);
    assert!(
        !content.contains(&unexpected_href),
        "Expected Reverse button to be ABSENT, but found href '{}' on detail page",
        unexpected_href
    );
}

#[when(regex = r#"^I click the Reverse button and submit the reverse form with amount (\d+)$"#)]
async fn when_click_reverse_and_submit(world: &mut UiWorld, amount_paisa: u64) {
    let transfer_id = world.last_transfer_id.clone().expect("No transfer ID set");

    // Navigate directly to the reverse form
    let url = world.url(&format!("/admin/transfers/{}/reverse", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to reverse form");
    sleep(Duration::from_millis(400)).await;

    // Clear the pre-filled amount_paisa input and type the desired value
    let page = world.ensure_page().await;
    let clear_js = "document.querySelector(\"input[name='amount_paisa']\").value = '';";
    page.evaluate(clear_js.to_string()).await.ok();

    let amount_input = page
        .find_element("input[name='amount_paisa']")
        .await
        .expect("Could not find amount_paisa input on reverse form");
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
        .expect("Could not find submit button on reverse form");
    submit.click().await.expect("Failed to click submit");

    // On success the server redirects to /admin/transfers/{id}.
    // On failure it re-renders the form with an error.
    sleep(Duration::from_millis(800)).await;
}

#[then(regex = r#"^the transfer detail page shows a "Reversed by" link$"#)]
async fn then_reversed_by_link_visible(world: &mut UiWorld) {
    let transfer_id = world.last_transfer_id.clone().expect("No transfer ID set");
    let url = world.url(&format!("/admin/transfers/{}", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transfer detail");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    assert!(
        content.contains("Reversed by") || content.contains("has been reversed"),
        "Expected 'Reversed by' / reversal section on detail page. Page snippet: {}",
        &content[..content.len().min(2000)]
    );

    // Capture the reversal transaction ID from the /admin/transactions/{id} link.
    let re = Regex::new(r"/admin/transactions/([0-9a-f-]{36})").unwrap();
    if let Some(cap) = re.captures(&content) {
        world.last_reversal_id = Some(cap[1].to_string());
    }
}

#[when(regex = r#"^I follow the "Reversed by" link$"#)]
async fn when_follow_reversed_by_link(world: &mut UiWorld) {
    let reversal_id = world
        .last_reversal_id
        .clone()
        .expect("No reversal ID captured (call 'shows a \"Reversed by\" link' first)");
    // The reversal row's ID is also a valid transfer_id accepted by /admin/transfers/{id}.
    // We navigate there so the transfer_detail template is used, which has the
    // Reverse button and "is_reversal" checks.
    world.last_transfer_id = Some(reversal_id.clone());
    let url = world.url(&format!("/admin/transfers/{}", reversal_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to reversal transfer detail");
    sleep(Duration::from_millis(400)).await;
}

#[then("the transfer detail page shows that this row is a reversal")]
async fn then_row_is_reversal(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    assert!(
        content.contains("This row is a reversal"),
        "Expected 'This row is a reversal' notice. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}

#[then("the reverse form shows an InsufficientFunds error")]
async fn then_reverse_form_shows_insufficient_funds(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    assert!(
        content.contains("InsufficientFunds") || content.contains("Insufficient funds"),
        "Expected InsufficientFunds error on reverse form. Page snippet: {}",
        &content[..content.len().min(2500)]
    );
}

/// Combined step: navigate to the normal account's transfer form, select the
/// first PB account in the dropdown, fill the amount, and submit. Restores
/// `world.account_id` to the PB account id stored in it before the normal
/// account was created (the PB account was created first in refund scenarios).
///
/// This step assumes the dropdown on the transfer form contains exactly one
/// real PB account for the holder.
#[when(regex = r#"^I transfer (\d+) paisa from the normal account to the PB account$"#)]
async fn when_transfer_to_pb_account(world: &mut UiWorld, amount: u64) {
    let normal_account_id = world
        .last_normal_account_id
        .clone()
        .expect("No normal account ID for transfer step");

    let url = world.url(&format!(
        "/admin/normal-accounts/{}/transfer",
        normal_account_id
    ));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transfer form");
    sleep(Duration::from_millis(400)).await;

    // Select the first (only) real PB account option in the dropdown using JS
    let page = world.ensure_page().await;
    let js = r#"
        const sel = document.querySelector("select[name='destination_pb_account_id']");
        const opts = Array.from(sel.options).filter(o => o.value !== '');
        if (opts.length > 0) { sel.value = opts[0].value; }
    "#;
    page.evaluate(js.to_string())
        .await
        .expect("Failed to select PB account in dropdown");

    let amount_input = page
        .find_element("input[name='amount']")
        .await
        .expect("Could not find amount input");
    amount_input.click().await.expect("Failed to click amount");
    amount_input
        .type_str(&amount.to_string())
        .await
        .expect("Failed to type amount");

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button");
    submit.click().await.expect("Failed to click submit");

    // Wait for redirect to /admin/transfers/{id}
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if let Some(transfer_id) = extract_transfer_id_from_url(&current_url) {
            world.last_transfer_id = Some(transfer_id);
            return;
        }
    }

    let page = world.ensure_page().await;
    let current_url = page.url().await.unwrap_or_default().unwrap_or_default();
    let content = page.content().await.unwrap_or_default();
    panic!(
        "Transfer did not redirect to detail page. URL: {}. Content: {}",
        current_url,
        &content[..content.len().min(800)]
    );
}

#[when(regex = r#"^the PB account spends (\d+) paisa on merchant "([^"]*)" with MCC "([^"]*)"$"#)]
async fn when_pb_account_spends(world: &mut UiWorld, amount: u64, merchant: String, mcc: String) {
    let account_id = world
        .account_id
        .clone()
        .expect("No PB account ID set before spend step");

    let url = world.url(&format!("/admin/accounts/{}/payment", account_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to PB payment form");
    sleep(Duration::from_millis(400)).await;

    // Fill fields via JS (avoids interaction issues with pre-filled values)
    let page = world.ensure_page().await;
    let js = format!(
        r#"
        document.querySelector("input[name='amount']").value = '{amount}';
        document.querySelector("input[name='merchant_id']").value = '{merchant}';
        document.querySelector("input[name='merchant_mcc']").value = '{mcc}';
        document.querySelector("input[name='description']").value = 'rev-ui-spend';
        "#,
        amount = amount,
        merchant = merchant.replace('\'', "\\'"),
        mcc = mcc.replace('\'', "\\'"),
    );
    page.evaluate(js).await.ok();

    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button on payment form");
    submit.click().await.expect("Failed to click submit");
    sleep(Duration::from_millis(800)).await;
}

// ---------------------------------------------------------------------------
// Two-phase transfer reversal UI steps
// ---------------------------------------------------------------------------

/// Navigate to the reverse form for the currently tracked transfer.
#[when("I open the reverse form for that transfer")]
async fn when_open_reverse_form(world: &mut UiWorld) {
    let transfer_id = world
        .last_transfer_id
        .clone()
        .expect("No transfer ID — land on transfer detail first");
    let url = world.url(&format!("/admin/transfers/{}/reverse", transfer_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to reverse form");
    sleep(Duration::from_millis(400)).await;
}

/// Submit the reverse form with whatever values are currently set (caller
/// selects mode and fills amount before calling this step).
///
/// After success the server redirects to `/admin/transfers/{original_id}`.
/// We then extract the reversal's transfer ID from the "Reversed by" section
/// on that page and update `last_transfer_id` to point at the reversal so
/// subsequent status-check steps operate on the right row.
#[when("I submit the reverse form")]
async fn when_submit_reverse_form(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let submit = page
        .find_element("button[type='submit']")
        .await
        .expect("Could not find submit button on reverse form");
    submit.click().await.expect("Failed to click submit");
    sleep(Duration::from_millis(1000)).await;

    // Wait for the redirect to the original transfer detail page.
    for _ in 0..20 {
        sleep(Duration::from_millis(300)).await;
        let page = world.ensure_page().await;
        let current_url = page
            .url()
            .await
            .expect("Failed to get URL")
            .unwrap_or_default();
        if extract_transfer_id_from_url(&current_url).is_some() {
            break;
        }
    }

    // Save the original transfer ID in last_reversal_id so we can return to
    // it later (e.g. to verify re-reversal is unlocked after voiding).
    let original_id = world.last_transfer_id.clone().unwrap_or_default();
    world.last_reversal_id = Some(original_id.clone());

    // The original transfer page now shows a "Reversed by" link pointing to
    // the reversal's transaction ID via /admin/transactions/{rid}.
    // Extract it and store in last_transfer_id so status-check steps work.
    let content = world
        .ensure_page()
        .await
        .content()
        .await
        .expect("Failed to read page after reverse form submit");

    let re_txn = Regex::new(r"/admin/transactions/([0-9a-f-]{36})").unwrap();
    let reversal_id = re_txn
        .captures_iter(&content)
        .map(|c| c[1].to_string())
        .find(|id| id != &original_id);

    if let Some(rid) = reversal_id {
        world.last_transfer_id = Some(rid);
    } else {
        // Fall back: check /admin/transfers/{uuid} links
        let re_tr = Regex::new(r"/admin/transfers/([0-9a-f-]{36})").unwrap();
        let reversal_tr = re_tr
            .captures_iter(&content)
            .map(|c| c[1].to_string())
            .find(|id| id != &original_id);
        if let Some(rid) = reversal_tr {
            world.last_transfer_id = Some(rid);
        } else {
            let page = world.ensure_page().await;
            let url = page.url().await.unwrap_or_default().unwrap_or_default();
            panic!(
                "Could not find reversal transfer ID after submitting reverse form. \
                 Original transfer: {}. Current URL: {}. Content snippet: {}",
                original_id,
                url,
                &content[..content.len().min(800)]
            );
        }
    }
}

/// Assert the Post button (form action ending `/post`) is visible on the
/// current transfer detail page.
#[then("the Post transfer button is visible on the detail page")]
async fn then_post_transfer_button_visible(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains("/post"),
        "Expected Post transfer button to be visible. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}

/// Assert the Void button (form action ending `/void`) is visible.
#[then("the Void transfer button is visible on the detail page")]
async fn then_void_transfer_button_visible(world: &mut UiWorld) {
    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    assert!(
        content.contains("/void"),
        "Expected Void transfer button to be visible. Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}

/// After voiding a pending reversal, navigate back to the original transfer
/// and assert its Reverse button is present again (can_reverse is true).
#[then("the original transfer is reversible again")]
async fn then_original_transfer_reversible(world: &mut UiWorld) {
    // last_reversal_id was set by when_submit_reverse_form to the original
    // transfer ID before it was overwritten by the reversal ID.
    let original_id = world
        .last_reversal_id
        .clone()
        .expect("No original transfer ID stored — was when_submit_reverse_form called?");

    let url = world.url(&format!("/admin/transfers/{}", original_id));
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to original transfer detail");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to read page content");
    let expected_href = format!("/admin/transfers/{}/reverse", original_id);
    assert!(
        content.contains(&expected_href),
        "Expected Reverse button on original transfer after voiding the pending reversal. \
         Page snippet: {}",
        &content[..content.len().min(2000)]
    );
}
