use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::account_kind::AccountKind;
use crate::domain::transaction::{TransactionDirection, TransactionStatus};
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
// Transfer form
// ---------------------------------------------------------------------------

struct PbAccountOption {
    id: String,
    holder_id: String,
    purpose_code: String,
}

#[derive(Template)]
#[template(path = "admin/normal_transfer.html")]
struct NormalTransferTemplate {
    prefix: String,
    account_id: String,
    holder_id: String,
    pb_accounts: Vec<PbAccountOption>,
    error: Option<String>,
}

pub async fn normal_transfer_form(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> Response {
    let account = match state.normal_account_repo.get_account(account_id).await {
        Ok(a) => a,
        Err(_) => return (StatusCode::NOT_FOUND, "Account not found").into_response(),
    };

    let pb_accounts = match state.pb_account_repo.list_accounts().await {
        Ok(accts) => accts
            .into_iter()
            .map(|a| PbAccountOption {
                id: a.id.to_string(),
                holder_id: a.holder_id,
                purpose_code: a.purpose_code,
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to list PB accounts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    render(NormalTransferTemplate {
        prefix: state.path_prefix.clone(),
        account_id: account_id.to_string(),
        holder_id: account.holder_id,
        pb_accounts,
        error: None,
    })
}

#[derive(Deserialize)]
pub struct NormalTransferForm {
    destination_pb_account_id: String,
    amount: u64,
    #[serde(default)]
    pending: Option<String>,
    gateway_ref: Option<String>,
    #[serde(default)]
    timeout_seconds: Option<String>,
    description: Option<String>,
}

pub async fn process_normal_transfer(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<NormalTransferForm>,
) -> Response {
    let dest_pb_id = match form.destination_pb_account_id.trim().parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => {
            return render_transfer_form_error(
                &state,
                account_id,
                "Invalid destination PB account ID".to_string(),
            )
            .await;
        }
    };

    let is_pending = form.pending.as_deref() == Some("true");
    let gateway_ref = form.gateway_ref.as_deref().filter(|s| !s.is_empty());
    let timeout_seconds: Option<u32> = form
        .timeout_seconds
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse().ok());
    let description = form.description.as_deref().filter(|s| !s.is_empty());

    match state
        .transfer_service
        .transfer(
            account_id,
            dest_pb_id,
            form.amount,
            is_pending,
            gateway_ref,
            timeout_seconds,
            description,
            None,
        )
        .await
    {
        Ok(result) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transfers/{}", result.source_txn_id),
        ))
        .into_response(),
        Err(e) => render_transfer_form_error(&state, account_id, format!("{e}")).await,
    }
}

async fn render_transfer_form_error(state: &AppState, account_id: Uuid, error: String) -> Response {
    let holder_id = state
        .normal_account_repo
        .get_account(account_id)
        .await
        .map(|a| a.holder_id)
        .unwrap_or_default();

    let pb_accounts = state
        .pb_account_repo
        .list_accounts()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|a| PbAccountOption {
            id: a.id.to_string(),
            holder_id: a.holder_id,
            purpose_code: a.purpose_code,
        })
        .collect();

    render(NormalTransferTemplate {
        prefix: state.path_prefix.clone(),
        account_id: account_id.to_string(),
        holder_id,
        pb_accounts,
        error: Some(error),
    })
}

// ---------------------------------------------------------------------------
// Transfer detail
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/transfer_detail.html")]
struct TransferDetailTemplate {
    prefix: String,
    transfer_id: String,
    correlation_id: String,
    status: String,
    status_class: String,
    amount: String,
    source_account_id: String,
    source_holder_id: String,
    dest_account_id: String,
    dest_holder_id: String,
    dest_purpose: String,
    gateway_ref: String,
    description: String,
    created_at: String,
    can_post_or_void: bool,
    can_reverse: bool,
    is_reversal: bool,
    reversed_by_id: Option<String>,
}

