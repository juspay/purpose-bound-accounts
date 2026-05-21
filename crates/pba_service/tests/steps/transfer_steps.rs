use cucumber::{then, when};

use crate::PbaWorld;

// ── Initiate transfer ─────────────────────────────────────────────────────────

#[when(regex = r#"^I transfer (\d+) paisa from the normal account to the PB account$"#)]
async fn transfer_to_pb(world: &mut PbaWorld, amount: i64) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let pb_account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let result = world
        .client
        .transfer_to_pb_account()
        .account_id(&normal_account_id)
        .destination_pb_account_id(&pb_account_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_transfer_id = Some(output.transfer_id().to_string());
            world.last_transfer_status = Some(output.status().to_string());
            world.last_transfer_correlation_id = Some(output.correlation_id().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            let message = extract_transfer_error_message(&e);
            world.last_error = Some(crate::PbaError { kind, message });
        }
    }
}

#[when(
    regex = r#"^I transfer (\d+) paisa from the normal account to the PB account with idempotency key "([^"]*)"$"#
)]
async fn transfer_to_pb_with_idempotency(
    world: &mut PbaWorld,
    amount: i64,
    idempotency_key: String,
) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let pb_account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let result = world
        .client
        .transfer_to_pb_account()
        .account_id(&normal_account_id)
        .destination_pb_account_id(&pb_account_id)
        .amount(amount)
        .idempotency_key(&idempotency_key)
        .send()
        .await;
    match result {
        Ok(output) => {
            let transfer_id = output.transfer_id().to_string();
            let ids = world.last_transfer_ids.get_or_insert_with(Vec::new);
            ids.push(transfer_id.clone());
            world.last_transfer_id = Some(transfer_id);
            world.last_transfer_status = Some(output.status().to_string());
            world.last_transfer_correlation_id = Some(output.correlation_id().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            let message = extract_transfer_error_message(&e);
            world.last_error = Some(crate::PbaError { kind, message });
        }
    }
}

#[when(regex = r#"^I attempt to transfer (\d+) paisa from the normal account to the PB account$"#)]
async fn attempt_transfer_to_pb(world: &mut PbaWorld, amount: i64) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let pb_account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let result = world
        .client
        .transfer_to_pb_account()
        .account_id(&normal_account_id)
        .destination_pb_account_id(&pb_account_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_transfer_id = Some(output.transfer_id().to_string());
            world.last_transfer_status = Some(output.status().to_string());
            world.last_transfer_correlation_id = Some(output.correlation_id().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            let message = extract_transfer_error_message(&e);
            world.last_error = Some(crate::PbaError { kind, message });
        }
    }
}

#[when(
    regex = r#"^I create a pending transfer of (\d+) paisa from the normal account to the PB account with timeout (\d+)$"#
)]
async fn create_pending_transfer(world: &mut PbaWorld, amount: i64, timeout: i32) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let pb_account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let result = world
        .client
        .transfer_to_pb_account()
        .account_id(&normal_account_id)
        .destination_pb_account_id(&pb_account_id)
        .amount(amount)
        .pending(true)
        .timeout_seconds(timeout)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_transfer_id = Some(output.transfer_id().to_string());
            world.last_transfer_status = Some(output.status().to_string());
            world.last_transfer_correlation_id = Some(output.correlation_id().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Pending transfer failed: {e:?}"),
    }
}

// ── Post / void transfer ──────────────────────────────────────────────────────

#[when("I post the transfer")]
async fn post_transfer(world: &mut PbaWorld) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let transfer_id = world
        .last_transfer_id
        .as_ref()
        .expect("No transfer ID")
        .clone();
    let result = world
        .client
        .post_normal_account_transfer()
        .account_id(&normal_account_id)
        .transfer_id(&transfer_id)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_transfer_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Post transfer failed: {e:?}"),
    }
}

