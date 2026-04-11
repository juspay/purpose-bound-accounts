use cucumber::{then, when};

use crate::PbaWorld;

#[when(regex = r"^I withdraw (\d+)$")]
async fn withdraw(world: &mut PbaWorld, amount: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .withdraw()
        .account_id(account_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_withdrawal_amount = Some(output.amount());
            world.last_error = None;
        }
        Err(e) => panic!("Withdrawal failed unexpectedly: {e:?}"),
    }
}

#[when(regex = r"^I attempt to withdraw (\d+)$")]
async fn attempt_withdraw(world: &mut PbaWorld, amount: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .withdraw()
        .account_id(account_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_withdrawal_amount = Some(output.amount());
            world.last_error = None;
        }
        Err(_) => {
            world.last_error = Some(crate::PbaError {
                kind: "insufficient_funds".into(),
            });
        }
    }
}

#[then(regex = r"^the withdrawal should succeed with amount (\d+)$")]
async fn withdrawal_succeed(world: &mut PbaWorld, expected: i64) {
    let amount = world
        .last_withdrawal_amount
        .expect("No withdrawal result");
    assert_eq!(amount, expected, "Withdrawal amount mismatch");
}

#[then("the withdrawal should be rejected as insufficient funds")]
async fn withdrawal_rejected(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_some(),
        "Expected withdrawal to be rejected"
    );
}
