Feature: Two-phase refund of PB -> merchant payments
  A refund may be initiated as Pending with a timeout, then committed via
  the new /pb-accounts/{id}/refunds/{refund_id}/post endpoint or rolled back
  via /void. Pending refunds reserve their slice of the remaining-refundable
  budget so concurrent initiates do not over-refund.

  @api
  Scenario: Pending single-pool refund reserves remaining
    Given a normal account exists for holder "pr2p-s01-alice"
    And the normal account has balance 50000
    And a "health" account exists for holder "pr2p-s01-alice" with origin IFSC "HDFC0092001" and account number "9092001001"
    When I transfer 50000 paisa from the normal account to the PB account
    And I pay 50000 to merchant "HOSP01" with MCC "8062" described as "others-only payment"
    And I initiate a pending refund of 20000 paisa from the last payment
    Then the refund status is "pending"
    And the remaining refundable amount is 30000
    When I attempt to refund 40000 paisa from the last payment
    Then the refund fails with "RefundAmountInvalid"
    And the refund error remaining field is 30000

  @api
  Scenario: Pending single-pool refund then post
    Given a normal account exists for holder "pr2p-s02-bob"
    And the normal account has balance 50000
    And a "health" account exists for holder "pr2p-s02-bob" with origin IFSC "HDFC0092002" and account number "9092002001"
    When I transfer 50000 paisa from the normal account to the PB account
    And I pay 50000 to merchant "HOSP02" with MCC "8062" described as "others-only payment"
    And I initiate a pending refund of 30000 paisa from the last payment
    And I post the pending refund
    Then the refund status is "settled"
    And the remaining refundable amount is 20000

  @api
  Scenario: Pending split refund then post (LINKED legs)
    Given a "health" account exists for holder "pr2p-s03-carol" with origin IFSC "HDFC0092003" and account number "9092003001"
    And the account has 30000 in self-pool and 20000 in others-pool
    When I pay 50000 to merchant "HOSP03" with MCC "8062" described as "split payment"
    And I initiate a pending refund of 50000 paisa from the last payment
    And I post the pending refund
    Then the refund status is "settled"
    And the refund credited 30000 to self and 20000 to others

  @api
  Scenario: Pending refund then void restores remaining
    Given a normal account exists for holder "pr2p-s04-dan"
    And the normal account has balance 40000
    And a "health" account exists for holder "pr2p-s04-dan" with origin IFSC "HDFC0092004" and account number "9092004001"
    When I transfer 40000 paisa from the normal account to the PB account
    And I pay 40000 to merchant "HOSP04" with MCC "8062" described as "refund-then-void"
    And I initiate a pending refund of 15000 paisa from the last payment
    And I void the pending refund
    Then the refund status is "voided"
    When I refund 40000 paisa from the last payment
    Then the refund is successful
    And the remaining refundable amount is 0

  @api
  Scenario: Concurrent pending refunds reserve remaining
    Given a "health" account exists for holder "pr2p-s05-eve" with origin IFSC "HDFC0092005" and account number "9092005001"
    And the account has 5000 in self-pool and 5000 in others-pool
    When I pay 1000 to merchant "HOSP05" with MCC "8062" described as "concurrent pending refund"
    And 5 concurrent pending refunds of 300 paisa each are attempted on the last payment
    Then the total refunded amount across all refunds is at most 1000 paisa

  @api
  Scenario: Post on already-posted refund is a no-op
    Given a normal account exists for holder "pr2p-s06-flo"
    And the normal account has balance 20000
    And a "health" account exists for holder "pr2p-s06-flo" with origin IFSC "HDFC0092006" and account number "9092006001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP06" with MCC "8062" described as "double-post"
    And I initiate a pending refund of 10000 paisa from the last payment
    And I post the pending refund
    And I post the pending refund
    Then the refund status is "settled"

  @api
  Scenario: Void on already-voided refund is a no-op
    Given a normal account exists for holder "pr2p-s07-gus"
    And the normal account has balance 20000
    And a "health" account exists for holder "pr2p-s07-gus" with origin IFSC "HDFC0092007" and account number "9092007001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP07" with MCC "8062" described as "double-void"
    And I initiate a pending refund of 10000 paisa from the last payment
    And I void the pending refund
    And I void the pending refund
    Then the refund status is "voided"

  @api
  Scenario: Mixed direction (post then void) rejected
    Given a normal account exists for holder "pr2p-s08-han"
    And the normal account has balance 20000
    And a "health" account exists for holder "pr2p-s08-han" with origin IFSC "HDFC0092008" and account number "9092008001"
    When I transfer 20000 paisa from the normal account to the PB account
    And I pay 20000 to merchant "HOSP08" with MCC "8062" described as "mixed-direction"
    And I initiate a pending refund of 10000 paisa from the last payment
    And I post the pending refund
    And I attempt to void the pending refund
    Then the operation fails with "TransactionNotPending"

  @api
  Scenario: Full lifecycle pay -> pending refund -> void -> pending refund -> post
    Given a normal account exists for holder "pr2p-s09-ivy"
    And the normal account has balance 30000
    And a "health" account exists for holder "pr2p-s09-ivy" with origin IFSC "HDFC0092009" and account number "9092009001"
    When I transfer 30000 paisa from the normal account to the PB account
    And I pay 30000 to merchant "HOSP09" with MCC "8062" described as "lifecycle"
    And I initiate a pending refund of 15000 paisa from the last payment
    And I void the pending refund
    And I initiate a pending refund of 15000 paisa from the last payment
    And I post the pending refund
    Then the refund status is "settled"
    And the remaining refundable amount is 15000
