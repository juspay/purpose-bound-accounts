use cucumber::{given, then, when};
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