#[when("I void the transfer")]
async fn void_transfer(world: &mut PbaWorld) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let transfer_id = world
        .last_transfer_id
        .as_ref()
        .expect("No transfer ID")
        .clone();
    let result = world
        .client
        .void_normal_account_transfer()
        .account_id(&normal_account_id)
        .transfer_id(&transfer_id)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_transfer_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Void transfer failed: {e:?}"),
    }
}

// ── Idempotency replay ────────────────────────────────────────────────────────

#[when(regex = r#"^I retry the same transfer with idempotency key "([^"]*)"$"#)]
async fn retry_transfer_with_idempotency(world: &mut PbaWorld, idempotency_key: String) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let pb_account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    // Re-use the same amount as the original transfer (2000 in the scenario).
    // The idempotency key ensures the server returns the same transfer.
    let result = world
        .client
        .transfer_to_pb_account()
        .account_id(&normal_account_id)
        .destination_pb_account_id(&pb_account_id)
        .amount(2000)
        .idempotency_key(&idempotency_key)
        .send()
        .await;
    match result {
        Ok(output) => {
            let transfer_id = output.transfer_id().to_string();
            let ids = world.last_transfer_ids.get_or_insert_with(Vec::new);
            ids.push(transfer_id.clone());
            world.last_transfer_id = Some(transfer_id);
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            let message = extract_transfer_error_message(&e);
            world.last_error = Some(crate::PbaError { kind, message });
        }
    }
}

// ── Payment from PB account (reversal scenarios use this to drain others-pool) ──

#[when(regex = r#"^I pay (\d+) paisa to merchant "([^"]*)" with MCC "([^"]*)"$"#)]
async fn pay_paisa_to_merchant(
    world: &mut PbaWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
) {
    let account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    world
        .client
        .make_payment()
        .account_id(&account_id)
        .amount(amount)
        .merchant_mcc(&mcc)
        .merchant_id(&merchant_id)
        .description("reversal-test payment")
        .send()
        .await
        .expect("Payment to drain others-pool failed unexpectedly");
}

// ── Freeze PB account ─────────────────────────────────────────────────────────

#[when("I freeze the PB account")]
async fn freeze_pb_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    world
        .client
        .update_account_status()
        .account_id(&account_id)
        .status(pba_client::types::Status::Frozen)
        .send()
        .await
        .expect("Failed to freeze PB account");
}

// ── Then: transfer assertions ─────────────────────────────────────────────────

#[then("the transfer is successful")]
async fn transfer_is_successful(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "expected transfer success but got {:?}",
        world.last_error
    );
    assert!(
        world.last_transfer_id.is_some(),
        "expected a transfer ID but none was recorded"
    );
}

#[then(regex = r#"^the transfer status field is "([^"]*)"$"#)]
async fn transfer_status_field_eq(world: &mut PbaWorld, expected: String) {
    let status = world
        .last_transfer_status
        .as_deref()
        .expect("No transfer status recorded");
    assert_eq!(
        status,
        expected.as_str(),
        "transfer status mismatch: expected '{}' but got '{}'",
        expected,
        status
    );
}

#[then(regex = r#"^the transfer fails with "([^"]*)"$"#)]
async fn transfer_fails_with(world: &mut PbaWorld, expected_kind: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected transfer failure but no error was recorded");
    assert!(
        err.kind.contains(&expected_kind),
        "expected error containing '{}' but got '{}'",
        expected_kind,
        err.kind
    );
}

#[then("both transfers return the same id")]
async fn both_transfers_same_id(world: &mut PbaWorld) {
    let ids = world
        .last_transfer_ids
        .as_ref()
        .expect("No transfer IDs recorded for idempotency check");
    assert_eq!(
        ids.len(),
        2,
        "expected exactly 2 recorded transfer IDs but got {}",
        ids.len()
    );
    assert_eq!(
        ids[0], ids[1],
        "idempotency failed: transfer IDs differ: {} vs {}",
        ids[0], ids[1]
    );
}

// ── Then: PB account balance assertions ──────────────────────────────────────

