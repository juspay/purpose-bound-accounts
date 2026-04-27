use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::tb_explorer::{TbAccountView, TbBalanceView, TbTransferView};
use crate::repository::ledger_repo::{
    LEDGER_INR_PAISA, MERCHANT_SETTLEMENT_TB_ID, SELF_FUNDING_SOURCE_TB_ID,
    THIRD_PARTY_FUNDING_SOURCE_TB_ID, TRUST_FUNDING_SOURCE_TB_ID, WITHDRAWAL_SETTLEMENT_TB_ID,
};
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

/// Parse a u128 from either decimal (e.g. "12345") or a UUID ("aaaa-...").
fn parse_u128_or_uuid(s: &str) -> Option<u128> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(v) = trimmed.parse::<u128>() {
        return Some(v);
    }
    if let Ok(uuid) = trimmed.parse::<Uuid>() {
        return Some(u128::from_be_bytes(*uuid.as_bytes()));
    }
    // Hex form (0x...)
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if let Ok(v) = u128::from_str_radix(hex, 16) {
            return Some(v);
        }
    }
    None
}

/// Parse a comma / whitespace separated list of IDs.
fn parse_id_list(s: &str) -> Vec<u128> {
    s.split(|c: char| c == ',' || c == '\n' || c.is_whitespace())
        .filter_map(|chunk| {
            let t = chunk.trim();
            if t.is_empty() {
                None
            } else {
                parse_u128_or_uuid(t)
            }
        })
        .collect()
}

/// Parse a datetime-local form input (e.g. "2026-04-19T12:34" or with seconds).
fn parse_local_dt(s: &str) -> Option<DateTime<Utc>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Try a few common formats
    for fmt in &["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Some(Utc.from_utc_datetime(&ndt));
        }
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Utc));
    }
    None
}

fn parse_optional_u16(s: &str) -> u16 {
    s.trim().parse::<u16>().unwrap_or(0)
}

// ======================================================================
// Overview
// ======================================================================

#[derive(Template)]
#[template(path = "admin/tb/overview.html")]
struct OverviewTemplate {
    prefix: String,
    cluster_id: String,
    addresses: Vec<String>,
    default_ledger: u32,
    sentinel_rows: Vec<SentinelRow>,
    pba_accounts: i64,
    error: Option<String>,
}

struct SentinelRow {
    label: &'static str,
    id: String,
    found: bool,
    credits_posted: String,
    debits_posted: String,
    balance: String,
}

pub async fn overview(State(state): State<AppState>) -> Response {
    let sentinel_ids = vec![
        SELF_FUNDING_SOURCE_TB_ID,
        TRUST_FUNDING_SOURCE_TB_ID,
        THIRD_PARTY_FUNDING_SOURCE_TB_ID,
        MERCHANT_SETTLEMENT_TB_ID,
        WITHDRAWAL_SETTLEMENT_TB_ID,
    ];
    let labels = [
        "Self funding source",
        "Trust funding source",
        "Third-party funding source",
        "Merchant settlement",
        "Withdrawal settlement",
    ];

    let mut rows = Vec::new();
    let mut error: Option<String> = None;
    match state
        .ledger_repo
        .explorer_lookup_accounts(sentinel_ids.clone())
        .await
    {
        Ok(accounts) => {
            for (idx, id) in sentinel_ids.iter().enumerate() {
                let found = accounts.iter().find(|a| a.id == *id);
                let label = labels[idx];
                if let Some(a) = found {
                    rows.push(SentinelRow {
                        label,
                        id: a.id_str.clone(),
                        found: true,
                        credits_posted: TbAccountView::amount_display(a.credits_posted),
                        debits_posted: TbAccountView::amount_display(a.debits_posted),
                        balance: a.balance_display(),
                    });
                } else {
                    rows.push(SentinelRow {
                        label,
                        id: id.to_string(),
                        found: false,
                        credits_posted: "-".to_string(),
                        debits_posted: "-".to_string(),
                        balance: "-".to_string(),
                    });
                }
            }
        }
        Err(e) => {
            tracing::error!("Overview: sentinel lookup failed: {e}");
            error = Some(format!("TigerBeetle error: {e}"));
            for (idx, id) in sentinel_ids.iter().enumerate() {
                rows.push(SentinelRow {
                    label: labels[idx],
                    id: id.to_string(),
                    found: false,
                    credits_posted: "-".to_string(),
                    debits_posted: "-".to_string(),
                    balance: "-".to_string(),
                });
            }
        }
    }

    let pba_accounts = state
        .account_repo
        .list_accounts()
        .await
        .map(|v| v.len() as i64)
        .unwrap_or(0);

    render(OverviewTemplate {
        prefix: state.path_prefix.clone(),
        cluster_id: state.tb_cluster_id.to_string(),
        addresses: state.tb_addresses.clone(),
        default_ledger: LEDGER_INR_PAISA,
        sentinel_rows: rows,
        pba_accounts,
        error,
    })
}

