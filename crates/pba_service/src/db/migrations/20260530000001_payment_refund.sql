-- Tighten the transfer-reversal uniqueness so payment refunds can have many
-- rows pointing at the same original payment row. The transfer-reversal
-- at-most-one invariant is preserved by restricting the index to type='transfer'.
--
-- See docs/superpowers/specs/2026-05-30-payment-refund-design.md.

DROP INDEX uq_transactions_reverses;

CREATE UNIQUE INDEX uq_transactions_reverses_transfer
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL AND type = 'transfer';

-- The plain partial index idx_transactions_reverses_transaction_id from the
-- previous migration is unchanged — it supports both find_reversal_of (single
-- row, transfers) and find_refunds_of (many rows, payments).
