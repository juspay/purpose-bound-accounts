Feature: Withdrawals
  Withdrawals can only be made from the self-contribution pool.
  All amounts are in paisa (1 INR = 100 paisa).

  Scenario: Withdraw from self-pool
    Given a "health" account exists for holder "66666666-6666-6666-6666-666666666666" with origin IFSC "HDFC0006666" and account number "6666666666"
    And the account has 5000 in self-pool and 3000 in others-pool
    When I withdraw 2000
    Then the withdrawal should succeed with amount 2000

  Scenario: Withdrawal of exact self-pool balance
    Given a "health" account exists for holder "68686868-6868-6868-6868-686868686868" with origin IFSC "HDFC0068686" and account number "6868686868"
    And the account has 5000 in self-pool and 3000 in others-pool
    When I withdraw 5000
    Then the withdrawal should succeed with amount 5000
    And the self contribution should be 0
    And the others contribution should be 3000

  Scenario: Withdrawal exceeding self-pool is rejected
    Given a "health" account exists for holder "77777777-7777-7777-7777-777777777777" with origin IFSC "HDFC0007777" and account number "7777777777"
    And the account has 1000 in self-pool and 5000 in others-pool
    When I attempt to withdraw 999999
    Then the withdrawal should be rejected as insufficient funds

  Scenario: Withdrawal on a frozen account is rejected
    Given a "health" account exists for holder "78787878-7878-7878-7878-787878787878" with origin IFSC "HDFC0078787" and account number "7878787878"
    And the account has 5000 in self-pool and 0 in others-pool
    And the account is frozen
    When I attempt to withdraw 1000
    Then the withdrawal should be rejected as account not active

  Scenario: Withdrawal on a closed account is rejected
    Given a "health" account exists for holder "79797979-7979-7979-7979-797979797979" with origin IFSC "HDFC0079797" and account number "7979797979"
    And the account has 5000 in self-pool and 0 in others-pool
    And the account is closed
    When I attempt to withdraw 1000
    Then the withdrawal should be rejected as account not active

  Scenario: Withdrawal echoes the supplied gateway_ref
    Given a "health" account exists for holder "7A7A7A7A-7A7A-7A7A-7A7A-7A7A7A7A0099" with origin IFSC "HDFC007A7A9" and account number "707070099"
    And the account has 5000 in self-pool and 0 in others-pool
    When I withdraw 1000 with gateway ref "gw-api-wd-99"
    Then the withdrawal should succeed with amount 1000
    And the withdrawal response should echo gateway ref "gw-api-wd-99"
