$version: "2"
namespace com.ppi.pba

/// List all available purpose types.
@readonly
@http(method: "GET", uri: "/purpose-types")
operation ListPurposeTypes {
    output := {
        @required
        purpose_types: PurposeTypeList
    }
}

/// Get a specific purpose type and its allowed MCCs.
@readonly
@http(method: "GET", uri: "/purpose-types/{purpose_code}")
operation GetPurposeType {
    input := {
        @required
        @httpLabel
        purpose_code: String
    }
    output := {
        @required
        purpose_code: String

        @required
        allowed_mccs: MccEntryList
    }
    errors: [PurposeTypeNotFoundError]
}

list PurposeTypeList {
    member: PurposeTypeSummary
}

structure PurposeTypeSummary {
    @required
    purpose_code: String

    @required
    allowed_mccs: MccEntryList
}

list MccEntryList {
    member: MccEntry
}

structure MccEntry {
    @required
    mcc: String

    description: String
}

@error("client")
@httpError(404)
structure PurposeTypeNotFoundError {
    @required
    error: String
    @required
    message: String
}
