Feature: Normal account lifecycle

  Scenario: Create a normal account with no origin bank
    When I create a normal account for holder "alice"
    Then the response is successful
    And the normal account holder_id is "alice"

  Scenario: Create a normal account with an origin bank
    When I create a normal account for holder "bob" with origin "HDFC0001234" and account "1111111111"
    Then the response is successful
    And the normal account origin_ifsc is "HDFC0001234"

  Scenario: Deposit to normal account credits the balance
    Given a normal account exists for holder "dan"
    When I deposit 5000 paisa to the normal account
    Then the deposit is successful
    And the normal account balance is 5000

  Scenario: Pending deposit + post lifecycle
    Given a normal account exists for holder "ed"
    When I create a pending deposit of 7500 paisa to the normal account with timeout 120
    Then the deposit status is "pending"
    When I post the normal account deposit
    Then the deposit status is "posted"
    And the normal account balance is 7500

  Scenario: Pending deposit + void
    Given a normal account exists for holder "fay"
    When I create a pending deposit of 9000 paisa to the normal account with timeout 120
    And I void the normal account deposit
    Then the deposit status is "voided"
    And the normal account balance is 0

  Scenario: Withdraw from normal account
    Given a normal account exists for holder "gus"
    And the normal account has balance 4000
    When I withdraw 2500 paisa from the normal account
    Then the withdrawal is successful
    And the normal account balance is 1500

  Scenario: Withdraw rejected when insufficient
    Given a normal account exists for holder "han"
    And the normal account has balance 100
    When I withdraw 500 paisa from the normal account
    Then the withdrawal fails with "InsufficientFunds"
    And the normal account balance is 100

  Scenario: Frozen normal account rejects deposits
    Given a normal account exists for holder "ira"
    When I freeze the normal account
    And I deposit 1000 paisa to the normal account
    Then the deposit fails with "NormalAccountNotActive"

  Scenario: Frozen normal account rejects withdrawals
    Given a normal account exists for holder "joy"
    And the normal account has balance 5000
    When I freeze the normal account
    And I withdraw 1000 paisa from the normal account
    Then the withdrawal fails with "NormalAccountNotActive"

  Scenario: Idempotency replay on deposit
    Given a normal account exists for holder "ken"
    When I deposit 1000 paisa to the normal account with idempotency key "k1"
    And I retry the same deposit with idempotency key "k1"
    Then both deposits return the same id
    And the normal account balance is 1000
