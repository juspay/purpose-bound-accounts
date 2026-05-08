#![allow(dead_code)]

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::account::AccountStatus;
use crate::domain::banking::{AccountNumber, Ifsc};
use crate::domain::normal_account::NormalAccount;
use crate::error::AppError;

pub struct NormalAccountRepo {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct NormalAccountRow {
    id: Uuid,
    holder_id: String,
    origin_ifsc: Option<Ifsc>,
    origin_account_number: Option<AccountNumber>,
    vpa: Option<String>,
    virtual_ifsc: Option<Ifsc>,
    virtual_account_number: Option<AccountNumber>,
    tb_account_id: String,
    kyc_tier: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl NormalAccountRow {
    fn into_domain(self) -> NormalAccount {
        NormalAccount {
            id: self.id,
            holder_id: self.holder_id,
            origin_ifsc: self.origin_ifsc,
            origin_account_number: self.origin_account_number,
            vpa: self.vpa,
            virtual_ifsc: self.virtual_ifsc,
            virtual_account_number: self.virtual_account_number,
            tb_account_id: self
                .tb_account_id
                .parse()
                .expect("invalid tb_account_id in DB"),
            kyc_tier: self.kyc_tier,
            status: AccountStatus::from_str(&self.status).unwrap_or(AccountStatus::Active),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl NormalAccountRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_account(
        &self,
        id: Uuid,
        holder_id: &str,
        origin_ifsc: Option<&Ifsc>,
        origin_account_number: Option<&AccountNumber>,
        tb_account_id: u128,
    ) -> Result<NormalAccount, AppError> {
        let tb_id_str = tb_account_id.to_string();
        let row = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            INSERT INTO normal_accounts (id, holder_id, origin_ifsc, origin_account_number, tb_account_id)
            VALUES ($1, $2, $3, $4, $5::numeric)
            RETURNING id, holder_id, origin_ifsc, origin_account_number,
                      vpa, virtual_ifsc, virtual_account_number,
                      tb_account_id::text as tb_account_id,
                      kyc_tier, status, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(holder_id)
        .bind(origin_ifsc)
        .bind(origin_account_number)
        .bind(&tb_id_str)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into_domain())
    }

    pub async fn get_account(&self, id: Uuid) -> Result<NormalAccount, AppError> {
        let row = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            SELECT id, holder_id, origin_ifsc, origin_account_number,
                   vpa, virtual_ifsc, virtual_account_number,
                   tb_account_id::text as tb_account_id,
                   kyc_tier, status, created_at, updated_at
            FROM normal_accounts WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NormalAccountNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn list_accounts(&self) -> Result<Vec<NormalAccount>, AppError> {
        let rows = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            SELECT id, holder_id, origin_ifsc, origin_account_number,
                   vpa, virtual_ifsc, virtual_account_number,
                   tb_account_id::text as tb_account_id,
                   kyc_tier, status, created_at, updated_at
            FROM normal_accounts
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AccountStatus,
    ) -> Result<NormalAccount, AppError> {
        let row = sqlx::query_as::<_, NormalAccountRow>(
            r#"
            UPDATE normal_accounts SET status = $2, updated_at = now()
            WHERE id = $1
            RETURNING id, holder_id, origin_ifsc, origin_account_number,
                      vpa, virtual_ifsc, virtual_account_number,
                      tb_account_id::text as tb_account_id,
                      kyc_tier, status, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(status.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NormalAccountNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn count_accounts_by_status(&self) -> Result<Vec<(String, i64)>, AppError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT status, COUNT(*) as count
            FROM normal_accounts
            GROUP BY status
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
