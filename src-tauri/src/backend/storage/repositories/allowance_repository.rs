use anyhow::Result;
use sqlx::Row;
use shared::AllowanceConfig;
use crate::backend::storage::connection::DbConnection;

/// Repository for allowance configuration operations
#[derive(Clone)]
pub struct AllowanceRepository {
    db: DbConnection,
}

impl AllowanceRepository {
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    /// Store an allowance configuration in the database
    pub async fn store_allowance_config(&self, config: &AllowanceConfig) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO allowance_configs (id, child_id, amount, day_of_week, is_active, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&config.id)
        .bind(&config.child_id)
        .bind(config.amount)
        .bind(config.day_of_week as i64)
        .bind(config.is_active)
        .bind(&config.created_at)
        .bind(&config.updated_at)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Get allowance configuration for a specific child
    pub async fn get_allowance_config(&self, child_id: &str) -> Result<Option<AllowanceConfig>> {
        let row = sqlx::query(
            r#"
            SELECT id, child_id, amount, day_of_week, is_active, created_at, updated_at
            FROM allowance_configs
            WHERE child_id = ?
            "#,
        )
        .bind(child_id)
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(r) => Ok(Some(AllowanceConfig {
                id: r.get("id"),
                child_id: r.get("child_id"),
                amount: r.get("amount"),
                day_of_week: r.get::<i64, _>("day_of_week") as u8,
                is_active: r.get("is_active"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })),
            None => Ok(None),
        }
    }

    /// Get all allowance configurations (for admin purposes)
    pub async fn list_allowance_configs(&self) -> Result<Vec<AllowanceConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, child_id, amount, day_of_week, is_active, created_at, updated_at
            FROM allowance_configs
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(self.db.pool())
        .await?;

        let configs = rows
            .iter()
            .map(|row| AllowanceConfig {
                id: row.get("id"),
                child_id: row.get("child_id"),
                amount: row.get("amount"),
                day_of_week: row.get::<i64, _>("day_of_week") as u8,
                is_active: row.get("is_active"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect();

        Ok(configs)
    }

    /// Delete allowance configuration for a specific child
    pub async fn delete_allowance_config(&self, child_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM allowance_configs WHERE child_id = ?"
        )
        .bind(child_id)
        .execute(self.db.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }
} 