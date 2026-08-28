use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::account::AccountStatus;
use crate::domain::account_kind::AccountKind;
use crate::domain::banking::{AccountNumber, Ifsc};
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

// ---------------------------------------------------------------------------
// List page
// ---------------------------------------------------------------------------

struct NormalAccountRow {
    id: String,
    holder_id: String,
    status: String,
    status_class: String,
    kyc_tier: String,
    created_at: String,
}

#[derive(Template)]
#[template(path = "admin/normal_accounts.html")]
struct NormalAccountsListTemplate {
    prefix: String,
    accounts: Vec<NormalAccountRow>,
    error: Option<String>,
    success: Option<String>,
}

pub async fn normal_accounts_list(State(state): State<AppState>) -> Response {
    render_normal_accounts_list(&state, None, None).await
}

async fn render_normal_accounts_list(
    state: &AppState,
    error: Option<String>,
    success: Option<String>,
) -> Response {
    let accounts = match state.normal_account_repo.list_accounts().await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to list normal accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let rows: Vec<NormalAccountRow> = accounts
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
            NormalAccountRow {
                id: a.id.to_string(),
                holder_id: a.holder_id,
                status: status_str,
                status_class,
                kyc_tier: a.kyc_tier,
                created_at: a.created_at.format("%Y-%m-%d %H:%M").to_string(),
            }
        })
        .collect();

    render(NormalAccountsListTemplate {
        prefix: state.path_prefix.clone(),
        accounts: rows,
        error,
        success,
    })
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateNormalAccountForm {
    holder_id: String,
    origin_ifsc: Option<String>,
    origin_account_number: Option<String>,
}

pub async fn create_normal_account(
    State(state): State<AppState>,
    axum::extract::Form(form): axum::extract::Form<CreateNormalAccountForm>,
) -> Response {
    let holder_id = form.holder_id.trim();
    if holder_id.is_empty() {
        return render_normal_accounts_list(
            &state,
            Some("Holder ID is required".to_string()),
            None,
        )
        .await;
    }
    if holder_id.len() > 255 {
        return render_normal_accounts_list(
            &state,
            Some("Holder ID must be at most 255 characters".to_string()),
            None,
        )
        .await;
    }

    let origin_ifsc_str = form.origin_ifsc.as_deref().unwrap_or("").trim().to_string();
    let origin_account_str = form
        .origin_account_number
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();

    let origin_ifsc = if origin_ifsc_str.is_empty() {
        None
    } else {
        match Ifsc::parse(&origin_ifsc_str) {
            Ok(v) => Some(v),
            Err(e) => {
                return render_normal_accounts_list(&state, Some(e.to_string()), None).await;
            }
        }
    };

    let origin_account_number = if origin_account_str.is_empty() {
        None
    } else {
        match AccountNumber::parse(&origin_account_str) {
            Ok(v) => Some(v),
            Err(e) => {
                return render_normal_accounts_list(&state, Some(e.to_string()), None).await;
            }
        }
    };

    match state
        .normal_account_service
        .create_account(
            holder_id,
            origin_ifsc.as_ref(),
            origin_account_number.as_ref(),
        )
        .await
    {
        Ok(account) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/normal-accounts/{}", account.id),
        ))
        .into_response(),
        Err(e) => render_normal_accounts_list(&state, Some(format!("{e}")), None).await,
    }
}

// ---------------------------------------------------------------------------
// Detail page
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/normal_account_detail.html")]
struct NormalAccountDetailTemplate {
    prefix: String,
    id: String,
    holder_id: String,
    kyc_tier: String,
    status: String,
    status_class: String,
    origin_ifsc: Option<String>,
    origin_account_number: Option<String>,
    balance: String,
    pending: String,
    created_at: String,
    updated_at: String,
}

pub async fn normal_account_detail(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let account = match state.normal_account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Normal account not found: {e}");
            return (StatusCode::NOT_FOUND, "Account not found").into_response();
        }
    };

    let balance = match state
        .ledger_repo
        .get_single_balance(account.tb_account_id)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to get normal account balance: {e}");
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

    render(NormalAccountDetailTemplate {
        prefix: state.path_prefix.clone(),
        id: account.id.to_string(),
        holder_id: account.holder_id,
        kyc_tier: account.kyc_tier,
        status: status_str,
        status_class,
        origin_ifsc: account.origin_ifsc.map(|v| v.to_string()),
        origin_account_number: account.origin_account_number.map(|v| v.to_string()),
        balance: fmt(balance.posted),
        pending: fmt(balance.pending),
        created_at: account.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: account.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

// ---------------------------------------------------------------------------
// Freeze / reactivate
// ---------------------------------------------------------------------------

pub async fn freeze_normal_account(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    match state
        .normal_account_service
        .update_status(account_id, AccountStatus::Frozen)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/normal-accounts/{account_id}"),
        ))
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to freeze normal account: {e}");
            Redirect::to(&prefixed(
                &state,
                &format!("/admin/normal-accounts/{account_id}"),
            ))
            .into_response()
        }
    }
}

