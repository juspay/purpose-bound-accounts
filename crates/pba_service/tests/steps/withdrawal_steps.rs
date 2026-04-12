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
        Err(e) => {
            let err_str = format!("{e:?}");
            let kind = if err_str.contains("AccountNotActive") || err_str.contains("409") {
                "account_not_active"
            } else {
                "insufficient_funds"
            };
            world.last_error = Some(crate::PbaError {
                kind: kind.to_string(),
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
    let err = world.last_error.as_ref().expect("Expected withdrawal to be rejected");
    assert_eq!(err.kind, "insufficient_funds");
}

#[then("the withdrawal should be rejected as account not active")]
async fn withdrawal_rejected_not_active(world: &mut PbaWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected withdrawal to be rejected");
    assert_eq!(
        err.kind, "account_not_active",
        "Expected account_not_active but got: {}",
        err.kind
    );
}
