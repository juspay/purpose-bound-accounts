Feature: Two-phase reversal of normal -> PB transfers
  A reversal may be initiated as Pending with a timeout, then committed via
  post or rolled back via void using the existing /transfers/{id}/post|void
  endpoints (because reversal rows are transaction_type='transfer').

  @api
  Scenario: Pending reversal then post credits the source only after post
    Given a normal account exists for holder "tr2p-s01-alice"
    And the normal account has balance 30000
    And a "health" account exists for holder "tr2p-s01-alice" with origin IFSC "HDFC0091001" and account number "9091001001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 10000 paisa on the last transfer
    Then the reversal status is "pending"
    And the normal account balance is 0
    When I post the pending reversal
    Then the reversal status is "posted"
    And the normal account balance is 10000

  @api
  Scenario: Pending reversal then void leaves balances unchanged and original re-reversible
    Given a normal account exists for holder "tr2p-s02-bob"
    And the normal account has balance 20000
    And a "health" account exists for holder "tr2p-s02-bob" with origin IFSC "HDFC0091002" and account number "9091002001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 20000 paisa on the last transfer
    And I void the pending reversal
    Then the reversal status is "voided"
    And the normal account balance is 0
    When I initiate a reversal of 20000 paisa on the last transfer
    Then the reversal is successful
    And the normal account balance is 20000

  @api
  Scenario: Pending reversal blocks a second reversal attempt on the same transfer
    Given a normal account exists for holder "tr2p-s03-carol"
    And the normal account has balance 50000
    And a "health" account exists for holder "tr2p-s03-carol" with origin IFSC "HDFC0091003" and account number "9091003001"
    When I transfer 50000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 25000 paisa on the last transfer
    And I attempt a reversal of 25000 paisa on the last transfer
    Then the reversal fails with "TransferAlreadyReversed"

  @api
  Scenario: Mixed-direction post-then-void rejected
    Given a normal account exists for holder "tr2p-s04-dan"
    And the normal account has balance 10000
    And a "health" account exists for holder "tr2p-s04-dan" with origin IFSC "HDFC0091004" and account number "9091004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 10000 paisa on the last transfer
    And I post the pending reversal
    And I attempt to void the reversal
    Then the operation fails with "TransactionNotPending"

  @api
  Scenario: Pending reversal with short timeout ages out and frees re-reversal
    Given a normal account exists for holder "tr2p-s05-eve"
    And the normal account has balance 20000
    And a "health" account exists for holder "tr2p-s05-eve" with origin IFSC "HDFC0091005" and account number "9091005001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending reversal of 20000 paisa on the last transfer with timeout 1 second
    And I wait 3 seconds for the timeout poller
    Then the last reversal has status "voided"
    When I initiate a reversal of 20000 paisa on the last transfer
    Then the reversal is successful
