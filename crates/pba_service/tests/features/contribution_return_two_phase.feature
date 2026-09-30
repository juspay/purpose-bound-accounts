Feature: Two-phase contribution return
  A contribution return may be initiated as Pending with a timeout, then
  posted (commits) or voided (rolls back). Pending returns reserve their
  slice of the remaining_returnable so concurrent initiates don't
  over-return.

  @api
  Scenario: Pending return then post credits the source only after post
    Given a normal account exists for holder "cr2p-s01-alice"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr2p-s01-alice" with origin IFSC "HDFC0081001" and account number "8081001001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending return of 15000 paisa of "trust" contributions
    Then the return status is "pending"
    And the normal account balance is 0
    When I post the pending return
    Then the return status is "settled"
    And the normal account balance is 15000

  @api
  Scenario: Pending return then void restores remaining
    Given a normal account exists for holder "cr2p-s02-bob"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr2p-s02-bob" with origin IFSC "HDFC0081002" and account number "8081002001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I initiate a pending return of 15000 paisa of "trust" contributions
    And I void the pending return
    Then the return status is "voided"
    When I fetch the contribution summary
    Then the trust remaining_returnable is 20000

  @api
  Scenario: Pending return blocks a second return that would exceed reserved
    Given a normal account exists for holder "cr2p-s03-carol"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s03-carol" with origin IFSC "HDFC0081003" and account number "8081003001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 6000 paisa of "trust" contributions
    And I attempt to return 5000 paisa of "trust" contributions
    Then the return fails with "ContributionAmountInvalid"

  @api
  Scenario: Post on already-posted return is a no-op
    Given a normal account exists for holder "cr2p-s04-dan"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s04-dan" with origin IFSC "HDFC0081004" and account number "8081004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions
    And I post the pending return
    And I post the pending return
    Then the return status is "settled"

  @api
  Scenario: Void on already-voided return is a no-op
    Given a normal account exists for holder "cr2p-s05-eve"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s05-eve" with origin IFSC "HDFC0081005" and account number "8081005001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions
    And I void the pending return
    And I void the pending return
    Then the return status is "voided"

  @api
  Scenario: Mixed-direction post-then-void rejected
    Given a normal account exists for holder "cr2p-s06-flo"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s06-flo" with origin IFSC "HDFC0081006" and account number "8081006001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions
    And I post the pending return
    And I attempt to void the pending return
    Then the return fails with "TransactionNotPending"

  @api
  Scenario: Pending return with short timeout ages out via pending_timeout poller
    Given a normal account exists for holder "cr2p-s07-gus"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr2p-s07-gus" with origin IFSC "HDFC0081007" and account number "8081007001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I initiate a pending return of 5000 paisa of "trust" contributions with timeout 1 second
    And I wait 3 seconds for the timeout poller
    When I fetch the contribution summary
    Then the trust remaining_returnable is 10000
