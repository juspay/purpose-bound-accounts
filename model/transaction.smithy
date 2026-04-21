$version: "2"
namespace com.ppi.pba

/// List transactions for an account with offset/limit pagination.
@readonly
@http(method: "GET", uri: "/accounts/{account_id}/transactions")
operation ListTransactions {
    input := {
        @required
        @httpLabel
        account_id: String

        @httpQuery("offset")
        offset: Long

        @httpQuery("limit")
        limit: Long

        @httpQuery("from_date")
        from_date: DateTime

        @httpQuery("to_date")
        to_date: DateTime
    }
    output := {
        @required
        transactions: TransactionList

        @required
        total: Long

        @required
        offset: Long

        @required
        limit: Long
    }
    errors: [AccountNotFoundError]
}

list TransactionList {
    member: TransactionSummary
}

structure TransactionSummary {
    @required
    id: String

    @required
    type: TransactionType

    @required
    status: TransactionStatus

    @required
    amount: Money

    @required
    pool: PoolType

    @required
    direction: TransactionDirection

    description: String
    merchant_id: String
    merchant_mcc: String
    source_ifsc: String
    source_account: String
    gateway_ref: String

    @required
    created_at: DateTime
}
