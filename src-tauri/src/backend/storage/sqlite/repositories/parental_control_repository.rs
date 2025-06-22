use anyhow::Result;
use sqlx::Row;
use shared::ParentalControlAttempt;
use crate::backend::storage::sqlite::connection::DbConnection;

/// Repository for parental control operations
#[derive(Clone)]
pub struct ParentalControlRepository {
    db: DbConnection,
}

impl ParentalControlRepository {
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    /// Record a parental control validation attempt
    pub async fn record_parental_control_attempt(&self, attempted_value: &str, success: bool) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO parental_control_attempts (attempted_value, success)
            VALUES (?, ?)
            "#,
        )
        .bind(attempted_value)
        .bind(success)
        .execute(self.db.pool())
        .await?;
        
        Ok(result.last_insert_rowid())
    }

    /// Get parental control attempts with optional limit
    pub async fn get_parental_control_attempts(&self, limit: Option<u32>) -> Result<Vec<ParentalControlAttempt>> {
        let query = if let Some(limit) = limit {
            sqlx::query(
                r#"
                SELECT id, attempted_value, timestamp, success
                FROM parental_control_attempts
                ORDER BY id DESC
                LIMIT ?
                "#,
            )
            .bind(limit as i64)
        } else {
            sqlx::query(
                r#"
                SELECT id, attempted_value, timestamp, success
                FROM parental_control_attempts
                ORDER BY id DESC
                "#,
            )
        };

        let rows = query.fetch_all(self.db.pool()).await?;

        let attempts = rows
            .iter()
            .map(|row| ParentalControlAttempt {
                id: row.get("id"),
                attempted_value: row.get("attempted_value"),
                timestamp: row.get("timestamp"),
                success: row.get("success"),
            })
            .collect();

        Ok(attempts)
    }
} 