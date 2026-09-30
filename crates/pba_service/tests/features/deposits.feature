Feature: Deposits
  Deposits are routed to self or others pool based on the source bank.
  All amounts are in paisa (1 INR = 100 paisa).

  Scenario: Deposit from origin bank goes to self-pool
    Given a "health" account exists for holder "11111111-1111-1111-1111-111111111111" with origin IFSC "HDFC0011111" and account number "1111100001"
    When I deposit 10000 from IFSC "HDFC0011111" account "1111100001"
    Then the deposit should go to "self" pool
    And the self contribution should be 10000

  Scenario: Deposit from other bank goes to others-pool
    Given a "health" account exists for holder "22222222-2222-2222-2222-222222222222" with origin IFSC "HDFC0022222" and account number "2222200001"
    When I deposit 5000 from IFSC "ICIC0009999" account "9876543210" with funding type "third_party"
    Then the deposit should go to "others" pool
    And the others contribution should be 5000

  Scenario: Multiple deposits accumulate correctly
    Given a "health" account exists for holder "34343434-3434-3434-3434-343434343434" with origin IFSC "HDFC0034343" and account number "3434300001"
    When I deposit 5000 from IFSC "HDFC0034343" account "3434300001"
    And I deposit 3000 from IFSC "HDFC0034343" account "3434300001"
    And I deposit 2000 from IFSC "ICIC0009999" account "9876543210" with funding type "third_party"
    Then the self contribution should be 8000
    And the others contribution should be 2000
    And the total balance should be 10000

  Scenario: Deposit on frozen account is rejected
    Given a "health" account exists for holder "33333333-3333-3333-3333-333333333333" with origin IFSC "HDFC0033333" and account number "3333300001"
    And the account is frozen
    When I attempt to deposit 1000 from IFSC "HDFC0033333" account "3333300001"
    Then the deposit should be rejected as account not active

  Scenario: Deposit on closed account is rejected
    Given a "health" account exists for holder "35353535-3535-3535-3535-353535353535" with origin IFSC "HDFC0035353" and account number "3535300001"
    And the account is closed
    When I attempt to deposit 1000 from IFSC "HDFC0035353" account "3535300001"
    Then the deposit should be rejected as account not active

  Scenario: Pending deposit then post moves funds to posted balance
    Given a "health" account exists for holder "a1a1a1a1-a1a1-a1a1-a1a1-a1a1a1a1a1a1" with origin IFSC "HDFC0051111" and account number "5111100001"
    When I create a pending deposit of 10000 from IFSC "HDFC0051111" account "5111100001"
    Then the self contribution should be 0
    And the pending self should be 10000
    When I post the pending deposit
    Then the self contribution should be 10000
    And the pending self should be 0

  Scenario: Pending deposit then void leaves balance unchanged
    Given a "health" account exists for holder "a2a2a2a2-a2a2-a2a2-a2a2-a2a2a2a2a2a2" with origin IFSC "HDFC0052222" and account number "5222200001"
    When I create a pending deposit of 5000 from IFSC "HDFC0052222" account "5222200001"
    Then the pending self should be 5000
    When I void the pending deposit
    Then the self contribution should be 0
    And the pending self should be 0

  Scenario: Pending deposit from other bank goes to others pending pool
    Given a "health" account exists for holder "a3a3a3a3-a3a3-a3a3-a3a3-a3a3a3a3a3a3" with origin IFSC "HDFC0053333" and account number "5333300001"
    When I create a pending deposit of 7000 from IFSC "ICIC0009999" account "9876543210" with funding type "third_party"
    Then the deposit should go to "others" pool
    And the others contribution should be 0
    And the pending others should be 7000
    When I post the pending deposit
    Then the others contribution should be 7000
    And the pending others should be 0

  Scenario: Pending deposit with gateway reference
    Given a "health" account exists for holder "a4a4a4a4-a4a4-a4a4-a4a4-a4a4a4a4a4a4" with origin IFSC "HDFC0054444" and account number "5444400001"
    When I create a pending deposit of 3000 from IFSC "HDFC0054444" account "5444400001" with gateway ref "gw-txn-12345"
    Then the pending self should be 3000
    When I post the pending deposit
    Then the self contribution should be 3000

  @api
  Scenario: Post on non-existent deposit is rejected
    Given a "health" account exists for holder "a5a5a5a5-a5a5-a5a5-a5a5-a5a5a5a5a5a5" with origin IFSC "HDFC0055555" and account number "5555500001"
    When I attempt to post deposit "00000000-0000-0000-0000-000000000000"
    Then the operation should be rejected

  @api
  Scenario: Void on non-existent deposit is rejected
    Given a "health" account exists for holder "a6a6a6a6-a6a6-a6a6-a6a6-a6a6a6a6a6a6" with origin IFSC "HDFC0056666" and account number "5666600001"
    When I attempt to void deposit "00000000-0000-0000-0000-000000000000"
    Then the operation should be rejected

  @api
  Scenario: Post on already-posted deposit is rejected
    Given a "health" account exists for holder "a7a7a7a7-a7a7-a7a7-a7a7-a7a7a7a7a7a7" with origin IFSC "HDFC0057777" and account number "5777700001"
    When I create a pending deposit of 2000 from IFSC "HDFC0057777" account "5777700001"
    And I post the pending deposit
    And I attempt to post the pending deposit again
    Then the operation should be rejected

  @api
  Scenario: Void on already-voided deposit is rejected
    Given a "health" account exists for holder "a8a8a8a8-a8a8-a8a8-a8a8-a8a8a8a8a8a8" with origin IFSC "HDFC0058888" and account number "5888800001"
    When I create a pending deposit of 2000 from IFSC "HDFC0058888" account "5888800001"
    And I void the pending deposit
    And I attempt to void the pending deposit again
    Then the operation should be rejected

  @api
  Scenario: Returned-by affordance surfaces contribution returns on the third-party deposit detail
    Given a "health" account exists for holder "rby-third-party" with origin IFSC "HDFC0079998" and account number "7079998001"
    And the PB account receives 12000 paisa via a third-party deposit
    When I return 5000 paisa of "third_party" contributions
    And I fetch the transfer detail page returns list
    Then the transfer detail returns list has 1 entry
    And the transfer detail returns entry 1 amount is "50.00"
