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
