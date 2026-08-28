-- Free-text reason captured when a pending transaction is cancelled.
-- Kept separate from `description` so voiding never clobbers the narration
-- the caller supplied when the transaction was created.
ALTER TABLE transactions ADD COLUMN void_reason TEXT;
