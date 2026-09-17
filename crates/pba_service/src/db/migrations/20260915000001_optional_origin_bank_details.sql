-- Make origin bank details optional on purpose-bound accounts.
-- Accounts may now be created without origin IFSC / account number when the
-- PBA_OPTIONAL_ORIGIN_ENABLED feature flag is on; deposits then rely on an
-- explicit funding_type = 'self' rather than origin matching.
ALTER TABLE pb_accounts ALTER COLUMN origin_ifsc DROP NOT NULL;
ALTER TABLE pb_accounts ALTER COLUMN origin_account_number DROP NOT NULL;
