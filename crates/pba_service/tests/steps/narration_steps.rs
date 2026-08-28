//! Steps covering the optional `description` on deposits/withdrawals and the
//! `reason` recorded when a pending deposit is voided.

use cucumber::{then, when};

use crate::PbaWorld;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Fetch a PB-account transaction by id from the account's listing.
async fn pb_txn(world: &PbaWorld, txn_id: &str) -> pba_client::types::TransactionSummary {
    let account_id = world.account_id.as_ref().expect("No account ID");
    world
        .client
        .list_pb_account_transactions()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to list PB account transactions")
        .transactions()
        .iter()
        .find(|t| t.id() == txn_id)
        .unwrap_or_else(|| panic!("No PB transaction with id {txn_id}"))
        .clone()
}

/// Fetch the most recent PB-account transaction of the given type. Used for
/// withdrawals, which do not return an id the caller can hold on to.
async fn latest_pb_txn_of_type(
    world: &PbaWorld,
    txn_type: &str,
) -> pba_client::types::TransactionSummary {
    let account_id = world.account_id.as_ref().expect("No account ID");
    world
        .client
        .list_pb_account_transactions()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to list PB account transactions")
        .transactions()
        .iter()
        .find(|t| t.r#type().as_str() == txn_type)
        .unwrap_or_else(|| panic!("No PB transaction of type {txn_type}"))
        .clone()
}

async fn normal_txn(world: &PbaWorld, txn_id: &str) -> pba_client::types::TransactionSummary {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID");
    world
        .client
        .list_normal_account_transactions()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to list normal account transactions")
        .transactions()
        .iter()
        .find(|t| t.id() == txn_id)
        .unwrap_or_else(|| panic!("No normal-account transaction with id {txn_id}"))
        .clone()
}

async fn latest_normal_txn_of_type(
    world: &PbaWorld,
    txn_type: &str,
) -> pba_client::types::TransactionSummary {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID");
    world
        .client
        .list_normal_account_transactions()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to list normal account transactions")
        .transactions()
        .iter()
        .find(|t| t.r#type().as_str() == txn_type)
        .unwrap_or_else(|| panic!("No normal-account transaction of type {txn_type}"))
        .clone()
}

fn record_validation_error(world: &mut PbaWorld, err_str: &str) {
    let kind = if err_str.contains("ValidationError") || err_str.contains("400") {
        "validation"
    } else {
        "other"
    };
    world.last_error = Some(crate::PbaError {
        kind: kind.to_string(),
        message: Some(err_str.to_string()),
    });
}

// ── When: PB account ──────────────────────────────────────────────────────────

#[when(
    regex = r#"^I deposit (\d+) from IFSC "([^"]*)" account "([^"]*)" with description "([^"]*)"$"#
)]
async fn deposit_with_description(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    description: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .deposit_to_pb_account()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .description(&description)
        .send()
        .await
        .expect("Deposit with description failed");
    world.last_deposit_pool = Some(output.pool().to_string());
    world.last_deposit_id = Some(output.deposit_id().to_string());
    world.last_error = None;
}

#[when(
    regex = r#"^I create a pending deposit of (\d+) from IFSC "([^"]*)" account "([^"]*)" with description "([^"]*)"$"#
)]
async fn pending_deposit_with_description(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    description: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .deposit_to_pb_account()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .pending(true)
        .description(&description)
        .send()
        .await
        .expect("Pending deposit with description failed");
    world.last_deposit_pool = Some(output.pool().to_string());
    world.last_deposit_id = Some(output.deposit_id().to_string());
    world.last_error = None;
}

#[when(regex = r#"^I void the pending deposit with reason "([^"]*)"$"#)]
async fn void_pending_deposit_with_reason(world: &mut PbaWorld, reason: String) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID to void")
        .clone();
    let output = world
        .client
        .void_pb_account_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .reason(&reason)
        .send()
        .await
        .expect("Void deposit with reason failed");
    assert_eq!(output.status(), "voided", "expected status 'voided'");
    world.last_error = None;
}

#[when(regex = r#"^I withdraw (\d+) with description "([^"]*)"$"#)]
async fn withdraw_with_description(world: &mut PbaWorld, amount: i64, description: String) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .withdraw_from_pb_account()
        .account_id(account_id)
        .amount(amount)
        .description(&description)
        .send()
        .await
        .expect("Withdrawal with description failed");
    world.last_withdrawal_amount = Some(output.amount());
    world.last_error = None;
}

#[when(
    regex = r#"^I attempt to deposit (\d+) from IFSC "([^"]*)" account "([^"]*)" with a (\d+) character description$"#
)]
async fn attempt_deposit_with_long_description(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    len: usize,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit_to_pb_account()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .description("x".repeat(len))
        .send()
        .await;
    match result {
        Ok(_) => world.last_error = None,
        Err(e) => record_validation_error(world, &format!("{e:?}")),
    }
}

#[when(regex = r#"^I attempt to void the pending deposit with a (\d+) character reason$"#)]
async fn attempt_void_with_long_reason(world: &mut PbaWorld, len: usize) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID to void")
        .clone();
    let result = world
        .client
        .void_pb_account_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .reason("x".repeat(len))
        .send()
        .await;
    match result {
        Ok(_) => world.last_error = None,
        Err(e) => record_validation_error(world, &format!("{e:?}")),
    }
}

// ── When: normal account ──────────────────────────────────────────────────────

