Feature: Payments
  Payments use others-pool first, then self-pool, and validate MCC against purpose.

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

  Scenario: Education account accepts education MCC
    Given a "education" account exists for holder "55555555-5555-5555-5555-555555555555" with origin IFSC "SBIN0055555" and account number "5555500001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I pay 1000 to merchant "BOOKSTORE001" with MCC "5942" described as "textbooks"
    Then the payment should succeed
