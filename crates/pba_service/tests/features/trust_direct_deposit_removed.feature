Feature: Trust deposits to PB accounts have been removed

  Scenario: Trust deposit on canonical /pb-accounts URL is rejected
    Given a "health" account exists for holder "tdr-1111-1111-1111-1111-111111111111" with origin IFSC "HDFC0021011" and account number "9021011001"
    When I attempt to deposit 1000 from IFSC "ICIC0001234" account "1234567890" with funding type "trust"
    Then the operation should be rejected
    And the error code is "TrustDepositRequiresTransfer"

  Scenario: Self deposit still works
    Given a "health" account exists for holder "tdr-2222-2222-2222-2222-222222222222" with origin IFSC "HDFC0022012" and account number "9022012001"
    When I deposit 5000 from IFSC "HDFC0022012" account "9022012001"
    Then the deposit should go to "self" pool
    And the funding type should be "self"

  Scenario: Third-party deposit still works
    Given a "health" account exists for holder "tdr-3333-3333-3333-3333-333333333333" with origin IFSC "HDFC0023013" and account number "9023013001"
    When I deposit 7500 from IFSC "SBIN0005678" account "5678901234" with funding type "third_party"
    Then the deposit should go to "others" pool
    And the funding type should be "third_party"
