Feature: Payments
  Payments use others-pool first, then self-pool, and validate MCC against purpose.
  All amounts are in paisa (1 INR = 100 paisa).

  Scenario: Payment fully from others-pool
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444441" with origin IFSC "HDFC0044441" and account number "4444400011"
    And the account has 10000 in self-pool and 5000 in others-pool
    When I pay 3000 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy purchase"
    Then the payment should succeed
    And 3000 should come from others-pool
    And 0 should come from self-pool

  Scenario: Payment split across both pools
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444442" with origin IFSC "HDFC0044442" and account number "4444400022"
    And the account has 10000 in self-pool and 5000 in others-pool
    When I pay 3000 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy purchase"
    And I pay 4000 to merchant "DOCTOR001" with MCC "8011" described as "doctor visit"
    Then the payment should succeed
    And 2000 should come from others-pool
    And 2000 should come from self-pool

  Scenario: Payment from self-pool only when others depleted
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444443" with origin IFSC "HDFC0044443" and account number "4444400033"
    And the account has 10000 in self-pool and 5000 in others-pool
    When I pay 5000 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy purchase"
    And I pay 1000 to merchant "PHARMACY002" with MCC "5912" described as "another pharmacy"
    Then the payment should succeed
    And 0 should come from others-pool
    And 1000 should come from self-pool

  Scenario: Payment rejected for insufficient funds
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444447" with origin IFSC "HDFC0044447" and account number "4444400077"
    And the account has 10000 in self-pool and 5000 in others-pool
    When I attempt to pay 999999 to merchant "PHARMACY001" with MCC "5912" described as "too much"
    Then the payment should be rejected as insufficient funds

  Scenario: Payment rejected for invalid MCC
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444445" with origin IFSC "HDFC0044445" and account number "4444400055"
    And the account has 10000 in self-pool and 5000 in others-pool
    When I attempt to pay 100 to merchant "RAILWAY001" with MCC "4011" described as "train ticket"
    Then the payment should be rejected as invalid MCC

  Scenario: Cross-purpose MCC rejection
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444446" with origin IFSC "HDFC0044446" and account number "4444400066"
    And the account has 10000 in self-pool and 5000 in others-pool
    When I attempt to pay 100 to merchant "BOOKSTORE001" with MCC "5942" described as "bookstore"
    Then the payment should be rejected as invalid MCC

  Scenario: Payment on a frozen account is rejected
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444448" with origin IFSC "HDFC0044448" and account number "4444400088"
    And the account has 10000 in self-pool and 5000 in others-pool
    And the account is frozen
    When I attempt to pay 100 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy"
    Then the payment should be rejected as account not active

  Scenario: Payment on a closed account is rejected
    Given a "health" account exists for holder "44444444-4444-4444-4444-444444444449" with origin IFSC "HDFC0044449" and account number "4444400099"
    And the account has 10000 in self-pool and 5000 in others-pool
    And the account is closed
    When I attempt to pay 100 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy"
    Then the payment should be rejected as account not active

  Scenario: Payment that exactly drains both pools
    Given a "health" account exists for holder "44444444-4444-4444-4444-44444444444a" with origin IFSC "HDFC004444a" and account number "444440000a"
    And the account has 3000 in self-pool and 2000 in others-pool
    When I pay 5000 to merchant "PHARMACY001" with MCC "5912" described as "exact drain"
    Then the payment should succeed
    And 2000 should come from others-pool
    And 3000 should come from self-pool
    And the total balance should be 0

  Scenario: Food account accepts food MCC and rejects health MCC
    Given a "food" account exists for holder "56565656-5656-5656-5656-565656565656" with origin IFSC "SBIN0056565" and account number "5656500001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I pay 1000 to merchant "GROCERY001" with MCC "5411" described as "groceries"
    Then the payment should succeed
    When I attempt to pay 100 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy"
    Then the payment should be rejected as invalid MCC

  Scenario: Education account accepts education MCC
    Given a "education" account exists for holder "55555555-5555-5555-5555-555555555555" with origin IFSC "SBIN0055555" and account number "5555500001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I pay 1000 to merchant "BOOKSTORE001" with MCC "5942" described as "textbooks"
    Then the payment should succeed

  Scenario: Concurrent payments do not double-spend
    Given a "health" account exists for holder "c0c0c0c0-c0c0-c0c0-c0c0-c0c0c0c0c0c0" with origin IFSC "HDFC00c0c0c" and account number "c0c0c00001"
    And the account has 5000 in self-pool and 5000 in others-pool
    When 10 concurrent payments of 1000 each are made to MCC "5912"
    Then exactly 10 payments should succeed
    And the total balance should be 0

  Scenario: Concurrent payments partially succeed when funds run out
    Given a "health" account exists for holder "c1c1c1c1-c1c1-c1c1-c1c1-c1c1c1c1c1c1" with origin IFSC "HDFC00c1c1c" and account number "c1c1c10001"
    And the account has 3000 in self-pool and 2000 in others-pool
    When 10 concurrent payments of 1000 each are made to MCC "5912"
    Then exactly 5 payments should succeed
    And the total balance should be 0
