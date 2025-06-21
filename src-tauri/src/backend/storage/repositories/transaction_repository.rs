use anyhow::Result;
use sqlx::Row;
use async_trait::async_trait;
use shared::{Transaction, TransactionType};
use crate::backend::storage::connection::DbConnection;
use crate::backend::storage::traits::TransactionStorage;

/// Repository for transaction operations
#[derive(Clone)]
pub struct TransactionRepository {
    db: DbConnection,
}

impl TransactionRepository {
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    /// Get the underlying database connection for testing purposes
    pub fn get_db_connection(&self) -> &DbConnection {
        &self.db
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
    /// NOTE: This now orders by date instead of ROWID for proper chronological ordering
    pub async fn get_latest_transaction(&self, child_id: &str) -> Result<Option<Transaction>> {
        let row = sqlx::query(
            r#"
            SELECT id, child_id, date, description, amount, balance
            FROM transactions
            WHERE child_id = ?
            ORDER BY date DESC, ROWID DESC
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
    /// Updated to order by date for proper chronological ordering
    pub async fn list_transactions(
        &self,
        child_id: &str,
        limit: u32,
        after_id: Option<&str>,
    ) -> Result<Vec<Transaction>> {
        let query = if let Some(after_id) = after_id {
            // For cursor-based pagination, we need to find the date of the cursor transaction
            // and get transactions before that date (or same date but earlier ROWID)
            sqlx::query(
                r#"
                SELECT id, child_id, date, description, amount, balance
                FROM transactions
                WHERE child_id = ? AND (
                    date < (SELECT date FROM transactions WHERE id = ?) OR
                    (date = (SELECT date FROM transactions WHERE id = ?) AND ROWID < (SELECT ROWID FROM transactions WHERE id = ?))
                )
                ORDER BY date DESC, ROWID DESC
                LIMIT ?
                "#,
            )
            .bind(child_id)
            .bind(after_id)
            .bind(after_id)
            .bind(after_id)
            .bind(limit as i64)
        } else {
            sqlx::query(
                r#"
                SELECT id, child_id, date, description, amount, balance
                FROM transactions
                WHERE child_id = ?
                ORDER BY date DESC, ROWID DESC
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

    /// Get all transactions after a specific date (inclusive) for balance recalculation
    /// Returns transactions in chronological order (oldest first)
    pub async fn get_transactions_after_date(&self, child_id: &str, date: &str) -> Result<Vec<Transaction>> {
        let rows = sqlx::query(
            r#"
            SELECT id, child_id, date, description, amount, balance
            FROM transactions
            WHERE child_id = ? AND date >= ?
            ORDER BY date ASC, ROWID ASC
            "#,
        )
        .bind(child_id)
        .bind(date)
        .fetch_all(self.db.pool())
        .await?;

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

    /// Get all transactions in chronological order for a child
    /// Returns transactions ordered by date (oldest first)
    pub async fn get_all_transactions_chronological(&self, child_id: &str) -> Result<Vec<Transaction>> {
        let rows = sqlx::query(
            r#"
            SELECT id, child_id, date, description, amount, balance
            FROM transactions
            WHERE child_id = ?
            ORDER BY date ASC, ROWID ASC
            "#,
        )
        .bind(child_id)
        .fetch_all(self.db.pool())
        .await?;

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

    /// Get the most recent transaction before a specific date
    /// This is useful for finding the starting balance when inserting backdated transactions
    pub async fn get_latest_transaction_before_date(&self, child_id: &str, date: &str) -> Result<Option<Transaction>> {
        let row = sqlx::query(
            r#"
            SELECT id, child_id, date, description, amount, balance
            FROM transactions
            WHERE child_id = ? AND date < ?
            ORDER BY date DESC, ROWID DESC
            LIMIT 1
            "#,
        )
        .bind(child_id)
        .bind(date)
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

    /// Update the balance of a specific transaction
    /// Used during balance recalculation after backdated transactions
    pub async fn update_transaction_balance(&self, transaction_id: &str, new_balance: f64) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE transactions 
            SET balance = ? 
            WHERE id = ?
            "#,
        )
        .bind(new_balance)
        .bind(transaction_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// Update multiple transaction balances atomically
    /// Used for bulk balance recalculation after backdated transactions
    pub async fn update_transaction_balances(&self, updates: &[(String, f64)]) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        
        for (transaction_id, new_balance) in updates {
            sqlx::query(
                r#"
                UPDATE transactions 
                SET balance = ? 
                WHERE id = ?
                "#,
            )
            .bind(new_balance)
            .bind(transaction_id)
            .execute(&mut *tx)
            .await?;
        }
        
        tx.commit().await?;
        Ok(())
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

#[async_trait]
impl TransactionStorage for TransactionRepository {
    /// Store a new transaction (trait implementation)
    async fn store_transaction(&self, transaction: &Transaction) -> Result<()> {
        // Call the actual implementation method directly to avoid recursion
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
    
    /// Retrieve a specific transaction by ID (trait implementation)
    async fn get_transaction(&self, child_id: &str, transaction_id: &str) -> Result<Option<Transaction>> {
        let row = sqlx::query(
            r#"
            SELECT id, child_id, date, description, amount, balance
            FROM transactions
            WHERE child_id = ? AND id = ?
            "#,
        )
        .bind(child_id)
        .bind(transaction_id)
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
    
    /// List transactions with pagination support (trait implementation)
    async fn list_transactions(&self, child_id: &str, limit: Option<u32>, after: Option<String>) -> Result<Vec<Transaction>> {
        let limit = limit.unwrap_or(20);
        self.list_transactions(child_id, limit, after.as_deref()).await
    }
    
    /// List transactions in chronological order with optional date filtering (trait implementation)
    async fn list_transactions_chronological(&self, child_id: &str, start_date: Option<String>, end_date: Option<String>) -> Result<Vec<Transaction>> {
        let mut query_str = String::from(
            r#"
            SELECT id, child_id, date, description, amount, balance
            FROM transactions
            WHERE child_id = ?
            "#
        );
        
        let mut bindings = vec![child_id.to_string()];
        
        if start_date.is_some() {
            query_str.push_str(" AND date >= ?");
            bindings.push(start_date.unwrap());
        }
        
        if end_date.is_some() {
            query_str.push_str(" AND date <= ?");
            bindings.push(end_date.unwrap());
        }
        
        query_str.push_str(" ORDER BY date ASC, ROWID ASC");
        
        let mut query = sqlx::query(&query_str);
        for binding in bindings {
            query = query.bind(binding);
        }
        
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
    
    /// Update an existing transaction (trait implementation)
    async fn update_transaction(&self, transaction: &Transaction) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE transactions 
            SET date = ?, description = ?, amount = ?, balance = ?
            WHERE id = ? AND child_id = ?
            "#,
        )
        .bind(&transaction.date)
        .bind(&transaction.description)
        .bind(transaction.amount)
        .bind(transaction.balance)
        .bind(&transaction.id)
        .bind(&transaction.child_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
    
    /// Delete a single transaction (trait implementation)
    async fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool> {
        // Call the actual implementation directly to avoid recursion
        let result = sqlx::query(
            "DELETE FROM transactions WHERE child_id = ? AND id = ?"
        )
        .bind(child_id)
        .bind(transaction_id)
        .execute(self.db.pool())
        .await?;

        Ok(result.rows_affected() > 0)
    }
    
    /// Delete multiple transactions (trait implementation)
    async fn delete_transactions(&self, child_id: &str, transaction_ids: &[String]) -> Result<u32> {
        // Call the actual implementation directly to avoid recursion
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
        Ok(result.rows_affected() as u32)
    }
} 