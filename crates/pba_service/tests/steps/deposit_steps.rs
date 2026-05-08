use cucumber::{given, then, when};

use crate::PbaWorld;

#[given(regex = r"^the account has (\d+) in self-pool and (\d+) in others-pool$")]
async fn account_has_balances(world: &mut PbaWorld, self_amount: i64, others_amount: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();

    // Get the account to find origin bank details
    let account = world
        .client
        .get_account()
        .account_id(&account_id)
        .send()
        .await
        .expect("Failed to get account");

    // Deposit to self-pool (from origin bank)
    if self_amount > 0 {
        world
            .client
            .deposit()
            .account_id(&account_id)
            .source_ifsc(account.origin_ifsc())
            .source_account_number(account.origin_account_number())
            .amount(self_amount)
            .send()
            .await
            .expect("Failed to deposit to self-pool");
    }

    // Deposit to others-pool (from a different bank)
    if others_amount > 0 {
        world
            .client
            .deposit()
            .account_id(&account_id)
            .source_ifsc("OTHER0009999")
            .source_account_number("9999999999")
            .amount(others_amount)
            .funding_type(pba_client::types::FundingType::from("third_party"))
            .send()
            .await
            .expect("Failed to deposit to others-pool");
    }
}

#[when(regex = r#"^I deposit (\d+) from IFSC "([^"]*)" account "([^"]*)"$"#)]
async fn deposit(world: &mut PbaWorld, amount: i64, ifsc: String, account_number: String) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_funding_type = Some(output.funding_type().to_string());
        }
        Err(e) => panic!("Deposit failed: {e:?}"),
    }
}

#[when(regex = r#"^I attempt to deposit (\d+) from IFSC "([^"]*)" account "([^"]*)"$"#)]
async fn attempt_deposit(world: &mut PbaWorld, amount: i64, ifsc: String, account_number: String) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "account_not_active".into(),
            });
        }
    }
}

#[then(regex = r#"^the deposit should go to "([^"]*)" pool$"#)]
async fn deposit_pool(world: &mut PbaWorld, expected_pool: String) {
    let pool = world.last_deposit_pool.as_ref().expect("No deposit result");
    assert_eq!(pool, &expected_pool);
}

#[then(regex = r"^the self contribution should be (\d+)$")]
async fn self_contribution_should_be(world: &mut PbaWorld, expected: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let balance = world
        .client
        .get_balance()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get balance");
    assert_eq!(
        balance.self_contribution(),
        expected,
        "Self contribution mismatch"
    );
}

#[then(regex = r"^the others contribution should be (\d+)$")]
async fn others_contribution_should_be(world: &mut PbaWorld, expected: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let balance = world
        .client
        .get_balance()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get balance");
    assert_eq!(
        balance.others_contribution(),
        expected,
        "Others contribution mismatch"
    );
}

#[then(regex = r"^the total balance should be (\d+)$")]
async fn total_balance_should_be(world: &mut PbaWorld, expected: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let balance = world
        .client
        .get_balance()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get balance");
    assert_eq!(balance.total(), expected, "Total balance mismatch");
}

#[then("the deposit should be rejected as account not active")]
async fn deposit_rejected_not_active(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_some(),
        "Expected deposit to be rejected"
    );
}

#[when(regex = r#"^I create a pending deposit of (\d+) from IFSC "([^"]*)" account "([^"]*)"$"#)]
async fn create_pending_deposit(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .pending(true)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Pending deposit failed: {e:?}"),
    }
}

#[when(
    regex = r#"^I create a pending deposit of (\d+) from IFSC "([^"]*)" account "([^"]*)" with funding type "([^"]*)"$"#
)]
async fn create_pending_deposit_with_funding_type(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    funding_type: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .pending(true)
        .funding_type(pba_client::types::FundingType::from(funding_type.as_str()))
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_funding_type = Some(output.funding_type().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Pending deposit with funding type failed: {e:?}"),
    }
}

#[when(
    regex = r#"^I create a pending deposit of (\d+) from IFSC "([^"]*)" account "([^"]*)" with gateway ref "([^"]*)"$"#
)]
async fn create_pending_deposit_with_ref(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    gateway_ref: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .pending(true)
        .gateway_ref(&gateway_ref)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Pending deposit with gateway ref failed: {e:?}"),
    }
}

#[when("I post the pending deposit")]
async fn post_pending_deposit(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID to post")
        .clone();
    let result = world
        .client
        .post_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(output) => {
            assert_eq!(
                output.status(),
                "posted",
                "Expected status 'posted' after posting"
            );
            world.last_error = None;
        }
        Err(e) => panic!("Post deposit failed: {e:?}"),
    }
}

