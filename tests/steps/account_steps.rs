use cucumber::{given, then, when};
use pba_client::types::Status;

use crate::PbaWorld;

#[given(regex = r#"^a "([^"]*)" account exists for holder "([^"]*)" with origin IFSC "([^"]*)" and account number "([^"]*)"$"#)]
async fn create_account_given(
    world: &mut PbaWorld,
    purpose: String,
    holder_id: String,
    ifsc: String,
    account_number: String,
) {
    let result = world
        .client
        .create_account()
        .holder_id(&holder_id)
        .purpose_code(&purpose)
        .origin_ifsc(&ifsc)
        .origin_account_number(&account_number)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.account_id = Some(output.id().to_string());
            world.last_account_status = Some(output.status().to_string());
        }
        Err(e) => {
            // If duplicate, the account already exists — this is fine for a Given step.
            // We can't easily look it up by origin, so panic on non-duplicate errors.
            let err_str = format!("{e:?}");
            if !err_str.contains("DuplicateAccount") && !err_str.contains("409") {
                panic!("Failed to create account: {e:?}");
            }
            // For duplicate, we need to find the existing account. Since we don't have
            // a lookup-by-origin API, we'll panic with a helpful message.
            panic!(
                "Account already exists (duplicate). Tests must use unique origin bank details \
                 per scenario, or the DB must be reset between runs. Error: {e:?}"
            );
        }
    }
}

#[when(regex = r#"^I create a "([^"]*)" account for holder "([^"]*)" with origin IFSC "([^"]*)" and account number "([^"]*)"$"#)]
async fn create_account_when(
    world: &mut PbaWorld,
    purpose: String,
    holder_id: String,
    ifsc: String,
    account_number: String,
) {
    let result = world
        .client
        .create_account()
        .holder_id(&holder_id)
        .purpose_code(&purpose)
        .origin_ifsc(&ifsc)
        .origin_account_number(&account_number)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.account_id = Some(output.id().to_string());
            world.last_account_status = Some(output.status().to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Failed to create account: {e:?}"),
    }
}

#[when(regex = r#"^I create a duplicate "([^"]*)" account for holder "([^"]*)" with origin IFSC "([^"]*)" and account number "([^"]*)"$"#)]
async fn create_duplicate_account(
    world: &mut PbaWorld,
    purpose: String,
    holder_id: String,
    ifsc: String,
    account_number: String,
) {
    let result = world
        .client
        .create_account()
        .holder_id(&holder_id)
        .purpose_code(&purpose)
        .origin_ifsc(&ifsc)
        .origin_account_number(&account_number)
        .send()
        .await;
    world.duplicate_rejected = result.is_err();
}

#[then("the account should be created successfully")]
async fn account_created(world: &mut PbaWorld) {
    assert!(
        world.account_id.is_some(),
        "Account should have been created"
    );
}

#[then(regex = r#"^the account purpose should be "([^"]*)"$"#)]
async fn account_purpose_should_be(world: &mut PbaWorld, expected: String) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .get_account()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get account");
    assert_eq!(output.purpose_code(), expected);
}

#[then(regex = r#"^the account status should be "([^"]*)"$"#)]
async fn account_status_should_be(world: &mut PbaWorld, expected: String) {
    let status = world
        .last_account_status
        .as_ref()
        .expect("No account status");
    assert_eq!(status, &expected);
}

#[when("I get the account")]
async fn get_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .get_account()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get account");
    world.last_account_status = Some(output.status().to_string());
}

#[when("I get the account balance")]
async fn get_balance(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .get_balance()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to get balance");
    world.last_balance = Some(crate::BalanceResult {
        self_contribution: output.self_contribution(),
        others_contribution: output.others_contribution(),
        total: output.total(),
    });
}

#[when("I freeze the account")]
async fn freeze_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .update_account_status()
        .account_id(account_id)
        .status(Status::Frozen)
        .send()
        .await
        .expect("Failed to freeze account");
    world.last_account_status = Some(output.status().to_string());
}

#[when("I reactivate the account")]
async fn reactivate_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .update_account_status()
        .account_id(account_id)
        .status(Status::Active)
        .send()
        .await
        .expect("Failed to reactivate account");
    world.last_account_status = Some(output.status().to_string());
}

#[when("I close the account")]
async fn close_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let output = world
        .client
        .update_account_status()
        .account_id(account_id)
        .status(Status::Closed)
        .send()
        .await
        .expect("Failed to close account");
    world.last_account_status = Some(output.status().to_string());
}

#[given("the account is frozen")]
async fn given_account_frozen(world: &mut PbaWorld) {
    freeze_account(world).await;
}

#[given("the account is closed")]
async fn given_account_closed(world: &mut PbaWorld) {
    close_account(world).await;
}

#[then("the duplicate should be rejected")]
async fn duplicate_rejected(world: &mut PbaWorld) {
    assert!(
        world.duplicate_rejected,
        "Expected duplicate account to be rejected"
    );
}
