$version: "2"
namespace com.ppi.pba

/// Make a payment from a purpose-bound account.
/// Validates the merchant's MCC against the account's purpose type.
/// Uses others-contribution pool first, then self-contribution.
@http(method: "POST", uri: "/accounts/{account_id}/payments", code: 201)
operation MakePayment {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money

        @required
        merchant_mcc: String

        @required
        merchant_id: String

        @required
        description: String
    }
    output := {
        @required
        account_id: String

        @required
        amount: Money

        @required
        from_others: Money

        @required
        from_self: Money

        @required
        merchant_id: String

        @required
        merchant_mcc: String
    }
    errors: [
        AccountNotFoundError
        AccountNotActiveError
        InvalidMccError
        InsufficientFundsError
    ]
}

@error("client")
@httpError(422)
structure InvalidMccError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(422)
structure InsufficientFundsError {
    @required
    error: String
    @required
    message: String
}
