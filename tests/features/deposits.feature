Feature: Deposits
  Deposits are routed to self or others pool based on the source bank.

  Scenario: Deposit from origin bank goes to self-pool
    Given a "health" account exists for holder "11111111-1111-1111-1111-111111111111" with origin IFSC "HDFC0011111" and account number "1111100001"
    When I deposit 10000 from IFSC "HDFC0011111" account "1111100001"
    Then the deposit should go to "self_contribution" pool
    And the self contribution should be 10000

  Scenario: Deposit from other bank goes to others-pool
    Given a "health" account exists for holder "22222222-2222-2222-2222-222222222222" with origin IFSC "HDFC0022222" and account number "2222200001"
    When I deposit 5000 from IFSC "ICIC0009999" account "9876543210"
    Then the deposit should go to "others_contribution" pool
    And the others contribution should be 5000

  Scenario: Deposit on frozen account is rejected
    Given a "health" account exists for holder "33333333-3333-3333-3333-333333333333" with origin IFSC "HDFC0033333" and account number "3333300001"
    And the account is frozen
    When I attempt to deposit 1000 from IFSC "HDFC0033333" account "3333300001"
    Then the deposit should be rejected as account not active
