$version: "2"
namespace com.ppi.pba

/// Withdraw funds from the self-contribution pool only.
/// Cannot withdraw from the others-contribution pool.
@http(method: "POST", uri: "/accounts/{account_id}/withdrawals", code: 201)
operation Withdraw {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money

        idempotency_key: String

        gateway_ref: String
    }
    output := {
        @required
        account_id: String

        @required
        amount: Money

        gateway_ref: String
    }
    errors: [
        AccountNotFoundError
        AccountNotActiveError
        InsufficientFundsError
    ]
}
