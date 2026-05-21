-- Add reverses_transaction_id link to support reversal of posted transfers.
-- See docs/superpowers/specs/2026-05-21-transfer-reversal-design.md.

ALTER TABLE transactions
    ADD COLUMN reverses_transaction_id UUID NULL;

-- Enforce at-most-one reversal per original transfer.
CREATE UNIQUE INDEX uq_transactions_reverses
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL;

-- Supports `find_reversal_of(original_id)` lookups.
CREATE INDEX idx_transactions_reverses_transaction_id
    ON transactions (reverses_transaction_id)
    WHERE reverses_transaction_id IS NOT NULL;