#[then(regex = r"^the PB account others-pool balance is (\d+)$")]
async fn pb_others_pool_balance(world: &mut PbaWorld, expected: i64) {
    let account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let balance = world
        .client
        .get_pb_account_balance()
        .account_id(&account_id)
        .send()
        .await
        .expect("Failed to get PB account balance");
    assert_eq!(
        balance.others_contribution(),
        expected,
        "PB others-pool balance mismatch: expected {} but got {}",
        expected,
        balance.others_contribution()
    );
}

#[then(regex = r"^the PB account self-pool balance is (\d+)$")]
async fn pb_self_pool_balance(world: &mut PbaWorld, expected: i64) {
    let account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let balance = world
        .client
        .get_pb_account_balance()
        .account_id(&account_id)
        .send()
        .await
        .expect("Failed to get PB account balance");
    assert_eq!(
        balance.self_contribution(),
        expected,
        "PB self-pool balance mismatch: expected {} but got {}",
        expected,
        balance.self_contribution()
    );
}

// ── Then: correlation_id + transaction type assertions ────────────────────────

#[then("the source-side and destination-side transactions share the same correlation_id")]
async fn legs_share_correlation_id(world: &mut PbaWorld) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let pb_account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let correlation_id = world
        .last_transfer_correlation_id
        .as_ref()
        .expect("No transfer correlation_id recorded")
        .clone();

    // Fetch normal-account transactions and find one with the correlation_id.
    let normal_txns = world
        .client
        .list_normal_account_transactions()
        .account_id(&normal_account_id)
        .send()
        .await
        .expect("Failed to list normal account transactions");

    let source_txn = normal_txns
        .transactions()
        .iter()
        .find(|t| t.correlation_id() == Some(correlation_id.as_str()))
        .expect("No normal-account transaction with matching correlation_id");

    // Fetch PB-account transactions and find one with the same correlation_id.
    let pb_txns = world
        .client
        .list_pb_account_transactions()
        .account_id(&pb_account_id)
        .send()
        .await
        .expect("Failed to list PB account transactions");

    let dest_txn = pb_txns
        .transactions()
        .iter()
        .find(|t| t.correlation_id() == Some(correlation_id.as_str()))
        .expect("No PB-account transaction with matching correlation_id");

    // Store for subsequent assertion steps.
    world.last_source_txn_type = Some(source_txn.r#type().to_string());
    world.last_source_txn_direction = Some(source_txn.direction().to_string());
    world.last_dest_txn_type = Some(dest_txn.r#type().to_string());
    world.last_dest_txn_pool = dest_txn.pool().map(|p| p.to_string());
    world.last_dest_txn_funding_type = dest_txn.funding_type().map(|s| s.to_string());
}

#[then(regex = r#"^the source-side transaction has type "([^"]*)" and direction "([^"]*)"$"#)]
async fn source_txn_type_and_direction(
    world: &mut PbaWorld,
    expected_type: String,
    expected_direction: String,
) {
    let actual_type = world
        .last_source_txn_type
        .as_deref()
        .expect("No source transaction type recorded — run the correlation_id step first");
    let actual_direction = world
        .last_source_txn_direction
        .as_deref()
        .expect("No source transaction direction recorded");
    assert_eq!(
        actual_type,
        expected_type.as_str(),
        "source-side type mismatch: expected '{}' got '{}'",
        expected_type,
        actual_type
    );
    assert_eq!(
        actual_direction,
        expected_direction.as_str(),
        "source-side direction mismatch: expected '{}' got '{}'",
        expected_direction,
        actual_direction
    );
}

#[then(
    regex = r#"^the destination-side transaction has type "([^"]*)" and pool "([^"]*)" and funding_type "([^"]*)"$"#
)]
async fn dest_txn_type_pool_funding_type(
    world: &mut PbaWorld,
    expected_type: String,
    expected_pool: String,
    expected_funding_type: String,
) {
    let actual_type = world
        .last_dest_txn_type
        .as_deref()
        .expect("No destination transaction type recorded — run the correlation_id step first");
    let actual_pool = world
        .last_dest_txn_pool
        .as_deref()
        .expect("No destination transaction pool recorded");
    let actual_ft = world
        .last_dest_txn_funding_type
        .as_deref()
        .expect("No destination transaction funding_type recorded");
    assert_eq!(
        actual_type,
        expected_type.as_str(),
        "destination-side type mismatch: expected '{}' got '{}'",
        expected_type,
        actual_type
    );
    assert_eq!(
        actual_pool,
        expected_pool.as_str(),
        "destination-side pool mismatch: expected '{}' got '{}'",
        expected_pool,
        actual_pool
    );
    assert_eq!(
        actual_ft,
        expected_funding_type.as_str(),
        "destination-side funding_type mismatch: expected '{}' got '{}'",
        expected_funding_type,
        actual_ft
    );
}

