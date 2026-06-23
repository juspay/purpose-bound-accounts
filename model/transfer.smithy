$version: "2"
namespace com.ppi.pba

/// Initiate a transfer of trust funds from a normal account to a PB account's others-pool.
///
/// Source side records a Transfer (outbound, pool=null, funding_type='trust').
/// Destination side records a Deposit (inbound, pool='others', funding_type='trust').
/// Both legs share a correlation_id.
@http(method: "POST", uri: "/normal-accounts/{account_id}/transfers", code: 201)
operation TransferToPBAccount {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        destination_pb_account_id: String

        @required
        amount: Money

        pending: Boolean

        gateway_ref: String

        timeout_seconds: Integer

        description: String

        idempotency_key: String
    }
    output := with [TransferResponseMixin] {}
    errors: [AccountNotFoundError]
}

/// Post a pending transfer — both legs flip to 'posted' atomically via correlation_id.
@http(method: "POST", uri: "/normal-accounts/{account_id}/transfers/{transfer_id}/post")
operation PostNormalAccountTransfer {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        transfer_id: String
    }
    output := with [TransferResponseMixin] {}
    errors: [AccountNotFoundError]
}

/// Void a pending transfer — both legs flip to 'voided'.
@http(method: "POST", uri: "/normal-accounts/{account_id}/transfers/{transfer_id}/void")
operation VoidNormalAccountTransfer {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        transfer_id: String
    }
    output := with [TransferResponseMixin] {}
    errors: [AccountNotFoundError]
}

/// Reverse a posted normal→PB transfer.
///
/// Records a new compensating transaction pair (PB others-pool debit + normal
/// account credit) plus a new TB transfer in the opposite direction. The
/// original transfer rows are not mutated; the reversal links back via
/// `reverses_transaction_id` on the normal-side reversal row.
///
/// Only `posted` transfers can be reversed. Pending transfers should be
/// cancelled via VoidNormalAccountTransfer. At most one reversal per original
/// transfer. Both source and destination accounts must be Active. The PB
/// others-pool must have sufficient balance; if not, returns InsufficientFunds
/// with the available amount.
@http(
    method: "POST",
    uri: "/normal-accounts/{account_id}/transfers/{transfer_id}/reverse",
    code: 201
)
operation ReverseNormalAccountTransfer {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        transfer_id: String

        @required
        amount: Money

        pending: Boolean

        gateway_ref: String

        timeout_seconds: Integer

        description: String

        idempotency_key: String
    }
    output := with [ReversalResponseMixin] {}
    errors: [AccountNotFoundError]
}

@mixin
structure ReversalResponseMixin {
    @required
    reversal_id: String

    @required
    original_transfer_id: String

    @required
    source_account_id: String

    @required
    destination_account_id: String

    @required
    amount: Money

    @required
    original_amount: Money

    @required
    status: String

    @required
    correlation_id: String

    @required
    created_at: DateTime
}

@mixin
structure TransferResponseMixin {
    @required
    transfer_id: String

    @required
    source_account_id: String

    @required
    destination_account_id: String

    @required
    amount: Money

    @required
    status: String

    @required
    correlation_id: String

    @required
    created_at: DateTime
}
