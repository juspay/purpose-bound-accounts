# Webhook Invocation on Transaction Events — Design

**Date:** 2026-05-31
**Status:** Draft

## Goal

Add webhook invocation infrastructure so that external consumers can receive
real-time notifications when transaction state changes occur on any account
(PB or normal). Events cover all transaction types — deposits, payments,
withdrawals, transfers, and reversals. Delivery is durable and retried via
Kronos, with PBA retaining full ownership of the outbound HTTP call for
observability.

## Background

PBA currently has no event or notification mechanism. All state changes are
recorded in the PostgreSQL `transactions` table and TigerBeetle ledger, but
external systems have no way to learn about them except by polling the API.

Kronos (`juspay/kronos`, `feat/library-compatible` branch) provides a Rust
library client (`KronosLibraryClient`) that embeds a durable job scheduler
into the calling application. It supports immediate, delayed, and cron job
triggers with at-least-once delivery, exponential backoff retries, and full
execution observability. PBA will use Kronos in library mode — same PG
instance, same process — as a pure scheduler that calls back into PBA.

## Non-goals

- **Transactional outbox.** Webhook dispatches happen after the business
  transaction commits. There is a brief crash window (between `tx.commit()` and
  the Kronos `create_job` call) where an event can be lost. This is accepted
  as a trade-off for simplicity.
- **Exactly-once delivery.** Kronos provides at-least-once. Consumers must
  handle duplicate deliveries via the `event_id` field.
- **Webhook delivery status API.** No endpoint to query delivery status —
  Kronos's own execution/attempt APIs serve that purpose.
- **CRON or delayed webhooks.** All webhooks fire immediately. Kronos's
  delayed/cron triggers are not used.
- **Event replay.** No mechanism to replay historical events.

## Event Surface

All transaction state changes emit events. The event type follows the pattern
`{transaction_type}.{status}`:

| Event Type | Triggered By | When |
|---|---|---|
| `deposit.pending` | PbDepositService, NormalDepositService | Pending deposit created |
| `deposit.posted` | PbDepositService, NormalDepositService, deposit timeout poller | Deposit confirmed |
| `deposit.voided` | PbDepositService, NormalDepositService, deposit timeout poller | Deposit voided or timed out |
| `payment.settled` | PbPaymentService | Payment completed |
| `withdrawal.settled` | PbWithdrawalService, NormalWithdrawalService | Withdrawal completed |
| `transfer.pending` | TransferService | Pending transfer created |
| `transfer.posted` | TransferService | Transfer confirmed |
| `transfer.voided` | TransferService | Transfer voided |
| `transfer.reversed` | TransferService | Transfer reversed |

For **cross-account transactions** (transfers, reversals), both the source and
destination accounts emit separate events. A transfer from normal account A to
PB account B fires `transfer.pending` for A and `transfer.pending` for B.

## Event Payload

Each event carries a fat payload — consumers do not need to call back to get
details. The payload is JSON, signed with HMAC-SHA256.

```json
{
  "event_id": "0196a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b",
  "event_type": "transfer.posted",
  "timestamp": "2026-05-31T10:00:00Z",
  "data": {
    "transaction_id": "uuid",
    "account_id": "uuid",
    "account_kind": "normal",
    "transaction_type": "transfer",
    "status": "posted",
    "amount": 100000,
    "pool": "others",
    "direction": "outbound",
    "gateway_ref": "GR-123",
    "correlation_id": "uuid",
    "description": "Monthly funding",
    "funding_type": "trust",
    "merchant_id": null,
    "merchant_mcc": null,
    "source_ifsc": null,
    "source_account": null,
    "created_at": "2026-05-31T10:00:00Z",
    "updated_at": "2026-05-31T10:00:00Z"
  }
}
```

All fields are always present. Inapplicable fields are `null` — e.g.,
`merchant_id` and `merchant_mcc` are `null` for deposits and transfers,
`source_ifsc` and `source_account` are `null` for payments and withdrawals.

The `data` object mirrors `TransactionRecord` fields. All fields are always
present — inapplicable fields carry `null` rather than being omitted (e.g.,
`merchant_id` is `null` for deposits, `source_ifsc` is `null` for payments).
This gives the payload a constant shape, making deserialization and schema
evolution simpler for consumers.

## Webhook Registration

### Layered model: Global + Per-Account

**Global webhooks** (`account_id = NULL`) fire for all matching events across
all accounts.