#[when(regex = r#"^I deposit (\d+) paisa to the normal account with description "([^"]*)"$"#)]
async fn normal_deposit_with_description(world: &mut PbaWorld, amount: i64, description: String) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let output = world
        .client
        .deposit_to_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .description(&description)
        .send()
        .await
        .expect("Normal deposit with description failed");
    world.last_normal_deposit_id = Some(output.deposit_id().to_string());
    world.last_normal_deposit_status = Some(output.status().to_string());
    world.last_error = None;
}

#[when(
    regex = r#"^I create a pending deposit of (\d+) paisa to the normal account with description "([^"]*)"$"#
)]
async fn normal_pending_deposit_with_description(
    world: &mut PbaWorld,
    amount: i64,
    description: String,
) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let output = world
        .client
        .deposit_to_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .pending(true)
        .description(&description)
        .send()
        .await
        .expect("Pending normal deposit with description failed");
    world.last_normal_deposit_id = Some(output.deposit_id().to_string());
    world.last_normal_deposit_status = Some(output.status().to_string());
    world.last_error = None;
}

#[when(regex = r#"^I void the normal account deposit with reason "([^"]*)"$"#)]
async fn void_normal_deposit_with_reason(world: &mut PbaWorld, reason: String) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let deposit_id = world
        .last_normal_deposit_id
        .as_ref()
        .expect("No normal deposit ID")
        .clone();
    let output = world
        .client
        .void_normal_account_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .reason(&reason)
        .send()
        .await
        .expect("Void normal deposit with reason failed");
    world.last_normal_deposit_status = Some(output.status().to_string());
    world.last_error = None;
}

#[when(regex = r#"^I withdraw (\d+) paisa from the normal account with description "([^"]*)"$"#)]
async fn normal_withdraw_with_description(world: &mut PbaWorld, amount: i64, description: String) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    world
        .client
        .withdraw_from_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .description(&description)
        .send()
        .await
        .expect("Normal withdrawal with description failed");
    world.last_error = None;
}

// ── Then ──────────────────────────────────────────────────────────────────────

#[then(regex = r#"^the deposit transaction description should be "([^"]*)"$"#)]
async fn deposit_description_is(world: &mut PbaWorld, expected: String) {
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID")
        .clone();
    let txn = pb_txn(world, &deposit_id).await;
    assert_eq!(
        txn.description(),
        Some(expected.as_str()),
        "deposit {deposit_id} description mismatch"
    );
}

#[then("the deposit transaction should have no description")]
async fn deposit_has_no_description(world: &mut PbaWorld) {
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID")
        .clone();
    let txn = pb_txn(world, &deposit_id).await;
    assert_eq!(
        txn.description(),
        None,
        "expected no description on deposit {deposit_id}"
    );
}

#[then(regex = r#"^the deposit transaction void reason should be "([^"]*)"$"#)]
async fn deposit_void_reason_is(world: &mut PbaWorld, expected: String) {
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID")
        .clone();
    let txn = pb_txn(world, &deposit_id).await;
    assert_eq!(
        txn.void_reason(),
        Some(expected.as_str()),
        "deposit {deposit_id} void_reason mismatch"
    );
}

#[then("the deposit transaction should have no void reason")]
async fn deposit_has_no_void_reason(world: &mut PbaWorld) {
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID")
        .clone();
    let txn = pb_txn(world, &deposit_id).await;
    assert_eq!(
        txn.void_reason(),
        None,
        "expected no void_reason on deposit {deposit_id}"
    );
}

#[then(regex = r#"^the withdrawal transaction description should be "([^"]*)"$"#)]
async fn withdrawal_description_is(world: &mut PbaWorld, expected: String) {
    let txn = latest_pb_txn_of_type(world, "withdrawal").await;
    assert_eq!(
        txn.description(),
        Some(expected.as_str()),
        "withdrawal description mismatch"
    );
}

#[then(regex = r#"^the normal deposit transaction description should be "([^"]*)"$"#)]
async fn normal_deposit_description_is(world: &mut PbaWorld, expected: String) {
    let deposit_id = world
        .last_normal_deposit_id
        .as_ref()
        .expect("No normal deposit ID")
        .clone();
    let txn = normal_txn(world, &deposit_id).await;
    assert_eq!(
        txn.description(),
        Some(expected.as_str()),
        "normal deposit {deposit_id} description mismatch"
    );
}

#[then(regex = r#"^the normal deposit transaction void reason should be "([^"]*)"$"#)]
async fn normal_deposit_void_reason_is(world: &mut PbaWorld, expected: String) {
    let deposit_id = world
        .last_normal_deposit_id
        .as_ref()
        .expect("No normal deposit ID")
        .clone();
    let txn = normal_txn(world, &deposit_id).await;
    assert_eq!(
        txn.void_reason(),
        Some(expected.as_str()),
        "normal deposit {deposit_id} void_reason mismatch"
    );
}

#[then(regex = r#"^the normal withdrawal transaction description should be "([^"]*)"$"#)]
async fn normal_withdrawal_description_is(world: &mut PbaWorld, expected: String) {
    let txn = latest_normal_txn_of_type(world, "withdrawal").await;
    assert_eq!(
        txn.description(),
        Some(expected.as_str()),
        "normal withdrawal description mismatch"
    );
}

#[then("the request should be rejected as invalid")]
async fn request_rejected_as_invalid(world: &mut PbaWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("expected a validation error but the request succeeded");
    assert_eq!(
        err.kind, "validation",
        "expected a validation error, got {err:?}"
    );
}
