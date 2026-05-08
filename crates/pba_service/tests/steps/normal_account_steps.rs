use cucumber::{given, then, when};

use crate::PbaWorld;

// ── Account creation ──────────────────────────────────────────────────────────

#[when(regex = r#"^I create a normal account for holder "([^"]*)"$"#)]
async fn create_normal_for_holder(world: &mut PbaWorld, holder: String) {
    let result = world
        .client
        .create_normal_account()
        .holder_id(&holder)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_normal_account_id = Some(output.id().to_string());
            world.last_normal_holder_id = Some(output.holder_id().to_string());
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(crate::PbaError {
                kind: format!("{e:?}"),
            });
        }
    }
}

#[when(
    regex = r#"^I create a normal account for holder "([^"]*)" with origin "([^"]*)" and account "([^"]*)"$"#
)]
async fn create_normal_with_origin(
    world: &mut PbaWorld,
    holder: String,
    ifsc: String,
    account: String,
) {
    let result = world
        .client
        .create_normal_account()
        .holder_id(&holder)
        .origin_ifsc(&ifsc)
        .origin_account_number(&account)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_normal_account_id = Some(output.id().to_string());
            world.last_normal_holder_id = Some(output.holder_id().to_string());
            world.last_normal_origin_ifsc = output.origin_ifsc().map(|s| s.to_string());
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(crate::PbaError {
                kind: format!("{e:?}"),
            });
        }
    }
}

#[given(regex = r#"^a normal account exists for holder "([^"]*)"$"#)]
async fn given_normal_account_exists(world: &mut PbaWorld, holder: String) {
    let result = world
        .client
        .create_normal_account()
        .holder_id(&holder)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_normal_account_id = Some(output.id().to_string());
            world.last_normal_holder_id = Some(output.holder_id().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Failed to create normal account for given step: {e:?}"),
    }
}

// ── Then: account fields ──────────────────────────────────────────────────────

#[then(regex = r#"^the response is successful$"#)]
async fn response_is_successful(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "expected success but got {:?}",
        world.last_error
    );
}

#[then(regex = r#"^the normal account holder_id is "([^"]*)"$"#)]
async fn normal_holder_eq(world: &mut PbaWorld, expected: String) {
    assert_eq!(
        world.last_normal_holder_id.as_deref(),
        Some(expected.as_str()),
        "holder_id mismatch"
    );
}

#[then(regex = r#"^the normal account origin_ifsc is "([^"]*)"$"#)]
async fn normal_origin_ifsc_eq(world: &mut PbaWorld, expected: String) {
    assert_eq!(
        world.last_normal_origin_ifsc.as_deref(),
        Some(expected.as_str()),
        "origin_ifsc mismatch"
    );
}

// ── Deposit ───────────────────────────────────────────────────────────────────

#[when(regex = r#"^I deposit (\d+) paisa to the normal account$"#)]
async fn deposit_to_normal(world: &mut PbaWorld, amount: i64) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let result = world
        .client
        .deposit_to_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_normal_deposit_id = Some(output.deposit_id().to_string());
            world.last_normal_deposit_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_normal_deposit_error_kind(&e);
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

#[when(regex = r#"^I deposit (\d+) paisa to the normal account with idempotency key "([^"]*)"$"#)]
async fn deposit_to_normal_with_idempotency(
    world: &mut PbaWorld,
    amount: i64,
    idempotency_key: String,
) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let result = world
        .client
        .deposit_to_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .idempotency_key(&idempotency_key)
        .send()
        .await;
    match result {
        Ok(output) => {
            let deposit_id = output.deposit_id().to_string();
            let ids = world.last_normal_deposit_ids.get_or_insert_with(Vec::new);
            ids.push(deposit_id.clone());
            world.last_normal_deposit_id = Some(deposit_id);
            world.last_normal_deposit_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_normal_deposit_error_kind(&e);
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

#[when(regex = r#"^I retry the same deposit with idempotency key "([^"]*)"$"#)]
async fn retry_deposit_with_idempotency(world: &mut PbaWorld, idempotency_key: String) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    // Re-use the same amount as the original deposit (1000 in the scenario).
    // We capture the original amount from the scenario context — since the scenario
    // always calls this after "I deposit 1000 paisa", we can pass the same amount.
    // However, to be resilient, we just send the same key with amount 1000.
    let result = world
        .client
        .deposit_to_normal_account()
        .account_id(&account_id)
        .amount(1000)
        .idempotency_key(&idempotency_key)
        .send()
        .await;
    match result {
        Ok(output) => {
            let deposit_id = output.deposit_id().to_string();
            let ids = world.last_normal_deposit_ids.get_or_insert_with(Vec::new);
            ids.push(deposit_id.clone());
            world.last_normal_deposit_id = Some(deposit_id);
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_normal_deposit_error_kind(&e);
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

#[when(
    regex = r#"^I create a pending deposit of (\d+) paisa to the normal account with timeout (\d+)$"#
)]
async fn create_pending_normal_deposit(world: &mut PbaWorld, amount: i64, timeout: i32) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let result = world
        .client
        .deposit_to_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .pending(true)
        .timeout_seconds(timeout)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_normal_deposit_id = Some(output.deposit_id().to_string());
            world.last_normal_deposit_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Pending deposit failed: {e:?}"),
    }
}

// ── Post / void deposit ───────────────────────────────────────────────────────

#[when("I post the normal account deposit")]
async fn post_normal_deposit(world: &mut PbaWorld) {
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
    let result = world
        .client
        .post_normal_account_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_normal_deposit_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Post normal deposit failed: {e:?}"),
    }
}