// ── Then: error code assertion ────────────────────────────────────────────────

#[then(regex = r#"^the error code is "([^"]*)"$"#)]
async fn error_code_is(world: &mut PbaWorld, expected_code: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected an error but none was recorded");
    assert!(
        err.kind.contains(&expected_code),
        "expected error code '{}' in error but got '{}'",
        expected_code,
        err.kind
    );
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn extract_transfer_error_kind<E>(e: &E) -> String
where
    E: std::fmt::Debug + std::fmt::Display,
{
    let s = format!("{e:?}");
    for code in &[
        "InsufficientFunds",
        "NormalAccountNotActive",
        "PbAccountNotActive",
        "AccountNotFound",
        "AccountNotActive",
        "TransferNotReversible",
        "TransferAlreadyReversed",
        "ReversalAmountInvalid",
        "TransactionNotFound",
    ] {
        if s.contains(code) {
            return code.to_string();
        }
    }
    s
}

fn extract_transfer_error_message<E>(e: &E) -> Option<String>
where
    E: std::fmt::Debug,
{
    Some(format!("{e:?}"))
}

// ── Reversal ──────────────────────────────────────────────────────────────────

#[when(regex = r#"^I reverse (\d+) paisa from the transfer$"#)]
async fn reverse_transfer(world: &mut PbaWorld, amount: i64) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let transfer_id = world
        .last_transfer_id
        .as_ref()
        .expect("No transfer ID")
        .clone();
    let result = world
        .client
        .reverse_normal_account_transfer()
        .account_id(&normal_account_id)
        .transfer_id(&transfer_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_reversal_id = Some(output.reversal_id().to_string());
            world.last_reversal_status = Some(output.status().to_string());
            world.last_reversal_correlation_id = Some(output.correlation_id().to_string());
            world.last_reversal_original_amount = Some(output.original_amount());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            let message = extract_transfer_error_message(&e);
            world.last_error = Some(crate::PbaError { kind, message });
        }
    }
}

#[when(regex = r#"^I attempt to reverse (\d+) paisa from the transfer$"#)]
async fn attempt_reverse_transfer(world: &mut PbaWorld, amount: i64) {
    reverse_transfer(world, amount).await;
}

#[when(
    regex = r#"^I reverse (\d+) paisa from the transfer with idempotency key "([^"]*)"$"#
)]
async fn reverse_transfer_with_idempotency(
    world: &mut PbaWorld,
    amount: i64,
    idempotency_key: String,
) {
    let normal_account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let transfer_id = world
        .last_transfer_id
        .as_ref()
        .expect("No transfer ID")
        .clone();
    let result = world
        .client
        .reverse_normal_account_transfer()
        .account_id(&normal_account_id)
        .transfer_id(&transfer_id)
        .amount(amount)
        .idempotency_key(&idempotency_key)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_reversal_id = Some(output.reversal_id().to_string());
            world.last_reversal_status = Some(output.status().to_string());
            world.last_reversal_correlation_id = Some(output.correlation_id().to_string());
            world.last_reversal_original_amount = Some(output.original_amount());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_transfer_error_kind(&e);
            let message = extract_transfer_error_message(&e);
            world.last_error = Some(crate::PbaError { kind, message });
        }
    }
}

