-- Add account_kind discriminator. Default 'pb' backfills existing rows; then drop default.
ALTER TABLE transactions
    ADD COLUMN account_kind   VARCHAR(10) NOT NULL DEFAULT 'pb',
    ADD COLUMN correlation_id UUID NULL,
    ALTER COLUMN pool DROP NOT NULL;

ALTER TABLE transactions ALTER COLUMN account_kind DROP DEFAULT;

-- Constrain account_kind to the known set. Adding a third kind requires a follow-up
-- migration alongside the code that handles it, which is the desired coupling.
ALTER TABLE transactions
    ADD CONSTRAINT transactions_account_kind_check
    CHECK (account_kind IN ('pb', 'normal'));

-- Drop the FK on transactions.account_id (was pointing at pb_accounts after the
-- Phase 1 rename). With normal_accounts as a sibling table, the column now
-- references one of two tables; the application enforces the link via account_kind.
ALTER TABLE transactions DROP CONSTRAINT transactions_account_id_fkey;

-- Replace the per-account index with a kind-aware composite.
DROP INDEX IF EXISTS idx_transactions_account;
CREATE INDEX idx_transactions_account_kind_account
    ON transactions (account_kind, account_id, created_at DESC);

-- Correlation lookup index (used to find both legs of an internal transfer).
CREATE INDEX idx_transactions_correlation
    ON transactions (correlation_id) WHERE correlation_id IS NOT NULL;

-- Idempotency unique index now keyed on (kind, account, key).
DROP INDEX IF EXISTS idx_transactions_idempotency;
CREATE UNIQUE INDEX uq_transactions_idempotency
    ON transactions (account_kind, account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
