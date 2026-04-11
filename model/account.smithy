$version: "2"
namespace com.ppi.pba

/// Create a new purpose-bound account.
@http(method: "POST", uri: "/accounts", code: 201)
operation CreateAccount {
    input := {
        @required
        holderId: String

        @required
        purposeCode: String

        @required
        originIfsc: String

        @required
        originAccountNumber: String
    }
    output := with [AccountMixin] {}
    errors: [PurposeTypeNotFoundError, DuplicateAccountError]
}

/// Get account metadata.
@readonly
@http(method: "GET", uri: "/accounts/{accountId}")
operation GetAccount {
    input := {
        @required
        @httpLabel
        accountId: String
    }
    output := with [AccountMixin] {}
    errors: [AccountNotFoundError]
}

/// Get pool balances for an account.
@readonly
@http(method: "GET", uri: "/accounts/{accountId}/balance")
operation GetBalance {
    input := {
        @required
        @httpLabel
        accountId: String
    }
    output := {
        @required
        accountId: String

        @required
        selfContribution: Money

        @required
        othersContribution: Money

        @required
        total: Money
    }
    errors: [AccountNotFoundError]
}

/// Update account status (freeze, close, reactivate).
@http(method: "PATCH", uri: "/accounts/{accountId}/status")
operation UpdateAccountStatus {
    input := {
        @required
        @httpLabel
        accountId: String

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
    holderId: String

    @required
    purposeCode: String

    @required
    originIfsc: String

    @required
    originAccountNumber: String

    vpa: String
    virtualIfsc: String
    virtualAccountNumber: String

    @required
    kycTier: String

    @required
    status: String

    @required
    createdAt: String

    @required
    updatedAt: String
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
