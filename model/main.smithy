$version: "2"
namespace com.ppi.pba

use aws.protocols#restJson1
@httpApiKeyAuth(name: "Authorization", in: "header", scheme: "ApiKey")
@restJson1
service PurposeBoundAccountService {
    version: "2026-04-14"
    operations: [
        // Canonical PB account operations
        CreatePBAccount
        GetPBAccount
        GetPBAccountBalance
        UpdatePBAccountStatus
        DepositToPBAccount
        PostPBAccountDeposit
        VoidPBAccountDeposit
        MakePBAccountPayment
        WithdrawFromPBAccount
        ListPBAccountTransactions

        // Deprecated PB account aliases (legacy /accounts/... URLs)
        CreateAccount
        GetAccount
        GetBalance
        UpdateAccountStatus
        Deposit
        PostDeposit
        VoidDeposit
        MakePayment
        Withdraw
        ListTransactions

        // Normal account operations
        CreateNormalAccount
        GetNormalAccount
        ListNormalAccounts
        UpdateNormalAccountStatus
        GetNormalAccountBalance
        DepositToNormalAccount
        PostNormalAccountDeposit
        VoidNormalAccountDeposit
        WithdrawFromNormalAccount
        ListNormalAccountTransactions
        TransferToPBAccount
        PostNormalAccountTransfer
        VoidNormalAccountTransfer
        ReverseNormalAccountTransfer

        // Unchanged operations
        ListPurposeTypes
        GetPurposeType
        ListAllTransactions
    ]
}
