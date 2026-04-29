Feature: Account Management
  Purpose-bound accounts can be created, retrieved, and have their status updated.

  Scenario: Create a health account
    When I create a "health" account for holder "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa" with origin IFSC "HDFC0001234" and account number "1234567890"
    Then the account should be created successfully
    And the account purpose should be "health"
    And the account status should be "active"

  Scenario: Get an existing account
    Given a "health" account exists for holder "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb" with origin IFSC "SBIN0005678" and account number "5678901234"
    When I get the account
    Then the account purpose should be "health"

  Scenario: Initial balance is zero
    Given a "health" account exists for holder "cccccccc-cccc-cccc-cccc-cccccccccccc" with origin IFSC "ICIC0001111" and account number "1111111111"
    When I get the account balance
    Then the self contribution should be 0
    And the others contribution should be 0
    And the total balance should be 0

  Scenario: Freeze and reactivate account
    Given a "health" account exists for holder "dddddddd-dddd-dddd-dddd-dddddddddddd" with origin IFSC "AXIS0002222" and account number "2222222222"
    When I freeze the account
    Then the account status should be "frozen"
    When I reactivate the account
    Then the account status should be "active"

  Scenario: Create accounts for different purpose types
    When I create a "education" account for holder "a1111111-1111-1111-1111-111111111111" with origin IFSC "SBIN0010001" and account number "1000100001"
    Then the account should be created successfully
    And the account purpose should be "education"
    When I create a "food" account for holder "a2222222-2222-2222-2222-222222222222" with origin IFSC "SBIN0010002" and account number "1000100002"
    Then the account should be created successfully
    And the account purpose should be "food"
    When I create a "transport" account for holder "a3333333-3333-3333-3333-333333333333" with origin IFSC "SBIN0010003" and account number "1000100003"
    Then the account should be created successfully
    And the account purpose should be "transport"

  Scenario: Close an account
    Given a "health" account exists for holder "a4444444-4444-4444-4444-444444444444" with origin IFSC "HDFC0004444" and account number "4444444444"
    When I close the account
    Then the account status should be "closed"

  Scenario: Reject operations on a closed account
    Given a "health" account exists for holder "a5555555-5555-5555-5555-555555555555" with origin IFSC "HDFC0005555" and account number "5555555555"
    And the account is closed
    When I attempt to deposit 1000 from IFSC "HDFC0005555" account "5555555555"
    Then the deposit should be rejected as account not active
