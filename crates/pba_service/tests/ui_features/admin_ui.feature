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

  Scenario: Purpose types page lists all purposes
    When I visit the purpose types page
    Then I should see at least 4 purpose types listed
