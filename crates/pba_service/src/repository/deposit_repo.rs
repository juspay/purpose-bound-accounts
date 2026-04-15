use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::deposit::{DepositRecord, DepositStatus};
use crate::error::AppError;

pub struct DepositRepo {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct DepositRow {
    id: Uuid,
    account_id: Uuid,
    amount: i64,
    pool: String,
    source_ifsc: String,
    source_account: String,
    status: String,
    tb_transfer_id: String,
    gateway_ref: Option<String>,
    timeout_seconds: Option<i32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl DepositRow {
    fn into_domain(self) -> DepositRecord {
        DepositRecord {
            id: self.id,
            account_id: self.account_id,
            amount: self.amount as u64,
            pool: self.pool,
            source_ifsc: self.source_ifsc,
            source_account: self.source_account,
            status: DepositStatus::from_str(&self.status).unwrap_or(DepositStatus::Pending),
            tb_transfer_id: self.tb_transfer_id.parse().expect("invalid tb_transfer_id in DB"),
            gateway_ref: self.gateway_ref,
            timeout_seconds: self.timeout_seconds.map(|s| s as u32),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl DepositRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(
        &self,
        id: Uuid,
        account_id: Uuid,
        amount: u64,
        pool: &str,
        source_ifsc: &str,
        source_account: &str,
        status: DepositStatus,
        tb_transfer_id: u128,
        gateway_ref: Option<&str>,
        timeout_seconds: Option<u32>,
    ) -> Result<DepositRecord, AppError> {
        let tb_id_str = tb_transfer_id.to_string();
        let row = sqlx::query_as::<_, DepositRow>(
            r#"
            INSERT INTO deposits (id, account_id, amount, pool, source_ifsc, source_account,
                                  status, tb_transfer_id, gateway_ref, timeout_seconds)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8::numeric, $9, $10)
            RETURNING id, account_id, amount, pool, source_ifsc, source_account,
                      status, tb_transfer_id::text as tb_transfer_id,
                      gateway_ref, timeout_seconds, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(account_id)
        .bind(amount as i64)
        .bind(pool)
        .bind(source_ifsc)
        .bind(source_account)
        .bind(status.as_str())
        .bind(&tb_id_str)
        .bind(gateway_ref)
        .bind(timeout_seconds.map(|s| s as i32))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn get_by_id(
        &self,
        deposit_id: Uuid,
        account_id: Uuid,
    ) -> Result<DepositRecord, AppError> {
        let row = sqlx::query_as::<_, DepositRow>(
            r#"
            SELECT id, account_id, amount, pool, source_ifsc, source_account,
                   status, tb_transfer_id::text as tb_transfer_id,
                   gateway_ref, timeout_seconds, created_at, updated_at
            FROM deposits
            WHERE id = $1 AND account_id = $2
            "#,
        )
        .bind(deposit_id)
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::DepositNotFound(deposit_id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn update_status(
        &self,
        deposit_id: Uuid,
        new_status: DepositStatus,
    ) -> Result<DepositRecord, AppError> {
        let row = sqlx::query_as::<_, DepositRow>(
            r#"
            UPDATE deposits SET status = $2, updated_at = now()
            WHERE id = $1
            RETURNING id, account_id, amount, pool, source_ifsc, source_account,
                      status, tb_transfer_id::text as tb_transfer_id,
                      gateway_ref, timeout_seconds, created_at, updated_at
            "#,
        )
        .bind(deposit_id)
        .bind(new_status.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::DepositNotFound(deposit_id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn activate_pending(
        &self,
        deposit_id: Uuid,
        tb_transfer_id: u128,
    ) -> Result<DepositRecord, AppError> {
        let tb_id_str = tb_transfer_id.to_string();
        let row = sqlx::query_as::<_, DepositRow>(
            r#"
            UPDATE deposits SET status = 'pending', tb_transfer_id = $2::numeric, updated_at = now()
            WHERE id = $1 AND status = 'created'
            RETURNING id, account_id, amount, pool, source_ifsc, source_account,
                      status, tb_transfer_id::text as tb_transfer_id,
                      gateway_ref, timeout_seconds, created_at, updated_at
            "#,
        )
        .bind(deposit_id)
        .bind(&tb_id_str)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::DepositNotFound(deposit_id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn list_pending_by_account(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<DepositRecord>, AppError> {
        let rows = sqlx::query_as::<_, DepositRow>(
            r#"
            SELECT id, account_id, amount, pool, source_ifsc, source_account,
                   status, tb_transfer_id::text as tb_transfer_id,
                   gateway_ref, timeout_seconds, created_at, updated_at
            FROM deposits
            WHERE account_id = $1 AND status = 'pending'
            ORDER BY created_at DESC
            "#,
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn find_timed_out_pending(&self) -> Result<Vec<DepositRecord>, AppError> {
        let rows = sqlx::query_as::<_, DepositRow>(
            r#"
            SELECT id, account_id, amount, pool, source_ifsc, source_account,
                   status, tb_transfer_id::text as tb_transfer_id,
                   gateway_ref, timeout_seconds, created_at, updated_at
            FROM deposits
            WHERE status = 'pending'
              AND timeout_seconds IS NOT NULL
              AND created_at + timeout_seconds * interval '1 second' < now()
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }
}
