use anyhow::Result;
use sqlx::Row;
use shared::Child;
use crate::backend::storage::sqlite::connection::DbConnection;

/// Repository for child operations
#[derive(Clone)]
pub struct ChildRepository {
    db: DbConnection,
}

impl ChildRepository {
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    /// Store a child in the database
    pub async fn store_child(&self, child: &Child) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO children (id, name, birthdate, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&child.id)
        .bind(&child.name)
        .bind(&child.birthdate)
        .bind(&child.created_at)
        .bind(&child.updated_at)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Get a child by ID
    pub async fn get_child(&self, child_id: &str) -> Result<Option<Child>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, birthdate, created_at, updated_at
            FROM children
            WHERE id = ?
            "#,
        )
        .bind(child_id)
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(r) => Ok(Some(Child {
                id: r.get("id"),
                name: r.get("name"),
                birthdate: r.get("birthdate"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })),
            None => Ok(None),
        }
    }

    /// List all children ordered by name
    pub async fn list_children(&self) -> Result<Vec<Child>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, birthdate, created_at, updated_at
            FROM children
            ORDER BY name ASC
            "#,
        )
        .fetch_all(self.db.pool())
        .await?;

        let children = rows
            .iter()
            .map(|row| Child {
                id: row.get("id"),
                name: row.get("name"),
                birthdate: row.get("birthdate"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect();

        Ok(children)
    }

    /// Update a child in the database
    pub async fn update_child(&self, child: &Child) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE children 
            SET name = ?, birthdate = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&child.name)
        .bind(&child.birthdate)
        .bind(&child.updated_at)
        .bind(&child.id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Delete a child from the database
    pub async fn delete_child(&self, child_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM children WHERE id = ?
            "#,
        )
        .bind(child_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Get the currently active child ID
    pub async fn get_active_child(&self) -> Result<Option<String>> {
        let row = sqlx::query(
            r#"
            SELECT child_id
            FROM active_child
            WHERE id = 1
            "#,
        )
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(r) => Ok(Some(r.get("child_id"))),
            None => Ok(None),
        }
    }

    /// Set the currently active child
    pub async fn set_active_child(&self, child_id: &str) -> Result<()> {
        // First verify the child exists
        let child_exists = sqlx::query(
            r#"
            SELECT 1 FROM children WHERE id = ?
            "#,
        )
        .bind(child_id)
        .fetch_optional(self.db.pool())
        .await?
        .is_some();

        if !child_exists {
            return Err(anyhow::anyhow!("Child not found: {}", child_id));
        }

        // Use INSERT OR REPLACE to handle both initial insert and updates
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO active_child (id, child_id, updated_at)
            VALUES (1, ?, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(child_id)
        .execute(self.db.pool())
        .await?;

        Ok(())
    }
} 