// ======================================================================
// Accounts page (query + lookup)
// ======================================================================

#[derive(Deserialize, Default)]
pub struct AccountsQuery {
    #[serde(default)]
    mode: Option<String>, // "query" or "lookup"
    #[serde(default)]
    ids: Option<String>,
    #[serde(default)]
    ledger: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    timestamp_min: Option<String>,
    #[serde(default)]
    timestamp_max: Option<String>,
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    reversed: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/tb/accounts.html")]
struct AccountsTemplate {
    prefix: String,
    default_ledger: u32,
    form: AccountsFormEcho,
    results: Vec<AccountRow>,
    error: Option<String>,
    ran_query: bool,
}

struct AccountsFormEcho {
    mode: String,
    ids: String,
    ledger: String,
    code: String,
    timestamp_min: String,
    timestamp_max: String,
    limit: String,
    reversed: bool,
}

struct AccountRow {
    id: String,
    id_uuid: String,
    sentinel: Option<&'static str>,
    ledger: u32,
    code: u16,
    code_label: &'static str,
    flags: String,
    credits_posted: String,
    debits_posted: String,
    credits_pending: String,
    debits_pending: String,
    balance: String,
    timestamp: String,
}

fn account_to_row(a: &TbAccountView) -> AccountRow {
    AccountRow {
        id: a.id_str.clone(),
        id_uuid: a.id_uuid.clone(),
        sentinel: a.id_sentinel,
        ledger: a.ledger,
        code: a.code,
        code_label: a.code_label,
        flags: a.flags_labels.join(", "),
        credits_posted: TbAccountView::amount_display(a.credits_posted),
        debits_posted: TbAccountView::amount_display(a.debits_posted),
        credits_pending: TbAccountView::amount_display(a.credits_pending),
        debits_pending: TbAccountView::amount_display(a.debits_pending),
        balance: a.balance_display(),
        timestamp: a.timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
    }
}

pub async fn accounts_page(
    State(state): State<AppState>,
    Query(q): Query<AccountsQuery>,
) -> Response {
    let form = AccountsFormEcho {
        mode: q.mode.clone().unwrap_or_default(),
        ids: q.ids.clone().unwrap_or_default(),
        ledger: q.ledger.clone().unwrap_or_default(),
        code: q.code.clone().unwrap_or_default(),
        timestamp_min: q.timestamp_min.clone().unwrap_or_default(),
        timestamp_max: q.timestamp_max.clone().unwrap_or_default(),
        limit: q.limit.clone().unwrap_or_else(|| "100".to_string()),
        reversed: q.reversed.as_deref() == Some("on"),
    };

    let mut results: Vec<AccountRow> = Vec::new();
    let mut error: Option<String> = None;
    let ran_query = q.mode.is_some();

    match q.mode.as_deref() {
        Some("lookup") => {
            let ids = parse_id_list(q.ids.as_deref().unwrap_or(""));
            if ids.is_empty() {
                error = Some("No valid IDs parsed from input.".to_string());
            } else {
                match state.ledger_repo.explorer_lookup_accounts(ids).await {
                    Ok(rows) => {
                        results = rows.iter().map(account_to_row).collect();
                    }
                    Err(e) => error = Some(format!("{e}")),
                }
            }
        }
        Some("query") => {
            let ledger = form
                .ledger
                .trim()
                .parse::<u32>()
                .unwrap_or(LEDGER_INR_PAISA);
            let code = parse_optional_u16(&form.code);
            let tmin = parse_local_dt(&form.timestamp_min);
            let tmax = parse_local_dt(&form.timestamp_max);
            let limit = form.limit.trim().parse::<u32>().unwrap_or(100);
            match state
                .ledger_repo
                .explorer_query_accounts(ledger, code, tmin, tmax, limit, form.reversed)
                .await
            {
                Ok(rows) => {
                    results = rows.iter().map(account_to_row).collect();
                }
                Err(e) => error = Some(format!("{e}")),
            }
        }
        _ => {}
    }

    render(AccountsTemplate {
        prefix: state.path_prefix.clone(),
        default_ledger: LEDGER_INR_PAISA,
        form,
        results,
        error,
        ran_query,
    })
}

// ======================================================================
// Single account detail
// ======================================================================

#[derive(Template)]
#[template(path = "admin/tb/account_detail.html")]
struct AccountDetailTemplate {
    prefix: String,
    account: AccountFullRow,
    transfers: Vec<TransferRow>,
    balance_history: Vec<BalanceRow>,
    error: Option<String>,
}

struct AccountFullRow {
    id: String,
    id_uuid: String,
    sentinel: Option<&'static str>,
    ledger: u32,
    code: u16,
    code_label: &'static str,
    flags_bits: u16,
    flags: String,
    credits_posted: String,
    debits_posted: String,
    credits_pending: String,
    debits_pending: String,
    balance: String,
    timestamp: String,
}

struct TransferRow {
    id: String,
    timestamp: String,
    code: u16,
    code_label: &'static str,
    amount: String,
    debit_id: String,
    credit_id: String,
    flags: String,
    is_pending: bool,
    pending_id: String,
}

struct BalanceRow {
    timestamp: String,
    debits_posted: String,
    credits_posted: String,
    debits_pending: String,
    credits_pending: String,
    balance: String,
}

fn transfer_to_row(t: &TbTransferView) -> TransferRow {
    TransferRow {
        id: t.id_str.clone(),
        timestamp: t.timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
        code: t.code,
        code_label: t.code_label,
        amount: t.amount_display.clone(),
        debit_id: t
            .debit_account_sentinel
            .map(|s| format!("{} ({})", s, t.debit_account_str))
            .unwrap_or_else(|| t.debit_account_str.clone()),
        credit_id: t
            .credit_account_sentinel
            .map(|s| format!("{} ({})", s, t.credit_account_str))
            .unwrap_or_else(|| t.credit_account_str.clone()),
        flags: t.flags_labels.join(", "),
        is_pending: t.is_pending,
        pending_id: t.pending_id_str.clone(),
    }
}

fn balance_to_row(b: &TbBalanceView) -> BalanceRow {
    BalanceRow {
        timestamp: b.timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
        debits_posted: TbAccountView::amount_display(b.debits_posted),
        credits_posted: TbAccountView::amount_display(b.credits_posted),
        debits_pending: TbAccountView::amount_display(b.debits_pending),
        credits_pending: TbAccountView::amount_display(b.credits_pending),
        balance: b.balance_display.clone(),
    }
}

pub async fn account_detail(State(state): State<AppState>, Path(id_str): Path<String>) -> Response {
    let id = match parse_u128_or_uuid(&id_str) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, "Invalid account id").into_response(),
    };

    let accounts = match state.ledger_repo.explorer_lookup_accounts(vec![id]).await {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("TigerBeetle error: {e}"),
            )
                .into_response()
        }
    };

    let account = match accounts.into_iter().next() {
        Some(a) => a,
        None => return (StatusCode::NOT_FOUND, "Account not found in TigerBeetle").into_response(),
    };

    let (transfers, balance_history, error) = match state
        .ledger_repo
        .explorer_account_transfers(id, true, true, true, None, None, 200)
        .await
    {
        Ok(ts) => {
            let transfers: Vec<TransferRow> = ts.iter().map(transfer_to_row).collect();
            // Balance history only works if HISTORY flag is set. Try it but don't fail hard.
            let bh = state
                .ledger_repo
                .explorer_account_balances(id, true, None, None, 200)
                .await
                .unwrap_or_default();
            let balances: Vec<BalanceRow> = bh.iter().map(balance_to_row).collect();
            (transfers, balances, None::<String>)
        }
        Err(e) => (Vec::new(), Vec::new(), Some(format!("{e}"))),
    };

    render(AccountDetailTemplate {
        prefix: state.path_prefix.clone(),
        account: AccountFullRow {
            id: account.id_str.clone(),
            id_uuid: account.id_uuid.clone(),
            sentinel: account.id_sentinel,
            ledger: account.ledger,
            code: account.code,
            code_label: account.code_label,
            flags_bits: account.flags_bits,
            flags: account.flags_labels.join(", "),
            credits_posted: TbAccountView::amount_display(account.credits_posted),
            debits_posted: TbAccountView::amount_display(account.debits_posted),
            credits_pending: TbAccountView::amount_display(account.credits_pending),
            debits_pending: TbAccountView::amount_display(account.debits_pending),
            balance: account.balance_display(),
            timestamp: account
                .timestamp
                .format("%Y-%m-%d %H:%M:%S%.3f UTC")
                .to_string(),
        },
        transfers,
        balance_history,
        error,
    })
}

