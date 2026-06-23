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
                payment_id: output.payment_id().to_string(),
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
                payment_id: output.payment_id().to_string(),
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
                message: None,
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
                payment_id: output.payment_id().to_string(),
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

#[when(
    regex = r#"^I pay (\d+) to merchant "([^"]*)" with MCC "([^"]*)" described as "([^"]*)" with idempotency key "([^"]*)"$"#
)]
async fn make_payment_with_idempotency_key(
    world: &mut PbaWorld,
    amount: i64,
    merchant_id: String,
    mcc: String,
    description: String,
    idempotency_key: String,
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
        .idempotency_key(&idempotency_key)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_payment = Some(crate::PaymentResult {
                payment_id: output.payment_id().to_string(),
                amount: output.amount(),
                from_others: output.from_others(),
                from_self: output.from_self(),
            });
            world.last_error = None;
        }
        Err(e) => panic!("Payment failed unexpectedly: {e:?}"),
    }
}

#[then("the payment response should include a payment_id")]
async fn payment_response_has_id(world: &mut PbaWorld) {
    let payment = world.last_payment.as_ref().expect("No payment result");
    assert!(
        !payment.payment_id.is_empty(),
        "Expected non-empty payment_id in response"
    );
    uuid::Uuid::parse_str(&payment.payment_id).unwrap_or_else(|e| {
        panic!(
            "payment_id `{}` is not a valid UUID: {e}",
            payment.payment_id
        )
    });
}

#[when("I remember the payment_id")]
#[then("I remember the payment_id")]
async fn remember_payment_id(world: &mut PbaWorld) {
    let payment = world.last_payment.as_ref().expect("No payment result");
    world.remembered_payment_id = Some(payment.payment_id.clone());
}

#[then("the payment_id should match the remembered payment_id")]
async fn payment_id_matches_remembered(world: &mut PbaWorld) {
    let payment = world.last_payment.as_ref().expect("No payment result");
    let remembered = world
        .remembered_payment_id
        .as_ref()
        .expect("No remembered payment_id");
    assert_eq!(
        &payment.payment_id, remembered,
        "Expected payment_id to match remembered (idempotency replay): got `{}`, expected `{}`",
        payment.payment_id, remembered
    );
}

#[when("I list transactions for the current account")]
async fn list_transactions_for_current_account(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("No account ID");
    let result = world
        .client
        .list_pb_account_transactions()
        .account_id(account_id)
        .send()
        .await
        .expect("Failed to list transactions for account");
    world.last_account_transactions_types = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.r#type().to_string())
            .collect(),
    );
    world.last_account_transactions_correlation_ids = Some(
        result
            .transactions()
            .iter()
            .map(|t| t.correlation_id().map(|s| s.to_string()))
            .collect(),
    );
}

// ── Refund step bindings ──────────────────────────────────────────────────────

// Helper: extract PascalCase error kind from the SDK error Debug string.
fn classify_refund_error(err_str: &str) -> &'static str {
    if err_str.contains("RefundNotRefundable") {
        "RefundNotRefundable"
    } else if err_str.contains("RefundAmountInvalid") {
        "RefundAmountInvalid"
    } else if err_str.contains("PaymentFullyRefunded") {
        "PaymentFullyRefunded"
    } else if err_str.contains("PbAccountNotActive") {
        "PbAccountNotActive"
    } else if err_str.contains("TransactionNotPending") {
        "TransactionNotPending"
    } else if err_str.contains("TransactionNotFound") {
        "TransactionNotFound"
    } else {
        "unknown"
    }
}

