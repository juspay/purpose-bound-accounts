Feature: Funding Source Types
  Deposits are classified by funding source type: self, trust, or third_party.
  Self-deposits are auto-detected when the source IFSC/account matches the account origin.
  Non-origin deposits must specify funding_type as "third_party". Trust funds reach PB
  accounts via internal transfers from normal accounts (not direct deposits).

  Scenario: Self-deposit auto-detected from origin bank
    Given a "health" account exists for holder "f4444444-4444-4444-4444-444444444444" with origin IFSC "HDFC0094444" and account number "9444400001"
    When I deposit 5000 from IFSC "HDFC0094444" account "9444400001"
    Then the deposit should go to "self" pool
    And the funding type should be "self"

  Scenario: Trust funds reach PB others-pool via a transfer from a normal account
    Given a normal account exists for holder "f5555555-5555-5555-5555-555555555555"
    And the normal account has balance 10000
    And a "health" account exists for holder "f5555555-5555-5555-5555-555555555555" with origin IFSC "HDFC0095555" and account number "9555500001"
    When I transfer 10000 paisa from the normal account to the PB account
    Then the transfer is successful
    And the PB account others-pool balance is 10000

  Scenario: Third-party deposit from non-origin source
    Given a "health" account exists for holder "f6666666-6666-6666-6666-666666666666" with origin IFSC "HDFC0096666" and account number "9666600001"
    When I deposit 3000 from IFSC "SBIN0005678" account "5678901234" with funding type "third_party"
    Then the deposit should go to "others" pool
    And the funding type should be "third_party"

  Scenario: Non-origin deposit without funding type is rejected
    Given a "health" account exists for holder "f7777777-7777-7777-7777-777777777777" with origin IFSC "HDFC0097777" and account number "9777700001"
    When I attempt to deposit 2000 from IFSC "ICIC0009999" account "9999999999" without funding type
    Then the operation should be rejected

  Scenario: Transactions listing includes self deposits and trust transfer rows
    Given a normal account exists for holder "f8888888-8888-8888-8888-888888888888"
    And the normal account has balance 5000
    And a "health" account exists for holder "f8888888-8888-8888-8888-888888888888" with origin IFSC "HDFC0098888" and account number "9888800001"
    When I deposit 5000 from IFSC "HDFC0098888" account "9888800001"
    And I transfer 3000 paisa from the normal account to the PB account
    And I list all transactions
    Then the transactions list should contain a funding type "self"
    And the transactions list should contain a funding type "trust"