pub async fn transfer_detail(
    State(state): State<AppState>,
    Path(transfer_id): Path<Uuid>,
) -> Response {
    // Look up the source-side transaction (any account — get_transaction doesn't scope by account)
    let source_row = match state.transaction_repo.get_transaction(transfer_id).await {
        Ok(row) => row,
        Err(e) => {
            tracing::error!("Transfer source row not found: {e}");
            return (StatusCode::NOT_FOUND, "Transfer not found").into_response();
        }
    };

    let correlation_id = match source_row.correlation_id {
        Some(cid) => cid,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Transfer has no correlation_id",
            )
                .into_response()
        }
    };

    // Fetch both legs
    let legs = match state
        .transaction_repo
        .find_by_correlation_id(correlation_id)
        .await
    {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to fetch transfer legs: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Identify source (normal) and destination (pb) legs
    let source_leg = legs.iter().find(|l| l.account_kind == AccountKind::Normal);
    let dest_leg = legs.iter().find(|l| l.account_kind == AccountKind::Pb);

    let (source_leg, dest_leg) = match (source_leg, dest_leg) {
        (Some(s), Some(d)) => (s, d),
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Transfer legs incomplete",
            )
                .into_response()
        }
    };

    // Look up source normal account holder
    let source_holder_id = match state
        .normal_account_repo
        .get_account(source_leg.account_id)
        .await
    {
        Ok(a) => a.holder_id,
        Err(_) => source_leg.account_id.to_string(),
    };

    // Look up destination PB account holder + purpose
    let (dest_holder_id, dest_purpose) =
        match state.pb_account_repo.get_account(dest_leg.account_id).await {
            Ok(a) => (a.holder_id, a.purpose_code),
            Err(_) => (dest_leg.account_id.to_string(), "—".to_string()),
        };

    let fmt_amount = |amt: u64| format!("{}.{:02}", amt / 100, amt % 100);
    let status_str = source_leg.status.as_str().to_string();
    let status_class = match source_leg.status {
        TransactionStatus::Pending => "status-frozen",
        TransactionStatus::Posted | TransactionStatus::Settled => "status-active",
        TransactionStatus::Voided => "status-closed",
    }
    .to_string();

    let can_post_or_void = source_leg.status == TransactionStatus::Pending;

    // A row is reversible when:
    // - It is the original transfer's source-side row (kind=Normal, direction=Outbound, status=Posted)
    // - It is not itself a reversal (reverses_transaction_id IS NULL)
    let can_reverse = source_leg.status == TransactionStatus::Posted
        && source_leg.direction == TransactionDirection::Outbound
        && source_leg.reverses_transaction_id.is_none()
        && state
            .transaction_repo
            .find_reversal_of(source_leg.id)
            .await
            .ok()
            .flatten()
            .is_none();

    let is_reversal = source_leg.reverses_transaction_id.is_some();

    let reversed_by_id = if !is_reversal {
        state
            .transaction_repo
            .find_reversal_of(source_leg.id)
            .await
            .ok()
            .flatten()
            .map(|r| r.id.to_string())
    } else {
        None
    };

    render(TransferDetailTemplate {
        prefix: state.path_prefix.clone(),
        transfer_id: transfer_id.to_string(),
        correlation_id: correlation_id.to_string(),
        status: status_str,
        status_class,
        amount: fmt_amount(source_leg.amount),
        source_account_id: source_leg.account_id.to_string(),
        source_holder_id,
        dest_account_id: dest_leg.account_id.to_string(),
        dest_holder_id,
        dest_purpose,
        gateway_ref: source_leg
            .gateway_ref
            .clone()
            .unwrap_or_else(|| "—".to_string()),
        description: source_leg
            .description
            .clone()
            .unwrap_or_else(|| "—".to_string()),
        created_at: source_leg
            .created_at
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        can_post_or_void,
        can_reverse,
        is_reversal,
        reversed_by_id,
    })
}

// ---------------------------------------------------------------------------
// Post transfer
// ---------------------------------------------------------------------------

