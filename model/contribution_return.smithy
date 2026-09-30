$version: "2"
namespace com.ppi.pba

/// Entry in the allocation list for a contribution return.
structure AllocationEntry {
    @required
    original_transaction_id: String

    @required
    amount: Money
}

list AllocationEntries {
    member: AllocationEntry
}

/// Shared fields returned by all contribution-return operations.
@mixin
structure ContributionReturnResponseMixin {
    @required
    return_id: String

    @required
    correlation_id: String

    @required
    account_id: String

    @required
    funding_type: FundingType

    @required
    amount: Money

    @required
    allocations: AllocationEntries

    @required
    remaining_returnable_after: Money

    @required
    status: TransactionStatus

    @required
    created_at: DateTime
}

/// Per-funding-type contribution summary.
structure FundingTypeSummary {
    @required
    total_contributed: Money

    @required
    total_returned: Money

    @required
    remaining_returnable: Money
}

/// Return a contribution to the originating funder.
@http(method: "POST", uri: "/pb-accounts/{account_id}/contribution-returns", code: 201)
operation ReturnPBAccountContribution {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        amount: Money

        @required
        funding_type: FundingType

        pending: Boolean

        timeout_seconds: Integer

        gateway_ref: String

        description: String

        idempotency_key: String
    }
    output := with [ContributionReturnResponseMixin] {}
    errors: [
        AccountNotFoundError,
        AccountNotActiveError,
        ContributionAmountInvalidError,
        ContributionFullyReturnedError,
    ]
}

/// Post a pending contribution return (confirm the held reversal).
@http(method: "POST", uri: "/pb-accounts/{account_id}/contribution-returns/{return_id}/post", code: 200)
operation PostPBAccountContributionReturn {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        return_id: String
    }
    output := with [ContributionReturnResponseMixin] {}
    errors: [TransactionNotFoundError, TransactionNotPendingError]
}

/// Void a pending contribution return (cancel the held reversal).
@http(method: "POST", uri: "/pb-accounts/{account_id}/contribution-returns/{return_id}/void", code: 200)
operation VoidPBAccountContributionReturn {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        return_id: String
    }
    output := with [ContributionReturnResponseMixin] {}
    errors: [TransactionNotFoundError, TransactionNotPendingError]
}

/// Get a summary of contributions and returns per funding type.
@readonly
@http(method: "GET", uri: "/pb-accounts/{account_id}/contributions/summary", code: 200)
operation GetPBAccountContributionSummary {
    input := {
        @required
        @httpLabel
        account_id: String
    }
    output := {
        @required
        trust: FundingTypeSummary

        @required
        third_party: FundingTypeSummary
    }
    errors: [AccountNotFoundError]
}

@error("client")
@httpError(400)
structure ContributionAmountInvalidError {
    @required
    message: String

    @required
    requested: Money

    @required
    remaining: Money
}

@error("client")
@httpError(409)
structure ContributionFullyReturnedError {
    @required
    message: String

    @required
    pb_account_id: String
}
