$version: "2"
namespace com.ppi.pba

/// Make a payment from a purpose-bound account.
/// Validates the merchant's MCC against the account's purpose type.
/// Uses others-contribution pool first, then self-contribution.
@http(method: "POST", uri: "/accounts/{accountId}/payments", code: 201)
operation MakePayment {
    input := {
        @required
        @httpLabel
        accountId: String

        @required
        amount: Money

        @required
        merchantMcc: String

        @required
        merchantId: String

        @required
        description: String
    }
    output := {
        @required
        accountId: String

        @required
        amount: Money

        @required
        fromOthers: Money

        @required
        fromSelf: Money

        @required
        merchantId: String

        @required
        merchantMcc: String
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
