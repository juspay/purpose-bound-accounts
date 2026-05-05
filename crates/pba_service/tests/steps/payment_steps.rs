use cucumber::{then, when};
use futures::future::join_all;

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
            let kind = if err_str.contains("AccountNotActive") || err_str.contains("409") {
                "account_not_active"
            } else if err_str.contains("InvalidMcc") {
                "invalid_mcc"
            } else if err_str.contains("InsufficientFunds") {
                "insufficient_funds"
            } else if err_str.contains("422") {
                "insufficient_funds"
            } else {
                "unknown"
            };
            world.last_error = Some(crate::PbaError {
                kind: kind.to_string(),
            });
        }
    }
}

#[when(
    regex = r#"^I pay (\d+) to merchant "([^"]*)" with MCC "([^"]*)" described as "([^"]*)" with gateway ref "([^"]*)"$"#
)]
async fn make_payment_with_gateway_ref(
    world: &mut PbaWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
    description: String,
    gateway_ref: String,
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
        .gateway_ref(&gateway_ref)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_payment = Some(crate::PaymentResult {
                amount: output.amount(),
                from_others: output.from_others(),
                from_self: output.from_self(),
            });
            world.last_payment_gateway_ref = output.gateway_ref().map(|s| s.to_string());
            world.last_error = None;
        }
        Err(e) => panic!("Payment failed unexpectedly: {e:?}"),
    }
}

#[then(regex = r#"^the payment response should echo gateway ref "([^"]*)"$"#)]
async fn payment_echoes_gateway_ref(world: &mut PbaWorld, expected: String) {
    let actual = world
        .last_payment_gateway_ref
        .as_deref()
        .expect("No gateway_ref captured from last payment response");
    assert_eq!(
        actual, expected,
        "Gateway ref mismatch: response echoed `{}`, expected `{}`",
        actual, expected
    );
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

#[then("the payment should be rejected as account not active")]
async fn payment_rejected_not_active(world: &mut PbaWorld) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected an error but got success");
    assert_eq!(
        err.kind, "account_not_active",
        "Expected account_not_active but got: {}",
        err.kind
    );
}

#[when(regex = r#"^(\d+) concurrent payments of (\d+) each are made to MCC "([^"]*)"$"#)]
async fn concurrent_payments(world: &mut PbaWorld, count: usize, amount: i64, mcc: String) {
    let account_id = world.account_id.clone().expect("No account ID");
    let client = world.client.clone();

    let futures: Vec<_> = (0..count)
        .map(|i| {
            let client = client.clone();
            let account_id = account_id.clone();
            let mcc = mcc.clone();
            async move {
                client
                    .make_payment()
                    .account_id(&account_id)
                    .amount(amount)
                    .merchant_mcc(&mcc)
                    .merchant_id(&format!("CONCURRENT{i:03}"))
                    .description("concurrent test")
                    .send()
                    .await
            }
        })
        .collect();

    let results = join_all(futures).await;
    let successes = results.iter().filter(|r| r.is_ok()).count();
    let failures = results.iter().filter(|r| r.is_err()).count();

    world.concurrent_successes = Some(successes);
    world.concurrent_failures = Some(failures);
}

#[then(regex = r"^exactly (\d+) payments should succeed$")]
async fn exactly_n_succeed(world: &mut PbaWorld, expected: usize) {
    let successes = world
        .concurrent_successes
        .expect("No concurrent payment results");
    assert_eq!(
        successes, expected,
        "Expected {expected} successes but got {successes}"
    );
}
