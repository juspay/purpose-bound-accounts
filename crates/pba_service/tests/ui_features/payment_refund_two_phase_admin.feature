Feature: Admin UI for two-phase refunds

  Scenario: Initiating a pending refund renders pending detail page
    Given a normal account exists for holder "rp2-ui-alice-01"
    And the normal account has balance 20000
    And a "health" account exists for holder "rp2-ui-alice-01" with origin IFSC "HDFC0060001" and account number "9060001001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 15000 to merchant "HOSP10" with MCC "8062" described as "two-phase-test"
    And I visit the transaction detail page for the last payment
    And I open the refund form for that payment
    And I select "Hold as pending" mode
    And I enter 15000 as the refund amount paisa
    And I submit the refund form
    Then the refund detail page shows status "pending"
    And the Post refund button is visible
    And the Void refund button is visible

  Scenario: Posting a pending refund via UI flips status to settled
    Given a normal account exists for holder "rp2-ui-bob-01"
    And the normal account has balance 20000
    And a "health" account exists for holder "rp2-ui-bob-01" with origin IFSC "HDFC0060002" and account number "9060002001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 10000 to merchant "HOSP11" with MCC "8062" described as "two-phase-post"
    And I visit the transaction detail page for the last payment
    And I open the refund form for that payment
    And I select "Hold as pending" mode
    And I enter 10000 as the refund amount paisa
    And I submit the refund form
    And I click the Post refund button on the refund detail page
    Then the refund detail page shows status "settled"

  Scenario: Voiding a pending refund via UI flips status to voided
    Given a normal account exists for holder "rp2-ui-carla-01"
    And the normal account has balance 30000
    And a "health" account exists for holder "rp2-ui-carla-01" with origin IFSC "HDFC0060003" and account number "9060003001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I pay 30000 to merchant "HOSP12" with MCC "8062" described as "two-phase-void"
    And I visit the transaction detail page for the last payment
    And I open the refund form for that payment
    And I select "Hold as pending" mode
    And I enter 10000 as the refund amount paisa
    And I submit the refund form
    And I click the Void refund button on the refund detail page
    Then the refund detail page shows status "voided"

  @todo
  Scenario: Refund history table shows pending and voided entries with strike-through
    Given a normal account exists for holder "rp2-ui-dan-01"
    And the normal account has balance 30000
    And a "health" account exists for holder "rp2-ui-dan-01" with origin IFSC "HDFC0060004" and account number "9060004001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I pay 30000 to merchant "HOSP13" with MCC "8062" described as "two-phase-history"
    And I visit the transaction detail page for the last payment
    And I open the refund form for that payment
    And I select "Hold as pending" mode
    And I enter 10000 as the refund amount paisa
    And I submit the refund form
    And I click the Void refund button on the refund detail page
    And I visit the transaction detail page for the last payment
    And I open the refund form for that payment
    And I enter 10000 as the refund amount paisa
    And I submit the refund form
    And I visit the transaction detail page for the last payment
    Then the refund history shows two entries
    And the voided entry is rendered with strike-through