pub async fn reactivate_normal_account(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    match state
        .normal_account_service
        .update_status(account_id, AccountStatus::Active)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/normal-accounts/{account_id}"),
        ))
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to reactivate normal account: {e}");
            Redirect::to(&prefixed(
                &state,
                &format!("/admin/normal-accounts/{account_id}"),
            ))
            .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Deposit form
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/normal_deposit.html")]
struct NormalDepositTemplate {
    prefix: String,
    account_id: String,
    holder_id: String,
    error: Option<String>,
}

pub async fn normal_deposit_form(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let account = match state.normal_account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(NormalDepositTemplate {
        prefix: state.path_prefix.clone(),
        account_id: account_id.to_string(),
        holder_id: account.holder_id,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct NormalDepositForm {
    amount: u64,
    #[serde(default)]
    pending: Option<String>,
    gateway_ref: Option<String>,
    #[serde(default)]
    description: Option<String>,
    /// Submitted as a text input; empty string → None
    #[serde(default)]
    timeout_seconds: Option<String>,
}

pub async fn process_normal_deposit(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<NormalDepositForm>,
) -> Response {
    let is_pending = form.pending.as_deref() == Some("true");
    let gateway_ref = form.gateway_ref.as_deref().filter(|s| !s.is_empty());
    let timeout_seconds: Option<u32> = form
        .timeout_seconds
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok());

    let description = form.description.as_deref().filter(|s| !s.is_empty());
    match state
        .normal_deposit_service
        .deposit(
            account_id,
            form.amount,
            is_pending,
            gateway_ref,
            timeout_seconds,
            description,
            None,
        )
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/normal-accounts/{account_id}"),
        ))
        .into_response(),
        Err(e) => {
            let holder_id = state
                .normal_account_repo
                .get_account(account_id)
                .await
                .map(|a| a.holder_id)
                .unwrap_or_default();
            render(NormalDepositTemplate {
                prefix: state.path_prefix.clone(),
                account_id: account_id.to_string(),
                holder_id,
                error: Some(format!("{e}")),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Withdrawal form
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/normal_withdrawal.html")]
struct NormalWithdrawalTemplate {
    prefix: String,
    account_id: String,
    holder_id: String,
    error: Option<String>,
}

pub async fn normal_withdrawal_form(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let account = match state.normal_account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };
    render(NormalWithdrawalTemplate {
        prefix: state.path_prefix.clone(),
        account_id: account_id.to_string(),
        holder_id: account.holder_id,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct NormalWithdrawalForm {
    amount: u64,
    gateway_ref: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

pub async fn process_normal_withdrawal(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<NormalWithdrawalForm>,
) -> Response {
    let gateway_ref = form.gateway_ref.as_deref().filter(|s| !s.is_empty());
    let description = form.description.as_deref().filter(|s| !s.is_empty());
    match state
        .normal_withdrawal_service
        .withdraw(account_id, form.amount, None, gateway_ref, description)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/normal-accounts/{account_id}"),
        ))
        .into_response(),
        Err(e) => {
            let holder_id = state
                .normal_account_repo
                .get_account(account_id)
                .await
                .map(|a| a.holder_id)
                .unwrap_or_default();
            render(NormalWithdrawalTemplate {
                prefix: state.path_prefix.clone(),
                account_id: account_id.to_string(),
                holder_id,
                error: Some(format!("{e}")),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Normal account transfers fragment (HTMX lazy-load)
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/normal_transfers_fragment.html")]
struct NormalTransfersFragmentTemplate {
    prefix: String,
    account_id: String,
    transfers: Vec<NormalTransferRow>,
    total: i64,
    offset: i64,
    limit: i64,
    count: i64,
    prev_offset: i64,
    next_offset: i64,
    has_next: bool,
}

struct NormalTransferRow {
    id: String,
    timestamp: String,
    transfer_type: String,
    status: String,
    status_class: String,
    direction: String,
    direction_class: String,
    amount: String,
    gateway_ref: String,
}

#[derive(Deserialize)]
pub struct NormalTransfersFragmentQuery {
    offset: Option<i64>,
    limit: Option<i64>,
}

pub async fn normal_transfers_fragment(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Query(query): Query<NormalTransfersFragmentQuery>,
) -> Response {
    let offset = query.offset.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    let transactions: Vec<TransactionRecord> = match state
        .transaction_repo
        .list_by_account(AccountKind::Normal, account_id, offset, limit, None, None)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to get normal account transactions: {e}");
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
            tracing::error!("Failed to count normal account transactions: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let count = transactions.len() as i64;
    let rows: Vec<NormalTransferRow> = transactions
        .into_iter()
        .map(|t| {
            let status_class = match t.status {
                TransactionStatus::Pending => "status-frozen",
                TransactionStatus::Posted | TransactionStatus::Settled => "status-active",
                TransactionStatus::Voided => "status-closed",
            };
            NormalTransferRow {
                id: t.id.to_string(),
                timestamp: t.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
                transfer_type: t.type_label().to_string(),
                status: t.status.as_str().to_string(),
                status_class: status_class.to_string(),
                direction: t.direction.label().to_string(),
                direction_class: t.direction.css_class().to_string(),
                amount: t.amount_display(),
                gateway_ref: t.gateway_ref.unwrap_or_else(|| "—".to_string()),
            }
        })
        .collect();

    render(NormalTransfersFragmentTemplate {
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
