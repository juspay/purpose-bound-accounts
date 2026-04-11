$version: "2"
namespace com.ppi.pba

/// List all available purpose types.
@readonly
@http(method: "GET", uri: "/purpose-types")
operation ListPurposeTypes {
    output := {
        @required
        purposeTypes: PurposeTypeList
    }
}

/// Get a specific purpose type and its allowed MCCs.
@readonly
@http(method: "GET", uri: "/purpose-types/{purposeCode}")
operation GetPurposeType {
    input := {
        @required
        @httpLabel
        purposeCode: String
    }
    output := {
        @required
        purposeCode: String

        @required
        allowedMccs: MccEntryList
    }
    errors: [PurposeTypeNotFoundError]
}

list PurposeTypeList {
    member: PurposeTypeSummary
}

structure PurposeTypeSummary {
    @required
    purposeCode: String

    @required
    allowedMccs: MccEntryList
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
