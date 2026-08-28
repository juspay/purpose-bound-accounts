$version: "2"
namespace com.ppi.pba

/// List transactions for a purpose-bound account with offset/limit pagination.
@readonly
@http(method: "GET", uri: "/pb-accounts/{account_id}/transactions")
operation ListPBAccountTransactions {
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

@deprecated(message: "Use ListPBAccountTransactions.", since: "2026-05-08")
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

/// List all transactions across all accounts with offset/limit pagination.
@readonly
@http(method: "GET", uri: "/transactions")
operation ListAllTransactions {
    input := {
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
}

list TransactionList {
    member: TransactionSummary
}

structure TransactionSummary {
    @required
    id: String

    @required
    account_id: String

    @required
    account_kind: String

    @required
    type: TransactionType

    @required
    status: TransactionStatus

    @required
    amount: Money

    pool: PoolType

    @required
    direction: TransactionDirection

    description: String
    void_reason: String
    merchant_id: String
    merchant_mcc: String
    source_ifsc: String
    source_account: String
    gateway_ref: String
    funding_type: String
    correlation_id: String

    @required
    created_at: DateTime
}
