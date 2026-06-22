Feature: post_transfer and void_transfer are idempotent in the same direction
  Re-applying the same lifecycle resolution must be a no-op, not an error.

  @api
  Scenario: Post on an already-posted transfer is a no-op
    Given a normal account exists for holder "tpvi-s01-alice"
    And the normal account has balance 20000
    And a "health" account exists for holder "tpvi-s01-alice" with origin IFSC "HDFC0090001" and account number "9090001001"
    When I initiate a pending transfer of 10000 paisa from the normal account to the PB account
    And I post the pending transfer
    And I post the pending transfer
    Then the second post is a no-op
