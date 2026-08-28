$version: "2"
namespace com.ppi.pba

/// Create a new normal (non-purpose-bound) account.
@http(method: "POST", uri: "/normal-accounts", code: 201)
operation CreateNormalAccount {
    input := {
        @required
        holder_id: String

        origin_ifsc: String

        origin_account_number: String
    }
    output := with [NormalAccountMixin] {}
}

/// Get normal account metadata.
@readonly
@http(method: "GET", uri: "/normal-accounts/{account_id}")
operation GetNormalAccount {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := with [NormalAccountMixin] {}
    errors: [AccountNotFoundError]
}

/// List all normal accounts.
@readonly
@http(method: "GET", uri: "/normal-accounts")
operation ListNormalAccounts {
    output := {
        @required
        accounts: NormalAccountList

        @required
        total: Long

        @required
        offset: Long

        @required
        limit: Long
    }
}

/// Update normal account status (freeze, close, reactivate).
@http(method: "PATCH", uri: "/normal-accounts/{account_id}/status")
operation UpdateNormalAccountStatus {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        status: String
    }
    output := with [NormalAccountMixin] {}
    errors: [AccountNotFoundError]
}

/// Get balance for a normal account.
@readonly
@http(method: "GET", uri: "/normal-accounts/{account_id}/balance")
operation GetNormalAccountBalance {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := {
        @required
        account_id: String

        @required
        balance: Money

        @required
        pending: Money
    }
    errors: [AccountNotFoundError]
}

/// Deposit funds into a normal account.
@http(method: "POST", uri: "/normal-accounts/{account_id}/deposits", code: 201)
operation DepositToNormalAccount {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money

        pending: Boolean

        gateway_ref: String

        timeout_seconds: Integer

        description: String

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
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, AccountNotActiveError]
}

/// Confirm a pending normal-account deposit (post the held funds).
@http(method: "POST", uri: "/normal-accounts/{account_id}/deposits/{deposit_id}/post")
operation PostNormalAccountDeposit {
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
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, DepositNotFoundError, DepositNotPendingError]
}

/// Cancel a pending normal-account deposit (void the held funds).
@http(method: "POST", uri: "/normal-accounts/{account_id}/deposits/{deposit_id}/void")
operation VoidNormalAccountDeposit {
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
        status: String

        gateway_ref: String

        timeout_seconds: Integer
    }
    errors: [AccountNotFoundError, DepositNotFoundError, DepositNotPendingError]
}

/// Withdraw funds from a normal account.
@http(method: "POST", uri: "/normal-accounts/{account_id}/withdrawals", code: 201)
operation WithdrawFromNormalAccount {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money

        gateway_ref: String

        description: String

        idempotency_key: String
    }
    output := {
        @required
        account_id: String

        @required
        amount: Money

        gateway_ref: String
    }
    errors: [AccountNotFoundError, AccountNotActiveError, InsufficientFundsError]
}

/// List transactions for a normal account with offset/limit pagination.
@readonly
@http(method: "GET", uri: "/normal-accounts/{account_id}/transactions")
operation ListNormalAccountTransactions {
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

/// Shared normal account fields.
@mixin
structure NormalAccountMixin {
    @required
    id: String

    @required
    holder_id: String

    origin_ifsc: String

    origin_account_number: String

    vpa: String

    virtual_ifsc: String

    virtual_account_number: String

    @required
    kyc_tier: String

    @required
    status: String

    @required
    created_at: DateTime

    @required
    updated_at: DateTime
}

list NormalAccountList {
    member: NormalAccountSummary
}

structure NormalAccountSummary with [NormalAccountMixin] {}
