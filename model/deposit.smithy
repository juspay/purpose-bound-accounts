$version: "2"
namespace com.ppi.pba

/// Deposit funds into a purpose-bound account.
/// Automatically routes to self-contribution or others-contribution pool
/// based on whether the source matches the account's origin bank.
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
    }
    output := {
        @required
        accountId: String

        @required
        amount: Money

        @required
        pool: String
    }
    errors: [AccountNotFoundError, AccountNotActiveError]
}
