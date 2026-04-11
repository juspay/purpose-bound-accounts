use cucumber::{then, when};

use crate::PbaWorld;

#[when("I list all purpose types")]
async fn list_purpose_types(world: &mut PbaWorld) {
    let result = world.client.list_purpose_types().send().await;
    match result {
        Ok(output) => {
            world.purpose_types_count = Some(output.purpose_types().len());
        }
        Err(e) => panic!("Failed to list purpose types: {e:?}"),
    }
}

#[then(regex = r"^I should see at least (\d+) purpose types$")]
async fn should_see_purpose_types(world: &mut PbaWorld, min_count: usize) {
    let count = world.purpose_types_count.expect("No purpose types listed");
    assert!(
        count >= min_count,
        "Expected at least {min_count} purpose types, got {count}"
    );
}

#[when(regex = r#"^I get the "([^"]*)" purpose type$"#)]
async fn get_purpose_type(world: &mut PbaWorld, purpose_code: String) {
    let result = world
        .client
        .get_purpose_type()
        .purpose_code(&purpose_code)
        .send()
        .await;
    match result {
        Ok(output) => {
            world.last_purpose_code = Some(output.purpose_code().to_string());
            world.last_purpose_mccs_count = Some(output.allowed_mccs().len());
        }
        Err(e) => panic!("Failed to get purpose type: {e:?}"),
    }
}

#[then(regex = r#"^the purpose code should be "([^"]*)"$"#)]
async fn purpose_code_should_be(world: &mut PbaWorld, expected: String) {
    let actual = world.last_purpose_code.as_ref().expect("No purpose code");
    assert_eq!(actual, &expected);
}

#[then("it should have allowed MCCs")]
async fn should_have_mccs(world: &mut PbaWorld) {
    let count = world
        .last_purpose_mccs_count
        .expect("No MCC count available");
    assert!(count > 0, "Expected at least 1 allowed MCC, got 0");
}
