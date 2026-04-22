Feature: All Transactions
  The system-wide transactions endpoint lists transactions across all accounts.
  All amounts are in paisa (1 INR = 100 paisa).

  @empty-db
  Scenario: Empty ledger returns zero transactions
    When I list all transactions
    Then the total transaction count should be 0
    And the transactions list should be empty

  Scenario: Transactions appear after deposits
    Given a "health" account exists for holder "f1111111-1111-1111-1111-111111111111" with origin IFSC "HDFC0091111" and account number "9111100001"
    When I deposit 5000 from IFSC "HDFC0091111" account "9111100001"
    And I deposit 3000 from IFSC "ICIC0009999" account "9876543210"
    And I list all transactions
    Then the total transaction count should be at least 2
    And the transactions list should contain the current account

  Scenario: Transactions include payments
    Given a "health" account exists for holder "f2222222-2222-2222-2222-222222222222" with origin IFSC "HDFC0092222" and account number "9222200001"
    And the account has 10000 in self-pool and 5000 in others-pool
    When I pay 2000 to merchant "PHARMACY001" with MCC "5912" described as "pharmacy purchase"
    And I list all transactions
    Then the total transaction count should be at least 3
    And the transactions list should contain a "payment" transaction

  Scenario: Pagination works correctly
    Given a "health" account exists for holder "f3333333-3333-3333-3333-333333333333" with origin IFSC "HDFC0093333" and account number "9333300001"
    When I deposit 1000 from IFSC "HDFC0093333" account "9333300001"
    And I deposit 2000 from IFSC "HDFC0093333" account "9333300001"
    And I deposit 3000 from IFSC "HDFC0093333" account "9333300001"
    And I list all transactions with limit 2
    Then the transactions list should have 2 entries
    And the total transaction count should be at least 3
