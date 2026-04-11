-- Purpose-based MCC allowlist (single table — purpose list derived via DISTINCT)
CREATE TABLE purpose_mcc_allowlist (
    purpose_code VARCHAR(20) NOT NULL,
    mcc VARCHAR(4) NOT NULL,
    mcc_description TEXT,
    active BOOLEAN NOT NULL DEFAULT true,
    PRIMARY KEY (purpose_code, mcc)
);

-- Purpose-bound accounts
CREATE TABLE accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    holder_id UUID NOT NULL,
    purpose_code VARCHAR(20) NOT NULL,
    origin_ifsc VARCHAR(11) NOT NULL,
    origin_account_number VARCHAR(20) NOT NULL,
    vpa VARCHAR(50),
    virtual_ifsc VARCHAR(11),
    virtual_account_number VARCHAR(20),
    tb_self_account_id NUMERIC(39) NOT NULL,
    tb_others_account_id NUMERIC(39) NOT NULL,
    kyc_tier VARCHAR(10) NOT NULL DEFAULT 'minimum',
    status VARCHAR(10) NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for duplicate detection: same origin bank + purpose = one account
CREATE UNIQUE INDEX idx_accounts_origin_purpose
    ON accounts (origin_ifsc, origin_account_number, purpose_code);

-- Index for holder lookup
CREATE INDEX idx_accounts_holder ON accounts (holder_id);
