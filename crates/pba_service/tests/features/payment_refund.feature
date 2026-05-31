Feature: Refund of PB → merchant payments
  Refunds credit back pool balances in the reverse order of the original
  payment split (self-pool first, then others-pool up to each pool's
  unrefunded portion).  All amounts are in paisa (1 INR = 100 paisa).

  # ── Scenario 1: Full refund of an others-only payment ──────────────────────

  Scenario: Full refund of an others-only payment
    Given a normal account exists for holder "rf-s01-alice"
    And the normal account has balance 50000
    And a "health" account exists for holder "rf-s01-alice" with origin IFSC "HDFC0030001" and account number "9030001001"
    When I transfer 50000 paisa from the normal account to the PB account
    And I pay 50000 to merchant "HOSP01" with MCC "8062" described as "others-only payment"
    And I refund 50000 paisa from the last payment
    Then the refund is successful
    And the refund credited 0 to self and 50000 to others
    And the remaining refundable amount is 0
    And the PB account others-pool balance is 50000

  # ── Scenario 2: Full refund of a split (self+others) payment ───────────────

  Scenario: Full refund of a split payment restores both pools
    Given a "health" account exists for holder "rf-s02-bob" with origin IFSC "HDFC0030002" and account number "9030002001"
    And the account has 30000 in self-pool and 20000 in others-pool
    When I pay 50000 to merchant "HOSP02" with MCC "8062" described as "split payment"
    And I refund 50000 paisa from the last payment
    Then the refund is successful
    And the refund credited 30000 to self and 20000 to others
    And the remaining refundable amount is 0

  # ── Scenario 3: Partial refund — others-only pool ──────────────────────────

  Scenario: Partial refund of an others-only payment
    Given a normal account exists for holder "rf-s03-carol"
    And the normal account has balance 60000
    And a "health" account exists for holder "rf-s03-carol" with origin IFSC "HDFC0030003" and account number "9030003001"
    When I transfer 60000 paisa from the normal account to the PB account
    And I pay 40000 to merchant "HOSP03" with MCC "8062" described as "others partial"
    And I refund 15000 paisa from the last payment
    Then the refund is successful
    And the refund credited 0 to self and 15000 to others
    And the remaining refundable amount is 25000
    And the PB account others-pool balance is 35000

  # ── Scenario 4: Partial refund spans self+others pools ────────────────────

  Scenario: Partial refund of a split payment spans both pools
    Given a "health" account exists for holder "rf-s04-dan" with origin IFSC "HDFC0030004" and account number "9030004001"
    And the account has 30000 in self-pool and 20000 in others-pool
    When I pay 50000 to merchant "HOSP04" with MCC "8062" described as "split for partial"
    And I refund 40000 paisa from the last payment
    Then the refund is successful
    And the refund credited 30000 to self and 10000 to others
    And the remaining refundable amount is 10000

  # ── Scenario 5: Sequential partial refunds; third attempt fails ───────────

  Scenario: Sequential partial refunds; fully-refunded payment is rejected
    Given a normal account exists for holder "rf-s05-eve"
    And the normal account has balance 30000
    And a "health" account exists for holder "rf-s05-eve" with origin IFSC "HDFC0030005" and account number "9030005001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I pay 30000 to merchant "HOSP05" with MCC "8062" described as "sequential refund"
    And I refund 10000 paisa from the last payment
    Then the refund is successful
    When I refund 10000 paisa from the last payment
    Then the refund is successful
    When I refund 10000 paisa from the last payment
    Then the refund is successful
    And the remaining refundable amount is 0
    When I attempt to refund 1 paisa from the last payment
    Then the refund fails with "PaymentFullyRefunded"

  # ── Scenario 6: Reject amount > total remaining ────────────────────────────

  Scenario: Refund amount exceeding remaining is rejected
    Given a normal account exists for holder "rf-s06-flo"
    And the normal account has balance 20000
    And a "health" account exists for holder "rf-s06-flo" with origin IFSC "HDFC0030006" and account number "9030006001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP06" with MCC "8062" described as "over-refund test"
    And I refund 5000 paisa from the last payment
    Then the refund is successful
    And the remaining refundable amount is 15000
    When I attempt to refund 16000 paisa from the last payment
    Then the refund fails with "RefundAmountInvalid"
    And the refund error remaining field is 15000

  # ── Scenario 7: Reject amount = 0 ─────────────────────────────────────────

  Scenario: Refund amount of zero is rejected
    Given a normal account exists for holder "rf-s07-gus"
    And the normal account has balance 10000
    And a "health" account exists for holder "rf-s07-gus" with origin IFSC "HDFC0030007" and account number "9030007001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I pay 10000 to merchant "HOSP07" with MCC "8062" described as "zero refund test"
    When I attempt to refund 0 paisa from the last payment
    Then the refund fails with "RefundAmountInvalid"

  # ── Scenario 8: Reject refunding a refund row ─────────────────────────────

  Scenario: Attempt to refund a refund row is rejected as not refundable
    Given a normal account exists for holder "rf-s08-han"
    And the normal account has balance 10000
    And a "health" account exists for holder "rf-s08-han" with origin IFSC "HDFC0030008" and account number "9030008001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I pay 10000 to merchant "HOSP08" with MCC "8062" described as "refund-of-refund test"
    And I refund 10000 paisa from the last payment
    Then the refund is successful
    When I attempt to refund 1000 paisa from the last refund
    Then the refund fails with "RefundNotRefundable" reason "is_itself_a_refund"

  # ── Scenario 9: Reject when frozen; succeed after reactivate ──────────────

  Scenario: Frozen PB account rejects refund; reactivation allows it
    Given a normal account exists for holder "rf-s09-ivy"
    And the normal account has balance 15000
    And a "health" account exists for holder "rf-s09-ivy" with origin IFSC "HDFC0030009" and account number "9030009001"
    When I transfer 15000 paisa from the normal account to the PB account
    And I pay 15000 to merchant "HOSP09" with MCC "8062" described as "freeze-then-refund"
    And I freeze the account
    And I attempt to refund 15000 paisa from the last payment
    Then the refund fails with "PbAccountNotActive"
    When I reactivate the account
    And I refund 15000 paisa from the last payment
    Then the refund is successful

  # ── Scenario 10: Idempotency replay ───────────────────────────────────────

  Scenario: Idempotency replay returns the same correlation_id
    Given a normal account exists for holder "rf-s10-jay"
    And the normal account has balance 20000
    And a "health" account exists for holder "rf-s10-jay" with origin IFSC "HDFC0030010" and account number "9030010001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP10" with MCC "8062" described as "idempotency test"
    And I refund 5000 paisa from the last payment with idempotency key "rf-idem-jay-1"
    Then the refund is successful
    When I refund 5000 paisa from the last payment with idempotency key "rf-idem-jay-1"
    Then the refund is successful
    And both refunds share the same correlation_id

  # ── Scenario 11: Wrong PB account ─────────────────────────────────────────

  Scenario: Refund with a non-existent PB account id fails
    Given a normal account exists for holder "rf-s11-kay"
    And the normal account has balance 10000
    And a "health" account exists for holder "rf-s11-kay" with origin IFSC "HDFC0030011" and account number "9030011001"
    When I transfer 10000 paisa from the normal account to the PB account
    And I pay 10000 to merchant "HOSP11" with MCC "8062" described as "wrong-account test"
    When I attempt to refund 10000 paisa from the last payment under a different PB account
    Then the refund fails with "RefundNotRefundable" reason "wrong_account"
