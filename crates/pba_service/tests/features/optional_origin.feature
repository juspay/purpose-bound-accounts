Feature: Optional origin bank details and explicit self-funding
  When the PBA_OPTIONAL_ORIGIN_ENABLED feature is on, a purpose-bound account may
  be created without origin bank details. Such accounts cannot classify deposits
  as "self" by origin matching, so depositors pass funding_type "self" explicitly
  to reach the self pool. Third-party deposits and origin-matching self-detection
  continue to work unchanged.

  Scenario: Create a PB account without origin bank details
    When I create a "health" account for holder "a1111111-1111-1111-1111-111111111111" without origin bank details
    Then the account should be created successfully
    And the account purpose should be "health"
    And the account status should be "active"
    And the account should have no origin bank details

  Scenario: Explicit self-funded deposit to an account without origin
    Given a "health" account exists for holder "a2222222-2222-2222-2222-222222222222" without origin bank details
    When I deposit 5000 from IFSC "HDFC0012222" account "1222200001" with funding type "self"
    Then the deposit should go to "self" pool
    And the funding type should be "self"

  Scenario: Third-party deposit to an account without origin still reaches others-pool
    Given a "health" account exists for holder "a3333333-3333-3333-3333-333333333333" without origin bank details
    When I deposit 3000 from IFSC "SBIN0005678" account "5678901234" with funding type "third_party"
    Then the deposit should go to "others" pool
    And the funding type should be "third_party"
