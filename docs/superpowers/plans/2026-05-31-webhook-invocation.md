# Webhook Invocation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add webhook invocation infrastructure that emits events on all transaction state changes and delivers them to registered consumer URLs via Kronos-scheduled callbacks.

**Architecture:** PBA embeds Kronos as a library (same PG, same process). After each transaction commit, `WebhookService` queries matching subscriptions, creates Kronos IMMEDIATE jobs. Kronos calls back PBA's internal endpoint. PBA makes the outbound webhook call with HMAC-signed payload. Kronos handles retries on failure.

**Tech Stack:** Rust/Axum, PostgreSQL, Kronos (library mode), HMAC-SHA256 (ring or openssl crate), reqwest

---

## Status as of 2026-06-01 — paused, Kronos blocker

**Done:** `hex`, `hmac`, `sha2` added to `crates/pba_service/Cargo.toml` (Task 1 Step 1, partial).

**Blocker found on Task 1:** depending on `kronos-worker` / `kronos-common` from
`juspay/kronos` branch `feat/library-compatible` fails Cargo resolution with a
`links = "sqlite3"` collision. Kronos's workspace pins `sqlx = "0.7"` (default
features on, leaving `sqlx-sqlite` reachable via weak feature refs); PBA uses
`sqlx = "0.8"`. Both `sqlx-sqlite` 0.7 and 0.8 declare `links = "sqlite3"` against
incompatible `libsqlite3-sys` versions (0.26 vs 0.30). Cargo's resolver rejects
the graph regardless of feature activation — `default-features = false` on
either side does not help, because the `links` collision check fires on any
reachable optional crate.

The `KronosHttpClient` workaround (use Kronos in HTTP mode) does *not* dodge
this, because `KronosHttpClient` lives in the same `kronos-worker` crate as
`KronosLibraryClient`, so just linking `kronos-worker` drags `sqlx 0.7` in.

**Next step before resuming this plan:** land a fix in `juspay/kronos` (or a
fork) that gates `kronos-common`'s sqlx-dependent code behind a feature so
`kronos-worker` can be built `default-features = false` for HTTP-only use.
Then Tasks 1 (rest) → 12 can proceed as written.

---

### Task 1: Add kronos-worker dependency and HMAC crate to Cargo.toml

**Files:**
- Modify: `crates/pba_service/Cargo.toml`

- [ ] **Step 1: Add dependencies**

Add `hmac`, `sha2`, `hex`, and `kronos-worker` to `crates/pba_service/Cargo.toml` under `[dependencies]`:

```toml
hmac = "0.12"
sha2 = "0.10"
hex = "0.4"
kronos-worker = { git = "https://github.com/juspay/kronos.git", branch = "feat/library-compatible" }
kronos-common = { git = "https://github.com/juspay/kronos.git", branch = "feat/library-compatible" }
tokio-util = "0.7"
```

Also add `tokio-util` since Kronos's `start_worker` requires `CancellationToken`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p pba-service 2>&1 | head -30`
Expected: compilation errors from unused imports are fine; no dependency resolution errors.

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/Cargo.toml Cargo.lock
git commit -m "chore: add kronos-worker, hmac, sha2 dependencies for webhook infrastructure"
```

---

### Task 2: Database migration for webhook_subscriptions table

**Files:**
- Create: `crates/pba_service/src/db/migrations/20260531000001_webhook_subscriptions.sql`

- [ ] **Step 1: Write the migration**

Create `crates/pba_service/src/db/migrations/20260531000001_webhook_subscriptions.sql`:

```sql
CREATE TABLE webhook_subscriptions (
    id                       UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    account_id               UUID        NULL,
    account_kind             TEXT        NULL CHECK (account_kind IN ('pb', 'normal')),
    url                      TEXT        NOT NULL,
    secret                   TEXT        NOT NULL,
    subscribed_event_types   TEXT[]      NOT NULL,
    is_active                BOOLEAN     NOT NULL DEFAULT true,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_account_scope CHECK (
        (account_id IS NULL AND account_kind IS NULL) OR
        (account_id IS NOT NULL AND account_kind IS NOT NULL)
    )
);

CREATE INDEX idx_webhook_subscriptions_account
    ON webhook_subscriptions (account_id) WHERE is_active = true;

CREATE INDEX idx_webhook_subscriptions_global
    ON webhook_subscriptions (id) WHERE account_id IS NULL AND is_active = true;
```

- [ ] **Step 2: Run migration**

Run: `just migrate`
Expected: migration applied successfully.

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/src/db/migrations/20260531000001_webhook_subscriptions.sql
git commit -m "feat: add webhook_subscriptions table migration"
```

---

### Task 3: WebhookSubscription domain model

**Files:**
- Create: `crates/pba_service/src/domain/webhook_subscription.rs`
- Modify: `crates/pba_service/src/domain.rs`

- [ ] **Step 1: Write the domain model**

Create `crates/pba_service/src/domain/webhook_subscription.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscription {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub account_kind: Option<String>,
    pub url: String,
    pub secret: String,
    pub subscribed_event_types: Vec<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookAccountKind {
    Pb,
    Normal,
}

impl WebhookAccountKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pb => "pb",
            Self::Normal => "normal",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pb" => Some(Self::Pb),
            "normal" => Some(Self::Normal),
            _ => None,
        }
    }
}
```

- [ ] **Step 2: Register the module**

Add to `crates/pba_service/src/domain.rs`:

