use cucumber::{then, when};
use std::time::Duration;
use tokio::time::sleep;

use crate::UiWorld;

async fn current_page_content(world: &mut UiWorld) -> String {
    let page = world.ensure_page().await;
    page.content().await.expect("Failed to read page content")
}

/// Navigate to the all-transactions list and click the timestamp link of the
/// first row, which leads to /admin/transactions/{id}.
async fn open_first_transaction_detail(world: &mut UiWorld) {
    let url = world.url("/admin/transactions");
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to transactions list");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let js = r#"
        (() => {
            const links = Array.from(document.querySelectorAll("a[href*='/admin/transactions/']"));
            const detail = links.find(a => /\/admin\/transactions\/[0-9a-f-]{36}$/.test(a.getAttribute('href')));
            if (!detail) throw new Error('no detail link found on transactions list');
            detail.click();
        })();
    "#;
    page.evaluate(js)
        .await
        .expect("Failed to click first transaction detail link");
    sleep(Duration::from_millis(600)).await;
}

#[when("I view the most recent transaction's detail page")]
async fn when_view_most_recent_detail(world: &mut UiWorld) {
    open_first_transaction_detail(world).await;
}

#[then("the transaction detail should show the transaction ID")]
async fn then_show_transaction_id(world: &mut UiWorld) {
    let content = current_page_content(world).await;
    assert!(
        content.contains("Transaction ID:"),
        "expected `Transaction ID:` label on detail page, got snippet: {}",
        &content[..content.len().min(500)]
    );
}

#[then("the transaction detail should show the account ID")]
async fn then_show_account_id(world: &mut UiWorld) {
    let account_id = world
        .account_id
        .clone()
        .expect("No account ID on world for assertion");
    let content = current_page_content(world).await;
    assert!(
        content.contains(&account_id),
        "expected account ID `{}` on detail page",
        account_id
    );
}

#[then(regex = r#"^the transaction detail should show amount "([^"]*)"$"#)]
async fn then_show_amount(world: &mut UiWorld, expected: String) {
    let content = current_page_content(world).await;
    let needle = format!("₹{}", expected);
    assert!(
        content.contains(&needle),
        "expected amount `{}` on detail page",
        needle
    );
}

#[then(regex = r#"^the transaction detail should show type "([^"]*)"$"#)]
async fn then_show_type(world: &mut UiWorld, expected: String) {
    let content = current_page_content(world).await;
    let marker = "Type:</strong>";
    let idx = content
        .find(marker)
        .unwrap_or_else(|| panic!("`Type:` label not found on detail page"));
    let after = &content[idx + marker.len()..];
    let snippet = &after[..after.len().min(200)];
    assert!(
        snippet.contains(&expected),
        "expected type `{}` on detail page, after-Type snippet: {}",
        expected,
        snippet
    );
}