#[then(regex = r#"^the reversal is successful$"#)]
async fn reversal_is_successful(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "Expected reversal to succeed, got error: {:?}",
        world.last_error
    );
    assert!(world.last_reversal_id.is_some(), "No reversal ID captured");
}

#[then(regex = r#"^the reversal status field is "([^"]*)"$"#)]
async fn reversal_status_field_is(world: &mut PbaWorld, expected: String) {
    let actual = world
        .last_reversal_status
        .as_ref()
        .expect("No reversal status captured");
    assert_eq!(actual, &expected);
}

#[then(regex = r#"^the reversal fails with "([^"]*)"$"#)]
async fn reversal_fails_with(world: &mut PbaWorld, expected_kind: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected reversal to fail, but it succeeded");
    assert_eq!(err.kind, expected_kind);
}

#[then(regex = r#"^the reversal fails with "([^"]*)" reason "([^"]*)"$"#)]
async fn reversal_fails_with_reason(
    world: &mut PbaWorld,
    expected_kind: String,
    expected_reason: String,
) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected reversal to fail, but it succeeded");
    assert_eq!(err.kind, expected_kind);
    assert!(
        err.message
            .as_ref()
            .map_or(false, |m| m.contains(&expected_reason)),
        "Expected reason '{expected_reason}' in error message; got {:?}",
        err.message
    );
}

#[then(regex = r#"^the reversal available balance is (\d+)$"#)]
async fn reversal_available_balance_is(world: &mut PbaWorld, expected: i64) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected an error with available balance, but none was recorded");
    let msg = err.message.as_ref().expect("Error has no message");
    let expected_str = format!("available {expected}");
    assert!(
        msg.contains(&expected_str),
        "expected '{}' substring in error message, got '{}'",
        expected_str,
        msg
    );
}

#[when(
    regex = r#"^I switch the current normal account to a fresh holder "([^"]*)"$"#
)]
async fn switch_normal_account_to_fresh_holder(world: &mut PbaWorld, holder: String) {
    let result = world
        .client
        .create_normal_account()
        .holder_id(&holder)
        .send()
        .await
        .expect("Failed to create fresh normal account");
    world.last_normal_account_id = Some(result.id().to_string());
}

#[when("I treat the reversal row as the current transfer")]
async fn treat_reversal_row_as_current_transfer(world: &mut PbaWorld) {
    world.last_transfer_id = world.last_reversal_id.clone();
}

#[then(regex = r#"^the normal account has at least (\d+) transactions$"#)]
async fn normal_account_has_at_least_n_transactions(
    world: &mut PbaWorld,
    min_count: usize,
) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let txns = world
        .client
        .list_normal_account_transactions()
        .account_id(&account_id)
        .send()
        .await
        .expect("Failed to list normal account transactions");
    let count = txns.transactions().len();
    assert!(
        count >= min_count,
        "Expected at least {min_count} transactions on normal account, got {count}"
    );
}

#[then(regex = r#"^the PB account has at least (\d+) transactions$"#)]
async fn pb_account_has_at_least_n_transactions(
    world: &mut PbaWorld,
    min_count: usize,
) {
    let account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    let txns = world
        .client
        .list_pb_account_transactions()
        .account_id(&account_id)
        .send()
        .await
        .expect("Failed to list PB account transactions");
    let count = txns.transactions().len();
    assert!(
        count >= min_count,
        "Expected at least {min_count} transactions on PB account, got {count}"
    );
}

#[when("I reactivate the PB account")]
async fn reactivate_pb_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No PB account ID").clone();
    world
        .client
        .update_account_status()
        .account_id(&account_id)
        .status(pba_client::types::Status::Active)
        .send()
        .await
        .expect("Failed to reactivate PB account");
}
