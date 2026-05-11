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
            world.last_error = Some(crate::PbaError { kind });
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
            world.last_error = Some(crate::PbaError { kind });
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
            world.last_error = Some(crate::PbaError { kind });
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
            world.last_error = Some(crate::PbaError { kind });
        }
    }
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
    ] {
        if s.contains(code) {
            return code.to_string();
        }
    }
    s
}
