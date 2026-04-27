$version: "2"
namespace com.ppi.pba

/// Deposit funds into a purpose-bound account.
/// Automatically routes to self-contribution or others-contribution pool
/// based on whether the source matches the account's origin bank.
/// Set `pending` to true for two-phase deposits (pending → post/void).
@http(method: "POST", uri: "/accounts/{account_id}/deposits", code: 201)
operation Deposit {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        source_ifsc: String

        @required
        source_account_number: String

        funding_type: FundingType

        @required
        amount: Money

        pending: Boolean

        gateway_ref: String

        timeout_seconds: Integer

        idempotency_key: String
    }
    output := {
        @required
        deposit_id: String

        @required
        account_id: String

        @required
        amount: Money

        @required
        pool: String

        @required
        funding_type: String

        @required
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, AccountNotActiveError]
}

/// Confirm a pending deposit (post the held funds).
@http(method: "POST", uri: "/accounts/{account_id}/deposits/{deposit_id}/post")
operation PostDeposit {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        deposit_id: String
    }
    output := {
        @required
        deposit_id: String

        @required
        account_id: String

        @required
        amount: Money

        @required
        pool: String

        funding_type: String

        @required
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, DepositNotFoundError, DepositNotPendingError]
}

/// Cancel a pending deposit (void the held funds).
@http(method: "POST", uri: "/accounts/{account_id}/deposits/{deposit_id}/void")
operation VoidDeposit {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        deposit_id: String

        reason: String
    }
    output := {
        @required
        deposit_id: String

        @required
        account_id: String

        @required
        amount: Money

        @required
        pool: String

        funding_type: String

        @required
        status: String

        gateway_ref: String

        timeout_seconds: Integer
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
