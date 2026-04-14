$version: "2"
namespace com.ppi.pba

/// Deposit funds into a purpose-bound account.
/// Automatically routes to self-contribution or others-contribution pool
/// based on whether the source matches the account's origin bank.
/// Set `pending` to true for two-phase deposits (pending → post/void).
@http(method: "POST", uri: "/accounts/{accountId}/deposits", code: 201)
operation Deposit {
    input := {
        @required
        @httpLabel
        accountId: String

        @required
        sourceIfsc: String

        @required
        sourceAccountNumber: String

        @required
        amount: Money

        pending: Boolean

        gatewayRef: String

        timeoutSeconds: Integer
    }
    output := {
        @required
        depositId: String

        @required
        accountId: String

        @required
        amount: Money

        @required
        pool: String

        @required
        status: String

        gatewayRef: String

        timeoutSeconds: Integer
    }
    errors: [AccountNotFoundError, AccountNotActiveError]
}

/// Confirm a pending deposit (post the held funds).
@http(method: "POST", uri: "/accounts/{accountId}/deposits/{depositId}/post")
operation PostDeposit {
    input := {
        @required
        @httpLabel
        accountId: String

        @required
        @httpLabel
        depositId: String
    }
    output := {
        @required
        depositId: String

        @required
        accountId: String

        @required
        amount: Money

        @required
        pool: String

        @required
        status: String

        gatewayRef: String

        timeoutSeconds: Integer
    }
    errors: [AccountNotFoundError, DepositNotFoundError, DepositNotPendingError]
}

/// Cancel a pending deposit (void the held funds).
@http(method: "POST", uri: "/accounts/{accountId}/deposits/{depositId}/void")
operation VoidDeposit {
    input := {
        @required
        @httpLabel
        accountId: String

        @required
        @httpLabel
        depositId: String

        reason: String
    }
    output := {
        @required
        depositId: String

        @required
        accountId: String

        @required
        amount: Money

        @required
        pool: String

        @required
        status: String

        gatewayRef: String

        timeoutSeconds: Integer
    }
    errors: [AccountNotFoundError, DepositNotFoundError, DepositNotPendingError]
}

@error("client")
@httpError(404)
structure DepositNotFoundError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(409)
structure DepositNotPendingError {
    @required
    error: String
    @required
    message: String
}