// ======================================================================
// Transfers page (query + lookup)
// ======================================================================

#[derive(Deserialize, Default)]
pub struct TransfersQuery {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    ids: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    include_debits: Option<String>,
    #[serde(default)]
    include_credits: Option<String>,
    #[serde(default)]
    ledger: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    timestamp_min: Option<String>,
    #[serde(default)]
    timestamp_max: Option<String>,
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    reversed: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/tb/transfers.html")]
struct TransfersTemplate {
    prefix: String,
    default_ledger: u32,
    form: TransfersFormEcho,
    results: Vec<TransferRow>,
    error: Option<String>,
    ran_query: bool,
}

struct TransfersFormEcho {
    mode: String,
    ids: String,
    account_id: String,
    include_debits: bool,
    include_credits: bool,
    ledger: String,
    code: String,
    timestamp_min: String,
    timestamp_max: String,
    limit: String,
    reversed: bool,
}

pub async fn transfers_page(
    State(state): State<AppState>,
    Query(q): Query<TransfersQuery>,
) -> Response {
    let form = TransfersFormEcho {
        mode: q.mode.clone().unwrap_or_default(),
        ids: q.ids.clone().unwrap_or_default(),
        account_id: q.account_id.clone().unwrap_or_default(),
        include_debits: q
            .include_debits
            .as_deref()
            .map(|s| s == "on")
            .unwrap_or(true),
        include_credits: q
            .include_credits
            .as_deref()
            .map(|s| s == "on")
            .unwrap_or(true),
        ledger: q.ledger.clone().unwrap_or_default(),
        code: q.code.clone().unwrap_or_default(),
        timestamp_min: q.timestamp_min.clone().unwrap_or_default(),
        timestamp_max: q.timestamp_max.clone().unwrap_or_default(),
        limit: q.limit.clone().unwrap_or_else(|| "100".to_string()),
        reversed: q.reversed.as_deref() == Some("on"),
    };

    let mut results: Vec<TransferRow> = Vec::new();
    let mut error: Option<String> = None;
    let ran_query = q.mode.is_some();

    match q.mode.as_deref() {
        Some("lookup") => {
            let ids = parse_id_list(q.ids.as_deref().unwrap_or(""));
            if ids.is_empty() {
                error = Some("No valid IDs parsed from input.".to_string());
            } else {
                match state.ledger_repo.explorer_lookup_transfers(ids).await {
                    Ok(rows) => {
                        results = rows.iter().map(transfer_to_row).collect();
                    }
                    Err(e) => error = Some(format!("{e}")),
                }
            }
        }
        Some("for_account") => {
            let acct = parse_u128_or_uuid(&form.account_id);
            match acct {
                Some(id) => {
                    let tmin = parse_local_dt(&form.timestamp_min);
                    let tmax = parse_local_dt(&form.timestamp_max);
                    let limit = form.limit.trim().parse::<u32>().unwrap_or(100);
                    match state
                        .ledger_repo
                        .explorer_account_transfers(
                            id,
                            form.include_debits,
                            form.include_credits,
                            form.reversed,
                            tmin,
                            tmax,
                            limit,
                        )
                        .await
                    {
                        Ok(rows) => {
                            results = rows.iter().map(transfer_to_row).collect();
                        }
                        Err(e) => error = Some(format!("{e}")),
                    }
                }
                None => error = Some("Invalid account id".to_string()),
            }
        }
        Some("query") => {
            let ledger = form
                .ledger
                .trim()
                .parse::<u32>()
                .unwrap_or(LEDGER_INR_PAISA);
            let code = parse_optional_u16(&form.code);
            let tmin = parse_local_dt(&form.timestamp_min);
            let tmax = parse_local_dt(&form.timestamp_max);
            let limit = form.limit.trim().parse::<u32>().unwrap_or(100);
            match state
                .ledger_repo
                .explorer_query_transfers(ledger, code, tmin, tmax, limit, form.reversed)
                .await
            {
                Ok(rows) => {
                    results = rows.iter().map(transfer_to_row).collect();
                }
                Err(e) => error = Some(format!("{e}")),
            }
        }
        _ => {}
    }

    render(TransfersTemplate {
        prefix: state.path_prefix.clone(),
        default_ledger: LEDGER_INR_PAISA,
        form,
        results,
        error,
        ran_query,
    })
}

// ======================================================================
// Single transfer detail
// ======================================================================

#[derive(Template)]
#[template(path = "admin/tb/transfer_detail.html")]
struct TransferDetailTemplate {
    prefix: String,
    t: TransferFullRow,
}

struct TransferFullRow {
    id: String,
    id_uuid: String,
    timestamp: String,
    code: u16,
    code_label: &'static str,
    amount: String,
    debit_id: String,
    debit_sentinel: Option<&'static str>,
    credit_id: String,
    credit_sentinel: Option<&'static str>,
    ledger: u32,
    flags_bits: u16,
    flags: String,
    pending_id: String,
    timeout: u32,
    is_pending: bool,
}

pub async fn transfer_detail(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Response {
    let id = match parse_u128_or_uuid(&id_str) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, "Invalid transfer id").into_response(),
    };
    let transfers = match state.ledger_repo.explorer_lookup_transfers(vec![id]).await {
        Ok(ts) => ts,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("TigerBeetle error: {e}"),
            )
                .into_response()
        }
    };
    let t = match transfers.into_iter().next() {
        Some(t) => t,
        None => return (StatusCode::NOT_FOUND, "Transfer not found").into_response(),
    };
    render(TransferDetailTemplate {
        prefix: state.path_prefix.clone(),
        t: TransferFullRow {
            id: t.id_str.clone(),
            id_uuid: t.id_uuid.clone(),
            timestamp: t.timestamp.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
            code: t.code,
            code_label: t.code_label,
            amount: t.amount_display.clone(),
            debit_id: t.debit_account_str.clone(),
            debit_sentinel: t.debit_account_sentinel,
            credit_id: t.credit_account_str.clone(),
            credit_sentinel: t.credit_account_sentinel,
            ledger: t.ledger,
            flags_bits: t.flags_bits,
            flags: t.flags_labels.join(", "),
            pending_id: t.pending_id_str.clone(),
            timeout: t.timeout,
            is_pending: t.is_pending,
        },
    })
}

