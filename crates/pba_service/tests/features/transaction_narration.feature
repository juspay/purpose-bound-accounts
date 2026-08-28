Feature: Transaction narration
  Deposits and withdrawals accept an optional free-text description, and
  cancelling a pending deposit records the caller's reason. Both are stored on
  the transaction and returned by the transaction listings.
  All amounts are in paisa (1 INR = 100 paisa).

  Scenario: PB deposit stores the supplied description
    Given a "health" account exists for holder "d1d1d1d1-d1d1-d1d1-d1d1-d1d1d1d1d1d1" with origin IFSC "HDFC0061111" and account number "6111100001"
    When I deposit 10000 from IFSC "HDFC0061111" account "6111100001" with description "Salary credit March"
    Then the deposit transaction description should be "Salary credit March"

  Scenario: PB deposit without a description leaves it unset
    Given a "health" account exists for holder "d2d2d2d2-d2d2-d2d2-d2d2-d2d2d2d2d2d2" with origin IFSC "HDFC0062222" and account number "6222200001"
    When I deposit 10000 from IFSC "HDFC0062222" account "6222200001"
    Then the deposit transaction should have no description

  Scenario: Pending PB deposit carries its description through post
    Given a "health" account exists for holder "d3d3d3d3-d3d3-d3d3-d3d3-d3d3d3d3d3d3" with origin IFSC "HDFC0063333" and account number "6333300001"
    When I create a pending deposit of 8000 from IFSC "HDFC0063333" account "6333300001" with description "Held pending UPI confirmation"
    Then the deposit transaction description should be "Held pending UPI confirmation"
    When I post the pending deposit
    Then the deposit transaction description should be "Held pending UPI confirmation"

  Scenario: Voiding a pending PB deposit records the reason without losing the description
    Given a "health" account exists for holder "d4d4d4d4-d4d4-d4d4-d4d4-d4d4d4d4d4d4" with origin IFSC "HDFC0064444" and account number "6444400001"
    When I create a pending deposit of 5000 from IFSC "HDFC0064444" account "6444400001" with description "Held pending UPI confirmation"
    And I void the pending deposit with reason "Upstream charge failed"
    Then the deposit transaction void reason should be "Upstream charge failed"
    And the deposit transaction description should be "Held pending UPI confirmation"

  Scenario: Voiding without a reason leaves the void reason unset
    Given a "health" account exists for holder "d5d5d5d5-d5d5-d5d5-d5d5-d5d5d5d5d5d5" with origin IFSC "HDFC0065555" and account number "6555500001"
    When I create a pending deposit of 5000 from IFSC "HDFC0065555" account "6555500001"
    And I void the pending deposit
    Then the deposit transaction should have no void reason

  Scenario: PB withdrawal stores the supplied description
    Given a "health" account exists for holder "d6d6d6d6-d6d6-d6d6-d6d6-d6d6d6d6d6d6" with origin IFSC "HDFC0066666" and account number "6666600001"
    When I deposit 10000 from IFSC "HDFC0066666" account "6666600001"
    And I withdraw 4000 with description "Refund to source account"
    Then the withdrawal transaction description should be "Refund to source account"

  Scenario: Normal-account deposit stores the supplied description
    Given a normal account exists for holder "d7d7d7d7-d7d7-d7d7-d7d7-d7d7d7d7d7d7"
    When I deposit 9000 paisa to the normal account with description "Sponsor top-up"
    Then the normal deposit transaction description should be "Sponsor top-up"

  Scenario: Normal-account withdrawal stores the supplied description
    Given a normal account exists for holder "d8d8d8d8-d8d8-d8d8-d8d8-d8d8d8d8d8d8"
    And the normal account has balance 9000
    When I withdraw 3000 paisa from the normal account with description "Sponsor refund"
    Then the normal withdrawal transaction description should be "Sponsor refund"

  Scenario: Voiding a pending normal-account deposit records the reason
    Given a normal account exists for holder "d9d9d9d9-d9d9-d9d9-d9d9-d9d9d9d9d9d9"
    When I create a pending deposit of 6000 paisa to the normal account with description "Awaiting sponsor settlement"
    And I void the normal account deposit with reason "Sponsor mandate revoked"
    Then the normal deposit transaction void reason should be "Sponsor mandate revoked"
    And the normal deposit transaction description should be "Awaiting sponsor settlement"

  Scenario: An over-long deposit description is rejected
    Given a "health" account exists for holder "dadadada-dada-dada-dada-dadadadadada" with origin IFSC "HDFC0067777" and account number "6777700001"
    When I attempt to deposit 1000 from IFSC "HDFC0067777" account "6777700001" with a 257 character description
    Then the request should be rejected as invalid

  Scenario: An over-long void reason is rejected
    Given a "health" account exists for holder "dbdbdbdb-dbdb-dbdb-dbdb-dbdbdbdbdbdb" with origin IFSC "HDFC0068888" and account number "6888800001"
    When I create a pending deposit of 1000 from IFSC "HDFC0068888" account "6888800001"
    And I attempt to void the pending deposit with a 257 character reason
    Then the request should be rejected as invalid