**Per-account webhooks** (`account_id = <specific UUID>`) fire for events on
that account. For cross-account transactions, per-account webhooks fire for
**both sides** — e.g., a webhook on PB account B fires when a transfer arrives
into B, and a webhook on normal account A fires when the same transfer departs
from A.

### Event type filtering

Each subscription specifies which event types it subscribes to via a
`subscribed_event_types` array. Only matching events are dispatched.

### Schema

```sql
CREATE TABLE webhook_subscriptions (
    id                       UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    account_id               UUID        NULL,
    account_kind             TEXT        NULL CHECK (account_kind IN ('pb', 'normal')),
    url                      TEXT        NOT NULL,
    secret                   TEXT        NOT NULL,  -- HMAC-SHA256 signing key
    subscribed_event_types   TEXT[]      NOT NULL,  -- e.g. '{"transfer.posted","deposit.voided"}'
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

The `account_id` column has no foreign key constraint. It references either a
`pb_accounts.id` or a `normal_accounts.id`, distinguished by the
`account_kind` column. Application-level validation ensures the referenced
account exists when creating a subscription. A subscription is either global
(NULL `account_id`) or scoped to exactly one account.

### API

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `/webhooks` | API key | Register a webhook subscription |
| `GET` | `/webhooks` | API key | List subscriptions (filter: `?account_id=`) |
| `GET` | `/webhooks/{id}` | API key | Get subscription details |
| `PUT` | `/webhooks/{id}` | API key | Update subscription |
| `DELETE` | `/webhooks/{id}` | API key | Delete subscription |

**Create request:**

```json
{
  "account_id": null,
  "url": "https://consumer.example.com/hooks/pba",
  "secret": "whsec_abc123...",
  "subscribed_event_types": ["transfer.posted", "transfer.reversed", "deposit.posted"]
}
```

When `account_id` is null or absent, the subscription is global. The `secret`
is stored as-is in PG (the table should be access-controlled; future work may
encrypt at rest via PBA's existing secrets provider).

**Response (201):**

```json
{
  "id": "uuid",
  "account_id": null,
  "url": "https://consumer.example.com/hooks/pba",
  "subscribed_event_types": ["transfer.posted", "transfer.reversed", "deposit.posted"],
  "is_active": true,
  "created_at": "2026-05-31T10:00:00Z",
  "updated_at": "2026-05-31T10:00:00Z"
}
```

The `secret` is never returned in responses (write-only, like Kronos secrets).

### Event matching logic

When an event is emitted for a set of involved account IDs:

1. Collect all active subscriptions where:
   - `account_id IS NULL` (global), OR
   - `account_id = ANY(involved_account_ids)` (per-account match)
2. Filter by `subscribed_event_types` containing the emitted event type
3. Deduplicate (a global subscription and a per-account subscription with the
   same URL receive separate deliveries — they are distinct subscriptions)

## Kronos Integration

### Architecture: Kronos as scheduler, PBA as deliverer

Kronos is used purely as a durable scheduler. It does not call the webhook
target directly. Instead:

1. PBA emits an event → creates a Kronos IMMEDIATE job
2. Kronos fires the job → HTTP POST back to PBA's internal endpoint
3. PBA makes the outbound webhook call, signs the payload, logs the result
4. PBA returns HTTP status to Kronos: 200 on success, 503 on failure
5. Kronos retries on failure per its retry policy

This gives PBA full observability over every outbound webhook call (in PBA's
traces and logs) while Kronos handles durability, scheduling, and retry
orchestration.

### Startup sequence

1. Create `KronosLibraryClient::new(pg_pool, "pba", encryption_key, Some(reqwest_client))`
2. Call `kronos.provision_workspace("pba")` — creates Kronos tables in the
   `pba` schema (idempotent)
3. Register a Kronos endpoint via `kronos.register_endpoint()`:

```json
{
  "name": "pba-webhook-callback",
  "type": "HTTP",
  "spec": {
    "url": "http://localhost:{PORT}/internal/webhooks/deliver",
    "method": "POST",
    "headers": {
      "Content-Type": "application/json",
      "Authorization": "Bearer {{secret.pba_internal_token}}"
    },
    "timeout_ms": 5000,
    "expected_status_codes": [200]
  },
  "retry_policy": {
    "max_attempts": 5,
    "backoff": "exponential",
    "initial_delay_ms": 1000,
    "max_delay_ms": 60000,
    "retry_on_status_codes": [500, 502, 503, 504]
  }
}
```

4. Store a Kronos secret for the internal callback auth token via
   `kronos.upsert_secret("pba", "pba_internal_token", <generated_token>)`
5. Start the Kronos background worker via
   `kronos.start_worker(schema_provider, cancel_token, worker_config)`

### Per-event dispatch flow

After `tx.commit()` in a service method:

1. `webhook_service.emit(involved_account_ids, event_type, transaction_record)`
2. `WebhookService` queries matching `webhook_subscriptions`
3. For each match, calls `kronos.create_job("pba", "pba-webhook-callback", input, max_attempts=5, trigger=Immediate, idempotency_key=Some(&format!("{event_id}-{subscription_id}")))`
4. The job `input`:

```json
{
  "subscription_id": "uuid",
  "event_id": "uuid",
  "event_type": "transfer.posted",
  "event_timestamp": "2026-05-31T10:00:00Z",
  "target_url": "https://consumer.example.com/hooks/pba",
  "signature": "sha256=<hex-hmac>",
  "body": { "event_id": "...", "event_type": "...", "timestamp": "...", "data": { ... } }
}
```

All data is computed upfront and stored in the job input. When Kronos calls
back, PBA just needs to read the input and make the outbound call — no DB
lookups required.

### Internal delivery endpoint

`POST /internal/webhooks/deliver` — not externally accessible (protected by
internal auth token in `Authorization` header).

Handler logic:

1. Validate the `Authorization: Bearer <pba_internal_token>` header
2. Read the job input from the request body
3. Make an outbound HTTP POST to `input.target_url` with:
   - Header `Content-Type: application/json`
   - Header `X-PBA-Signature: {input.signature}`
   - Header `X-PBA-Event-ID: {input.event_id}`
   - Body: `input.body` (the full JSON payload)
4. If the target responds 2xx → return 200 to Kronos (delivery complete)
5. If the target responds non-2xx or times out → return 503 to Kronos
   (triggers retry)
6. Log the delivery attempt with target URL, status code, latency

### HMAC-SHA256 signature

Computed by `WebhookService` at dispatch time (before creating the Kronos job):

```
body_bytes = serde_json::to_vec(&body)    // the exact JSON that will be sent
signature  = HMAC-SHA256(secret, body_bytes)
header     = "sha256={hex(signature)}"
```

The `body` JSON is serialized once and the resulting bytes are both stored in
the Kronos job input (as the `body` field) and used as the HMAC input. When
Kronos calls back PBA and PBA forwards the webhook, it sends the same `body`
bytes to the target. The consumer verifies by computing HMAC-SHA256 over the
raw HTTP request body using their copy of the secret, then comparing against
the `X-PBA-Signature` header.

## Service Layer Changes

### New: `WebhookService`

```rust
pub struct WebhookService {
    pub subscription_repo: Arc<WebhookSubscriptionRepo>,
    pub kronos_client: Arc<dyn KronosClient>,
    pub schema_name: String,           // "pba"
    pub endpoint_name: String,         // "pba-webhook-callback"
    pub max_delivery_attempts: i64,    // default 5
}
```

`emit()` method:

```rust
pub async fn emit(
    &self,
    involved_account_ids: &[Uuid],
    event_type: &str,
    txn: &TransactionRecord,
) {
    let subscriptions = self.subscription_repo
        .find_matching(involved_account_ids, event_type)
        .await
        .unwrap_or_default();

    for sub in subscriptions {
        let event_id = Uuid::now_v7();
        let body = build_event_payload(event_id, event_type, txn);
        let signature = compute_hmac(&sub.secret, &body);

        let input = serde_json::json!({
            "subscription_id": sub.id,
            "event_id": event_id,
            "event_type": event_type,
            "event_timestamp": chrono::Utc::now(),
            "target_url": sub.url,
            "signature": signature,
            "body": body,
        });

        let idem_key = format!("{event_id}-{}", sub.id);
        match self.kronos_client
            .create_job(&self.schema_name, &self.endpoint_name, input,
                        self.max_delivery_attempts, JobTrigger::Immediate,
                        Some(&idem_key))
            .await
        {
            Ok(_) => tracing::info!(event_id=%event_id, sub_id=%sub.id, "Webhook dispatch scheduled"),
            Err(e) => tracing::error!(event_id=%event_id, sub_id=%sub.id, error=%e, "Failed to schedule webhook dispatch"),
        }
    }
}
```

### Modifications to existing services

Each service calls `webhook_service.emit()` after `tx.commit()`. The
`WebhookService` is added to `AppState` and passed to each service that needs
it.

**PbDepositService:**
- `deposit()` (pending path): emit `deposit.pending` for `account_id`
- `deposit()` (immediate path): emit `deposit.posted` for `account_id`
- `post_deposit()`: emit `deposit.posted` for `account_id`
- `void_deposit()`: emit `deposit.voided` for `account_id`

**NormalDepositService:** same pattern as PbDepositService.

**PbPaymentService:**
- `make_payment()`: emit `payment.settled` for `account_id`

**PbWithdrawalService:**
- `withdraw()`: emit `withdrawal.settled` for `account_id`

**NormalWithdrawalService:**
- `withdraw()`: emit `withdrawal.settled` for `account_id`

**TransferService:**
- `transfer()` (pending): emit `transfer.pending` for both `source_normal_id`
  and `destination_pb_id`
- `transfer()` (immediate): emit `transfer.posted` for both accounts
- `post_transfer()`: emit `transfer.posted` for both accounts
- `void_transfer()`: emit `transfer.voided` for both accounts
- `reverse_transfer()`: emit `transfer.reversed` for both accounts

**Deposit timeout poller** (`deposit_timeout.rs`):
- When voiding a timed-out deposit: emit `deposit.voided` for the account

### AppState changes

```rust
pub struct AppState {
    // ... existing fields ...
    pub webhook_service: Arc<WebhookService>,
    pub kronos_client: Arc<dyn KronosClient>,
}
```

### API routes

The existing `api/routes.rs` adds webhook CRUD under the protected router.
The internal delivery endpoint is mounted separately, protected by the
internal auth token (not by API key or admin session).

```rust
// Protected (API key auth)
let protected = Router::new()
    // ... existing routes ...
    .route("/webhooks", post(handlers::webhooks::create))
    .route("/webhooks", get(handlers::webhooks::list))
    .route("/webhooks/{id}", get(handlers::webhooks::get))
    .route("/webhooks/{id}", put(handlers::webhooks::update))
    .route("/webhooks/{id}", delete(handlers::webhooks::delete));

