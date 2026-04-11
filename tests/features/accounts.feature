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

  Scenario: Reject duplicate account
    Given a "health" account exists for holder "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee" with origin IFSC "UTIB0003333" and account number "3333333333"
    When I create a duplicate "health" account for holder "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee" with origin IFSC "UTIB0003333" and account number "3333333333"
    Then the duplicate should be rejected