#[when("I void the pending deposit")]
async fn void_pending_deposit(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID to void")
        .clone();
    let result = world
        .client
        .void_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(output) => {
            assert_eq!(
                output.status(),
                "voided",
                "Expected status 'voided' after voiding"
            );
            world.last_error = None;
        }
        Err(e) => panic!("Void deposit failed: {e:?}"),
    }
}

#[when(regex = r#"^I attempt to post deposit "([^"]*)"$"#)]
async fn attempt_post_deposit(world: &mut PbaWorld, deposit_id: String) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let result = world
        .client
        .post_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(_) => {
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "deposit_error".into(),
            });
        }
    }
}

#[when(regex = r#"^I attempt to void deposit "([^"]*)"$"#)]
async fn attempt_void_deposit(world: &mut PbaWorld, deposit_id: String) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let result = world
        .client
        .void_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(_) => {
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "deposit_error".into(),
            });
        }
    }
}

#[when("I attempt to post the pending deposit again")]
async fn attempt_post_again(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID")
        .clone();
    let result = world
        .client
        .post_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(_) => {
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "deposit_not_pending".into(),
            });
        }
    }
}

#[when("I attempt to void the pending deposit again")]
async fn attempt_void_again(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let deposit_id = world
        .last_deposit_id
        .as_ref()
        .expect("No deposit ID")
        .clone();
    let result = world
        .client
        .void_deposit()
        .account_id(&account_id)
        .deposit_id(&deposit_id)
        .send()
        .await;
    match result {
        Ok(_) => {
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "deposit_not_pending".into(),
            });
        }
    }
}

#[then("the operation should be rejected")]
async fn operation_rejected(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_some(),
        "Expected operation to be rejected, but no error was recorded"
    );
}

#[then(regex = r"^the pending self should be (\d+)$")]
async fn pending_self_should_be(world: &mut PbaWorld, expected: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let balance = world
        .client
        .get_balance()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get balance");
    assert_eq!(
        balance.pending_self(),
        expected,
        "Pending self mismatch: expected {} but got {}",
        expected,
        balance.pending_self()
    );
}

#[then(regex = r"^the pending others should be (\d+)$")]
async fn pending_others_should_be(world: &mut PbaWorld, expected: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let balance = world
        .client
        .get_balance()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get balance");
    assert_eq!(
        balance.pending_others(),
        expected,
        "Pending others mismatch: expected {} but got {}",
        expected,
        balance.pending_others()
    );
}

#[when(
    regex = r#"^I deposit (\d+) from IFSC "([^"]*)" account "([^"]*)" with funding type "([^"]*)"$"#
)]
async fn deposit_with_funding_type(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    funding_type: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .funding_type(pba_client::types::FundingType::from(funding_type.as_str()))
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_funding_type = Some(output.funding_type().to_string());
        }
        Err(e) => panic!("Deposit with funding type failed: {e:?}"),
    }
}

#[when(
    regex = r#"^I attempt to deposit (\d+) from IFSC "([^"]*)" account "([^"]*)" without funding type$"#
)]
async fn attempt_deposit_without_funding_type(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .deposit()
        .account_id(account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "funding_type_required".into(),
            });
        }
    }
}

#[when(
    regex = r#"^I attempt to deposit (\d+) from IFSC "([^"]*)" account "([^"]*)" with funding type "([^"]*)"$"#
)]
async fn attempt_deposit_with_funding_type(
    world: &mut PbaWorld,
    amount: i64,
    ifsc: String,
    account_number: String,
    funding_type: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let result = world
        .client
        .deposit()
        .account_id(&account_id)
        .source_ifsc(&ifsc)
        .source_account_number(&account_number)
        .amount(amount)
        .funding_type(pba_client::types::FundingType::from(funding_type.as_str()))
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_deposit_pool = Some(output.pool().to_string());
            world.last_deposit_id = Some(output.deposit_id().to_string());
            world.last_funding_type = Some(output.funding_type().to_string());
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(crate::PbaError {
                kind: format!("{e:?}"),
            });
        }
    }
}

#[then(regex = r#"^the funding type should be "([^"]*)"$"#)]
async fn funding_type_should_be(world: &mut PbaWorld, expected: String) {
    let ft = world
        .last_funding_type
        .as_ref()
        .expect("No funding type recorded");
    assert_eq!(
        ft, &expected,
        "Expected funding type '{expected}', got '{ft}'"
    );
}
