$version: "2"
namespace com.ppi.pba

/// Withdraw funds from the self-contribution pool only.
/// Cannot withdraw from the others-contribution pool.
@http(method: "POST", uri: "/accounts/{accountId}/withdrawals", code: 201)
operation Withdraw {
    input := {
        @required
        @httpLabel
        accountId: String

        @required
        amount: Money
    }
    output := {
        @required
        accountId: String

        @required
        amount: Money
    }
    errors: [
        AccountNotFoundError
        AccountNotActiveError
        InsufficientFundsError
    ]
}
