use cucumber::{then, when};
use std::time::Duration;
use tokio::time::sleep;

use crate::UiWorld;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn count_purpose_types_in_content(content: &str) -> usize {
    // The purpose types page has <p id="purpose-count">N purpose types available.</p>
    // Try to parse that first.
    if let Some(pos) = content.find(r#"id="purpose-count""#) {
        let after = &content[pos..];
        if let Some(gt) = after.find('>') {
            let inner = &after[gt + 1..];
            if let Some(end) = inner.find('<') {
                let text = inner[..end].trim();
                // text looks like "4 purpose types available."
                if let Some(first_word) = text.split_whitespace().next() {
                    if let Ok(n) = first_word.parse::<usize>() {
                        return n;
                    }
                }
            }
        }
    }

    // Fallback: count the number of <h3 id="purpose-..."> elements
    let mut count = 0;
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find(r#"id="purpose-"#) {
        let abs_pos = search_from + pos;
        // Make sure this isn't the purpose-count element
        let snippet = &content[abs_pos..abs_pos.min(content.len()).min(abs_pos + 30)];
        if !snippet.contains("count") {
            count += 1;
        }
        search_from = abs_pos + 1;
    }
    count
}

fn extract_mccs_count_for_purpose(content: &str, purpose_code: &str) -> usize {
    // Find the section starting with id="purpose-{code}"
    let anchor = format!(r#"id="purpose-{}""#, purpose_code);
    if let Some(pos) = content.find(&anchor) {
        // Count <tr> elements after this anchor until the next <article> or end
        let section = &content[pos..];
        let end = section[1..]
            .find("<article")
            .map(|p| p + 1)
            .unwrap_or(section.len());
        let section = &section[..end];

        // Count <tbody rows (skip the header row)
        // Each MCC row has a <tr> inside <tbody>
        let mut count = 0;
        let mut search = 0;
        while let Some(tr_pos) = section[search..].find("<tr>") {
            count += 1;
            search += tr_pos + 4;
        }
        // Subtract 1 for the header <tr> if present
        if count > 0 {
            count - 1
        } else {
            0
        }
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// When
// ---------------------------------------------------------------------------

#[when("I list all purpose types")]
async fn when_list_purpose_types(world: &mut UiWorld) {
    let url = world.url("/admin/purpose-types");
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to purpose types page");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");
    let count = count_purpose_types_in_content(&content);
    world.purpose_types_count = Some(count);
}

#[when(regex = r#"^I get the "([^"]*)" purpose type$"#)]
async fn when_get_purpose_type(world: &mut UiWorld, purpose_code: String) {
    let url = world.url("/admin/purpose-types");
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to purpose types page");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");

    // Check if the purpose type exists on the page
    let anchor = format!(r#"id="purpose-{}""#, purpose_code);
    if content.contains(&anchor) {
        let mccs_count = extract_mccs_count_for_purpose(&content, &purpose_code);
        world.last_purpose_code = Some(purpose_code);
        world.last_purpose_mccs_count = Some(mccs_count);
        world.last_error = None;
    } else {
        world.last_error = Some(crate::PbaError {
            kind: "not_found".into(),
        });
    }
}

#[when(regex = r#"^I attempt to get the "([^"]*)" purpose type$"#)]
async fn when_attempt_get_purpose_type(world: &mut UiWorld, purpose_code: String) {
    let url = world.url("/admin/purpose-types");
    let page = world.ensure_page().await;
    page.goto(url)
        .await
        .expect("Failed to navigate to purpose types page");
    sleep(Duration::from_millis(400)).await;

    let page = world.ensure_page().await;
    let content = page.content().await.expect("Failed to get page content");

    let anchor = format!(r#"id="purpose-{}""#, purpose_code);
    if content.contains(&anchor) {
        let mccs_count = extract_mccs_count_for_purpose(&content, &purpose_code);
        world.last_purpose_code = Some(purpose_code);
        world.last_purpose_mccs_count = Some(mccs_count);
        world.last_error = None;
    } else {
        world.last_error = Some(crate::PbaError {
            kind: "not_found".into(),
        });
    }
}

// ---------------------------------------------------------------------------
// Then
// ---------------------------------------------------------------------------

#[then(regex = r"^I should see at least (\d+) purpose types$")]
async fn then_see_purpose_types(world: &mut UiWorld, min_count: usize) {
    let count = world
        .purpose_types_count
        .expect("No purpose types count recorded");
    assert!(
        count >= min_count,
        "Expected at least {} purpose types, but got {}",
        min_count,
        count
    );
}

#[then(regex = r#"^the purpose code should be "([^"]*)"$"#)]
async fn then_purpose_code(world: &mut UiWorld, expected: String) {
    let actual = world
        .last_purpose_code
        .as_ref()
        .expect("No purpose code recorded");
    assert_eq!(
        actual, &expected,
        "Purpose code mismatch: expected '{}' but got '{}'",
        expected, actual
    );
}

#[then("it should have allowed MCCs")]
async fn then_should_have_mccs(world: &mut UiWorld) {
    let count = world
        .last_purpose_mccs_count
        .expect("No MCC count recorded");
    assert!(count > 0, "Expected at least 1 allowed MCC, but got 0");
}

#[then("the purpose type should not be found")]
async fn then_purpose_type_not_found(world: &mut UiWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected purpose type to not be found, but no error was recorded");
    assert_eq!(
        err.kind, "not_found",
        "Expected not_found but got: {}",
        err.kind
    );
}
