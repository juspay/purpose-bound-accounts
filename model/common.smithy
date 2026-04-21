$version: "2"
namespace com.ppi.pba

/// Monetary amount in the smallest currency unit (e.g., paise for INR).
long Money

/// ISO 8601 date-time.
@timestampFormat("date-time")
timestamp DateTime

/// Transaction type.
@enum([
    { value: "deposit", name: "DEPOSIT" },
    { value: "payment", name: "PAYMENT" },
    { value: "withdrawal", name: "WITHDRAWAL" },
])
string TransactionType

/// Transaction status.
@enum([
    { value: "pending", name: "PENDING" },
    { value: "posted", name: "POSTED" },
    { value: "voided", name: "VOIDED" },
    { value: "settled", name: "SETTLED" },
])
string TransactionStatus

/// Pool type indicating the source of funds.
@enum([
    { value: "self", name: "SELF_POOL" },
    { value: "others", name: "OTHERS_POOL" },
])
string PoolType

/// Transaction direction.
@enum([
    { value: "inbound", name: "INBOUND" },
    { value: "outbound", name: "OUTBOUND" },
])
string TransactionDirection

/// Account status.
enum Status {
    ACTIVE
    FROZEN
    CLOSED
}

/// KYC tier level.
enum KycTier {
    MINIMUM
    FULL
}

/// Standard error structure.
structure ErrorResponse {
    @required
    error: String
    @required
    message: String
}
