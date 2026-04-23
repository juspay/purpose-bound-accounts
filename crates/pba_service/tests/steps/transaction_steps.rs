use cucumber::{then, when};

use crate::PbaWorld;

#[when("I list all transactions")]
async fn list_all_transactions(world: &mut PbaWorld) {
    let result = world
        .client
        .list_all_transactions()
        .send()
        .await
        .expect("Failed to list all transactions");

    world.all_transactions_total = Some(result.total());
    world.all_transactions_count = Some(result.transactions().len());
    world.all_transactions_types = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.r#type().to_string())
            .collect(),
    );
    world.all_transactions_account_ids = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.account_id().to_string())
            .collect(),
    );
    world.all_transactions_funding_types = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.funding_type().map(|s| s.to_string()))
            .collect(),
    );
}

#[when(regex = r"^I list all transactions with limit (\d+)$")]
async fn list_all_transactions_with_limit(world: &mut PbaWorld, limit: i64) {
    let result = world
        .client
        .list_all_transactions()
        .limit(limit)
        .send()
        .await
        .expect("Failed to list all transactions with limit");

    world.all_transactions_total = Some(result.total());
    world.all_transactions_count = Some(result.transactions().len());
    world.all_transactions_types = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.r#type().to_string())
            .collect(),
    );
    world.all_transactions_account_ids = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.account_id().to_string())
            .collect(),
    );
    world.all_transactions_funding_types = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.funding_type().map(|s| s.to_string()))
            .collect(),
    );
}

#[then(regex = r"^the total transaction count should be (\d+)$")]
async fn total_transaction_count(world: &mut PbaWorld, expected: i64) {
    let total = world
        .all_transactions_total
        .expect("No all-transactions result");
    assert_eq!(total, expected, "Expected total {expected}, got {total}");
}

#[then(regex = r"^the total transaction count should be at least (\d+)$")]
async fn total_transaction_count_at_least(world: &mut PbaWorld, min: i64) {
    let total = world
        .all_transactions_total
        .expect("No all-transactions result");
    assert!(
        total >= min,
        "Expected at least {min} transactions, got {total}"
    );
}

#[then("the transactions list should be empty")]
async fn transactions_list_empty(world: &mut PbaWorld) {
    let count = world
        .all_transactions_count
        .expect("No all-transactions result");
    assert_eq!(count, 0, "Expected empty list, got {count} entries");
}

#[then("the transactions list should contain the current account")]
async fn transactions_contain_current_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID set");
    let ids = world
        .all_transactions_account_ids
        .as_ref()
        .expect("No all-transactions result");
    assert!(
        ids.iter().any(|id| id == account_id),
        "Expected transaction for account '{account_id}', got: {ids:?}"
    );
}

#[then(regex = r#"^the transactions list should contain a "([^"]*)" transaction$"#)]
async fn transactions_contain_type(world: &mut PbaWorld, expected_type: String) {
    let types = world
        .all_transactions_types
        .as_ref()
        .expect("No all-transactions result");
    assert!(
        types.iter().any(|t| t == &expected_type),
        "Expected a '{expected_type}' transaction, got types: {types:?}"
    );
}

#[then(regex = r"^the transactions list should have (\d+) entries$")]
async fn transactions_list_count(world: &mut PbaWorld, expected: usize) {
    let count = world
        .all_transactions_count
        .expect("No all-transactions result");
    assert_eq!(count, expected, "Expected {expected} entries, got {count}");
}

#[then(regex = r#"^the transactions list should contain a funding type "([^"]*)"$"#)]
async fn transactions_contain_funding_type(world: &mut PbaWorld, expected_type: String) {
    let types = world
        .all_transactions_funding_types
        .as_ref()
        .expect("No all-transactions result");
    assert!(
        types
            .iter()
            .any(|t| t.as_deref() == Some(expected_type.as_str())),
        "Expected a funding type '{expected_type}', got: {types:?}"
    );
}