// ======================================================================
// Pending transfers
// ======================================================================

#[derive(Deserialize, Default)]
pub struct PendingQuery {
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    ledger: Option<String>,
    #[serde(default)]
    limit: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/tb/pending.html")]
struct PendingTemplate {
    prefix: String,
    form: PendingFormEcho,
    default_ledger: u32,
    results: Vec<TransferRow>,
    error: Option<String>,
    ran_query: bool,
}

struct PendingFormEcho {
    account_id: String,
    ledger: String,
    limit: String,
}

pub async fn pending_page(
    State(state): State<AppState>,
    Query(q): Query<PendingQuery>,
) -> Response {
    let form = PendingFormEcho {
        account_id: q.account_id.clone().unwrap_or_default(),
        ledger: q.ledger.clone().unwrap_or_default(),
        limit: q.limit.clone().unwrap_or_else(|| "100".to_string()),
    };

    let mut results: Vec<TransferRow> = Vec::new();
    let mut error: Option<String> = None;
    let ran_query = q.account_id.is_some() || q.ledger.is_some();

    if ran_query {
        // Filter by PENDING flag requires filtering client-side since neither query nor filter
        // API supports direct flag filtering. We fetch a batch and filter.
        let limit = form.limit.trim().parse::<u32>().unwrap_or(100);
        let source_transfers = if let Some(acct_str) = q.account_id.as_ref() {
            match parse_u128_or_uuid(acct_str) {
                Some(id) => state
                    .ledger_repo
                    .explorer_account_transfers(id, true, true, true, None, None, limit)
                    .await
                    .map_err(|e| format!("{e}")),
                None => Err("Invalid account id".to_string()),
            }
        } else {
            let ledger = form
                .ledger
                .trim()
                .parse::<u32>()
                .unwrap_or(LEDGER_INR_PAISA);
            state
                .ledger_repo
                .explorer_query_transfers(ledger, 0, None, None, limit, true)
                .await
                .map_err(|e| format!("{e}"))
        };
        match source_transfers {
            Ok(rows) => {
                results = rows
                    .iter()
                    .filter(|t| t.is_pending)
                    .map(transfer_to_row)
                    .collect();
            }
            Err(e) => error = Some(e),
        }
    }

    render(PendingTemplate {
        prefix: state.path_prefix.clone(),
        form,
        default_ledger: LEDGER_INR_PAISA,
        results,
        error,
        ran_query,
    })
}

pub async fn pending_post(State(state): State<AppState>, Path(id_str): Path<String>) -> Response {
    let id = match parse_u128_or_uuid(&id_str) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, "Invalid pending id").into_response(),
    };
    match state.ledger_repo.post_pending_transfer(id).await {
        Ok(_) => Redirect::to(&format!("{}/admin/tb/pending", state.path_prefix)).into_response(),
        Err(e) => {
            tracing::error!("Manual post_pending failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

pub async fn pending_void(State(state): State<AppState>, Path(id_str): Path<String>) -> Response {
    let id = match parse_u128_or_uuid(&id_str) {
        Some(v) => v,
        None => return (StatusCode::BAD_REQUEST, "Invalid pending id").into_response(),
    };
    match state.ledger_repo.void_pending_transfer(id).await {
        Ok(_) => Redirect::to(&format!("{}/admin/tb/pending", state.path_prefix)).into_response(),
        Err(e) => {
            tracing::error!("Manual void_pending failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response()
        }
    }
}

// ======================================================================
// Decoder
// ======================================================================

#[derive(Deserialize, Default)]
pub struct DecoderQuery {
    #[serde(default)]
    input: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/tb/decoder.html")]
struct DecoderTemplate {
    prefix: String,
    input: String,
    as_u128: String,
    as_uuid: String,
    as_hex: String,
    as_inr: String,
    sentinel: Option<&'static str>,
    error: Option<String>,
}

pub async fn decoder(State(state): State<AppState>, Query(q): Query<DecoderQuery>) -> Response {
    let input = q.input.clone().unwrap_or_default();
    let (as_u128, as_uuid, as_hex, as_inr, sentinel, error) = if input.trim().is_empty() {
        (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            None,
            None,
        )
    } else if let Some(v) = parse_u128_or_uuid(&input) {
        let uuid = Uuid::from_bytes(v.to_be_bytes()).to_string();
        let hex = format!("0x{v:032x}");
        let inr = TbAccountView::amount_display(v);
        let sent = crate::domain::tb_explorer::sentinel_label(v);
        (v.to_string(), uuid, hex, inr, sent, None)
    } else {
        (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            None,
            Some(
                "Could not parse input — provide a UUID, decimal u128, or 0x-prefixed hex."
                    .to_string(),
            ),
        )
    };
    render(DecoderTemplate {
        prefix: state.path_prefix.clone(),
        input,
        as_u128,
        as_uuid,
        as_hex,
        as_inr,
        sentinel,
        error,
    })
}