pub async fn post_transfer(
    State(state): State<AppState>,
    Path(transfer_id): Path<Uuid>,
) -> Response {
    // Look up the source row to find the source normal account ID
    let source_row = match state.transaction_repo.get_transaction(transfer_id).await {
        Ok(row) => row,
        Err(e) => {
            tracing::error!("Transfer source row not found for post: {e}");
            return (StatusCode::NOT_FOUND, "Transfer not found").into_response();
        }
    };

    match state
        .transfer_service
        .post_transfer(source_row.account_id, transfer_id)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transfers/{transfer_id}"),
        ))
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to post transfer {transfer_id}: {e}");
            Redirect::to(&prefixed(
                &state,
                &format!("/admin/transfers/{transfer_id}"),
            ))
            .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Void transfer
// ---------------------------------------------------------------------------

pub async fn void_transfer(
    State(state): State<AppState>,
    Path(transfer_id): Path<Uuid>,
) -> Response {
    let source_row = match state.transaction_repo.get_transaction(transfer_id).await {
        Ok(row) => row,
        Err(e) => {
            tracing::error!("Transfer source row not found for void: {e}");
            return (StatusCode::NOT_FOUND, "Transfer not found").into_response();
        }
    };

    match state
        .transfer_service
        .void_transfer(source_row.account_id, transfer_id)
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transfers/{transfer_id}"),
        ))
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to void transfer {transfer_id}: {e}");
            Redirect::to(&prefixed(
                &state,
                &format!("/admin/transfers/{transfer_id}"),
            ))
            .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Reverse transfer
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/transfer_reverse.html")]
struct TransferReverseTemplate {
    prefix: String,
    transfer_id: String,
    source_account_id: String,
    destination_account_id: String,
    original_amount: String,
    original_amount_paisa: u64,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct ReverseTransferForm {
    pub amount_paisa: u64,
    pub description: Option<String>,
}

async fn build_reverse_template(
    state: &AppState,
    transfer_id: Uuid,
    error: Option<String>,
) -> Result<TransferReverseTemplate, Response> {
    let source_row = state
        .transaction_repo
        .get_transaction(transfer_id)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Transfer not found").into_response())?;

    let correlation_id = source_row.correlation_id.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Transfer has no correlation_id",
        )
            .into_response()
    })?;

    let legs = state
        .transaction_repo
        .find_by_correlation_id(correlation_id)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response())?;

    let dest_leg = legs
        .iter()
        .find(|l| l.account_kind == AccountKind::Pb)
        .ok_or_else(|| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Destination leg missing").into_response()
        })?;

    let fmt_amount = |amt: u64| format!("{}.{:02}", amt / 100, amt % 100);

    Ok(TransferReverseTemplate {
        prefix: state.path_prefix.clone(),
        transfer_id: transfer_id.to_string(),
        source_account_id: source_row.account_id.to_string(),
        destination_account_id: dest_leg.account_id.to_string(),
        original_amount: fmt_amount(source_row.amount),
        original_amount_paisa: source_row.amount,
        error,
    })
}

pub async fn reverse_transfer_form(
    State(state): State<AppState>,
    Path(transfer_id): Path<Uuid>,
) -> Response {
    match build_reverse_template(&state, transfer_id, None).await {
        Ok(tmpl) => render(tmpl),
        Err(resp) => resp,
    }
}

pub async fn process_reverse_transfer(
    State(state): State<AppState>,
    Path(transfer_id): Path<Uuid>,
    axum::extract::Form(form): axum::extract::Form<ReverseTransferForm>,
) -> Response {
    let source_row = match state.transaction_repo.get_transaction(transfer_id).await {
        Ok(row) => row,
        Err(_) => return (StatusCode::NOT_FOUND, "Transfer not found").into_response(),
    };

    match state
        .transfer_service
        .reverse_transfer(
            source_row.account_id,
            transfer_id,
            form.amount_paisa,
            None,
            form.description.as_deref(),
            None,
        )
        .await
    {
        Ok(_) => Redirect::to(&prefixed(
            &state,
            &format!("/admin/transfers/{transfer_id}"),
        ))
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to reverse transfer {transfer_id}: {e}");
            match build_reverse_template(&state, transfer_id, Some(e.to_string())).await {
                Ok(tmpl) => render(tmpl),
                Err(resp) => resp,
            }
        }
    }
}
