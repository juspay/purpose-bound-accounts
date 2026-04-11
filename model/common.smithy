$version: "2"
namespace com.ppi.pba

/// Monetary amount in the smallest currency unit (e.g., paise for INR).
long Money

/// Pool type indicating the source of funds.
enum PoolType {
    SELF_CONTRIBUTION
    OTHERS_CONTRIBUTION
}

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
