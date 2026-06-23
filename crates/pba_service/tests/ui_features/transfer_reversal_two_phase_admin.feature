Feature: Admin UI for two-phase transfer reversals

  Scenario: Initiating a pending reversal renders pending detail page
    Given a normal account exists for holder "rv2-ui-alice-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rv2-ui-alice-01" with origin IFSC "HDFC0070001" and account number "9070001001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 2000 paisa
    Then I land on the transfer detail page
    When I open the reverse form for that transfer
    And I select "Hold as pending" mode
    And I submit the reverse form
    Then the transfer detail page shows status "pending"
    And the Post transfer button is visible on the detail page
    And the Void transfer button is visible on the detail page

  Scenario: Posting a pending reversal via UI flips status to posted
    Given a normal account exists for holder "rv2-ui-bob-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rv2-ui-bob-01" with origin IFSC "HDFC0070002" and account number "9070002001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 2000 paisa
    Then I land on the transfer detail page
    When I open the reverse form for that transfer
    And I select "Hold as pending" mode
    And I submit the reverse form
    Then the transfer detail page shows status "pending"
    When I click the post button on the transfer detail page
    Then the transfer detail page shows status "posted"

  Scenario: Voiding a pending reversal via UI flips status to voided and unlocks re-reversal
    Given a normal account exists for holder "rv2-ui-carla-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rv2-ui-carla-01" with origin IFSC "HDFC0070003" and account number "9070003001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 2000 paisa
    Then I land on the transfer detail page
    When I open the reverse form for that transfer
    And I select "Hold as pending" mode
    And I submit the reverse form
    Then the transfer detail page shows status "pending"
    When I click the void button on the transfer detail page
    Then the transfer detail page shows status "voided"
    And the original transfer is reversible again
