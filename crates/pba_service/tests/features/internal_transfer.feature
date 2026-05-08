Feature: Internal transfers from normal accounts to PB accounts

  Scenario: Immediate transfer credits the PB others-pool
    Given a normal account exists for holder "alice-tx-01"
    And the normal account has balance 10000
    And a "health" account exists for holder "alice-tx-01" with origin IFSC "HDFC0011001" and account number "9011001001"
    When I transfer 5000 paisa from the normal account to the PB account
    Then the transfer is successful
    And the transfer status field is "posted"
    And the normal account balance is 5000
    And the PB account others-pool balance is 5000
    And the PB account self-pool balance is 0

  Scenario: Pending transfer + post lifecycle
    Given a normal account exists for holder "bob-tx-01"
    And the normal account has balance 10000
    And a "education" account exists for holder "bob-tx-01" with origin IFSC "HDFC0012002" and account number "9012002001"
    When I create a pending transfer of 3000 paisa from the normal account to the PB account with timeout 120
    Then the transfer status field is "pending"
    When I post the transfer
    Then the transfer status field is "posted"
    And the PB account others-pool balance is 3000

  Scenario: Pending transfer + void
    Given a normal account exists for holder "carla-tx-01"
    And the normal account has balance 5000
    And a "food" account exists for holder "carla-tx-01" with origin IFSC "HDFC0013003" and account number "9013003001"
    When I create a pending transfer of 1500 paisa from the normal account to the PB account with timeout 120
    And I void the transfer
    Then the transfer status field is "voided"
    And the normal account balance is 5000
    And the PB account others-pool balance is 0

  Scenario: Insufficient balance rejects transfer
    Given a normal account exists for holder "ed-tx-01"
    And the normal account has balance 100
    And a "health" account exists for holder "ed-tx-01" with origin IFSC "HDFC0014004" and account number "9014004001"
    When I attempt to transfer 500 paisa from the normal account to the PB account
    Then the transfer fails with "InsufficientFunds"

  Scenario: Source frozen rejects transfer
    Given a normal account exists for holder "fay-tx-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "fay-tx-01" with origin IFSC "HDFC0015005" and account number "9015005001"
    When I freeze the normal account
    And I attempt to transfer 1000 paisa from the normal account to the PB account
    Then the transfer fails with "NormalAccountNotActive"

  Scenario: Destination frozen rejects transfer
    Given a normal account exists for holder "gus-tx-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "gus-tx-01" with origin IFSC "HDFC0016006" and account number "9016006001"
    When I freeze the PB account
    And I attempt to transfer 1000 paisa from the normal account to the PB account
    Then the transfer fails with "PbAccountNotActive"

  Scenario: Idempotency replay returns the same transfer
    Given a normal account exists for holder "han-tx-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "han-tx-01" with origin IFSC "HDFC0017007" and account number "9017007001"
    When I transfer 2000 paisa from the normal account to the PB account with idempotency key "tk1"
    And I retry the same transfer with idempotency key "tk1"
    Then both transfers return the same id
    And the PB account others-pool balance is 2000

  Scenario: Both legs share the same correlation_id
    Given a normal account exists for holder "ira-tx-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "ira-tx-01" with origin IFSC "HDFC0018008" and account number "9018008001"
    When I transfer 2500 paisa from the normal account to the PB account
    Then the source-side and destination-side transactions share the same correlation_id
    And the source-side transaction has type "transfer" and direction "outbound"
    And the destination-side transaction has type "deposit" and pool "others" and funding_type "trust"
