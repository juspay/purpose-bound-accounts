-- Widen the reversal-uniqueness partial index so voided pending reversals do
-- not permanently block re-reversal of the original transfer. After a void
-- the original becomes re-eligible.
--
-- See docs/superpowers/specs/2026-06-22-two-phase-reversal-refund-design.md.

DROP INDEX IF EXISTS uq_transactions_reverses_transfer;

CREATE UNIQUE INDEX uq_transactions_reverses_transfer
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL
      AND type = 'transfer'
      AND status <> 'voided';