```rust
pub mod webhook_subscription;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p pba-service`
Expected: compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/domain/webhook_subscription.rs crates/pba_service/src/domain.rs
git commit -m "feat: add WebhookSubscription domain model"
```

---

### Task 4: WebhookSubscriptionRepo

**Files:**
- Create: `crates/pba_service/src/repository/webhook_subscription_repo.rs`
- Modify: `crates/pba_service/src/repository.rs`

- [ ] **Step 1: Write the repository**

Create `crates/pba_service/src/repository/webhook_subscription_repo.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::webhook_subscription::WebhookSubscription;

#[derive(Clone)]
pub struct WebhookSubscriptionRepo {
    pool: PgPool,
}

impl WebhookSubscriptionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn create(
        &self,
        account_id: Option<Uuid>,
        account_kind: Option<&str>,
        url: &str,
        secret: &str,
        subscribed_event_types: &[String],
    ) -> Result<WebhookSubscription, sqlx::Error> {
        let row = sqlx::query_as::<_, WebhookSubscriptionRow>(
            r#"INSERT INTO webhook_subscriptions
               (account_id, account_kind, url, secret, subscribed_event_types)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(account_id)
        .bind(account_kind)
        .bind(url)
        .bind(secret)
        .bind(subscribed_event_types)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<WebhookSubscription>, sqlx::Error> {
        let row = sqlx::query_as::<_, WebhookSubscriptionRow>(
            r#"SELECT * FROM webhook_subscriptions WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn list(
        &self,
        account_id: Option<Uuid>,
    ) -> Result<Vec<WebhookSubscription>, sqlx::Error> {
        let rows = if let Some(aid) = account_id {
            sqlx::query_as::<_, WebhookSubscriptionRow>(
                r#"SELECT * FROM webhook_subscriptions
                   WHERE account_id = $1 OR account_id IS NULL
                   ORDER BY created_at DESC"#,
            )
            .bind(aid)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, WebhookSubscriptionRow>(
                r#"SELECT * FROM webhook_subscriptions
                   ORDER BY created_at DESC"#,
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn update(
        &self,
        id: Uuid,
        url: Option<&str>,
        secret: Option<&str>,
        subscribed_event_types: Option<&[String]>,
        is_active: Option<bool>,
    ) -> Result<WebhookSubscription, sqlx::Error> {
        let row = sqlx::query_as::<_, WebhookSubscriptionRow>(
            r#"UPDATE webhook_subscriptions
               SET url = COALESCE($2, url),
                   secret = COALESCE($3, secret),
                   subscribed_event_types = COALESCE($4, subscribed_event_types),
                   is_active = COALESCE($5, is_active),
                   updated_at = now()
               WHERE id = $1
               RETURNING *"#,
        )
        .bind(id)
        .bind(url)
        .bind(secret)
        .bind(subscribed_event_types)
        .bind(is_active)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(r#"DELETE FROM webhook_subscriptions WHERE id = $1"#)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Find all active subscriptions matching the given account IDs and event type.
    /// Returns global subscriptions (account_id IS NULL) plus per-account subscriptions
    /// where account_id matches any of the involved accounts and the event type is in
    /// their subscribed_event_types array.
    pub async fn find_matching(
        &self,
        involved_account_ids: &[Uuid],
        event_type: &str,
    ) -> Result<Vec<WebhookSubscription>, sqlx::Error> {
        let rows = sqlx::query_as::<_, WebhookSubscriptionRow>(
            r#"SELECT * FROM webhook_subscriptions
               WHERE is_active = true
                 AND (account_id = ANY($1) OR account_id IS NULL)
                 AND $2 = ANY(subscribed_event_types)
               ORDER BY created_at ASC"#,
        )
        .bind(involved_account_ids)
        .bind(event_type)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

// Internal row type for sqlx mapping
#[derive(Debug, sqlx::FromRow)]
struct WebhookSubscriptionRow {
    id: Uuid,
    account_id: Option<Uuid>,
    account_kind: Option<String>,
    url: String,
    secret: String,
    subscribed_event_types: Vec<String>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<WebhookSubscriptionRow> for WebhookSubscription {
    fn from(r: WebhookSubscriptionRow) -> Self {
        Self {
            id: r.id,
            account_id: r.account_id,
            account_kind: r.account_kind,
            url: r.url,
            secret: r.secret,
            subscribed_event_types: r.subscribed_event_types,
            is_active: r.is_active,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}
```

- [ ] **Step 2: Register the module**

Add to `crates/pba_service/src/repository.rs`:

```rust
pub mod webhook_subscription_repo;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p pba-service`
Expected: compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/repository/webhook_subscription_repo.rs crates/pba_service/src/repository.rs
git commit -m "feat: add WebhookSubscriptionRepo with CRUD and event matching query"
```

---

### Task 5: Webhook DTOs and API handlers

**Files:**
- Modify: `crates/pba_service/src/api/dto.rs`
- Create: `crates/pba_service/src/api/handlers/webhooks.rs`
- Modify: `crates/pba_service/src/api/handlers.rs`

- [ ] **Step 1: Add webhook DTOs to dto.rs**

Append to `crates/pba_service/src/api/dto.rs`:

```rust
// ── Webhook Subscription ──

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub account_id: Option<Uuid>,
    pub account_kind: Option<String>,
    pub url: String,
    pub secret: String,
    pub subscribed_event_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebhookRequest {
    pub url: Option<String>,
    pub secret: Option<String>,
    pub subscribed_event_types: Option<Vec<String>>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct WebhookSubscriptionResponse {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub account_kind: Option<String>,
    pub url: String,
    pub subscribed_event_types: Vec<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<crate::domain::webhook_subscription::WebhookSubscription> for WebhookSubscriptionResponse {
    fn from(s: crate::domain::webhook_subscription::WebhookSubscription) -> Self {
        Self {
            id: s.id,
            account_id: s.account_id,
            account_kind: s.account_kind,
            url: s.url,
            subscribed_event_types: s.subscribed_event_types,
            is_active: s.is_active,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}
```

- [ ] **Step 2: Create webhook handlers**

Create `crates/pba_service/src/api/handlers/webhooks.rs`:

```rust
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::dto::{
    CreateWebhookRequest, UpdateWebhookRequest, WebhookSubscriptionResponse,
};
use crate::error::AppError;
use crate::AppState;

pub async fn create_webhook(
    State(state): State<AppState>,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookSubscriptionResponse>), AppError> {
    // Validate URL
    if req.url.is_empty() {
        return Err(AppError::Validation("url must not be empty".into()));
    }
    if !req.url.starts_with("https://") && !req.url.starts_with("http://") {
        return Err(AppError::Validation(
            "url must start with http:// or https://".into(),
        ));
    }
    // Validate event types
    if req.subscribed_event_types.is_empty() {
        return Err(AppError::Validation(
            "subscribed_event_types must not be empty".into(),
        ));
    }
    for et in &req.subscribed_event_types {
        if !is_valid_event_type(et) {
            return Err(AppError::Validation(format!(
                "invalid event type: {et}"
            )));
        }
    }
    // Validate account_kind when account_id is provided
    if req.account_id.is_some() && req.account_kind.is_none() {
        return Err(AppError::Validation(
            "account_kind is required when account_id is provided".into(),
        ));
    }
    if req.account_id.is_none() && req.account_kind.is_some() {
        return Err(AppError::Validation(
            "account_kind must be null when account_id is null (global subscription)".into(),
        ));
    }
    // Validate secret
    if req.secret.is_empty() {
        return Err(AppError::Validation("secret must not be empty".into()));
    }

    let sub = state
        .webhook_subscription_repo
        .create(
            req.account_id,
            req.account_kind.as_deref(),
            &req.url,
            &req.secret,
            &req.subscribed_event_types,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(sub.into())))
}

#[derive(Debug, Deserialize)]
pub struct ListWebhooksQuery {
    pub account_id: Option<Uuid>,
}

pub async fn list_webhooks(
    State(state): State<AppState>,
    Query(query): Query<ListWebhooksQuery>,
) -> Result<Json<Vec<WebhookSubscriptionResponse>>, AppError> {
    let subs = state
        .webhook_subscription_repo
        .list(query.account_id)
        .await?;
    Ok(Json(subs.into_iter().map(Into::into).collect()))
}

pub async fn get_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookSubscriptionResponse>, AppError> {
    let sub = state
        .webhook_subscription_repo
        .get(id)
        .await?
        .ok_or_else(|| AppError::WebhookSubscriptionNotFound(id.to_string()))?;
    Ok(Json(sub.into()))
}

pub async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookSubscriptionResponse>, AppError> {
    if let Some(url) = &req.url {
        if url.is_empty() {
            return Err(AppError::Validation("url must not be empty".into()));
        }
    }
    if let Some(ets) = &req.subscribed_event_types {
        if ets.is_empty() {
            return Err(AppError::Validation(
                "subscribed_event_types must not be empty".into(),
            ));
        }
        for et in ets {
            if !is_valid_event_type(et) {
                return Err(AppError::Validation(format!(
                    "invalid event type: {et}"
                )));
            }
        }
    }
    let sub = state
        .webhook_subscription_repo
        .update(
            id,
            req.url.as_deref(),
            req.secret.as_deref(),
            req.subscribed_event_types.as_deref(),
            req.is_active,
        )
        .await?;
    Ok(Json(sub.into()))
}

pub async fn delete_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state.webhook_subscription_repo.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Validate that a string is a known event type.
fn is_valid_event_type(s: &str) -> bool {
    matches!(
        s,
        "deposit.pending"
            | "deposit.posted"
            | "deposit.voided"
            | "payment.settled"
            | "withdrawal.settled"
            | "transfer.pending"
            | "transfer.posted"
            | "transfer.voided"
            | "transfer.reversed"
    )
}
```

- [ ] **Step 3: Register the handler module**

Modify `crates/pba_service/src/api/handlers.rs` to add:

```rust
pub mod webhooks;
```

- [ ] **Step 4: Add WebhookSubscriptionNotFound error variant**

Add to `crates/pba_service/src/error.rs` in the `AppError` enum:

```rust
WebhookSubscriptionNotFound(String),
```

Add to the `Display` impl:

```rust
Self::WebhookSubscriptionNotFound(id) => write!(f, "Webhook subscription not found: {id}"),
```

Add to the `IntoResponse` impl match arm:

```rust
AppError::WebhookSubscriptionNotFound(_) => (StatusCode::NOT_FOUND, "WebhookSubscriptionNotFound"),
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p pba-service`
Expected: compiles (routes not yet wired — that's Task 7).

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/src/api/dto.rs crates/pba_service/src/api/handlers/webhooks.rs crates/pba_service/src/api/handlers.rs crates/pba_service/src/error.rs
git commit -m "feat: add webhook subscription DTOs, handlers, and error type"
```

---

### Task 6: WebhookService — event emission and HMAC signing

**Files:**
- Create: `crates/pba_service/src/service/webhook_service.rs`
- Modify: `crates/pba_service/src/service.rs`

- [ ] **Step 1: Write the WebhookService**

Create `crates/pba_service/src/service/webhook_service.rs`:

```rust
use std::sync::Arc;

use hmac::{Hmac, Mac};
use kronos_worker::client::{JobTrigger, KronosClient};
use sha2::Sha256;
use uuid::Uuid;

use crate::domain::transaction::TransactionRecord;
use crate::domain::webhook_subscription::WebhookSubscription;
use crate::repository::webhook_subscription_repo::WebhookSubscriptionRepo;

type HmacSha256 = Hmac<Sha256>;

pub struct WebhookService {
    pub subscription_repo: Arc<WebhookSubscriptionRepo>,
    pub kronos_client: Arc<dyn KronosClient>,
    pub schema_name: String,
    pub endpoint_name: String,
    pub max_delivery_attempts: i64,
}

impl WebhookService {
    pub fn new(
        subscription_repo: Arc<WebhookSubscriptionRepo>,
        kronos_client: Arc<dyn KronosClient>,
        schema_name: String,
        endpoint_name: String,
        max_delivery_attempts: i64,
    ) -> Self {
        Self {
            subscription_repo,
            kronos_client,
            schema_name,
            endpoint_name,
            max_delivery_attempts,
        }
    }

    /// Emit a webhook event for the given accounts and event type.
    /// This is called after tx.commit() — failures are logged but do not
    /// propagate to the caller.
    pub async fn emit(
        &self,
        involved_account_ids: &[Uuid],
        event_type: &str,
        txn: &TransactionRecord,
    ) {
        let subscriptions = match self
            .subscription_repo
            .find_matching(involved_account_ids, event_type)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "Failed to query webhook subscriptions");
                return;
            }
        };

        for sub in subscriptions {
            if let Err(e) = self.dispatch(&sub, event_type, txn).await {
                tracing::error!(
                    sub_id = %sub.id,
                    event_type,
                    error = %e,
                    "Failed to schedule webhook dispatch"
                );
            }
        }
    }

    async fn dispatch(
        &self,
        sub: &WebhookSubscription,
        event_type: &str,
        txn: &TransactionRecord,
    ) -> anyhow::Result<()> {
        let event_id = Uuid::now_v7();
        let body = build_event_payload(event_id, event_type, txn);
        let body_bytes = serde_json::to_vec(&body)?;
        let signature = compute_hmac(&sub.secret, &body_bytes);

        let input = serde_json::json!({
            "subscription_id": sub.id.to_string(),
            "event_id": event_id.to_string(),
            "event_type": event_type,
            "event_timestamp": chrono::Utc::now().to_rfc3339(),
            "target_url": sub.url,
            "signature": signature,
            "body": body,
        });

        let idem_key = format!("{event_id}-{}", sub.id);
        self.kronos_client
            .create_job(
                &self.schema_name,
                &self.endpoint_name,
                input,
                self.max_delivery_attempts,
                JobTrigger::Immediate,
                Some(&idem_key),
            )
            .await?;

        tracing::info!(
            event_id = %event_id,
            sub_id = %sub.id,
            event_type,
            "Webhook dispatch scheduled"
        );
        Ok(())
    }
}

/// Build the webhook event payload from a TransactionRecord.
/// All fields are always present — inapplicable fields are null.
fn build_event_payload(
    event_id: Uuid,
    event_type: &str,
    txn: &TransactionRecord,
) -> serde_json::Value {
    serde_json::json!({
        "event_id": event_id.to_string(),
        "event_type": event_type,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "data": {
            "transaction_id": txn.id.to_string(),
            "account_id": txn.account_id.to_string(),
            "account_kind": txn.account_kind.as_str(),
            "transaction_type": txn.transaction_type.as_str(),
            "status": txn.status.as_str(),
            "amount": txn.amount,
            "pool": txn.pool,
            "direction": txn.direction.as_str(),
            "gateway_ref": txn.gateway_ref,
            "correlation_id": txn.correlation_id.map(|id| id.to_string()),
            "description": txn.description,
            "funding_type": txn.funding_type,
            "merchant_id": txn.merchant_id,
            "merchant_mcc": txn.merchant_mcc,
            "source_ifsc": txn.source_ifsc,
            "source_account": txn.source_account,
            "created_at": txn.created_at.to_rfc3339(),
            "updated_at": txn.updated_at.to_rfc3339(),
        }
    })
}

/// Compute HMAC-SHA256 over the given bytes using the shared secret.
/// Returns the signature in the format `sha256=<hex>`.
fn compute_hmac(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(body);
    let result = mac.finalize();
    let code_bytes = result.into_bytes();
    format!("sha256={}", hex::encode(code_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::account_kind::AccountKind;
    use crate::domain::transaction::{
        TransactionDirection, TransactionStatus, TransactionType,
    };

    fn make_txn() -> TransactionRecord {
        TransactionRecord {
            id: Uuid::now_v7(),
            account_id: Uuid::now_v7(),
            account_kind: AccountKind::Normal,
            transaction_type: TransactionType::Transfer,
            status: TransactionStatus::Posted,
            amount: 100000,
            pool: Some("others".to_string()),
            direction: TransactionDirection::Outbound,
            source_ifsc: None,
            source_account: None,
            gateway_ref: Some("GR-123".to_string()),
            timeout_seconds: None,
            merchant_id: None,
            merchant_mcc: None,
            description: Some("test transfer".to_string()),
            funding_type: Some("trust".to_string()),
            tb_transfer_id: 0,
            idempotency_key: None,
            correlation_id: Some(Uuid::now_v7()),
            reverses_transaction_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn compute_hmac_produces_sha256_hex() {
        let secret = "test-secret";
        let body = b"{\"event_id\":\"test\"}";
        let sig = compute_hmac(secret, body);
        assert!(sig.starts_with("sha256="));
        // After "sha256=", should be 64 hex chars (256 bits)
        let hex_part = &sig[7..];
        assert_eq!(hex_part.len(), 64);
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hmac_is_deterministic() {
        let secret = "test-secret";
        let body = b"same body";
        let sig1 = compute_hmac(secret, body);
        let sig2 = compute_hmac(secret, body);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn payload_has_constant_shape() {
        let txn = make_txn();
        let event_id = Uuid::now_v7();
        let payload = build_event_payload(event_id, "transfer.posted", &txn);
        let data = payload.get("data").unwrap();
        // All data fields should be present (as value, possibly null)
        assert!(data.get("merchant_id").is_some());
        assert!(data.get("source_ifsc").is_some());
        assert!(data.get("source_account").is_some());
        // These are null for a transfer
        assert!(data.get("merchant_id").unwrap().is_null());
        assert!(data.get("source_ifsc").unwrap().is_null());
    }
}
```

- [ ] **Step 2: Register the module**

Add to `crates/pba_service/src/service.rs`:

```rust
pub mod webhook_service;
```

- [ ] **Step 3: Verify compilation and run unit tests**

Run: `cargo test -p pba-service -- webhook_service 2>&1 | tail -20`
Expected: 3 unit tests pass (hmac_produces_sha256_hex, hmac_is_deterministic, payload_has_constant_shape).

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/service/webhook_service.rs crates/pba_service/src/service.rs
git commit -m "feat: add WebhookService with HMAC signing and Kronos dispatch"
```

---

### Task 7: Internal webhook delivery endpoint

**Files:**
- Create: `crates/pba_service/src/api/handlers/internal.rs`
- Modify: `crates/pba_service/src/api/handlers.rs`

- [ ] **Step 1: Write the internal delivery handler**

Create `crates/pba_service/src/api/handlers/internal.rs`:

```rust
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use reqwest::Client;
use serde::Deserialize;

use crate::AppState;

/// Request body from Kronos when it calls back PBA to deliver a webhook.
#[derive(Debug, Deserialize)]
pub struct WebhookDeliveryRequest {
    pub subscription_id: String,
    pub event_id: String,
    pub event_type: String,
    pub event_timestamp: String,
    pub target_url: String,
    pub signature: String,
    pub body: serde_json::Value,
}

/// POST /internal/webhooks/deliver
///
/// Called by Kronos when a webhook job fires. PBA makes the outbound HTTP
/// call to the consumer and returns the result to Kronos.
pub async fn deliver_webhook(
    State(state): State<AppState>,
    Json(req): Json<WebhookDeliveryRequest>,
) -> Result<StatusCode, StatusCode> {
    let client = Client::new();

    let mut builder = client
        .post(&req.target_url)
        .header("Content-Type", "application/json")
        .header("X-PBA-Signature", &req.signature)
        .header("X-PBA-Event-ID", &req.event_id);

    builder = builder.body(serde_json::to_vec(&req.body).map_err(|_| StatusCode::BAD_REQUEST)?);

    let start = std::time::Instant::now();
    let result = builder.timeout(std::time::Duration::from_secs(10)).send().await;
    let elapsed = start.elapsed();

    match result {
        Ok(resp) => {
            let status = resp.status();
            tracing::info!(
                target_url = %req.target_url,
                event_id = %req.event_id,
                status = status.as_u16(),
                elapsed_ms = elapsed.as_millis() as u64,
                "Webhook delivered"
            );
            if status.is_success() {
                Ok(StatusCode::OK)
            } else {
                tracing::warn!(
                    target_url = %req.target_url,
                    event_id = %req.event_id,
                    status = status.as_u16(),
                    "Webhook target returned non-2xx"
                );
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
        Err(e) => {
            tracing::warn!(
                target_url = %req.target_url,
                event_id = %req.event_id,
                error = %e,
                elapsed_ms = elapsed.as_millis() as u64,
                "Webhook delivery failed"
            );
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}
```

- [ ] **Step 2: Register the handler module**

Add to `crates/pba_service/src/api/handlers.rs`:

```rust
pub mod internal;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p pba-service`
Expected: compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/src/api/handlers/internal.rs crates/pba_service/src/api/handlers.rs
git commit -m "feat: add internal webhook delivery endpoint (Kronos callback)"
```

---

### Task 8: Wire routes, AppState, and Kronos initialization

**Files:**
- Modify: `crates/pba_service/src/api/routes.rs`
- Modify: `crates/pba_service/src/main.rs`
- Modify: `crates/pba_service/src/config.rs`

- [ ] **Step 1: Add config fields**

Add to `crates/pba_service/src/config.rs` in the `AppConfig` struct:

```rust
pub kronos_encryption_key: String,
pub kronos_table_prefix: String,
pub kronos_worker_max_concurrent: usize,
pub kronos_worker_poll_interval_ms: u64,
pub webhook_max_delivery_attempts: i64,
pub pba_internal_token: String,
```

Add to `AppConfig::from_env()` the corresponding env var reads:

```rust
let kronos_encryption_key = std::env::var("KRONOS_ENCRYPTION_KEY")
    .unwrap_or_else(|_| "0000000000000000000000000000000000000000000000000000000000000000".to_string());
let kronos_table_prefix = std::env::var("KRONOS_TABLE_PREFIX")
    .unwrap_or_else(|_| "pba".to_string());
let kronos_worker_max_concurrent: usize = std::env::var("KRONOS_WORKER_MAX_CONCURRENT")
    .unwrap_or_else(|_| "50".to_string())
    .parse()
    .expect("KRONOS_WORKER_MAX_CONCURRENT must be a valid usize");
let kronos_worker_poll_interval_ms: u64 = std::env::var("KRONOS_WORKER_POLL_INTERVAL_MS")
    .unwrap_or_else(|_| "200".to_string())
    .parse()
    .expect("KRONOS_WORKER_POLL_INTERVAL_MS must be a valid u64");
let webhook_max_delivery_attempts: i64 = std::env::var("WEBHOOK_MAX_DELIVERY_ATTEMPTS")
    .unwrap_or_else(|_| "5".to_string())
    .parse()
    .expect("WEBHOOK_MAX_DELIVERY_ATTEMPTS must be a valid i64");
let pba_internal_token = std::env::var("PBA_INTERNAL_TOKEN")
    .unwrap_or_else(|_| format!("pba-internal-{}", uuid::Uuid::now_v7()));
```

Include these in the `Self { ... }` return.

- [ ] **Step 2: Add webhook fields to AppState**

Add to `AppState` in `crates/pba_service/src/main.rs`:

```rust
pub webhook_service: Arc<crate::service::webhook_service::WebhookService>,
pub webhook_subscription_repo: Arc<crate::repository::webhook_subscription_repo::WebhookSubscriptionRepo>,
pub kronos_client: Arc<dyn kronos_worker::client::KronosClient>,
```

- [ ] **Step 3: Initialize Kronos and WebhookService in main()**

After the existing service initialization block (after `transfer_service`) in `main()`, add:

```rust
// Initialize Kronos client (library mode)
let kronos_library_client = kronos_worker::client::KronosLibraryClient::new(
    pg_pool.clone(),
    &config.kronos_table_prefix,
    &config.kronos_encryption_key,
    Some(reqwest::Client::new()),
)
.expect("Failed to create Kronos library client");

let kronos_client: Arc<dyn kronos_worker::client::KronosClient> =
    Arc::new(kronos_library_client);

// Provision Kronos workspace (idempotent)
kronos_client
    .provision_workspace("pba")
    .await
    .expect("Failed to provision Kronos workspace");

// Register the PBA callback endpoint in Kronos
let callback_url = format!("http://localhost:{}/internal/webhooks/deliver", config.port);
let kronos_endpoint_spec = serde_json::json!({
    "url": callback_url,
    "method": "POST",
    "headers": {
        "Content-Type": "application/json",
        "Authorization": format!("Bearer {}", config.pba_internal_token)
    },
    "timeout_ms": 5000,
    "expected_status_codes": [200]
});
let kronos_retry_policy = serde_json::json!({
    "max_attempts": config.webhook_max_delivery_attempts,
    "backoff": "exponential",
    "initial_delay_ms": 1000,
    "max_delay_ms": 60000,
    "retry_on_status_codes": [500, 502, 503, 504]
});
kronos_client
    .register_endpoint(
        "pba",
        "pba-webhook-callback",
        "HTTP",
        kronos_endpoint_spec,
        Some(kronos_retry_policy),
    )
    .await
    .expect("Failed to register Kronos callback endpoint");

// Initialize webhook infrastructure
let webhook_subscription_repo = Arc::new(
    crate::repository::webhook_subscription_repo::WebhookSubscriptionRepo::new(pg_pool.clone()),
);
let webhook_service = Arc::new(
    crate::service::webhook_service::WebhookService::new(
        Arc::clone(&webhook_subscription_repo),
        Arc::clone(&kronos_client),
        "pba".to_string(),
        "pba-webhook-callback".to_string(),
        config.webhook_max_delivery_attempts,
    ),
);

// Start Kronos background worker
let worker_config = kronos_worker::client::WorkerConfig {
    max_concurrent: config.kronos_worker_max_concurrent,
    poll_interval_ms: config.kronos_worker_poll_interval_ms,
    ..Default::default()
};
let cancel_token = tokio_util::sync::CancellationToken::new();
// Schema provider for PBA — always just the "pba" schema
let schema_provider = kronos_common::tenant::StaticSchemaProvider::new(vec!["pba".to_string()]);
kronos_client
    .start_worker(schema_provider, cancel_token, worker_config);
```

Note: `StaticSchemaProvider` needs to be checked against the actual Kronos `SchemaProvider` trait. If it doesn't exist, implement a simple one:

```rust
use kronos_common::tenant::SchemaProvider;
use async_trait::async_trait;

struct PbaSchemaProvider;

#[async_trait]
impl SchemaProvider for PbaSchemaProvider {
    async fn active_schemas(&self) -> Vec<String> {
        vec!["pba".to_string()]
    }
}
```

Add `webhook_subscription_repo`, `webhook_service`, and `kronos_client` to the `AppState { ... }` initialization.

- [ ] **Step 4: Add webhook and internal routes**

Add to `crates/pba_service/src/api/routes.rs` in `protected_router()`:

```rust
.route("/webhooks", post(handlers::webhooks::create_webhook))
.route("/webhooks", get(handlers::webhooks::list_webhooks))
.route("/webhooks/{id}", get(handlers::webhooks::get_webhook))
.route("/webhooks/{id}", put(handlers::webhooks::update_webhook))
.route("/webhooks/{id}", delete(handlers::webhooks::delete_webhook))
```

Add the internal router (mounted without API key or admin auth) in `main.rs` where the router is assembled:

```rust
let internal = Router::new()
    .route(
        "/internal/webhooks/deliver",
        post(crate::api::handlers::internal::deliver_webhook),
    )
    .with_state(state.clone());

let inner = api::routes::public_router()
    .merge(
        api::routes::protected_router().layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::api_key::require_api_key,
        )),
    )
    .merge(internal)
    .merge(
        admin::create_router().layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::admin_auth::require_admin_session,
        )),
    )
    .layer(CookieManagerLayer::new())
    .with_state(state);
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p pba-service`
Expected: compiles without errors.

- [ ] **Step 6: Commit**

```bash
git add crates/pba_service/src/api/routes.rs crates/pba_service/src/main.rs crates/pba_service/src/config.rs
git commit -m "feat: wire webhook routes, AppState, and Kronos initialization"
```

---

### Task 9: Integrate webhook emission into existing services

**Files:**
- Modify: `crates/pba_service/src/service/pb_deposit_service.rs`
- Modify: `crates/pba_service/src/service/normal_deposit_service.rs`
- Modify: `crates/pba_service/src/service/pb_payment_service.rs`
- Modify: `crates/pba_service/src/service/pb_withdrawal_service.rs`
- Modify: `crates/pba_service/src/service/normal_withdrawal_service.rs`
- Modify: `crates/pba_service/src/service/transfer_service.rs`
- Modify: `crates/pba_service/src/service/deposit_timeout.rs`

Each service gets an `Arc<WebhookService>` field. After each `tx.commit()`, call `webhook_service.emit()`. The pattern for every service is the same:

1. Add `pub webhook_service: Arc<crate::service::webhook_service::WebhookService>` to the struct
2. Add it to `new()`
3. After each `tx.commit()` or successful operation, add the emit call

**PbDepositService** — after `tx.commit()` in `deposit()` (pending path), emit `deposit.pending` for `account_id`. After `tx.commit()` in `deposit()` (immediate path), emit `deposit.posted`. In `post_deposit()`, emit `deposit.posted`. In `void_deposit()`, emit `deposit.voided`.

**NormalDepositService** — same pattern.

**PbPaymentService** — after `tx.commit()` in `make_payment()`, emit `payment.settled` for `account_id`.

**PbWithdrawalService** — after `tx.commit()` in `withdraw()`, emit `withdrawal.settled` for `account_id`.

**NormalWithdrawalService** — after `tx.commit()` in `withdraw()`, emit `withdrawal.settled` for `account_id`.

**TransferService** — after `tx.commit()` in `transfer()`, emit for both accounts:
- pending path: `transfer.pending` for `[source_normal_id, destination_pb_id]`
- immediate path: `transfer.posted` for `[source_normal_id, destination_pb_id]`
In `post_transfer()`, emit `transfer.posted` for both. In `void_transfer()`, emit `transfer.voided` for both. In `reverse_transfer()`, emit `transfer.reversed` for both.

**Deposit timeout poller** — after voiding, emit `deposit.voided` for the account. This requires passing `WebhookService` to the poller.

- [ ] **Step 1: Add webhook_service to each service struct and constructor**

For each service file listed above, add the field and update `new()`. Example for `PbDepositService`:

```rust
pub struct PbDepositService {
    pub account_repo: Arc<PbAccountRepo>,
    pub ledger_repo: Arc<LedgerRepo>,
    pub transaction_repo: Arc<TransactionRepo>,
    pub default_timeout_seconds: u32,
    pub webhook_service: Arc<crate::service::webhook_service::WebhookService>,
}

impl PbDepositService {
    pub fn new(
        account_repo: Arc<PbAccountRepo>,
        ledger_repo: Arc<LedgerRepo>,
        transaction_repo: Arc<TransactionRepo>,
        default_timeout_seconds: u32,
        webhook_service: Arc<crate::service::webhook_service::WebhookService>,
    ) -> Self {
        Self {
            account_repo,
            ledger_repo,
            transaction_repo,
            default_timeout_seconds,
            webhook_service,
        }
    }
```

- [ ] **Step 2: Add emit calls after each tx.commit()**

For each `tx.commit()` in each service, add the corresponding emit call immediately after. Example for `PbDepositService::deposit()` pending path:

```rust
tx.commit().await?;
self.webhook_service.emit(&[account_id], "deposit.pending", &record).await;
```

For transfer operations with two accounts:

```rust
tx.commit().await?;
self.webhook_service
    .emit(&[source_normal_id, destination_pb_id], "transfer.pending", &updated_legs[0])
    .await;
```

- [ ] **Step 3: Update main.rs constructors**

Update each service construction in `main.rs` to pass `Arc::clone(&webhook_service)`.

- [ ] **Step 4: Update deposit_timeout poller**

Modify `run_deposit_timeout_poller` signature to accept `Arc<WebhookService>`. After each void operation, call `webhook_service.emit(&[txn.account_id], "deposit.voided", &txn).await`.

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p pba-service`
Expected: compiles without errors.

- [ ] **Step 6: Run existing tests**

Run: `just test`
Expected: all existing unit tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/pba_service/src/service/ crates/pba_service/src/main.rs
git commit -m "feat: integrate webhook emission into all transaction services"
```

---

### Task 10: Unit tests for WebhookSubscriptionRepo and WebhookService

**Files:**
- Modify: `crates/pba_service/src/repository/webhook_subscription_repo.rs`
- Modify: `crates/pba_service/src/service/webhook_service.rs`

- [ ] **Step 1: Add repo tests using a mock KronosClient**

Add a mock `KronosClient` implementation for testing to `crates/pba_service/src/service/webhook_service.rs`:

```rust
#[cfg(test)]
mod mock_kronos {
    use async_trait::async_trait;
    use kronos_worker::client::{JobTrigger, KronosClient};
    use kronos_common::models::Execution;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, Default)]
    pub struct MockKronosClient {
        pub jobs: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    #[async_trait]
    impl KronosClient for MockKronosClient {
        async fn upsert_secret(&self, _schema: &str, _name: &str, _plaintext: &str) -> anyhow::Result<()> { Ok(()) }
        async fn delete_secret(&self, _schema: &str, _name: &str) -> anyhow::Result<()> { Ok(()) }
        async fn register_endpoint(&self, _schema: &str, _name: &str, _type: &str, _spec: serde_json::Value, _retry: Option<serde_json::Value>) -> anyhow::Result<()> { Ok(()) }
        async fn delete_endpoint(&self, _schema: &str, _name: &str) -> anyhow::Result<()> { Ok(()) }
        async fn create_job(&self, schema: &str, endpoint: &str, input: serde_json::Value, _max: i64, _trigger: JobTrigger, idem: Option<&str>) -> anyhow::Result<String> {
            self.jobs.lock().unwrap().push(input);
            Ok(idem.unwrap_or("test").to_string())
        }
        async fn provision_workspace(&self, _schema: &str) -> anyhow::Result<()> { Ok(()) }
        async fn cancel_job(&self, _schema: &str, _job_id: &str) -> anyhow::Result<()> { Ok(()) }
        async fn get_execution(&self, _schema: &str, _id: &str) -> anyhow::Result<Option<Execution>> { Ok(None) }
    }
}
```

Add tests using the mock:

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use super::mock_kronos::MockKronosClient;
    use crate::domain::account_kind::AccountKind;
    use crate::domain::transaction::{TransactionDirection, TransactionStatus, TransactionType};

    #[tokio::test]
    async fn emit_creates_job_for_matching_subscription() {
        let mock = MockKronosClient::default();
        let sub_repo = Arc::new(WebhookSubscriptionRepo::new(/* would need test pool */));
        // This test requires a test database; for unit-level testing,
        // we verify the dispatch logic with a pre-built subscription.
        let service = WebhookService::new(
            sub_repo,
            Arc::new(mock.clone()),
            "pba".to_string(),
            "pba-webhook-callback".to_string(),
            5,
        );

        let sub = WebhookSubscription {
            id: Uuid::now_v7(),
            account_id: Some(Uuid::now_v7()),
            account_kind: Some("pb".to_string()),
            url: "https://example.com/hook".to_string(),
            secret: "test-secret".to_string(),
            subscribed_event_types: vec!["deposit.posted".to_string()],
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let txn = make_txn();
        service.dispatch(&sub, "deposit.posted", &txn).await.unwrap();

        let jobs = mock.jobs.lock().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0]["target_url"], "https://example.com/hook");
        assert_eq!(jobs[0]["event_type"], "deposit.posted");
        assert!(jobs[0]["signature"].as_str().unwrap().starts_with("sha256="));
    }

    #[tokio::test]
    async fn emit_skips_non_matching_event_type() {
        let mock = MockKronosClient::default();
        let sub = WebhookSubscription {
            id: Uuid::now_v7(),
            account_id: None,
            account_kind: None,
            url: "https://example.com/hook".to_string(),
            secret: "test-secret".to_string(),
            subscribed_event_types: vec!["payment.settled".to_string()],
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let service = WebhookService::new(
            Arc::new(WebhookSubscriptionRepo::new(/* test pool */)),
            Arc::new(mock.clone()),
            "pba".to_string(),
            "pba-webhook-callback".to_string(),
            5,
        );

        let txn = make_txn();
        // dispatch would not be called if find_matching filtered it out
        // This tests the dispatch method directly — the filtering is in find_matching (SQL)
    }
}
```

Note: Full integration tests with a real PG pool are covered in Task 11 (E2E). The unit tests here validate the dispatch and HMAC logic without a database.

- [ ] **Step 2: Run unit tests**

Run: `cargo test -p pba-service -- webhook_service 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/pba_service/src/service/webhook_service.rs
git commit -m "test: add unit tests for WebhookService dispatch and HMAC"
```

---

### Task 11: E2E test for webhook delivery

**Files:**
- Modify: E2E test files in `crates/pba_service/tests/`

This task adds a Cucumber scenario that exercises the full webhook flow: create subscription → trigger transaction → verify Kronos job created → verify webhook delivered.

- [ ] **Step 1: Add a webhook step definition**

In the existing E2E test infrastructure, add steps for:
- Creating a webhook subscription via the API
- Verifying a webhook was received (using a mock HTTP server or checking Kronos execution status)

- [ ] **Step 2: Write a Cucumber scenario**

Add a feature file scenario:

```gherkin
Scenario: Webhook fires on deposit
  Given a PB account exists with purpose "health"
  And I register a webhook for "deposit.posted" events on that account
  When I deposit 100000 into the account
  Then a webhook should be delivered to the registered URL
```

- [ ] **Step 3: Run E2E tests**

Run: `just api-e2e`
Expected: new scenario passes.

- [ ] **Step 4: Commit**

```bash
git add crates/pba_service/tests/
git commit -m "test: add E2E scenario for webhook delivery on deposit"
```

---

### Task 12: Update .env.example and README

**Files:**
- Modify: `.env.example`
- Modify: `README.md`

- [ ] **Step 1: Add new env vars to .env.example**

Append to `.env.example`:

```
# Kronos (webhook scheduler)
KRONOS_ENCRYPTION_KEY=0000000000000000000000000000000000000000000000000000000000000000
KRONOS_TABLE_PREFIX=pba
KRONOS_WORKER_MAX_CONCURRENT=50
KRONOS_WORKER_POLL_INTERVAL_MS=200
WEBHOOK_MAX_DELIVERY_ATTEMPTS=5
PBA_INTERNAL_TOKEN=
```

- [ ] **Step 2: Add webhook API to README table**

Add to the API table in `README.md`:

```markdown
| `POST` | `/webhooks` | Register a webhook subscription |
| `GET` | `/webhooks` | List webhook subscriptions |
| `GET` | `/webhooks/{id}` | Get webhook subscription |
| `PUT` | `/webhooks/{id}` | Update webhook subscription |
| `DELETE` | `/webhooks/{id}` | Delete webhook subscription |
```

- [ ] **Step 3: Commit**

```bash
git add .env.example README.md
git commit -m "docs: add webhook env vars and API endpoints to README"
```
