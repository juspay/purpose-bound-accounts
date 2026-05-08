Feature: Transfer admin UI

  Scenario: Initiate immediate transfer from the normal account detail page
    Given a normal account exists for holder "alice-tx-admin-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "alice-tx-admin-01" with origin IFSC "HDFC0091001" and account number "9091001001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination and submit a transfer of 2000 paisa
    Then I land on the transfer detail page
    And the transfer detail page shows source account holder "alice-tx-admin-01"
    And the transfer detail page shows status "posted"

  Scenario: Pending transfer + post via UI
    Given a normal account exists for holder "bob-tx-admin-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "bob-tx-admin-01" with origin IFSC "HDFC0092002" and account number "9092002001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination, set amount 1500, mark as pending, and submit
    Then the transfer detail page shows status "pending"
    When I click the post button on the transfer detail page
    Then the transfer detail page shows status "posted"

  Scenario: Pending transfer + void via UI
    Given a normal account exists for holder "carla-tx-admin-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "carla-tx-admin-01" with origin IFSC "HDFC0093003" and account number "9093003001"
    When I navigate to the transfer form for the normal account
    And I select the PB account as destination, set amount 1000, mark as pending, and submit
    Then the transfer detail page shows status "pending"
    When I click the void button on the transfer detail page
    Then the transfer detail page shows status "voided"
