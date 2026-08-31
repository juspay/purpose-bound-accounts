Feature: Contribution return
  Admin returns others-pool contributions (trust or third_party) to their
  contributors. Return rows are TransactionType::Withdrawal in the others
  pool, linked via reverses_transaction_id to specific originals. Multiple
  partial returns per original are allowed; FIFO across originals when
  a single call draws from more than one.

  @api
  Scenario: Full return of a single trust contribution
    Given a normal account exists for holder "cr-s01-alice"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-s01-alice" with origin IFSC "HDFC0080001" and account number "8080001001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I return 20000 paisa of "trust" contributions
    Then the return is successful
    And the return status is "settled"
    And the return has 1 allocation
    And allocation 1 is for 20000 paisa
    And the return remaining_returnable_after is 0
    And the normal account balance is 20000

  @api
  Scenario: Full return of a single third-party contribution
    Given a "health" account exists for holder "cr-s02-bob" with origin IFSC "HDFC0080002" and account number "8080002001"
    And the PB account receives 15000 paisa via a third-party deposit
    When I return 15000 paisa of "third_party" contributions
    Then the return is successful
    And the return status is "settled"
    And the return has 1 allocation
    And allocation 1 is for 15000 paisa
    And the PB account others-pool balance is 0

  @api
  Scenario: Partial return of a single trust contribution
    Given a normal account exists for holder "cr-s03-carol"
    And the normal account has balance 30000
    And a "health" account exists for holder "cr-s03-carol" with origin IFSC "HDFC0080003" and account number "8080003001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I return 10000 paisa of "trust" contributions
    Then the return is successful
    And the return remaining_returnable_after is 20000
    And the normal account balance is 10000

  @api
  Scenario: FIFO across two trust contributions
    Given a normal account exists for holder "cr-s04-dan"
    And the normal account has balance 30000
    And a "health" account exists for holder "cr-s04-dan" with origin IFSC "HDFC0080004" and account number "8080004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I transfer 15000 paisa from the normal account to the PB account
    And I return 20000 paisa of "trust" contributions
    Then the return is successful
    And the return has 2 allocations
    And allocation 1 is for 10000 paisa
    And allocation 2 is for 10000 paisa
    And the return remaining_returnable_after is 5000

  @api
  Scenario: Return amount exceeding remaining is rejected
    Given a normal account exists for holder "cr-s05-eve"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr-s05-eve" with origin IFSC "HDFC0080005" and account number "8080005001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I attempt to return 15000 paisa of "trust" contributions
    Then the return fails with "ContributionAmountInvalid"

  @api
  Scenario: Return of zero is rejected
    Given a normal account exists for holder "cr-s06-flo"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr-s06-flo" with origin IFSC "HDFC0080006" and account number "8080006001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I attempt to return 0 paisa of "trust" contributions
    Then the return fails with "ContributionAmountInvalid"

  @api
  Scenario: Return on account with no matching originals is rejected
    Given a "health" account exists for holder "cr-s07-gus" with origin IFSC "HDFC0080007" and account number "8080007001"
    When I attempt to return 5000 paisa of "trust" contributions
    Then the return fails with "ContributionFullyReturned"

  @api
  Scenario: Trust and third-party pools are independent
    Given a normal account exists for holder "cr-s08-han"
    And the normal account has balance 15000
    And a "health" account exists for holder "cr-s08-han" with origin IFSC "HDFC0080008" and account number "8080008001"
    And the PB account receives 12000 paisa via a third-party deposit
    When I transfer 15000 paisa from the normal account to the PB account
    And I return 15000 paisa of "trust" contributions
    Then the return is successful
    When I fetch the contribution summary
    Then the trust remaining_returnable is 0
    And the third_party remaining_returnable is 12000

  @api
  Scenario: Frozen PB account rejects return; reactivation allows it
    Given a normal account exists for holder "cr-s09-ivy"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-s09-ivy" with origin IFSC "HDFC0080009" and account number "8080009001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I freeze the account
    And I attempt to return 10000 paisa of "trust" contributions
    Then the return fails with "PbAccountNotActive"
    When I reactivate the account
    And I return 10000 paisa of "trust" contributions
    Then the return is successful

  @api
  Scenario: Idempotency replay returns the same correlation_id
    Given a normal account exists for holder "cr-s10-jay"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-s10-jay" with origin IFSC "HDFC0080010" and account number "8080010001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I return 10000 paisa of "trust" contributions with idempotency key "cr-idem-jay-1"
    Then the return is successful
    When I return 10000 paisa of "trust" contributions with idempotency key "cr-idem-jay-1"
    Then the return is successful
    And both returns share the same correlation_id

  @api
  Scenario: Concurrent pending returns reserve remaining
    Given a "health" account exists for holder "cr-s11-lyn" with origin IFSC "HDFC0080011" and account number "8080011001"
    And the PB account receives 5000 paisa via a third-party deposit
    When 5 concurrent pending returns of 300 paisa each of "third_party" contributions are attempted
    Then the total returned amount across all returns is at most 5000 paisa
