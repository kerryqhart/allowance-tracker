use anyhow::Result;
use sqlx::Row;
use shared::{Transaction, TransactionType};
use crate::backend::storage::connection::DbConnection;

/// Repository for transaction operations
#[derive(Clone)]
pub struct TransactionRepository {
    db: DbConnection,
}

impl TransactionRepository {
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    /// Store a transaction in the database
    pub async fn store_transaction(&self, transaction: &Transaction) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO transactions (id, child_id, date, description, amount, balance)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&transaction.id)
        .bind(&transaction.child_id)
        .bind(&transaction.date)
        .bind(&transaction.description)
        .bind(transaction.amount)
        .bind(transaction.balance)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Get the most recent transaction for a specific child (for calculating next balance)
    pub async fn get_latest_transaction(&self, child_id: &str) -> Result<Option<Transaction>> {
        let row = sqlx::query(
            r#"
            SELECT id, child_id, date, description, amount, balance
            FROM transactions
            WHERE child_id = ?
            ORDER BY ROWID DESC
            LIMIT 1
            "#,
        )
        .bind(child_id)
        .fetch_optional(self.db.pool())
        .await?;

        match row {
            Some(r) => Ok(Some(Transaction {
                id: r.get("id"),
                child_id: r.get("child_id"),
                date: r.get("date"),
                description: r.get("description"),
                amount: r.get("amount"),
                balance: r.get("balance"),
                transaction_type: if r.get::<f64, _>("amount") >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
            })),
            None => Ok(None),
        }
    }

    /// List transactions for a specific child with pagination support
    pub async fn list_transactions(
        &self,
        child_id: &str,
        limit: u32,
        after_id: Option<&str>,
    ) -> Result<Vec<Transaction>> {
        let query = if let Some(after_id) = after_id {
            sqlx::query(
                r#"
                SELECT id, child_id, date, description, amount, balance
                FROM transactions
                WHERE child_id = ? AND ROWID < (
                    SELECT ROWID FROM transactions WHERE id = ?
                )
                ORDER BY ROWID DESC
                LIMIT ?
                "#,
            )
            .bind(child_id)
            .bind(after_id)
            .bind(limit as i64)
        } else {
            sqlx::query(
                r#"
                SELECT id, child_id, date, description, amount, balance
                FROM transactions
                WHERE child_id = ?
                ORDER BY ROWID DESC
                LIMIT ?
                "#,
            )
            .bind(child_id)
            .bind(limit as i64)
        };

        let rows = query.fetch_all(self.db.pool()).await?;

        let transactions = rows
            .iter()
            .map(|row| Transaction {
                id: row.get("id"),
                child_id: row.get("child_id"),
                date: row.get("date"),
                description: row.get("description"),
                amount: row.get("amount"),
                balance: row.get("balance"),
                transaction_type: if row.get::<f64, _>("amount") >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
            })
            .collect();

        Ok(transactions)
    }

    /// Delete multiple transactions by their IDs for a specific child
    pub async fn delete_transactions(&self, child_id: &str, transaction_ids: &[String]) -> Result<usize> {
        if transaction_ids.is_empty() {
            return Ok(0);
        }

        // Create placeholders for the IN clause
        let placeholders = transaction_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query_str = format!(
            "DELETE FROM transactions WHERE child_id = ? AND id IN ({})",
            placeholders
        );

        let mut query = sqlx::query(&query_str);
        query = query.bind(child_id);
        for id in transaction_ids {
            query = query.bind(id);
        }

        let result = query.execute(self.db.pool()).await?;
        Ok(result.rows_affected() as usize)
    }

    /// Delete a single transaction by ID for a specific child
    pub async fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM transactions WHERE child_id = ? AND id = ?"
        )
        .bind(child_id)
        .bind(transaction_id)
        .execute(self.db.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if transactions exist by their IDs for a specific child
    pub async fn check_transactions_exist(&self, child_id: &str, transaction_ids: &[String]) -> Result<Vec<String>> {
        if transaction_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = transaction_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query_str = format!(
            "SELECT id FROM transactions WHERE child_id = ? AND id IN ({})",
            placeholders
        );

        let mut query = sqlx::query(&query_str);
        query = query.bind(child_id);
        for id in transaction_ids {
            query = query.bind(id);
        }

        let rows = query.fetch_all(self.db.pool()).await?;
        let existing_ids: Vec<String> = rows.iter().map(|row| row.get("id")).collect();
        
        Ok(existing_ids)
    }
} 