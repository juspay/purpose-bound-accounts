Feature: Admin UI for contribution returns

  Scenario: Contributions panel renders correct totals
    Given a normal account exists for holder "cr-ui-alice-01"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-ui-alice-01" with origin IFSC "HDFC0070001" and account number "7070001001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I open the PB account detail page
    Then the contributions panel shows trust contributed as "200.00"
    And the contributions panel shows trust returnable as "200.00"

  Scenario: Return form pre-selects funding_type from panel button
    Given a normal account exists for holder "cr-ui-bob-01"
    And the normal account has balance 20000
    And a "health" account exists for holder "cr-ui-bob-01" with origin IFSC "HDFC0070002" and account number "7070002001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I open the PB account detail page
    And I click "Return..." for trust
    Then the return form shows funding_type "trust"

  Scenario: Full trust return via UI credits sponsor and updates panel
    Given a normal account exists for holder "cr-ui-carla-01"
    And the normal account has balance 15000
    And a "health" account exists for holder "cr-ui-carla-01" with origin IFSC "HDFC0070003" and account number "7070003001"
    When I transfer 15000 paisa from the normal account to the PB account
    And I open the return form for trust
    And I enter 15000 as the return amount
    And I submit the contribution return form
    Then the return detail page shows status "settled"
    When I open the PB account detail page
    Then the contributions panel shows trust returnable as "0.00"

  Scenario: Pending return via UI renders Post and Void buttons
    Given a normal account exists for holder "cr-ui-dan-01"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr-ui-dan-01" with origin IFSC "HDFC0070004" and account number "7070004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I open the return form for trust
    And I enter 5000 as the return amount
    And I select "Hold as pending" mode for return
    And I submit the contribution return form
    Then the return detail page shows status "pending"
    And the Post return button is visible
    And the Void return button is visible

  Scenario: Post via UI flips return status to Settled
    Given a normal account exists for holder "cr-ui-eve-01"
    And the normal account has balance 10000
    And a "health" account exists for holder "cr-ui-eve-01" with origin IFSC "HDFC0070005" and account number "7070005001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I open the return form for trust
    And I enter 5000 as the return amount
    And I select "Hold as pending" mode for return
    And I submit the contribution return form
    Then the return detail page shows status "pending"
    When I click the Post return button
    Then the return detail page shows status "settled"
