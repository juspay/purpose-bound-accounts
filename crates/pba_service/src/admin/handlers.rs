use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::purpose::PurposeType;
use crate::domain::transaction::{TransactionRecord, TransactionStatus};
use crate::AppState;

fn render<T: Template>(tmpl: T) -> Response {
    match tmpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("Template render error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Template render error").into_response()
        }
    }
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct LoginTemplate {}

pub async fn login_page() -> impl IntoResponse {
    render(LoginTemplate {})
}

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate {
    total_accounts: i64,
    active_accounts: i64,
    frozen_accounts: i64,
    closed_accounts: i64,
    purpose_counts: Vec<(String, i64)>,
}

pub async fn dashboard(State(state): State<AppState>) -> Response {
    let status_counts = match state.account_repo.count_accounts_by_status().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to count accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let purpose_counts = match state.account_repo.count_accounts_by_purpose().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to count by purpose: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let mut active = 0i64;
    let mut frozen = 0i64;
    let mut closed = 0i64;
    for (status, count) in &status_counts {
        match status.as_str() {
            "active" => active = *count,
            "frozen" => frozen = *count,
            "closed" => closed = *count,
            _ => {}
        }
    }

    render(DashboardTemplate {
        total_accounts: active + frozen + closed,
        active_accounts: active,
        frozen_accounts: frozen,
        closed_accounts: closed,
        purpose_counts,
    })
}

#[derive(Template)]
#[template(path = "admin/accounts.html")]
struct AccountsListTemplate {
    accounts: Vec<AccountRow>,
    purpose_codes: Vec<String>,
    error: Option<String>,
    success: Option<String>,
}

struct AccountRow {
    id: String,
    holder_id: String,
    purpose_code: String,
    status: String,
    status_class: String,
    created_at: String,
}

pub async fn accounts_list(State(state): State<AppState>) -> Response {
    render_accounts_list(&state, None, None).await
}

async fn render_accounts_list(
    state: &AppState,
    error: Option<String>,
    success: Option<String>,
) -> Response {
    let accounts = match state.account_repo.list_accounts().await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to list accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let purpose_codes = match state.account_repo.list_purpose_types().await {
        Ok(pts) => pts.into_iter().map(|p| p.purpose_code).collect(),
        Err(_) => vec![],
    };

    let rows: Vec<AccountRow> = accounts
        .into_iter()
        .map(|a| {
            let status_str = a.status.as_str().to_string();
            let status_class = match a.status.as_str() {
                "active" => "status-active",
                "frozen" => "status-frozen",
                "closed" => "status-closed",
                _ => "",
            }
            .to_string();
            AccountRow {
                id: a.id.to_string(),
                holder_id: a.holder_id.to_string(),
                purpose_code: a.purpose_code,
                status: status_str,
                status_class,
                created_at: a.created_at.format("%Y-%m-%d %H:%M").to_string(),
            }
        })
        .collect();

    render(AccountsListTemplate {
        accounts: rows,
        purpose_codes,
        error,
        success,
    })
}

#[derive(Deserialize)]
pub struct CreateAccountForm {
    holder_id: String,
    purpose_code: String,
    origin_ifsc: String,
    origin_account_number: String,
}

pub async fn create_account(
    State(state): State<AppState>,
    axum::extract::Form(form): axum::extract::Form<CreateAccountForm>,
) -> Response {
    let holder_id = match form.holder_id.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => {
            return render_accounts_list(
                &state,
                Some("Invalid holder ID — must be a valid UUID".to_string()),
                None,
            )
            .await;
        }
    };

    match state
        .account_service
        .create_account(
            holder_id,
            &form.purpose_code,
            &form.origin_ifsc,
            &form.origin_account_number,
        )
        .await
    {
        Ok(account) => Redirect::to(&format!("/admin/accounts/{}", account.id)).into_response(),
        Err(e) => render_accounts_list(&state, Some(format!("{e}")), None).await,
    }
}

#[derive(Deserialize)]
pub struct UpdateStatusForm {
    status: String,
}

