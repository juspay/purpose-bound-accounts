$version: "2"
namespace com.ppi.pba

use aws.protocols#restJson1

@restJson1
service PurposeBoundAccountService {
    version: "2026-04-11"
    operations: [
        CreateAccount
        GetAccount
        GetBalance
        Deposit
        MakePayment
        Withdraw
        UpdateAccountStatus
        ListPurposeTypes
        GetPurposeType
    ]
}
