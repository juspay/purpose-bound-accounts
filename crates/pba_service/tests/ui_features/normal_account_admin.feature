Feature: Normal account admin pages

  Scenario: Create normal account through the admin form
    When I navigate to the new normal account form
    And I submit the normal account form with holder "alice-na-01"
    Then I land on the normal account detail page
    And the normal account page shows holder "alice-na-01"
    And the normal account page shows status "active"

  Scenario: Deposit and withdraw from the admin UI
    Given a normal account exists for holder "bob-na-01"
    When I navigate to the deposit form for the normal account
    And I submit a normal deposit of 5000 paisa
    Then I am redirected to the normal account detail page
    And the normal account balance shown is "50.00"
    When I navigate to the withdrawal form for the normal account
    And I submit a normal withdrawal of 2000 paisa
    Then the normal account balance shown is "30.00"

  Scenario: Transactions list filters to normal-account rows
    Given a normal account exists for holder "carla-na-01" with one deposit and one withdrawal
    When I navigate to the normal account detail page
    Then I see exactly 2 transaction rows
