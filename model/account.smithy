$version: "2"
namespace com.ppi.pba

/// Create a new purpose-bound account.
@http(method: "POST", uri: "/accounts", code: 201)
operation CreateAccount {
    input := {
        @required
        holder_id: String

        @required
        purpose_code: String

        @required
        origin_ifsc: String

        @required
        origin_account_number: String
    }
    output := with [AccountMixin] {}
    errors: [PurposeTypeNotFoundError, DuplicateAccountError]
}

/// Get account metadata.
@readonly
@http(method: "GET", uri: "/accounts/{account_id}")
operation GetAccount {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := with [AccountMixin] {}
    errors: [AccountNotFoundError]
}

/// Get pool balances for an account.
@readonly
@http(method: "GET", uri: "/accounts/{account_id}/balance")
operation GetBalance {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := {
        @required
        account_id: String

        @required
        self_contribution: Money

        @required
        others_contribution: Money

        @required
        total: Money

        @required
        pending_self: Money

        @required
        pending_others: Money
    }
    errors: [AccountNotFoundError]
}

/// Update account status (freeze, close, reactivate).
@http(method: "PATCH", uri: "/accounts/{account_id}/status")
operation UpdateAccountStatus {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        status: Status
    }
    output := with [AccountMixin] {}
    errors: [AccountNotFoundError]
}

/// Shared account fields.
@mixin
structure AccountMixin {
    @required
    id: String

    @required
    holder_id: String

    @required
    purpose_code: String

    @required
    origin_ifsc: String

    @required
    origin_account_number: String

    vpa: String
    virtual_ifsc: String
    virtual_account_number: String

    @required
    kyc_tier: String

    @required
    status: String

    @required
    created_at: String

    @required
    updated_at: String
}

@error("client")
@httpError(404)
structure AccountNotFoundError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(409)
structure AccountNotActiveError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(409)
structure DuplicateAccountError {
    @required
    error: String
    @required
    message: String
}
