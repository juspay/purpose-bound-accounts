use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::banking::{AccountNumber, Ifsc};
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

fn prefixed(state: &AppState, path: &str) -> String {
    format!("{}{path}", state.path_prefix)
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct LoginTemplate {
    prefix: String,
}

pub async fn login_page(State(state): State<AppState>) -> impl IntoResponse {
    render(LoginTemplate {
        prefix: state.path_prefix.clone(),
    })
}

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate {
    prefix: String,
    total_accounts: i64,
    active_accounts: i64,
    frozen_accounts: i64,
    closed_accounts: i64,
    purpose_counts: Vec<(String, i64)>,
}

pub async fn dashboard(State(state): State<AppState>) -> Response {
    let status_counts = match state.pb_account_repo.count_accounts_by_status().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to count accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let purpose_counts = match state.pb_account_repo.count_accounts_by_purpose().await {
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
        prefix: state.path_prefix.clone(),
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
    prefix: String,
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
    let accounts = match state.pb_account_repo.list_accounts().await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to list accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let purpose_codes = match state.pb_account_repo.list_purpose_types().await {
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
                holder_id: a.holder_id,
                purpose_code: a.purpose_code,
                status: status_str,
                status_class,
                created_at: a.created_at.format("%Y-%m-%d %H:%M").to_string(),
            }
        })
        .collect();

    render(AccountsListTemplate {
        prefix: state.path_prefix.clone(),
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
    let holder_id = form.holder_id.trim();
    if holder_id.is_empty() {
        return render_accounts_list(&state, Some("Holder ID is required".to_string()), None).await;
    }
    if holder_id.len() > 255 {
        return render_accounts_list(
            &state,
            Some("Holder ID must be at most 255 characters".to_string()),
            None,
        )
        .await;
    }

    let origin_ifsc = match Ifsc::parse(&form.origin_ifsc) {
        Ok(v) => v,
        Err(e) => return render_accounts_list(&state, Some(e.to_string()), None).await,
    };
    let origin_account_number = match AccountNumber::parse(&form.origin_account_number) {
        Ok(v) => v,
        Err(e) => return render_accounts_list(&state, Some(e.to_string()), None).await,
    };

    match state
        .pb_account_service
        .create_account(
            holder_id,
            &form.purpose_code,
            &origin_ifsc,
            &origin_account_number,
        )
        .await
    {
        Ok(account) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/accounts/{}", account.id),
        ))
        .into_response(),
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
        .pb_account_service
        .update_status(account_id, status)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to update status: {e}");
            Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
                .into_response()
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
    prefix: String,
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
    let account = match state.pb_account_repo.get_account(account_id).await {
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
        prefix: state.path_prefix.clone(),
        id: account.id.to_string(),
        holder_id: account.holder_id,
        purpose_code: account.purpose_code,
        status: status_str,
        status_class,
        origin_ifsc: account.origin_ifsc.to_string(),
        origin_account_number: account.origin_account_number.to_string(),
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
    prefix: String,
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
    id: String,
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
                id: t.id.to_string(),
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
        prefix: state.path_prefix.clone(),
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
    prefix: String,
    account_id: String,
    purpose_code: String,
    error: Option<String>,
}

pub async fn deposit_form(State(state): State<AppState>, Path(account_id): Path<Uuid>) -> Response {
    let account = match state.pb_account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(DepositTemplate {
        prefix: state.path_prefix.clone(),
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
        .pb_deposit_service
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
        Ok(_) => Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
            .into_response(),
        Err(e) => {
            let purpose_code = state
                .pb_account_repo
                .get_account(account_id)
                .await
                .map(|a| a.purpose_code)
                .unwrap_or_default();
            render(DepositTemplate {
                prefix: state.path_prefix.clone(),
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
        .pb_deposit_service
        .post_deposit(account_id, deposit_id)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to post deposit: {e}");
            Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
                .into_response()
        }
    }
}

pub async fn void_deposit(
    State(state): State<AppState>,
    Path((account_id, deposit_id)): Path<(Uuid, Uuid)>,
) -> Response {
    match state
        .pb_deposit_service
        .void_deposit(account_id, deposit_id, None)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to void deposit: {e}");
            Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
                .into_response()
        }
    }
}

#[derive(Template)]
#[template(path = "admin/payment.html")]
struct PaymentTemplate {
    prefix: String,
    account_id: String,
    purpose_code: String,
    error: Option<String>,
}

pub async fn payment_form(State(state): State<AppState>, Path(account_id): Path<Uuid>) -> Response {
    let account = match state.pb_account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(PaymentTemplate {
        prefix: state.path_prefix.clone(),
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
    gateway_ref: Option<String>,
}

pub async fn process_payment(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<PaymentForm>,
) -> Response {
    let gateway_ref = form.gateway_ref.as_deref().filter(|s| !s.is_empty());
    match state
        .pb_payment_service
        .make_payment(
            account_id,
            form.amount,
            &form.merchant_mcc,
            &form.merchant_id,
            &form.description,
            None,
            gateway_ref,
        )
        .await
    {
        Ok(_) => Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
            .into_response(),
        Err(e) => {
            let purpose_code = state
                .pb_account_repo
                .get_account(account_id)
                .await
                .map(|a| a.purpose_code)
                .unwrap_or_default();
            render(PaymentTemplate {
                prefix: state.path_prefix.clone(),
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
    prefix: String,
    account_id: String,
    purpose_code: String,
    error: Option<String>,
}

pub async fn withdrawal_form(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let account = match state.pb_account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(WithdrawalTemplate {
        prefix: state.path_prefix.clone(),
        account_id: account_id.to_string(),
        purpose_code: account.purpose_code,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct WithdrawalForm {
    amount: u64,
    gateway_ref: Option<String>,
}

pub async fn process_withdrawal(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<WithdrawalForm>,
) -> Response {
    let gateway_ref = form.gateway_ref.as_deref().filter(|s| !s.is_empty());
    match state
        .pb_withdrawal_service
        .withdraw(account_id, form.amount, None, gateway_ref)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(&state, &format!("/admin/accounts/{account_id}")))
            .into_response(),
        Err(e) => {
            let purpose_code = state
                .pb_account_repo
                .get_account(account_id)
                .await
                .map(|a| a.purpose_code)
                .unwrap_or_default();
            render(WithdrawalTemplate {
                prefix: state.path_prefix.clone(),
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
    prefix: String,
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
    id: String,
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
                id: t.id.to_string(),
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
        prefix: state.path_prefix.clone(),
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
    prefix: String,
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
        prefix: state.path_prefix.clone(),
        sentinel_accounts,
        pool_balances,
    })
}

#[derive(Template)]
#[template(path = "admin/purpose_types.html")]
struct PurposeTypesTemplate {
    prefix: String,
    purpose_types: Vec<PurposeType>,
}

pub async fn purpose_types_page(State(state): State<AppState>) -> Response {
    let purpose_types = match state.pb_account_repo.list_purpose_types().await {
        Ok(pts) => pts,
        Err(e) => {
            tracing::error!("Failed to list purpose types: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };
    render(PurposeTypesTemplate {
        prefix: state.path_prefix.clone(),
        purpose_types,
    })
}

#[derive(Template)]
#[template(path = "admin/transaction_detail.html")]
struct TransactionDetailTemplate {
    prefix: String,
    id: String,
    id_short: String,
    account_id: String,
    holder_id: String,
    purpose_code: String,
    tb_transfer_id: String,
    idempotency_key: String,
    transaction_type_label: String,
    status: String,
    status_class: String,
    direction: String,
    direction_class: String,
    pool: String,
    funding_type: String,
    amount: String,
    source_ifsc: String,
    source_account: String,
    gateway_ref: String,
    merchant_id: String,
    merchant_mcc: String,
    description: String,
    created_at: String,
    updated_at: String,
    timeout_seconds: String,
    is_deposit: bool,
    is_payment: bool,
    is_withdrawal: bool,
    can_post_or_void: bool,
}

pub async fn transaction_detail(
    State(state): State<AppState>,
    Path(transaction_id): Path<Uuid>,
) -> Response {
    let txn = match state.transaction_repo.get_transaction(transaction_id).await {
        Ok(t) => t,
        Err(crate::error::AppError::TransactionNotFound(_)) => {
            return (StatusCode::NOT_FOUND, "Transaction not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to get transaction: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let dash = "—".to_string();

    let (holder_id, purpose_code) = match state.pb_account_repo.get_account(txn.account_id).await {
        Ok(a) => (a.holder_id, a.purpose_code),
        Err(e) => {
            tracing::warn!("Failed to load parent account for transaction {transaction_id}: {e}");
            (dash.clone(), dash.clone())
        }
    };

    let id_str = txn.id.to_string();
    let id_short = id_str.chars().take(8).collect::<String>();

    let status_class = match txn.status {
        TransactionStatus::Pending => "status-frozen",
        TransactionStatus::Posted | TransactionStatus::Settled => "status-active",
        TransactionStatus::Voided => "status-closed",
    }
    .to_string();

    let pool = if txn.pool == "self" {
        "Self".to_string()
    } else {
        "Others".to_string()
    };

    let can_post_or_void = matches!(
        txn.transaction_type,
        crate::domain::transaction::TransactionType::Deposit
    ) && matches!(txn.status, TransactionStatus::Pending);

    // Collect all non-ownership-taking fields first
    let transaction_type_label = txn.type_label().to_string();
    let status = txn.status.as_str().to_string();
    let direction = txn.direction.label().to_string();
    let direction_class = txn.direction.css_class().to_string();
    let amount = txn.amount_display();
    let created_at = txn.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
    let updated_at = txn.updated_at.format("%Y-%m-%d %H:%M:%S").to_string();
    let is_deposit = matches!(
        txn.transaction_type,
        crate::domain::transaction::TransactionType::Deposit
    );
    let is_payment = matches!(
        txn.transaction_type,
        crate::domain::transaction::TransactionType::Payment
    );
    let is_withdrawal = matches!(
        txn.transaction_type,
        crate::domain::transaction::TransactionType::Withdrawal
    );

    // Now do the ownership-taking operations
    let account_id = txn.account_id.to_string();
    let tb_transfer_id = txn.tb_transfer_id.to_string();
    let idempotency_key = txn.idempotency_key.unwrap_or_else(|| dash.clone());
    let funding_type = txn.funding_type.unwrap_or_else(|| dash.clone());
    let source_ifsc = txn.source_ifsc.unwrap_or_else(|| dash.clone());
    let source_account = txn.source_account.unwrap_or_else(|| dash.clone());
    let gateway_ref = txn.gateway_ref.unwrap_or_else(|| dash.clone());
    let merchant_id = txn.merchant_id.unwrap_or_else(|| dash.clone());
    let merchant_mcc = txn.merchant_mcc.unwrap_or_else(|| dash.clone());
    let description = txn.description.unwrap_or_else(|| dash.clone());
    let timeout_seconds = txn
        .timeout_seconds
        .map(|s| s.to_string())
        .unwrap_or_else(|| dash.clone());

    render(TransactionDetailTemplate {
        prefix: state.path_prefix.clone(),
        id: id_str,
        id_short,
        account_id,
        holder_id,
        purpose_code,
        tb_transfer_id,
        idempotency_key,
        transaction_type_label,
        status,
        status_class,
        direction,
        direction_class,
        pool,
        funding_type,
        amount,
        source_ifsc,
        source_account,
        gateway_ref,
        merchant_id,
        merchant_mcc,
        description,
        created_at,
        updated_at,
        timeout_seconds,
        is_deposit,
        is_payment,
        is_withdrawal,
        can_post_or_void,
    })
}

pub async fn post_transaction(
    State(state): State<AppState>,
    Path(transaction_id): Path<Uuid>,
) -> Response {
    let txn = match state.transaction_repo.get_transaction(transaction_id).await {
        Ok(t) => t,
        Err(_) => return (StatusCode::NOT_FOUND, "Transaction not found").into_response(),
    };
    if let Err(e) = state
        .pb_deposit_service
        .post_deposit(txn.account_id, transaction_id)
        .await
    {
        tracing::error!("Failed to post deposit from detail page: {e}");
    }
    Redirect::to(&prefixed(
        &state,
        &format!("/admin/transactions/{transaction_id}"),
    ))
    .into_response()
}

pub async fn void_transaction(
    State(state): State<AppState>,
    Path(transaction_id): Path<Uuid>,
) -> Response {
    let txn = match state.transaction_repo.get_transaction(transaction_id).await {
        Ok(t) => t,
        Err(_) => return (StatusCode::NOT_FOUND, "Transaction not found").into_response(),
    };
    if let Err(e) = state
        .pb_deposit_service
        .void_deposit(txn.account_id, transaction_id, None)
        .await
    {
        tracing::error!("Failed to void deposit from detail page: {e}");
    }
    Redirect::to(&prefixed(
        &state,
        &format!("/admin/transactions/{transaction_id}"),
    ))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;

    fn deposit_pending_fixture() -> TransactionDetailTemplate {
        TransactionDetailTemplate {
            prefix: "".to_string(),
            id: "11111111-1111-1111-1111-111111111111".to_string(),
            id_short: "11111111".to_string(),
            account_id: "22222222-2222-2222-2222-222222222222".to_string(),
            holder_id: "holder-xyz".to_string(),
            purpose_code: "health".to_string(),
            tb_transfer_id: "9999999999".to_string(),
            idempotency_key: "idem-123".to_string(),
            transaction_type_label: "Deposit (Pending)".to_string(),
            status: "pending".to_string(),
            status_class: "status-frozen".to_string(),
            direction: "Inbound".to_string(),
            direction_class: "inbound".to_string(),
            pool: "Self".to_string(),
            funding_type: "origin".to_string(),
            amount: "50.00".to_string(),
            source_ifsc: "HDFC0001234".to_string(),
            source_account: "1234567890".to_string(),
            gateway_ref: "gw-ref-77".to_string(),
            merchant_id: "—".to_string(),
            merchant_mcc: "—".to_string(),
            description: "—".to_string(),
            created_at: "2026-04-30 12:00:00".to_string(),
            updated_at: "2026-04-30 12:00:00".to_string(),
            timeout_seconds: "—".to_string(),
            is_deposit: true,
            is_payment: false,
            is_withdrawal: false,
            can_post_or_void: true,
        }
    }

    fn payment_posted_fixture() -> TransactionDetailTemplate {
        TransactionDetailTemplate {
            prefix: "".to_string(),
            id: "33333333-3333-3333-3333-333333333333".to_string(),
            id_short: "33333333".to_string(),
            account_id: "22222222-2222-2222-2222-222222222222".to_string(),
            holder_id: "holder-xyz".to_string(),
            purpose_code: "health".to_string(),
            tb_transfer_id: "8888888888".to_string(),
            idempotency_key: "—".to_string(),
            transaction_type_label: "Payment".to_string(),
            status: "posted".to_string(),
            status_class: "status-active".to_string(),
            direction: "Outbound".to_string(),
            direction_class: "outbound".to_string(),
            pool: "Others".to_string(),
            funding_type: "—".to_string(),
            amount: "12.34".to_string(),
            source_ifsc: "—".to_string(),
            source_account: "—".to_string(),
            gateway_ref: "gw-pay-123".to_string(),
            merchant_id: "MERCH-1".to_string(),
            merchant_mcc: "8011".to_string(),
            description: "Doctor visit".to_string(),
            created_at: "2026-04-30 12:00:00".to_string(),
            updated_at: "2026-04-30 12:00:00".to_string(),
            timeout_seconds: "—".to_string(),
            is_deposit: false,
            is_payment: true,
            is_withdrawal: false,
            can_post_or_void: false,
        }
    }

    fn withdrawal_posted_fixture() -> TransactionDetailTemplate {
        TransactionDetailTemplate {
            prefix: "".to_string(),
            id: "44444444-4444-4444-4444-444444444444".to_string(),
            id_short: "44444444".to_string(),
            account_id: "22222222-2222-2222-2222-222222222222".to_string(),
            holder_id: "holder-xyz".to_string(),
            purpose_code: "health".to_string(),
            tb_transfer_id: "7777777777".to_string(),
            idempotency_key: "—".to_string(),
            transaction_type_label: "Withdrawal".to_string(),
            status: "settled".to_string(),
            status_class: "status-active".to_string(),
            direction: "Outbound".to_string(),
            direction_class: "outbound".to_string(),
            pool: "Self".to_string(),
            funding_type: "—".to_string(),
            amount: "20.00".to_string(),
            source_ifsc: "—".to_string(),
            source_account: "—".to_string(),
            gateway_ref: "gw-wd-456".to_string(),
            merchant_id: "—".to_string(),
            merchant_mcc: "—".to_string(),
            description: "—".to_string(),
            created_at: "2026-04-30 12:00:00".to_string(),
            updated_at: "2026-04-30 12:00:00".to_string(),
            timeout_seconds: "—".to_string(),
            is_deposit: false,
            is_payment: false,
            is_withdrawal: true,
            can_post_or_void: false,
        }
    }

    fn withdrawal_no_gateway_ref_fixture() -> TransactionDetailTemplate {
        let mut f = withdrawal_posted_fixture();
        f.gateway_ref = "—".to_string();
        f
    }

    #[test]
    fn renders_all_deposit_fields() {
        let html = deposit_pending_fixture().render().expect("render");
        for needle in [
            "11111111-1111-1111-1111-111111111111",
            "22222222-2222-2222-2222-222222222222",
            "holder-xyz",
            "health",
            "9999999999",
            "idem-123",
            "Deposit (Pending)",
            "pending",
            "Inbound",
            "Self",
            "origin",
            "50.00",
            "HDFC0001234",
            "1234567890",
            "gw-ref-77",
        ] {
            assert!(
                html.contains(needle),
                "expected `{}` in rendered HTML, got:\n{}",
                needle,
                html
            );
        }
    }

    #[test]
    fn shows_post_and_void_when_pending_deposit() {
        let html = deposit_pending_fixture().render().expect("render");
        assert!(
            html.contains("/admin/transactions/11111111-1111-1111-1111-111111111111/post"),
            "expected Post form action in HTML:\n{}",
            html
        );
        assert!(
            html.contains("/admin/transactions/11111111-1111-1111-1111-111111111111/void"),
            "expected Void form action in HTML:\n{}",
            html
        );
    }

    #[test]
    fn hides_actions_when_not_pending() {
        let html = payment_posted_fixture().render().expect("render");
        assert!(
            !html.contains("/post"),
            "did not expect Post form when can_post_or_void is false:\n{}",
            html
        );
        assert!(
            !html.contains("/void"),
            "did not expect Void form when can_post_or_void is false:\n{}",
            html
        );
    }

    #[test]
    fn renders_merchant_section_for_payment() {
        let html = payment_posted_fixture().render().expect("render");
        assert!(html.contains("MERCH-1"), "merchant_id missing: {}", html);
        assert!(html.contains("8011"), "merchant_mcc missing: {}", html);
        assert!(
            html.contains("Doctor visit"),
            "description missing: {}",
            html
        );
        assert!(
            html.contains("gw-pay-123"),
            "gateway_ref missing on payment detail: {}",
            html
        );
        assert!(
            html.contains("Gateway Ref:"),
            "Gateway Ref label missing on payment detail: {}",
            html
        );
        // For a payment, source IFSC value should not appear (we render "—"
        // for the absent source fields, so check the original value isn't there).
        assert!(
            !html.contains("HDFC0001234"),
            "payment should not show deposit-only source IFSC: {}",
            html
        );
    }

    #[test]
    fn renders_reference_section_for_withdrawal() {
        let html = withdrawal_posted_fixture().render().expect("render");
        assert!(
            html.contains("<strong>Reference</strong>"),
            "Reference card header missing on withdrawal detail: {}",
            html
        );
        assert!(
            html.contains("gw-wd-456"),
            "gateway_ref missing on withdrawal detail: {}",
            html
        );
        assert!(
            html.contains("Gateway Ref:"),
            "Gateway Ref label missing on withdrawal detail: {}",
            html
        );
        assert!(
            !html.contains("Withdrawals have no external source"),
            "old placeholder still present on withdrawal detail: {}",
            html
        );
    }

    #[test]
    fn renders_dash_for_absent_gateway_ref_on_withdrawal() {
        let html = withdrawal_no_gateway_ref_fixture()
            .render()
            .expect("render");
        let marker = "Gateway Ref:</strong>";
        let idx = html
            .find(marker)
            .expect("Gateway Ref label not found on withdrawal detail");
        let after = &html[idx + marker.len()..];
        let snippet = &after[..after.len().min(40)];
        assert!(
            snippet.contains("—"),
            "expected `—` after Gateway Ref label, got: {}",
            snippet
        );
    }
}
