Feature: Admin UI
  Visual and navigation tests for the admin interface.

  Scenario: Dashboard shows account statistics
    Given a "health" account exists for holder "d1111111-1111-1111-1111-111111111111" with origin IFSC "HDFC0001111" and account number "1111100001"
    When I visit the dashboard
    Then the dashboard should show at least 1 total accounts

  Scenario: Account detail shows balance breakdown
    Given a "health" account exists for holder "d2222222-2222-2222-2222-222222222222" with origin IFSC "HDFC0002222" and account number "2222200001"
    And the account has 5000 in self-pool and 3000 in others-pool
    When I view the account detail
    Then the page should show self pool as "50.00"
    And the page should show others pool as "30.00"
    And the page should show total balance as "80.00"

  Scenario: Transaction history loads via HTMX
    Given a "health" account exists for holder "d3333333-3333-3333-3333-333333333333" with origin IFSC "HDFC0003333" and account number "3333300001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I view the account detail
    Then the transaction history should load
    And the transaction history should show at least 1 entry

  Scenario: Action links hidden on frozen account
    Given a "health" account exists for holder "d4444444-4444-4444-4444-444444444444" with origin IFSC "HDFC0004444" and account number "4444400001"
    And the account is frozen
    When I view the account detail
    Then the deposit link should not be visible
    And the payment link should not be visible
    And the withdrawal link should not be visible

  Scenario: All transactions page shows transactions after deposit
    Given a "health" account exists for holder "d5555555-5555-5555-5555-555555555555" with origin IFSC "HDFC0005555" and account number "5555500001"
    And the account has 5000 in self-pool and 3000 in others-pool
    When I visit the all transactions page
    Then the all transactions page should show at least 2 transactions
    And the all transactions page should show pool balance summary

  Scenario: Purpose types page lists all purposes
    When I visit the purpose types page
    Then I should see at least 4 purpose types listed

  Scenario: System accounts page shows sentinel accounts and pool balances
    When I visit the system accounts page
    Then I should see "Sentinel Accounts" on the page
    And I should see "PBA Pool Balances" on the page

  Scenario: Transactions page shows funding type column
    When I visit the all transactions page
    Then I should see "Funding Type" on the page

  Scenario: Transaction detail page shows all fields
    Given a "health" account exists for holder "d6666666-6666-6666-6666-666666666666" with origin IFSC "HDFC0006666" and account number "6666600001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I view the most recent transaction's detail page
    Then the transaction detail should show the transaction ID
    And the transaction detail should show the account ID
    And the transaction detail should show amount "50.00"
    And the transaction detail should show type "Deposit"

  Scenario: Posting a pending deposit from the detail page
    Given a "health" account exists for holder "d7777777-7777-7777-7777-777777777777" with origin IFSC "HDFC0007777" and account number "7777700001"
    And the account has a pending deposit of 5000 in the self-pool
    When I view that pending deposit's detail page
    And I click the Post button
    Then the transaction detail status should be "posted"

  Scenario: Detail page hides Post and Void for posted transactions
    Given a "health" account exists for holder "d8888888-8888-8888-8888-888888888888" with origin IFSC "HDFC0008888" and account number "8888800001"
    And the account has 5000 in self-pool and 0 in others-pool
    When I view the most recent transaction's detail page
    Then the Post button should not be visible
    And the Void button should not be visible

  Scenario: Payment from admin UI persists gateway_ref to detail page
    Given a "health" account exists for holder "deadbeef-aaaa-bbbb-cccc-111111111111" with origin IFSC "HDFC00DEAD1" and account number "9999900001"
    And the account has 5000 in self-pool and 5000 in others-pool
    When I pay 1000 to merchant "PHARMACY001" with MCC "5912" described as "ui ref" with gateway ref "gw-ui-pay-1"
    Then the payment should succeed
    When I view the most recent transaction's detail page
    Then the transaction detail should show gateway ref "gw-ui-pay-1"

  Scenario: Withdrawal from admin UI persists gateway_ref to detail page
    Given a "health" account exists for holder "deadbeef-aaaa-bbbb-cccc-222222222222" with origin IFSC "HDFC00DEAD2" and account number "9999900002"
    And the account has 5000 in self-pool and 0 in others-pool
    When I withdraw 1000 from the admin UI with gateway ref "gw-ui-wd-1"
    When I view the most recent transaction's detail page
    Then the transaction detail should show gateway ref "gw-ui-wd-1"
