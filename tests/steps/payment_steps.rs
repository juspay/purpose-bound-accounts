use cucumber::{then, when};

use crate::PbaWorld;

#[when(regex = r#"^I pay (\d+) to merchant "([^"]*)" with MCC "([^"]*)" described as "([^"]*)"$"#)]
async fn make_payment(
    world: &mut PbaWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
    description: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .make_payment()
        .account_id(account_id)
        .amount(amount)
        .merchant_mcc(&mcc)
        .merchant_id(&merchant_id)
        .description(&description)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_payment = Some(crate::PaymentResult {
                amount: output.amount(),
                from_others: output.from_others(),
                from_self: output.from_self(),
            });
            world.last_error = None;
        }
        Err(e) => panic!("Payment failed unexpectedly: {e:?}"),
    }
}

#[when(
    regex = r#"^I attempt to pay (\d+) to merchant "([^"]*)" with MCC "([^"]*)" described as "([^"]*)"$"#
)]
async fn attempt_payment(
    world: &mut PbaWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
    description: String,
) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .make_payment()
        .account_id(account_id)
        .amount(amount)
        .merchant_mcc(&mcc)
        .merchant_id(&merchant_id)
        .description(&description)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_payment = Some(crate::PaymentResult {
                amount: output.amount(),
                from_others: output.from_others(),
                from_self: output.from_self(),
            });
            world.last_error = None;
        }
        Err(e) => {
            let err_str = format!("{e:?}");
            let kind = if err_str.contains("InvalidMcc") {
                "invalid_mcc"
            } else if err_str.contains("InsufficientFunds") {
                "insufficient_funds"
            } else if err_str.contains("422") {
                // Fallback: check the raw body for error type
                if err_str.contains("InvalidMcc") {
                    "invalid_mcc"
                } else {
                    "insufficient_funds"
                }
            } else {
                "unknown"
            };
            world.last_error = Some(crate::PbaError {
                kind: kind.to_string(),
            });
        }
    }
}

#[then("the payment should succeed")]
async fn payment_should_succeed(world: &mut PbaWorld) {
    assert!(
        world.last_payment.is_some(),
        "Expected payment to succeed, but no payment result"
    );
}

#[then(regex = r"^(\d+) should come from others-pool$")]
async fn from_others(world: &mut PbaWorld, expected: i64) {
    let payment = world.last_payment.as_ref().expect("No payment result");
    assert_eq!(
        payment.from_others, expected,
        "Others-pool contribution mismatch"
    );
}

#[then(regex = r"^(\d+) should come from self-pool$")]
async fn from_self(world: &mut PbaWorld, expected: i64) {
    let payment = world.last_payment.as_ref().expect("No payment result");
    assert_eq!(
        payment.from_self, expected,
        "Self-pool contribution mismatch"
    );
}

#[then("the payment should be rejected as insufficient funds")]
async fn payment_rejected_insufficient(world: &mut PbaWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected an error but got success (last_error is None)");
    assert_eq!(
        err.kind, "insufficient_funds",
        "Expected insufficient_funds but got: {}",
        err.kind
    );
}

#[then("the payment should be rejected as invalid MCC")]
async fn payment_rejected_invalid_mcc(world: &mut PbaWorld) {
    let err = world.last_error.as_ref().expect("Expected an error");
    assert_eq!(err.kind, "invalid_mcc");
}
