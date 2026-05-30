use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction as PgTransaction};
use uuid::Uuid;

use crate::domain::transaction::{
    TransactionDirection, TransactionRecord, TransactionStatus, TransactionType,
};
use crate::error::AppError;

#[derive(Debug, Default)]
pub struct PoolSummaryExtended {
    pub self_inbound: u64,
    pub self_outbound: u64,
    pub others_inbound: u64,
    pub others_outbound: u64,
    pub pending_self_inbound: u64,
    pub pending_self_outbound: u64,
    pub pending_others_inbound: u64,
    pub pending_others_outbound: u64,
}

#[derive(Debug, Default)]
pub struct PoolSummary {
    pub self_inbound: u64,
    pub self_outbound: u64,
    pub others_inbound: u64,
    pub others_outbound: u64,
    pub pending_self: u64,
    pub pending_others: u64,
}

impl PoolSummary {
    pub fn self_balance(&self) -> u64 {
        self.self_inbound.saturating_sub(self.self_outbound)
    }

    pub fn others_balance(&self) -> u64 {
        self.others_inbound.saturating_sub(self.others_outbound)
    }

    pub fn total_balance(&self) -> u64 {
        self.self_balance() + self.others_balance()
    }
}

pub struct TransactionRepo {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct TransactionRow {
    id: Uuid,
    account_id: Uuid,
    account_kind: String,
    #[sqlx(rename = "type")]
    transaction_type: String,
    status: String,
    amount: i64,
    pool: Option<String>,
    direction: String,
    source_ifsc: Option<String>,
    source_account: Option<String>,
    gateway_ref: Option<String>,
    timeout_seconds: Option<i32>,
    merchant_id: Option<String>,
    merchant_mcc: Option<String>,
    description: Option<String>,
    funding_type: Option<String>,
    tb_transfer_id: String,
    idempotency_key: Option<String>,
    correlation_id: Option<Uuid>,
    reverses_transaction_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TransactionRow {
    fn into_domain(self) -> TransactionRecord {
        TransactionRecord {
            id: self.id,
            account_id: self.account_id,
            account_kind: crate::domain::account_kind::AccountKind::from_str(&self.account_kind)
                .unwrap_or(crate::domain::account_kind::AccountKind::Pb),
            transaction_type: TransactionType::from_str(&self.transaction_type)
                .unwrap_or(TransactionType::Deposit),
            status: TransactionStatus::from_str(&self.status).unwrap_or(TransactionStatus::Pending),
            amount: self.amount as u64,
            pool: self.pool,
            direction: TransactionDirection::from_str(&self.direction)
                .unwrap_or(TransactionDirection::Inbound),
            source_ifsc: self.source_ifsc,
            source_account: self.source_account,
            gateway_ref: self.gateway_ref,
            timeout_seconds: self.timeout_seconds.map(|s| s as u32),
            merchant_id: self.merchant_id,
            merchant_mcc: self.merchant_mcc,
            description: self.description,
            funding_type: self.funding_type,
            tb_transfer_id: self.tb_transfer_id.parse().unwrap_or(0),
            idempotency_key: self.idempotency_key,
            correlation_id: self.correlation_id,
            reverses_transaction_id: self.reverses_transaction_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl TransactionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_in_tx(
        &self,
        tx: &mut PgTransaction<'_, Postgres>,
        id: Uuid,
        account_id: Uuid,
        account_kind: crate::domain::account_kind::AccountKind,
        transaction_type: TransactionType,
        status: TransactionStatus,
        amount: u64,
        pool: Option<&str>,
        direction: TransactionDirection,
        source_ifsc: Option<&str>,
        source_account: Option<&str>,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
        merchant_id: Option<&str>,
        merchant_mcc: Option<&str>,
        description: Option<&str>,
        funding_type: Option<&str>,
        tb_transfer_id: u128,
        idempotency_key: Option<&str>,
        correlation_id: Option<Uuid>,
        reverses_transaction_id: Option<Uuid>,
    ) -> Result<TransactionRecord, AppError> {
        let tb_id_str = tb_transfer_id.to_string();
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            INSERT INTO transactions (id, account_id, account_kind, type, status, amount, pool, direction,
                                      source_ifsc, source_account, gateway_ref, timeout_seconds,
                                      merchant_id, merchant_mcc, description, funding_type,
                                      tb_transfer_id, idempotency_key, correlation_id, reverses_transaction_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17::numeric, $18, $19, $20)
            RETURNING id, account_id, account_kind, type, status, amount, pool, direction,
                      source_ifsc, source_account, gateway_ref, timeout_seconds,
                      merchant_id, merchant_mcc, description, funding_type,
                      tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                      reverses_transaction_id, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(account_id)
        .bind(account_kind.as_str())
        .bind(transaction_type.as_str())
        .bind(status.as_str())
        .bind(amount as i64)
        .bind(pool)
        .bind(direction.as_str())
        .bind(source_ifsc)
        .bind(source_account)
        .bind(gateway_ref)
        .bind(timeout_seconds.map(|s| s as i32))
        .bind(merchant_id)
        .bind(merchant_mcc)
        .bind(description)
        .bind(funding_type)
        .bind(&tb_id_str)
        .bind(idempotency_key)
        .bind(correlation_id)
        .bind(reverses_transaction_id)
        .fetch_one(tx.as_mut())
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn update_tb_transfer_id_in_tx(
        &self,
        tx: &mut PgTransaction<'_, Postgres>,
        id: Uuid,
        tb_transfer_id: u128,
    ) -> Result<(), AppError> {
        let tb_id_str = tb_transfer_id.to_string();
        sqlx::query(
            r#"UPDATE transactions SET tb_transfer_id = $2::numeric, updated_at = now() WHERE id = $1"#,
        )
        .bind(id)
        .bind(&tb_id_str)
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        new_status: TransactionStatus,
    ) -> Result<TransactionRecord, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            UPDATE transactions SET status = $2, updated_at = now()
            WHERE id = $1
            RETURNING id, account_id, account_kind, type, status, amount, pool, direction,
                      source_ifsc, source_account, gateway_ref, timeout_seconds,
                      merchant_id, merchant_mcc, description, funding_type,
                      tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                      reverses_transaction_id, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(new_status.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::TransactionNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn get_by_id(
        &self,
        id: Uuid,
        account_id: Uuid,
    ) -> Result<TransactionRecord, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE id = $1 AND account_id = $2
            "#,
        )
        .bind(id)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::TransactionNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn get_transaction(&self, id: Uuid) -> Result<TransactionRecord, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::TransactionNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn find_by_idempotency_key(
        &self,
        account_kind: crate::domain::account_kind::AccountKind,
        account_id: Uuid,
        key: &str,
    ) -> Result<Option<TransactionRecord>, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE account_kind = $1 AND account_id = $2 AND idempotency_key = $3
            "#,
        )
        .bind(account_kind.as_str())
        .bind(account_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_domain()))
    }

    pub async fn list_by_account(
        &self,
        account_kind: crate::domain::account_kind::AccountKind,
        account_id: Uuid,
        offset: i64,
        limit: i64,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
    ) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE account_kind = $1 AND account_id = $2
              AND ($5::timestamptz IS NULL OR created_at >= $5)
              AND ($6::timestamptz IS NULL OR created_at <= $6)
            ORDER BY created_at DESC, id DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(account_kind.as_str())
        .bind(account_id)
        .bind(limit)
        .bind(offset)
        .bind(from_date)
        .bind(to_date)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn count_by_account(
        &self,
        account_id: Uuid,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
    ) -> Result<i64, AppError> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM transactions
            WHERE account_id = $1
              AND ($2::timestamptz IS NULL OR created_at >= $2)
              AND ($3::timestamptz IS NULL OR created_at <= $3)
            "#,
        )
        .bind(account_id)
        .bind(from_date)
        .bind(to_date)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn list_all(
        &self,
        offset: i64,
        limit: i64,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        kind: Option<crate::domain::account_kind::AccountKind>,
    ) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE ($3::timestamptz IS NULL OR created_at >= $3)
              AND ($4::timestamptz IS NULL OR created_at <= $4)
              AND ($5::text IS NULL OR account_kind = $5)
            ORDER BY created_at DESC, id DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .bind(from_date)
        .bind(to_date)
        .bind(kind.as_ref().map(|k| k.as_str()))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn count_all(
        &self,
        from_date: Option<DateTime<Utc>>,
        to_date: Option<DateTime<Utc>>,
        kind: Option<crate::domain::account_kind::AccountKind>,
    ) -> Result<i64, AppError> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM transactions
            WHERE ($1::timestamptz IS NULL OR created_at >= $1)
              AND ($2::timestamptz IS NULL OR created_at <= $2)
              AND ($3::text IS NULL OR account_kind = $3)
            "#,
        )
        .bind(from_date)
        .bind(to_date)
        .bind(kind.as_ref().map(|k| k.as_str()))
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn pool_summary(&self) -> Result<PoolSummary, AppError> {
        let rows: Vec<(Option<String>, String, String, i64)> = sqlx::query_as(
            r#"
            SELECT pool, direction, status, COALESCE(SUM(amount), 0)::bigint AS total
            FROM transactions
            WHERE status IN ('posted', 'settled', 'pending')
            GROUP BY pool, direction, status
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut summary = PoolSummary::default();
        for (pool, direction, status, total) in rows {
            let amt = total as u64;
            let pool_str = pool.as_deref().unwrap_or("");
            match (pool_str, direction.as_str(), status.as_str()) {
                ("self", "inbound", "posted" | "settled") => summary.self_inbound += amt,
                ("self", "outbound", "posted" | "settled") => summary.self_outbound += amt,
                ("others", "inbound", "posted" | "settled") => summary.others_inbound += amt,
                ("others", "outbound", "posted" | "settled") => summary.others_outbound += amt,
                ("self", "inbound", "pending") => summary.pending_self += amt,
                ("others", "inbound", "pending") => summary.pending_others += amt,
                _ => {}
            }
        }
        Ok(summary)
    }

    pub async fn pool_summary_extended(&self) -> Result<PoolSummaryExtended, AppError> {
        let rows: Vec<(Option<String>, String, String, i64)> = sqlx::query_as(
            r#"
            SELECT pool, direction, status, COALESCE(SUM(amount), 0)::bigint AS total
            FROM transactions
            WHERE status IN ('posted', 'settled', 'pending')
            GROUP BY pool, direction, status
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut summary = PoolSummaryExtended::default();
        for (pool, direction, status, total) in rows {
            let amt = total as u64;
            let pool_str = pool.as_deref().unwrap_or("");
            match (pool_str, direction.as_str(), status.as_str()) {
                ("self", "inbound", "posted" | "settled") => summary.self_inbound += amt,
                ("self", "outbound", "posted" | "settled") => summary.self_outbound += amt,
                ("others", "inbound", "posted" | "settled") => summary.others_inbound += amt,
                ("others", "outbound", "posted" | "settled") => summary.others_outbound += amt,
                ("self", "inbound", "pending") => summary.pending_self_inbound += amt,
                ("self", "outbound", "pending") => summary.pending_self_outbound += amt,
                ("others", "inbound", "pending") => summary.pending_others_inbound += amt,
                ("others", "outbound", "pending") => summary.pending_others_outbound += amt,
                _ => {}
            }
        }
        Ok(summary)
    }

    pub async fn list_pending_by_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE account_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            "#,
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn find_timed_out_pending(&self) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE status = 'pending'
              AND timeout_seconds IS NOT NULL
              AND created_at + timeout_seconds * interval '1 second' < now()
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    #[allow(dead_code)]
    pub async fn find_by_correlation_id(
        &self,
        correlation_id: Uuid,
    ) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE correlation_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(correlation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    /// Find the reversal row (if any) whose `reverses_transaction_id` matches the given
    /// original transfer's source-side row id. Returns the normal-side reversal row
    /// (the only row in a reversal pair that carries `reverses_transaction_id`).
    #[allow(dead_code)]
    pub async fn find_reversal_of(
        &self,
        original_source_id: Uuid,
    ) -> Result<Option<TransactionRecord>, AppError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE reverses_transaction_id = $1
            "#,
        )
        .bind(original_source_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into_domain()))
    }

