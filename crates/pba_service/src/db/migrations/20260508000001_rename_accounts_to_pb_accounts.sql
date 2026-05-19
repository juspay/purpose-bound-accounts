-- Rename the dual-pool account table to pb_accounts (purpose-bound).
-- A future migration introduces normal_accounts as a sibling table.
ALTER TABLE accounts RENAME TO pb_accounts;
-- NOTE: idx_accounts_origin_purpose was already dropped in
-- 20260428000001_drop_origin_purpose_uniqueness.sql; no rename needed here.
ALTER INDEX idx_accounts_holder RENAME TO idx_pb_accounts_holder;

-- The existing FK on transactions.account_id was created as
-- transactions_account_id_fkey REFERENCES accounts(id). Postgres preserves
-- the FK target across the rename automatically. We do NOT drop it here —
-- the FK now points at pb_accounts(id), which is correct for Phase 1.
