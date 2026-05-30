Feature: Payment refund admin UI

  Scenario: Refund button is visible on a settled single-pool payment
    Given a normal account exists for holder "rf-ui-alice-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rf-ui-alice-01" with origin IFSC "HDFC0050001" and account number "9050001001"
    When I transfer 5000 paisa from the normal account to the PB account
    And I pay 5000 to merchant "HOSP01" with MCC "8062" described as "consultation"
    And I visit the transaction detail page for the last payment
    Then the page shows a Refund button

  Scenario: Refund button is absent on a refund row
    Given a normal account exists for holder "rf-ui-bob-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rf-ui-bob-01" with origin IFSC "HDFC0050002" and account number "9050002001"
    When I transfer 5000 paisa from the normal account to the PB account
    And I pay 5000 to merchant "HOSP02" with MCC "8062" described as "scan"
    And I visit the transaction detail page for the last payment
    And I click the Refund button and submit the refund form with amount 5000
    And I visit the transaction detail page for the last refund
    Then the page does not show a Refund button
    And the page shows "Refund of payment"

  Scenario: Refund button is absent on a fully refunded payment, history visible
    Given a normal account exists for holder "rf-ui-carla-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rf-ui-carla-01" with origin IFSC "HDFC0050003" and account number "9050003001"
    When I transfer 5000 paisa from the normal account to the PB account
    And I pay 5000 to merchant "HOSP03" with MCC "8062" described as "labs"
    And I visit the transaction detail page for the last payment
    And I click the Refund button and submit the refund form with amount 5000
    And I visit the transaction detail page for the last payment
    Then the page does not show a Refund button
    And the page shows a refund history entry for 5000 paisa total

  Scenario: Partial refund leaves the button visible with reduced remaining
    Given a normal account exists for holder "rf-ui-dan-01"
    And the normal account has balance 10000
    And a "health" account exists for holder "rf-ui-dan-01" with origin IFSC "HDFC0050004" and account number "9050004001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I pay 10000 to merchant "HOSP04" with MCC "8062" described as "surgery"
    And I visit the transaction detail page for the last payment
    And I click the Refund button and submit the refund form with amount 3000
    And I visit the transaction detail page for the last payment
    Then the page shows a Refund button
    And the page shows a refund history entry for 3000 paisa total
    And the remaining refundable on the payment page shows 7000 paisa

  Scenario: Over-amount refund surfaces inline error
    Given a normal account exists for holder "rf-ui-eve-01"
    And the normal account has balance 5000
    And a "health" account exists for holder "rf-ui-eve-01" with origin IFSC "HDFC0050005" and account number "9050005001"
    When I transfer 5000 paisa from the normal account to the PB account
    And I pay 5000 to merchant "HOSP05" with MCC "8062" described as "checkup"
    And I visit the transaction detail page for the last payment
    And I click the Refund button and submit the refund form with amount 5001
    Then the refund form shows an error containing "Refund amount invalid"
