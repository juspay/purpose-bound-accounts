use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::account::{AccountStatus, PurposeBoundAccount};
use crate::domain::purpose::{MccEntry, PurposeType};
use crate::error::AppError;

pub struct AccountRepo {
    pool: PgPool,
}

impl AccountRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_account(
        &self,
        id: Uuid,
        holder_id: Uuid,
        purpose_code: &str,
        origin_ifsc: &str,
        origin_account_number: &str,
        tb_self_account_id: u128,
        tb_others_account_id: u128,
    ) -> Result<PurposeBoundAccount, AppError> {
        // Store u128 as NUMERIC(39) via its decimal string representation
        let tb_self_str = tb_self_account_id.to_string();
        let tb_others_str = tb_others_account_id.to_string();

        let row = sqlx::query_as::<_, AccountRow>(
            r#"
            INSERT INTO accounts (id, holder_id, purpose_code, origin_ifsc, origin_account_number,
                                  tb_self_account_id, tb_others_account_id)
            VALUES ($1, $2, $3, $4, $5, $6::numeric, $7::numeric)
            RETURNING id, holder_id, purpose_code, origin_ifsc, origin_account_number,
                      vpa, virtual_ifsc, virtual_account_number,
                      tb_self_account_id::text as tb_self_account_id,
                      tb_others_account_id::text as tb_others_account_id,
                      kyc_tier, status, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(holder_id)
        .bind(purpose_code)
        .bind(origin_ifsc)
        .bind(origin_account_number)
        .bind(&tb_self_str)
        .bind(&tb_others_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::DuplicateAccount(format!(
                    "Account already exists for origin {origin_ifsc}/{origin_account_number} with purpose {purpose_code}"
                ))
            } else {
                AppError::DatabaseError(e.to_string())
            }
        })?;

        Ok(row.into_domain())
    }

    pub async fn get_account(&self, id: Uuid) -> Result<PurposeBoundAccount, AppError> {
        let row = sqlx::query_as::<_, AccountRow>(
            r#"
            SELECT id, holder_id, purpose_code, origin_ifsc, origin_account_number,
                   vpa, virtual_ifsc, virtual_account_number,
                   tb_self_account_id::text as tb_self_account_id,
                   tb_others_account_id::text as tb_others_account_id,
                   kyc_tier, status, created_at, updated_at
            FROM accounts WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::AccountNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: AccountStatus,
    ) -> Result<PurposeBoundAccount, AppError> {
        let row = sqlx::query_as::<_, AccountRow>(
            r#"
            UPDATE accounts SET status = $2, updated_at = now()
            WHERE id = $1
            RETURNING id, holder_id, purpose_code, origin_ifsc, origin_account_number,
                      vpa, virtual_ifsc, virtual_account_number,
                      tb_self_account_id::text as tb_self_account_id,
                      tb_others_account_id::text as tb_others_account_id,
                      kyc_tier, status, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(status.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::AccountNotFound(id.to_string()))?;

        Ok(row.into_domain())
    }

    pub async fn list_purpose_types(&self) -> Result<Vec<PurposeType>, AppError> {
        let rows = sqlx::query_as::<_, MccRow>(
            r#"
            SELECT purpose_code, mcc, mcc_description
            FROM purpose_mcc_allowlist
            WHERE active = true
            ORDER BY purpose_code, mcc
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(group_by_purpose(rows))
    }

    pub async fn get_purpose_type(&self, purpose_code: &str) -> Result<PurposeType, AppError> {
        let rows = sqlx::query_as::<_, MccRow>(
            r#"
            SELECT purpose_code, mcc, mcc_description
            FROM purpose_mcc_allowlist
            WHERE purpose_code = $1 AND active = true
            ORDER BY mcc
            "#,
        )
        .bind(purpose_code)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Err(AppError::PurposeTypeNotFound(purpose_code.to_string()));
        }

        Ok(PurposeType {
            purpose_code: purpose_code.to_string(),
            allowed_mccs: rows
                .into_iter()
                .map(|r| MccEntry {
                    mcc: r.mcc,
                    description: r.mcc_description,
                })
                .collect(),
        })
    }

    pub async fn is_mcc_allowed(&self, purpose_code: &str, mcc: &str) -> Result<bool, AppError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM purpose_mcc_allowlist
            WHERE purpose_code = $1 AND mcc = $2 AND active = true
            "#,
        )
        .bind(purpose_code)
        .bind(mcc)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0 > 0)
    }
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: Uuid,
    holder_id: Uuid,
    purpose_code: String,
    origin_ifsc: String,
    origin_account_number: String,
    vpa: Option<String>,
    virtual_ifsc: Option<String>,
    virtual_account_number: Option<String>,
    tb_self_account_id: String,
    tb_others_account_id: String,
    kyc_tier: String,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl AccountRow {
    fn into_domain(self) -> PurposeBoundAccount {
        PurposeBoundAccount {
            id: self.id,
            holder_id: self.holder_id,
            purpose_code: self.purpose_code,
            origin_ifsc: self.origin_ifsc,
            origin_account_number: self.origin_account_number,
            vpa: self.vpa,
            virtual_ifsc: self.virtual_ifsc,
            virtual_account_number: self.virtual_account_number,
            tb_self_account_id: self
                .tb_self_account_id
                .parse()
                .expect("invalid tb_self_account_id in DB"),
            tb_others_account_id: self
                .tb_others_account_id
                .parse()
                .expect("invalid tb_others_account_id in DB"),
            kyc_tier: self.kyc_tier,
            status: AccountStatus::from_str(&self.status).unwrap_or(AccountStatus::Active),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MccRow {
    purpose_code: String,
    mcc: String,
    mcc_description: Option<String>,
}

fn group_by_purpose(rows: Vec<MccRow>) -> Vec<PurposeType> {
    let mut map: std::collections::BTreeMap<String, Vec<MccEntry>> =
        std::collections::BTreeMap::new();
    for row in rows {
        map.entry(row.purpose_code).or_default().push(MccEntry {
            mcc: row.mcc,
            description: row.mcc_description,
        });
    }
    map.into_iter()
        .map(|(purpose_code, allowed_mccs)| PurposeType {
            purpose_code,
            allowed_mccs,
        })
        .collect()
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        db_err.code().as_deref() == Some("23505")
    } else {
        false
    }
}
