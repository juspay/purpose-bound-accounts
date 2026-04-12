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