pub async fn update_account_status(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<UpdateStatusForm>,
) -> Response {
    let status = match crate::domain::account::AccountStatus::from_str(&form.status) {
        Some(s) => s,
        None => {
            return (StatusCode::BAD_REQUEST, "Invalid status").into_response();
        }
    };

    match state
        .account_service
        .update_status(account_id, status)
        .await
    {
        Ok(_) => Redirect::to(&format!("/admin/accounts/{account_id}")).into_response(),
        Err(e) => {
            tracing::error!("Failed to update status: {e}");
            Redirect::to(&format!("/admin/accounts/{account_id}")).into_response()
        }
    }
}

struct PendingDepositRow {
    id: String,
    amount: String,
    pool: String,
    source: String,
    gateway_ref: String,
    created_at: String,
}

#[derive(Template)]
#[template(path = "admin/account_detail.html")]
struct AccountDetailTemplate {
    id: String,
    holder_id: String,
    purpose_code: String,
    status: String,
    status_class: String,
    origin_ifsc: String,
    origin_account_number: String,
    vpa: String,
    self_balance: String,
    others_balance: String,
    total_balance: String,
    pending_self: String,
    pending_others: String,
    created_at: String,
    updated_at: String,
    pending_deposits: Vec<PendingDepositRow>,
}

pub async fn account_detail(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let account = match state.account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Account not found: {e}");
            return (StatusCode::NOT_FOUND, "Account not found").into_response();
        }
    };

    let balance = match state
        .ledger_repo
        .get_balance(account.tb_self_account_id, account.tb_others_account_id)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to get balance: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Ledger error").into_response();
        }
    };

    let fmt = |amt: u64| format!("{}.{:02}", amt / 100, amt % 100);
    let status_str = account.status.as_str().to_string();
    let status_class = match account.status.as_str() {
        "active" => "status-active",
        "frozen" => "status-frozen",
        "closed" => "status-closed",
        _ => "",
    }
    .to_string();

    let pending_deposits = match state
        .transaction_repo
        .list_pending_by_account(account_id)
        .await
    {
        Ok(deps) => deps
            .into_iter()
            .map(|d| PendingDepositRow {
                id: d.id.to_string(),
                amount: d.amount_display(),
                pool: if d.pool == "self" {
                    "Self".to_string()
                } else {
                    "Others".to_string()
                },
                source: format!(
                    "{} / {}",
                    d.source_ifsc.as_deref().unwrap_or("—"),
                    d.source_account.as_deref().unwrap_or("—")
                ),
                gateway_ref: d.gateway_ref.unwrap_or_else(|| "—".to_string()),
                created_at: d.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to list pending deposits: {e}");
            vec![]
        }
    };

    render(AccountDetailTemplate {
        id: account.id.to_string(),
        holder_id: account.holder_id.to_string(),
        purpose_code: account.purpose_code,
        status: status_str,
        status_class,
        origin_ifsc: account.origin_ifsc,
        origin_account_number: account.origin_account_number,
        vpa: account.vpa.unwrap_or_else(|| "N/A".to_string()),
        self_balance: fmt(balance.self_contribution),
        others_balance: fmt(balance.others_contribution),
        total_balance: fmt(balance.self_contribution + balance.others_contribution),
        pending_self: fmt(balance.pending_self),
        pending_others: fmt(balance.pending_others),
        created_at: account.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: account.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        pending_deposits,
    })
}

#[derive(Template)]
#[template(path = "admin/transfers_fragment.html")]
struct TransfersFragmentTemplate {
    account_id: String,
    transfers: Vec<TransferRow>,
    total: i64,
    offset: i64,
    limit: i64,
    count: i64,
    prev_offset: i64,
    next_offset: i64,
    has_next: bool,
}

struct TransferRow {
    timestamp: String,
    transfer_type: String,
    direction: String,
    direction_class: String,
    pool: String,
    amount: String,
}

#[derive(Deserialize)]
pub struct TransfersFragmentQuery {
    offset: Option<i64>,
    limit: Option<i64>,
}

