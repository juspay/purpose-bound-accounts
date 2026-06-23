$version: "2"
namespace com.ppi.pba

/// Make a payment from a purpose-bound account.
/// Validates the merchant's MCC against the account's purpose type.
/// Uses others-contribution pool first, then self-contribution.
@http(method: "POST", uri: "/pb-accounts/{account_id}/payments", code: 201)
operation MakePBAccountPayment {
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

        idempotency_key: String

        gateway_ref: String
    }
    output := {
        /// Stable identifier for this payment. For split payments, this is also the
        /// correlation_id linking the per-pool legs.
        @required
        payment_id: String

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

        gateway_ref: String
    }
    errors: [
        AccountNotFoundError
        AccountNotActiveError
        InvalidMccError
        InsufficientFundsError
    ]
}

@deprecated(message: "Use MakePBAccountPayment.", since: "2026-05-08")
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

        idempotency_key: String

        gateway_ref: String
    }
    output := {
        /// Stable identifier for this payment. For split payments, this is also the
        /// correlation_id linking the per-pool legs.
        @required
        payment_id: String

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

        gateway_ref: String
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

/// Refund a previously settled PB→merchant payment.
///
/// Records a new compensating transaction (1 or 2 rows mirroring the
/// payment's pool split) plus matching TB transfer(s) in the opposite
/// direction. Original payment rows are not mutated; each refund row
/// links back via `reverses_transaction_id`.
///
/// Multiple partial refunds are allowed per payment; the sum must not
/// exceed the original payment amount. Refund credits self-pool first up
/// to that pool's remaining-unrefunded amount, then others-pool. The PB
/// account must be Active. Refunds cannot themselves be refunded.
@http(
    method: "POST",
    uri: "/pb-accounts/{account_id}/payments/{payment_id}/refund",
    code: 201
)
operation RefundPBAccountPayment {
    input := {
        @required
        @httpLabel
        account_id: String

        @required
        @httpLabel
        payment_id: String

        @required
        amount: Money

        description: String

        gateway_ref: String

        idempotency_key: String

        pending: Boolean

        timeout_seconds: Integer
    }
    output := with [RefundResponseMixin] {}
    errors: [
        AccountNotFoundError,
        RefundNotRefundableError,
        RefundAmountInvalidError,
        PaymentFullyRefundedError,
    ]
}

@error("client")
@httpError(409)
structure RefundNotRefundableError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(400)
structure RefundAmountInvalidError {
    @required
    error: String
    @required
    message: String
}

@error("client")
@httpError(409)
structure PaymentFullyRefundedError {
    @required
    error: String
    @required
    message: String
}

@mixin
structure RefundResponseMixin {
    @required
    refund_id: String

    @required
    original_payment_id: String

    @required
    account_id: String

    @required
    amount: Money

    @required
    amount_to_self: Money

    @required
    amount_to_others: Money

    @required
    original_amount: Money

    @required
    remaining_refundable: Money

    @required
    status: String

    @required
    correlation_id: String

    @required
    created_at: DateTime
}
