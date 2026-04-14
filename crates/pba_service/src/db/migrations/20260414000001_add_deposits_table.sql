CREATE TABLE deposits (
    id              UUID PRIMARY KEY,
    account_id      UUID NOT NULL REFERENCES accounts(id),
    amount          BIGINT NOT NULL,
    pool            TEXT NOT NULL,
    source_ifsc     TEXT NOT NULL,
    source_account  TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    tb_transfer_id  NUMERIC(39,0) NOT NULL,
    gateway_ref     TEXT,
    timeout_seconds INTEGER CHECK (timeout_seconds > 0),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_deposits_account_status ON deposits(account_id, status);