pub async fn account_transfers_fragment(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<TransfersFragmentQuery>,
) -> Response {
    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    let transactions: Vec<TransactionRecord> = match state
        .transaction_repo
        .list_by_account(account_id, offset, limit, None, None)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to get transactions: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let total = match state
        .transaction_repo
        .count_by_account(account_id, None, None)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to count transactions: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let count = transactions.len() as i64;
    let rows: Vec<TransferRow> = transactions
        .into_iter()
        .map(|t| {
            let pool = if t.pool == "self" { "Self" } else { "Others" };
            TransferRow {
                timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                transfer_type: t.type_label().to_string(),
                direction: t.direction.label().to_string(),
                direction_class: t.direction.css_class().to_string(),
                pool: pool.to_string(),
                amount: t.amount_display(),
            }
        })
        .collect();

    render(TransfersFragmentTemplate {
        account_id: account_id.to_string(),
        transfers: rows,
        total,
        offset,
        limit,
        count,
        prev_offset: (offset - limit).max(0),
        next_offset: offset + limit,
        has_next: offset + count < total,
    })
}

#[derive(Template)]
#[template(path = "admin/deposit.html")]
struct DepositTemplate {
    account_id: String,
    purpose_code: String,
    error: Option<String>,
}

pub async fn deposit_form(State(state): State<AppState>, Path(account_id): Path<Uuid>) -> Response {
    let account = match state.account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(DepositTemplate {
        account_id: account_id.to_string(),
        purpose_code: account.purpose_code,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct DepositForm {
    amount: u64,
    source_ifsc: String,
    source_account_number: String,
    funding_type: Option<String>,
    #[serde(default)]
    pending: Option<String>,
    gateway_ref: Option<String>,
}

pub async fn process_deposit(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<DepositForm>,
) -> Response {
    let is_pending = form.pending.as_deref() == Some("true");
    let gateway_ref = form.gateway_ref.as_deref().filter(|s| !s.is_empty());
    let funding_type = form.funding_type.as_deref().filter(|s| !s.is_empty());
    match state
        .deposit_service
        .deposit(
            account_id,
            &form.source_ifsc,
            &form.source_account_number,
            funding_type,
            form.amount,
            is_pending,
            gateway_ref,
            None,
            None,
        )
        .await
    {
        Ok(_) => Redirect::to(&format!("/admin/accounts/{account_id}")).into_response(),
        Err(e) => {
            let purpose_code = state
                .account_repo
                .get_account(account_id)
                .await
                .map(|a| a.purpose_code)
                .unwrap_or_default();
            render(DepositTemplate {
                account_id: account_id.to_string(),
                purpose_code,
                error: Some(format!("{e}")),
            })
        }
    }
}

pub async fn post_deposit(
    State(state): State<AppState>,
    Path((account_id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Response {
    match state
        .deposit_service
        .post_deposit(account_id, deposit_id)
        .await
    {
        Ok(_) => Redirect::to(&format!("/admin/accounts/{account_id}")).into_response(),
        Err(e) => {
            tracing::error!("Failed to post deposit: {e}");
            Redirect::to(&format!("/admin/accounts/{account_id}")).into_response()
        }
    }
}

pub async fn void_deposit(
    State(state): State<AppState>,
    Path((account_id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Response {
    match state
        .deposit_service
        .void_deposit(account_id, deposit_id, None)
        .await
    {
        Ok(_) => Redirect::to(&format!("/admin/accounts/{account_id}")).into_response(),
        Err(e) => {
            tracing::error!("Failed to void deposit: {e}");
            Redirect::to(&format!("/admin/accounts/{account_id}")).into_response()
        }
    }
}

#[derive(Template)]
#[template(path = "admin/payment.html")]
struct PaymentTemplate {
    account_id: String,
    purpose_code: String,
    error: Option<String>,
}

pub async fn payment_form(State(state): State<AppState>, Path(account_id): Path<Uuid>) -> Response {
    let account = match state.account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(PaymentTemplate {
        account_id: account_id.to_string(),
        purpose_code: account.purpose_code,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct PaymentForm {
    amount: u64,
    merchant_id: String,
    merchant_mcc: String,
    description: String,
}

pub async fn process_payment(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<PaymentForm>,
) -> Response {
    match state
        .payment_service
        .make_payment(
            account_id,
            form.amount,
            &form.merchant_mcc,
            &form.merchant_id,
            &form.description,
            None,
        )
        .await
    {
        Ok(_) => Redirect::to(&format!("/admin/accounts/{account_id}")).into_response(),
        Err(e) => {
            let purpose_code = state
                .account_repo
                .get_account(account_id)
                .await
                .map(|a| a.purpose_code)
                .unwrap_or_default();
            render(PaymentTemplate {
                account_id: account_id.to_string(),
                purpose_code,
                error: Some(format!("{e}")),
            })
        }
    }
}

#[derive(Template)]
#[template(path = "admin/withdrawal.html")]
struct WithdrawalTemplate {
    account_id: String,
    purpose_code: String,
    error: Option<String>,
}

pub async fn withdrawal_form(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let account = match state.account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(WithdrawalTemplate {
        account_id: account_id.to_string(),
        purpose_code: account.purpose_code,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct WithdrawalForm {
    amount: u64,
}

pub async fn process_withdrawal(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<WithdrawalForm>,
) -> Response {
    match state
        .withdrawal_service
        .withdraw(account_id, form.amount, None)
        .await
    {
        Ok(_) => Redirect::to(&format!("/admin/accounts/{account_id}")).into_response(),
        Err(e) => {
            let purpose_code = state
                .account_repo
                .get_account(account_id)
                .await
                .map(|a| a.purpose_code)
                .unwrap_or_default();
            render(WithdrawalTemplate {
                account_id: account_id.to_string(),
                purpose_code,
                error: Some(format!("{e}")),
            })
        }
    }
}

#[derive(Template)]
#[template(path = "admin/transactions.html")]
struct TransactionsPageTemplate {
    self_balance: String,
    others_balance: String,
    total_balance: String,
    pending_self: String,
    pending_others: String,
    transactions: Vec<AllTransactionRow>,
    total: i64,
    offset: i64,
    limit: i64,
    count: i64,
    prev_offset: i64,
    next_offset: i64,
    has_next: bool,
}

struct AllTransactionRow {
    timestamp: String,
    account_id: String,
    account_id_short: String,
    transfer_type: String,
    status: String,
    status_class: String,
    pool: String,
    funding_type: String,
    direction: String,
    direction_class: String,
    amount: String,
}

#[derive(Deserialize)]
pub struct TransactionsPageQuery {
    offset: Option<i64>,
    limit: Option<i64>,
}

pub async fn transactions_page(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<TransactionsPageQuery>,
) -> Response {
    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    let pool_summary = match state.transaction_repo.pool_summary().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get pool summary: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let transactions = match state
        .transaction_repo
        .list_all(offset, limit, None, None)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to list transactions: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let total = match state.transaction_repo.count_all(None, None).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to count transactions: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let count = transactions.len() as i64;
    let rows: Vec<AllTransactionRow> = transactions
        .into_iter()
        .map(|t| {
            let pool = if t.pool == "self" { "Self" } else { "Others" };
            let status_class = match t.status {
                TransactionStatus::Pending => "status-frozen",
                TransactionStatus::Posted | TransactionStatus::Settled => "status-active",
                TransactionStatus::Voided => "status-closed",
            };
            AllTransactionRow {
                timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                account_id: t.account_id.to_string(),
                account_id_short: t.account_id.to_string()[..8].to_string(),
                transfer_type: t.type_label().to_string(),
                status: t.status.as_str().to_string(),
                status_class: status_class.to_string(),
                pool: pool.to_string(),
                funding_type: t.funding_type.as_deref().unwrap_or("—").to_string(),
                direction: t.direction.label().to_string(),
                direction_class: t.direction.css_class().to_string(),
                amount: t.amount_display(),
            }
        })
        .collect();

    let fmt = |amt: u64| format!("{}.{:02}", amt / 100, amt % 100);

    render(TransactionsPageTemplate {
        self_balance: fmt(pool_summary.self_balance()),
        others_balance: fmt(pool_summary.others_balance()),
        total_balance: fmt(pool_summary.total_balance()),
        pending_self: fmt(pool_summary.pending_self),
        pending_others: fmt(pool_summary.pending_others),
        transactions: rows,
        total,
        offset,
        limit,
        count,
        prev_offset: (offset - limit).max(0),
        next_offset: offset + limit,
        has_next: offset + count < total,
    })
}

#[derive(Template)]
#[template(path = "admin/system_accounts.html")]
struct SystemAccountsTemplate {
    sentinel_accounts: Vec<SentinelAccountRow>,
    pool_balances: Vec<PoolBalanceRow>,
}

struct SentinelAccountRow {
    name: String,
    credits_posted: String,
    debits_posted: String,
    credits_pending: String,
    debits_pending: String,
    balance_posted: String,
    balance_pending: String,
}

struct PoolBalanceRow {
    name: String,
    credits_posted: String,
    debits_posted: String,
    credits_pending: String,
    debits_pending: String,
    balance_posted: String,
    balance_pending: String,
}

pub async fn system_accounts_page(State(state): State<AppState>) -> Response {
    let fmt = |amt: u64| format!("{}.{:02}", amt / 100, amt % 100);
    let fmt_signed = |credits: u64, debits: u64| -> String {
        if credits >= debits {
            let diff = credits - debits;
            format!("{}.{:02}", diff / 100, diff % 100)
        } else {
            let diff = debits - credits;
            format!("-{}.{:02}", diff / 100, diff % 100)
        }
    };

    // Sentinel accounts from TigerBeetle
    let sentinel_accounts = match state.ledger_repo.lookup_sentinel_accounts().await {
        Ok(accounts) => accounts
            .into_iter()
            .map(|(name, cp, dp, cpend, dpend)| SentinelAccountRow {
                name,
                credits_posted: fmt(cp),
                debits_posted: fmt(dp),
                credits_pending: fmt(cpend),
                debits_pending: fmt(dpend),
                balance_posted: fmt_signed(cp, dp),
                balance_pending: fmt_signed(cpend, dpend),
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to lookup sentinel accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "TigerBeetle error").into_response();
        }
    };

    // PBA pool balances from Postgres
    let pool_summary = match state.transaction_repo.pool_summary_extended().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get pool summary: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let pool_balances = vec![
        PoolBalanceRow {
            name: "Self Pool (all accounts)".to_string(),
            credits_posted: fmt(pool_summary.self_inbound),
            debits_posted: fmt(pool_summary.self_outbound),
            credits_pending: fmt(pool_summary.pending_self_inbound),
            debits_pending: fmt(pool_summary.pending_self_outbound),
            balance_posted: fmt_signed(pool_summary.self_inbound, pool_summary.self_outbound),
            balance_pending: fmt_signed(
                pool_summary.pending_self_inbound,
                pool_summary.pending_self_outbound,
            ),
        },
        PoolBalanceRow {
            name: "Others Pool (all accounts)".to_string(),
            credits_posted: fmt(pool_summary.others_inbound),
            debits_posted: fmt(pool_summary.others_outbound),
            credits_pending: fmt(pool_summary.pending_others_inbound),
            debits_pending: fmt(pool_summary.pending_others_outbound),
            balance_posted: fmt_signed(pool_summary.others_inbound, pool_summary.others_outbound),
            balance_pending: fmt_signed(
                pool_summary.pending_others_inbound,
                pool_summary.pending_others_outbound,
            ),
        },
    ];

    render(SystemAccountsTemplate {
        sentinel_accounts,
        pool_balances,
    })
}

#[derive(Template)]
#[template(path = "admin/purpose_types.html")]
struct PurposeTypesTemplate {
    purpose_types: Vec<PurposeType>,
}

pub async fn purpose_types_page(State(state): State<AppState>) -> Response {
    let purpose_types = match state.account_repo.list_purpose_types().await {
        Ok(pts) => pts,
        Err(e) => {
            tracing::error!("Failed to list purpose types: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };
    render(PurposeTypesTemplate { purpose_types })
}
