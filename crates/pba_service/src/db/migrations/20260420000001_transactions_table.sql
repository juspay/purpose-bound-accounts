-- Replace deposits table with unified transactions table
DROP TABLE IF EXISTS deposits;

CREATE TABLE transactions (
    id                UUID PRIMARY KEY,
    account_id        UUID NOT NULL REFERENCES accounts(id),
    type              TEXT NOT NULL,
    status            TEXT NOT NULL,
    amount            BIGINT NOT NULL,
    pool              TEXT NOT NULL,
    direction         TEXT NOT NULL,
    source_ifsc       TEXT,
    source_account    TEXT,
    gateway_ref       TEXT,
    timeout_seconds   INTEGER CHECK (timeout_seconds > 0),
    merchant_id       TEXT,
    merchant_mcc      TEXT,
    description       TEXT,
    tb_transfer_id    NUMERIC(39,0) NOT NULL DEFAULT 0,
    idempotency_key   TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_transactions_account ON transactions(account_id, created_at DESC);
CREATE INDEX idx_transactions_account_status ON transactions(account_id, status);
CREATE UNIQUE INDEX idx_transactions_idempotency ON transactions(account_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
