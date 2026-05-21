Feature: Reversal of normal → PB transfers

  Scenario: Full reversal restores source balance and decrements others-pool
    Given a normal account exists for holder "rev-alice-01"
    And the normal account has balance 10000
    And a "health" account exists for holder "rev-alice-01" with origin IFSC "HDFC0021001" and account number "9021001001"
    When I transfer 5000 paisa from the normal account to the PB account
    Then the transfer is successful
    When I reverse 5000 paisa from the transfer
    Then the reversal is successful
    And the reversal status field is "posted"
    And the normal account balance is 10000
    And the PB account others-pool balance is 0

  Scenario: Partial reversal moves only the requested amount
    Given a normal account exists for holder "rev-bob-01"
    And the normal account has balance 10000
    And a "education" account exists for holder "rev-bob-01" with origin IFSC "HDFC0022002" and account number "9022002001"
    When I transfer 5000 paisa from the normal account to the PB account
    Then the transfer is successful
    When I reverse 3000 paisa from the transfer
    Then the reversal is successful
    And the normal account balance is 8000
    And the PB account others-pool balance is 2000

  Scenario: Pending transfer cannot be reversed
    Given a normal account exists for holder "rev-carla-01"
    And the normal account has balance 5000
    And a "food" account exists for holder "rev-carla-01" with origin IFSC "HDFC0023003" and account number "9023003001"
    When I create a pending transfer of 1500 paisa from the normal account to the PB account with timeout 120
    And I attempt to reverse 1500 paisa from the transfer
    Then the reversal fails with "TransferNotReversible" reason "not_posted"

  Scenario: Already-reversed transfer cannot be reversed again
    Given a normal account exists for holder "rev-dan-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-dan-01" with origin IFSC "HDFC0024004" and account number "9024004001"
    When I transfer 2000 paisa from the normal account to the PB account
    And I reverse 2000 paisa from the transfer
    Then the reversal is successful
    When I attempt to reverse 2000 paisa from the transfer
    Then the reversal fails with "TransferAlreadyReversed"

  Scenario: Reversal amount above original is rejected
    Given a normal account exists for holder "rev-eve-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-eve-01" with origin IFSC "HDFC0025005" and account number "9025005001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I attempt to reverse 1001 paisa from the transfer
    Then the reversal fails with "ReversalAmountInvalid"

  Scenario: Reversal amount of zero is rejected
    Given a normal account exists for holder "rev-flo-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-flo-01" with origin IFSC "HDFC0026006" and account number "9026006001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I attempt to reverse 0 paisa from the transfer
    Then the reversal fails with "ReversalAmountInvalid"

  Scenario: Insufficient others-pool balance rejects full reversal
    Given a normal account exists for holder "rev-gus-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-gus-01" with origin IFSC "HDFC0027007" and account number "9027007001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I pay 700 paisa to merchant "HOSP01" with MCC "8062"
    And I attempt to reverse 1000 paisa from the transfer
    Then the reversal fails with "InsufficientFunds"
    And the reversal available balance is 300
    When I reverse 300 paisa from the transfer
    Then the reversal is successful
    And the PB account others-pool balance is 0

  Scenario: Source normal account frozen rejects reversal
    Given a normal account exists for holder "rev-han-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-han-01" with origin IFSC "HDFC0028008" and account number "9028008001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I freeze the normal account
    And I attempt to reverse 1000 paisa from the transfer
    Then the reversal fails with "NormalAccountNotActive"

  Scenario: Destination PB account frozen rejects reversal, reactivation allows retry
    Given a normal account exists for holder "rev-ivy-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-ivy-01" with origin IFSC "HDFC0029009" and account number "9029009001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I freeze the PB account
    And I attempt to reverse 1000 paisa from the transfer
    Then the reversal fails with "PbAccountNotActive"
    When I reactivate the PB account
    And I reverse 1000 paisa from the transfer
    Then the reversal is successful

  Scenario: Idempotency replay returns the same reversal
    Given a normal account exists for holder "rev-jay-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-jay-01" with origin IFSC "HDFC0030001" and account number "9030001001"
    When I transfer 1500 paisa from the normal account to the PB account
    And I reverse 1500 paisa from the transfer with idempotency key "rev-jay-key-1"
    Then the reversal is successful
    When I reverse 1500 paisa from the transfer with idempotency key "rev-jay-key-1"
    Then the reversal is successful

  Scenario: Wrong source account in URL returns not-found
    Given a normal account exists for holder "rev-kay-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-kay-01" with origin IFSC "HDFC0031001" and account number "9031001001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I switch the current normal account to a fresh holder "rev-kay-02"
    And I attempt to reverse 1000 paisa from the transfer
    Then the reversal fails with "TransactionNotFound"

  Scenario: Reversal row itself cannot be reversed
    Given a normal account exists for holder "rev-lou-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-lou-01" with origin IFSC "HDFC0032001" and account number "9032001001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I reverse 1000 paisa from the transfer
    Then the reversal is successful
    When I treat the reversal row as the current transfer
    And I attempt to reverse 1000 paisa from the transfer
    Then the reversal fails with "TransferNotReversible" reason "wrong_type"

  Scenario: Both legs visible per-account after reversal
    Given a normal account exists for holder "rev-mia-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-mia-01" with origin IFSC "HDFC0033001" and account number "9033001001"
    When I transfer 1000 paisa from the normal account to the PB account
    And I reverse 1000 paisa from the transfer
    Then the reversal is successful
    And the normal account has at least 2 transactions
    And the PB account has at least 2 transactions
