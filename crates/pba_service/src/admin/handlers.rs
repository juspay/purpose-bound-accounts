use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use uuid::Uuid;

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
        Err(e) => {
            render_accounts_list(&state, Some(format!("{e}")), None).await
        }
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

    match state.account_service.update_status(account_id, status).await {
        Ok(_) => Redirect::to(&format!("/admin/accounts/{account_id}")).into_response(),
        Err(e) => {
            tracing::error!("Failed to update status: {e}");
            Redirect::to(&format!("/admin/accounts/{account_id}")).into_response()
        }
    }
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
    created_at: String,
    updated_at: String,
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
        created_at: account.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: account.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

#[derive(Template)]
#[template(path = "admin/transfers_fragment.html")]
struct TransfersFragmentTemplate {
    transfers: Vec<TransferRow>,
}

struct TransferRow {
    timestamp: String,
    transfer_type: String,
    direction: String,
    direction_class: String,
    amount: String,
}

pub async fn account_transfers_fragment(
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

    let transfers = match state
        .ledger_repo
        .get_account_transfers(
            account.tb_self_account_id,
            account.tb_others_account_id,
            100,
        )
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to get transfers: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Ledger error").into_response();
        }
    };

    let rows: Vec<TransferRow> = transfers
        .into_iter()
        .map(|t| TransferRow {
            timestamp: t.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
            transfer_type: t.transfer_type.label().to_string(),
            direction: t.direction.label().to_string(),
            direction_class: t.direction.css_class().to_string(),
            amount: t.amount_display(),
        })
        .collect();

    render(TransfersFragmentTemplate { transfers: rows })
}
