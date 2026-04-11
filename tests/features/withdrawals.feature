Feature: Withdrawals
  Withdrawals can only be made from the self-contribution pool.

  Scenario: Withdraw from self-pool
    Given a "health" account exists for holder "66666666-6666-6666-6666-666666666666" with origin IFSC "HDFC0006666" and account number "6666666666"
    And the account has 5000 in self-pool and 3000 in others-pool
    When I withdraw 2000
    Then the withdrawal should succeed with amount 2000

  Scenario: Withdrawal exceeding self-pool is rejected
    Given a "health" account exists for holder "77777777-7777-7777-7777-777777777777" with origin IFSC "HDFC0007777" and account number "7777777777"
    And the account has 1000 in self-pool and 5000 in others-pool
    When I attempt to withdraw 999999
    Then the withdrawal should be rejected as insufficient funds
