use cucumber::{given, then, when};

use crate::PbaWorld;

fn classify_return_error(err_str: &str) -> &'static str {
    if err_str.contains("ContributionAmountInvalid") {
        "ContributionAmountInvalid"
    } else if err_str.contains("ContributionFullyReturned") {
        "ContributionFullyReturned"
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

#[when(regex = r#"^I return (\d+) paisa of "([^"]+)" contributions$"#)]
async fn return_contribution(world: &mut PbaWorld, amount: i64, funding_type: String) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let result = world
        .client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type(pba_client::types::FundingType::from(funding_type.as_str()))
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_correlation_id = Some(out.correlation_id().to_string());
            world.last_return_status = Some(out.status().to_string());
            world.last_return_amount = Some(out.amount());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            let allocations: Vec<(String, i64)> = out
                .allocations()
                .iter()
                .map(|a| (a.original_transaction_id().to_string(), a.amount()))
                .collect();
            world.last_return_allocations_count = Some(allocations.len());
            world.last_return_allocations = Some(allocations);
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I initiate a pending return of (\d+) paisa of "([^"]+)" contributions$"#)]
async fn initiate_pending_return(world: &mut PbaWorld, amount: i64, funding_type: String) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let result = world
        .client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type(pba_client::types::FundingType::from(funding_type.as_str()))
        .pending(true)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_correlation_id = Some(out.correlation_id().to_string());
            world.last_return_status = Some(out.status().to_string());
            world.last_return_amount = Some(out.amount());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when(regex = r#"^I attempt to return (\d+) paisa of "([^"]+)" contributions$"#)]
async fn attempt_return(world: &mut PbaWorld, amount: i64, funding_type: String) {
    return_contribution(world, amount, funding_type).await;
}

#[when("I post the pending return")]
async fn post_pending_return(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let return_id = world
        .last_return_correlation_id
        .as_ref()
        .expect("no return id")
        .clone();
    let result = world
        .client
        .post_pb_account_contribution_return()
        .account_id(&account_id)
        .return_id(&return_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_status = Some(out.status().to_string());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when("I void the pending return")]
async fn void_pending_return(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let return_id = world
        .last_return_correlation_id
        .as_ref()
        .expect("no return id")
        .clone();
    let result = world
        .client
        .void_pb_account_contribution_return()
        .account_id(&account_id)
        .return_id(&return_id)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_status = Some(out.status().to_string());
            world.last_return_remaining_after = Some(out.remaining_returnable_after());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[when("I attempt to void the pending return")]
async fn attempt_void_pending_return(world: &mut PbaWorld) {
    void_pending_return(world).await;
}

#[when(regex = r#"^I fetch the contribution summary$"#)]
async fn fetch_summary(world: &mut PbaWorld) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let out = world
        .client
        .get_pb_account_contribution_summary()
        .account_id(&account_id)
        .send()
        .await
        .expect("summary fetch failed");
    let trust = out.trust();
    world.contribution_summary_trust_remaining = Some(trust.remaining_returnable());
    let third_party = out.third_party();
    world.contribution_summary_third_party_remaining = Some(third_party.remaining_returnable());
}

#[then("the return is successful")]
async fn return_success(world: &mut PbaWorld) {
    assert!(
        world.last_error.is_none(),
        "unexpected error: {:?}",
        world.last_error
    );
    assert!(
        world.last_return_correlation_id.is_some(),
        "no return correlation_id"
    );
}

#[then(regex = r#"^the return status is "([^"]+)"$"#)]
async fn return_status_is(world: &mut PbaWorld, expected: String) {
    assert_eq!(world.last_return_status.as_deref(), Some(expected.as_str()));
}

#[then(regex = r#"^the return remaining_returnable_after is (\d+)$"#)]
async fn return_remaining_is(world: &mut PbaWorld, expected: i64) {
    assert_eq!(world.last_return_remaining_after, Some(expected));
}

#[then(regex = r#"^the return has (\d+) allocation(?:s)?$"#)]
async fn return_allocations_count(world: &mut PbaWorld, expected: usize) {
    assert_eq!(world.last_return_allocations_count, Some(expected));
}

#[then(regex = r#"^allocation (\d+) is for (\d+) paisa$"#)]
async fn allocation_n_amount(world: &mut PbaWorld, index_1based: usize, amount: i64) {
    let allocations = world
        .last_return_allocations
        .as_ref()
        .expect("no allocations");
    let entry = allocations
        .get(index_1based - 1)
        .expect("allocation index out of range");
    assert_eq!(entry.1, amount);
}

#[then(regex = r#"^the return fails with "([^"]+)"$"#)]
async fn return_fails_with(world: &mut PbaWorld, kind: String) {
    let e = world.last_error.as_ref().expect("no error captured");
    assert_eq!(e.kind, kind);
}

#[then(regex = r#"^the trust remaining_returnable is (\d+)$"#)]
async fn trust_remaining_is(world: &mut PbaWorld, expected: i64) {
    assert_eq!(world.contribution_summary_trust_remaining, Some(expected));
}

#[then(regex = r#"^the third_party remaining_returnable is (\d+)$"#)]
async fn third_party_remaining_is(world: &mut PbaWorld, expected: i64) {
    assert_eq!(
        world.contribution_summary_third_party_remaining,
        Some(expected)
    );
}

#[when(
    regex = r#"^(\d+) concurrent pending returns of (\d+) paisa each of "([^"]+)" contributions are attempted$"#
)]
async fn concurrent_pending_returns(
    world: &mut PbaWorld,
    count: usize,
    amount: i64,
    funding_type: String,
) {
    let account_id = world.account_id.clone().expect("no account id");
    let client = world.client.clone();
    let futures: Vec<_> = (0..count)
        .map(|_| {
            let client = client.clone();
            let account_id = account_id.clone();
            let ft = funding_type.clone();
            async move {
                client
                    .return_pb_account_contribution()
                    .account_id(&account_id)
                    .amount(amount)
                    .funding_type(pba_client::types::FundingType::from(ft.as_str()))
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
    // Reuse the existing concurrent_refund_total_amount field for the sum;
    // the field name is a legacy from PR #42 but the semantic (total success
    // amount) applies here too.
    world.concurrent_refund_total_amount = Some(total);
}

#[then(regex = r#"^the total returned amount across all returns is at most (\d+) paisa$"#)]
async fn total_returned_at_most(world: &mut PbaWorld, max: i64) {
    let t = world
        .concurrent_refund_total_amount
        .expect("no total returned value");
    assert!(t <= max, "expected total returned <= {max}, got {t}");
}

#[given(regex = r#"^the PB account receives (\d+) paisa via a third-party deposit$"#)]
async fn pb_receives_third_party_deposit(world: &mut PbaWorld, amount: i64) {
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    world
        .client
        .deposit_to_pb_account()
        .account_id(&account_id)
        .amount(amount)
        .source_ifsc("HDFC0009999")
        .source_account_number("9999999999")
        .funding_type(pba_client::types::FundingType::from("third_party"))
        .send()
        .await
        .expect("third-party deposit failed");
}

#[when(
    regex = r#"^I return (\d+) paisa of "([^"]+)" contributions with idempotency key "([^"]+)"$"#
)]
async fn return_with_idem(
    world: &mut PbaWorld,
    amount: i64,
    funding_type: String,
    key: String,
) {
    world.previous_return_correlation_id = world.last_return_correlation_id.take();
    let account_id = world.account_id.as_ref().expect("no account id").clone();
    let result = world
        .client
        .return_pb_account_contribution()
        .account_id(&account_id)
        .amount(amount)
        .funding_type(pba_client::types::FundingType::from(funding_type.as_str()))
        .idempotency_key(&key)
        .send()
        .await;
    match result {
        Ok(out) => {
            world.last_return_correlation_id = Some(out.correlation_id().to_string());
            world.last_return_status = Some(out.status().to_string());
            world.last_error = None;
        }
        Err(e) => {
            let s = format!("{e:?}");
            world.last_error = Some(crate::PbaError {
                kind: classify_return_error(&s).to_string(),
                message: Some(s),
            });
        }
    }
}

#[then("both returns share the same correlation_id")]
async fn both_returns_same_correlation(world: &mut PbaWorld) {
    let prev = world
        .previous_return_correlation_id
        .as_ref()
        .expect("no previous return correlation_id");
    let now = world
        .last_return_correlation_id
        .as_ref()
        .expect("no current return correlation_id");
    assert_eq!(prev, now, "idempotency failed: correlation_ids differ");
}