    /// Return every refund row (or, in general, every row whose
    /// `reverses_transaction_id` matches `original_row_id`). Used by payment
    /// refund history and by `sum_refunds_of` consumers that want the rows
    /// themselves. Type-agnostic — works for transfer reversal too.
    pub async fn find_refunds_of(
        &self,
        original_row_id: Uuid,
    ) -> Result<Vec<TransactionRecord>, AppError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            r#"
            SELECT id, account_id, account_kind, type, status, amount, pool, direction,
                   source_ifsc, source_account, gateway_ref, timeout_seconds,
                   merchant_id, merchant_mcc, description, funding_type,
                   tb_transfer_id::text as tb_transfer_id, idempotency_key, correlation_id,
                   reverses_transaction_id, created_at, updated_at
            FROM transactions
            WHERE reverses_transaction_id = $1
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .bind(original_row_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    /// Sum the `amount` of every row whose `reverses_transaction_id` matches.
    /// Type-agnostic. Used by `pb_payment_service::refund_payment` to compute
    /// per-pool remaining-unrefunded. Returns 0 when no rows match.
    pub async fn sum_refunds_of(&self, original_row_id: Uuid) -> Result<u64, AppError> {
        let row: (Option<i64>,) = sqlx::query_as(
            r#"SELECT COALESCE(SUM(amount), 0)::bigint
               FROM transactions
               WHERE reverses_transaction_id = $1"#,
        )
        .bind(original_row_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0.unwrap_or(0) as u64)
    }
}
