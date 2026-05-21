Feature: Transfer reversal admin UI

  Scenario: Reverse button is visible on a posted transfer
    Given a normal account exists for holder "rev-ui-alice-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-ui-alice-01" with origin IFSC "HDFC0041001" and account number "9041001001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 2000 paisa
    Then I land on the transfer detail page
    And the Reverse button is visible on the transfer detail page

  Scenario: Reverse button is absent on a pending transfer
    Given a normal account exists for holder "rev-ui-bob-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-ui-bob-01" with origin IFSC "HDFC0042002" and account number "9042002001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination, set amount 1500, mark as pending, and submit
    Then the transfer detail page shows status "pending"
    And the Reverse button is not visible on the transfer detail page

  Scenario: Reverse button is absent on a reversal row
    Given a normal account exists for holder "rev-ui-carla-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-ui-carla-01" with origin IFSC "HDFC0043003" and account number "9043003001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 2000 paisa
    Then I land on the transfer detail page
    When I click the Reverse button and submit the reverse form with amount 2000
    Then the transfer detail page shows a "Reversed by" link
    When I follow the "Reversed by" link
    Then the Reverse button is not visible on the transfer detail page
    And the transfer detail page shows that this row is a reversal

  Scenario: Reversal action flow updates the original transfer
    Given a normal account exists for holder "rev-ui-dan-01"
    And the normal account has balance 10000
    And a "education" account exists for holder "rev-ui-dan-01" with origin IFSC "HDFC0044004" and account number "9044004001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 4000 paisa
    Then I land on the transfer detail page
    When I click the Reverse button and submit the reverse form with amount 4000
    Then I land on the transfer detail page
    And the transfer detail page shows a "Reversed by" link

  Scenario: Reverse form shows InsufficientFunds when others-pool has been spent
    Given a normal account exists for holder "rev-ui-eve-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rev-ui-eve-01" with origin IFSC "HDFC0045005" and account number "9045005001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 1000 paisa
    Then I land on the transfer detail page
    When the PB account spends 700 paisa on merchant "HOSP01" with MCC "8062"
    And I click the Reverse button and submit the reverse form with amount 1000
    Then the reverse form shows an InsufficientFunds error