#[when(regex = r#"^I refund (\d+) paisa from the last payment$"#)]
async fn refund_last_payment(world: &mut PbaWorld, amount: i64) {
    // Save previous correlation for idempotency-replay assertion.
    world.previous_refund_correlation_id = world.last_refund_correlation_id.take();

    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let payment_id = world
        .last_payment
        .as_ref()
        .expect("No prior payment")
        .payment_id
        .clone();
    let result = world
        .client
        .refund_pb_account_payment()
        .account_id(account_id)
        .payment_id(payment_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_refund_correlation_id = Some(output.correlation_id().to_string());
            world.last_refund_amount_to_self = Some(output.amount_to_self());
            world.last_refund_amount_to_others = Some(output.amount_to_others());
            world.last_refund_remaining = Some(output.remaining_refundable());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I attempt to refund (\d+) paisa from the last payment$"#)]
async fn attempt_refund_last_payment(world: &mut PbaWorld, amount: i64) {
    refund_last_payment(world, amount).await;
}

#[when(regex = r#"^I refund (\d+) paisa from the last payment with idempotency key "([^"]*)"$"#)]
async fn refund_with_idem(world: &mut PbaWorld, amount: i64, key: String) {
    world.previous_refund_correlation_id = world.last_refund_correlation_id.take();

    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let payment_id = world
        .last_payment
        .as_ref()
        .expect("No prior payment")
        .payment_id
        .clone();
    let result = world
        .client
        .refund_pb_account_payment()
        .account_id(account_id)
        .payment_id(payment_id)
        .amount(amount)
        .idempotency_key(key)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_refund_correlation_id = Some(output.correlation_id().to_string());
            world.last_refund_amount_to_self = Some(output.amount_to_self());
            world.last_refund_amount_to_others = Some(output.amount_to_others());
            world.last_refund_remaining = Some(output.remaining_refundable());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I attempt to refund (\d+) paisa from the last refund$"#)]
async fn attempt_refund_last_refund(world: &mut PbaWorld, amount: i64) {
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let refund_id = world
        .last_refund_correlation_id
        .as_ref()
        .expect("No prior refund")
        .clone();
    let result = world
        .client
        .refund_pb_account_payment()
        .account_id(account_id)
        .payment_id(refund_id) // refund's correlation_id, not a real payment
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(_) => panic!("expected refund-of-refund to fail"),
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(
    regex = r#"^I attempt to refund (\d+) paisa from the last payment under a different PB account$"#
)]
async fn attempt_refund_wrong_account(world: &mut PbaWorld, amount: i64) {
    let payment_id = world
        .last_payment
        .as_ref()
        .expect("No prior payment")
        .payment_id
        .clone();
    // Use a freshly minted (and unused) UUID as the wrong account id.
    let wrong_account = uuid::Uuid::now_v7().to_string();
    let result = world
        .client
        .refund_pb_account_payment()
        .account_id(wrong_account)
        .payment_id(payment_id)
        .amount(amount)
        .send()
        .await;
    match result {
        Ok(_) => panic!("expected refund with wrong account to fail"),
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(
    regex = r#"^(\d+) concurrent refunds of (\d+) paisa each are attempted on the last payment$"#
)]
async fn concurrent_refunds(world: &mut PbaWorld, count: usize, amount: i64) {
    let account_id = world.account_id.clone().expect("No account ID");
    let payment_id = world
        .last_payment
        .as_ref()
        .expect("No prior payment")
        .payment_id
        .clone();
    let client = world.client.clone();

    let futures: Vec<_> = (0..count)
        .map(|_| {
            let client = client.clone();
            let account_id = account_id.clone();
            let payment_id = payment_id.clone();
            async move {
                client
                    .refund_pb_account_payment()
                    .account_id(&account_id)
                    .payment_id(&payment_id)
                    .amount(amount)
                    .send()
                    .await
            }
        })
        .collect();

    let results = join_all(futures).await;
    let mut successes = 0usize;
    let mut total_refunded = 0i64;
    for r in &results {
        if let Ok(out) = r {
            successes += 1;
            total_refunded += out.amount();
        }
    }
    world.concurrent_successes = Some(successes);
    world.concurrent_failures = Some(results.len() - successes);
    world.concurrent_refund_total_amount = Some(total_refunded);
}

#[then(regex = r#"^the total refunded amount across all refunds is at most (\d+) paisa$"#)]
async fn total_refunded_at_most(world: &mut PbaWorld, max: i64) {
    let t = world
        .concurrent_refund_total_amount
        .expect("No total refunded value");
    assert!(
        t <= max,
        "Expected total refunded ≤ {max} paisa, got {t} — concurrent refunds exceeded original payment"
    );
}

#[then(regex = r#"^the refund is successful$"#)]
async fn refund_success(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "unexpected error: {:?}",
        world.last_error
    );
    assert!(
        world.last_refund_correlation_id.is_some(),
        "no refund correlation id"
    );
}

#[then(regex = r#"^the refund credited (\d+) to self and (\d+) to others$"#)]
async fn refund_split(world: &mut PbaWorld, to_self: i64, to_others: i64) {
    assert_eq!(world.last_refund_amount_to_self, Some(to_self));
    assert_eq!(world.last_refund_amount_to_others, Some(to_others));
}

#[then(regex = r#"^the remaining refundable amount is (\d+)$"#)]
async fn remaining_is(world: &mut PbaWorld, expected: i64) {
    assert_eq!(world.last_refund_remaining, Some(expected));
}

#[then(regex = r#"^the refund fails with "([^"]*)"$"#)]
async fn refund_fails(world: &mut PbaWorld, expected_kind: String) {
    let err = world.last_error.as_ref().expect("expected an error");
    assert_eq!(
        err.kind, expected_kind,
        "got error message: {:?}",
        err.message
    );
}

#[then(regex = r#"^the refund fails with "([^"]*)" reason "([^"]*)"$"#)]
async fn refund_fails_with_reason(world: &mut PbaWorld, expected_kind: String, reason: String) {
    let err = world.last_error.as_ref().expect("expected an error");
    assert_eq!(err.kind, expected_kind);
    let msg = err.message.as_deref().unwrap_or("");
    assert!(
        msg.contains(&reason),
        "expected reason '{reason}' in message {msg:?}"
    );
}

#[then(regex = r#"^the refund error remaining field is (\d+)$"#)]
async fn refund_error_remaining(world: &mut PbaWorld, expected: i64) {
    let err = world.last_error.as_ref().expect("expected an error");
    let msg = err.message.as_deref().unwrap_or("");
    let needle = format!("remaining refundable {expected}");
    assert!(
        msg.contains(&needle),
        "expected '{needle}' in message {msg:?}"
    );
}

#[then(regex = r#"^both refunds share the same correlation_id$"#)]
async fn refunds_share_correlation(world: &mut PbaWorld) {
    let curr = world
        .last_refund_correlation_id
        .as_ref()
        .expect("no current refund");
    let prev = world
        .previous_refund_correlation_id
        .as_ref()
        .expect("no previous refund");
    assert_eq!(
        curr, prev,
        "idempotency replay produced a different correlation_id"
    );
}

#[when(regex = r#"^I initiate a pending refund of (\d+) paisa from the last payment$"#)]
async fn initiate_pending_refund(world: &mut PbaWorld, amount: i64) {
    world.previous_refund_correlation_id = world.last_refund_correlation_id.take();
    let account_id = world.account_id.as_ref().expect("No account ID").clone();
    let payment_id = world
        .last_payment
        .as_ref()
        .expect("No prior payment")
        .payment_id
        .clone();
    let result = world
        .client
        .refund_pb_account_payment()
        .account_id(account_id)
        .payment_id(payment_id)
        .amount(amount)
        .pending(true)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_refund_correlation_id = Some(out.correlation_id().to_string());
            world.last_refund_amount_to_self = Some(out.amount_to_self());
            world.last_refund_amount_to_others = Some(out.amount_to_others());
            world.last_refund_remaining = Some(out.remaining_refundable());
            world.last_refund_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[then(regex = r#"^the refund status is "([^"]*)"$"#)]
async fn refund_status_is(world: &mut PbaWorld, expected: String) {
    assert_eq!(world.last_refund_status.as_deref(), Some(expected.as_str()));
}

#[when(regex = r#"^I post the pending refund$"#)]
async fn post_pending_refund(world: &mut PbaWorld) {
    let account_id = world.account_id.clone().expect("no account");
    let refund_id = world.last_refund_correlation_id.clone().expect("no refund");
    let result = world
        .client
        .post_pb_account_refund()
        .account_id(&account_id)
        .refund_id(&refund_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_refund_status = Some(out.status().to_string());
            world.last_refund_remaining = Some(out.remaining_refundable());
            world.last_refund_amount_to_self = Some(out.amount_to_self());
            world.last_refund_amount_to_others = Some(out.amount_to_others());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I void the pending refund$"#)]
async fn void_pending_refund(world: &mut PbaWorld) {
    let account_id = world.account_id.clone().expect("no account");
    let refund_id = world.last_refund_correlation_id.clone().expect("no refund");
    let result = world
        .client
        .void_pb_account_refund()
        .account_id(&account_id)
        .refund_id(&refund_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_refund_status = Some(out.status().to_string());
            world.last_refund_remaining = Some(out.remaining_refundable());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_refund_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I attempt to void the pending refund$"#)]
async fn attempt_void_pending_refund(world: &mut PbaWorld) {
    void_pending_refund(world).await;
}

#[when(
    regex = r#"^(\d+) concurrent pending refunds of (\d+) paisa each are attempted on the last payment$"#
)]
async fn concurrent_pending_refunds(world: &mut PbaWorld, count: usize, amount: i64) {
    let account_id = world.account_id.clone().expect("No account ID");
    let payment_id = world
        .last_payment
        .as_ref()
        .expect("No prior payment")
        .payment_id
        .clone();
    let client = world.client.clone();
    let futures: Vec<_> = (0..count)
        .map(|_| {
            let client = client.clone();
            let account_id = account_id.clone();
            let payment_id = payment_id.clone();
            async move {
                client
                    .refund_pb_account_payment()
                    .account_id(&account_id)
                    .payment_id(&payment_id)
                    .amount(amount)
                    .pending(true)
                    .send()
                    .await
            }
        })
        .collect();
    let results = futures::future::join_all(futures).await;
    let mut successes = 0usize;
    let mut total = 0i64;
    for r in &results {
        if let Ok(out) = r {
            successes += 1;
            total += out.amount();
        }
    }
    world.concurrent_successes = Some(successes);
    world.concurrent_failures = Some(results.len() - successes);
    world.concurrent_refund_total_amount = Some(total);
}

// ── End of refund step bindings ───────────────────────────────────────────────

#[then("the payment legs share correlation_id equal to the payment_id")]
async fn payment_legs_share_correlation_id(world: &mut PbaWorld) {
    let payment = world.last_payment.as_ref().expect("No payment result");
    let types = world
        .last_account_transactions_types
        .as_ref()
        .expect("Need to list transactions for the current account first");
    let corr_ids = world
        .last_account_transactions_correlation_ids
        .as_ref()
        .expect("Need to list transactions for the current account first");

    let payment_correlation_ids: Vec<&Option<String>> = types
        .iter()
        .zip(corr_ids.iter())
        .filter(|(t, _)| t.as_str() == "payment")
        .map(|(_, c)| c)
        .collect();

    assert!(
        !payment_correlation_ids.is_empty(),
        "Expected at least one payment transaction in account listing"
    );
    for (i, c) in payment_correlation_ids.iter().enumerate() {
        let actual = c
            .as_deref()
            .unwrap_or_else(|| panic!("Payment leg {i} has no correlation_id"));
        assert_eq!(
            actual, payment.payment_id,
            "Payment leg {i} has correlation_id `{}`, expected `{}` (payment_id)",
            actual, payment.payment_id
        );
    }
}