// Internal (token auth)
let internal = Router::new()
    .route("/internal/webhooks/deliver", post(handlers::webhooks::deliver));
```

## Configuration

New environment variables:

| Variable | Default | Description |
|---|---|---|
| `KRONOS_ENCRYPTION_KEY` | 64 zeros hex | AES-256 key for Kronos secrets |
| `KRONOS_TABLE_PREFIX` | `pba` | Prefix for Kronos tables in PG |
| `KRONOS_WORKER_MAX_CONCURRENT` | `50` | Max concurrent Kronos job executions |
| `KRONOS_WORKER_POLL_INTERVAL_MS` | `200` | Kronos worker poll interval |
| `WEBHOOK_MAX_DELIVERY_ATTEMPTS` | `5` | Max retry attempts per webhook delivery |
| `PBA_INTERNAL_TOKEN` | (generated) | Auth token for Kronos → PBA callback |

## Smithy Model Changes

Add a `WebhookSubscription` resource and operations to the Smithy model:

```smithy
resource WebhookSubscription {
    identifiers: { id: String },
    properties: {
        accountId: String,
        url: String,
        subscribedEventTypes: List,
        isActive: Boolean,
        createdAt: Timestamp,
        updatedAt: Timestamp
    }
    create: CreateWebhookSubscription,
    read: GetWebhookSubscription,
    update: UpdateWebhookSubscription,
    delete: DeleteWebhookSubscription,
    list: ListWebhookSubscriptions,
}
```

The generated SDK client gains corresponding types and methods.

## Testing Strategy

**Unit tests:**
- `WebhookService::emit()` with mock `KronosClient` — verify correct job
  creation for matching subscriptions, no dispatch for non-matching
- HMAC computation round-trip (sign + verify)
- Event matching logic (global vs per-account, event type filtering)

**Integration tests:**
- End-to-end: create subscription → trigger transaction → verify Kronos job
  created → verify delivery endpoint called → verify outbound webhook sent
- Idempotency: same `event_id + subscription_id` key creates only one delivery
- Retry: outbound target returns 500 → verify Kronos retries → target returns
  200 on second attempt

**E2E tests (Cucumber):**
- New scenarios for webhook subscription CRUD
- New scenarios for event delivery on each transaction type
