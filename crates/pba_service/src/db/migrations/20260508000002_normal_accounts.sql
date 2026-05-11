CREATE TABLE normal_accounts (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    holder_id              VARCHAR(255) NOT NULL,
    origin_ifsc            VARCHAR(11),
    origin_account_number  VARCHAR(20),
    vpa                    VARCHAR(50),
    virtual_ifsc           VARCHAR(11),
    virtual_account_number VARCHAR(20),
    tb_account_id          NUMERIC(39) NOT NULL,
    kyc_tier               VARCHAR(10) NOT NULL DEFAULT 'minimum',
    status                 VARCHAR(10) NOT NULL DEFAULT 'active',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_normal_accounts_holder ON normal_accounts (holder_id);