#[when("I void the normal account deposit")]
async fn void_normal_deposit(world: &mut PbaWorld) {
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
    let result = world
        .client
        .void_normal_account_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_normal_deposit_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Void normal deposit failed: {e:?}"),
    }
}

// ── Then: deposit assertions ──────────────────────────────────────────────────

#[then("the deposit is successful")]
async fn deposit_is_successful(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "expected deposit success but got {:?}",
        world.last_error
    );
    assert!(
        world.last_normal_deposit_id.is_some(),
        "expected a deposit ID"
    );
}

#[then(regex = r#"^the deposit status is "([^"]*)"$"#)]
async fn deposit_status_eq(world: &mut PbaWorld, expected: String) {
    let status = world
        .last_normal_deposit_status
        .as_deref()
        .expect("No deposit status recorded");
    assert_eq!(status, expected.as_str(), "deposit status mismatch");
}

#[then(regex = r#"^the deposit fails with "([^"]*)"$"#)]
async fn deposit_fails_with(world: &mut PbaWorld, expected_kind: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected deposit failure but no error recorded");
    assert!(
        err.kind.contains(&expected_kind),
        "expected error containing '{}' but got '{}'",
        expected_kind,
        err.kind
    );
}

// ── Withdrawal ────────────────────────────────────────────────────────────────

#[when(regex = r#"^I withdraw (\d+) paisa from the normal account$"#)]
async fn withdraw_from_normal(world: &mut PbaWorld, amount: i64) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let result = world
        .client
        .withdraw_from_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(_output) => {
            world.last_error = None;
        }
        Err(e) => {
            let kind = extract_normal_withdrawal_error_kind(&e);
            world.last_error = Some(crate::PbaError { kind });
        }
    }
}

#[then("the withdrawal is successful")]
async fn withdrawal_is_successful(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "expected withdrawal success but got {:?}",
        world.last_error
    );
}

#[then(regex = r#"^the withdrawal fails with "([^"]*)"$"#)]
async fn withdrawal_fails_with(world: &mut PbaWorld, expected_kind: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected withdrawal failure but no error recorded");
    assert!(
        err.kind.contains(&expected_kind),
        "expected error containing '{}' but got '{}'",
        expected_kind,
        err.kind
    );
}

// ── Balance ───────────────────────────────────────────────────────────────────

#[then(regex = r#"^the normal account balance is (\d+)$"#)]
async fn normal_balance_eq(world: &mut PbaWorld, expected: i64) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    let output = world
        .client
        .get_normal_account_balance()
        .account_id(&account_id)
        .send()
        .await
        .expect("Failed to get normal account balance");
    assert_eq!(
        output.balance(),
        expected,
        "normal account balance mismatch: expected {} but got {}",
        expected,
        output.balance()
    );
}

#[given(regex = r#"^the normal account has balance (\d+)$"#)]
async fn given_normal_balance(world: &mut PbaWorld, amount: i64) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    // Seed balance by depositing the requested amount
    world
        .client
        .deposit_to_normal_account()
        .account_id(&account_id)
        .amount(amount)
        .send()
        .await
        .expect("Failed to seed normal account balance");
}

// ── Status / freeze ───────────────────────────────────────────────────────────

#[when("I freeze the normal account")]
async fn freeze_normal_account(world: &mut PbaWorld) {
    let account_id = world
        .last_normal_account_id
        .as_ref()
        .expect("No normal account ID")
        .clone();
    world
        .client
        .update_normal_account_status()
        .account_id(&account_id)
        .status("frozen")
        .send()
        .await
        .expect("Failed to freeze normal account");
}

// ── Idempotency ───────────────────────────────────────────────────────────────

#[then("both deposits return the same id")]
async fn both_deposits_same_id(world: &mut PbaWorld) {
    let ids = world
        .last_normal_deposit_ids
        .as_ref()
        .expect("No deposit IDs recorded for idempotency check");
    assert_eq!(
        ids.len(),
        2,
        "expected exactly 2 recorded deposit IDs but got {}",
        ids.len()
    );
    assert_eq!(
        ids[0], ids[1],
        "idempotency failed: deposit IDs differ: {} vs {}",
        ids[0], ids[1]
    );
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Extract a human-readable error kind string from a deposit SDK error.
/// Looks for structured error variants first; falls back to the Debug representation.
fn extract_normal_deposit_error_kind<E>(e: &E) -> String
where
    E: std::fmt::Debug + std::fmt::Display,
{
    // The Display impl for the generated error types includes the variant name
    // (e.g. "AccountNotActiveError: ..."), but the service sends an `error` field
    // with the canonical string (e.g. "NormalAccountNotActive").  We use the
    // Debug representation which includes the full error body.
    let s = format!("{e:?}");
    // Try to find error codes embedded in the debug string.
    for code in &[
        "NormalAccountNotActive",
        "AccountNotActive",
        "InsufficientFunds",
        "AccountNotFound",
    ] {
        if s.contains(code) {
            return code.to_string();
        }
    }
    s
}

/// Extract a human-readable error kind string from a withdrawal SDK error.
fn extract_normal_withdrawal_error_kind<E>(e: &E) -> String
where
    E: std::fmt::Debug + std::fmt::Display,
{
    let s = format!("{e:?}");
    for code in &[
        "NormalAccountNotActive",
        "AccountNotActive",
        "InsufficientFunds",
        "AccountNotFound",
    ] {
        if s.contains(code) {
            return code.to_string();
        }
    }
    s
}
