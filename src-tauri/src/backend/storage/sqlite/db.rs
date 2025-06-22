use anyhow::Result;
use sqlx::{migrate::MigrateDatabase, Row, Sqlite, SqlitePool};
use std::sync::Arc;
use shared::{Transaction, TransactionType};

// The database URL for the production database
const DATABASE_URL: &str = "sqlite:keyvalue.db";

/// DbConnection manages database operations
#[derive(Clone)]
pub struct DbConnection {
    pool: Arc<SqlitePool>,
}

impl DbConnection {
    /// Create a new database connection
    pub async fn new(url: &str) -> Result<Self> {
        // Create database if it doesn't exist
        if !Sqlite::database_exists(url).await.unwrap_or(false) {
            Sqlite::create_database(url).await?
        }

        // Connect to the database
        let pool = SqlitePool::connect(url).await?;

        // Setup database schema
        Self::setup_schema(&pool).await?;

        Ok(Self { pool: Arc::new(pool) })
    }
    
    /// Initialize the standard database
    pub async fn init() -> Result<Self> {
        Self::new(DATABASE_URL).await
    }
    
    /// Initialize a test database with a unique name
    #[cfg(test)]
    pub async fn init_test() -> Result<Self> {
        // Generate a unique database name for tests
        let test_id = uuid::Uuid::new_v4().to_string();
        let db_url = format!("file:memdb_{}?mode=memory&cache=shared", test_id);
        
        Self::new(&db_url).await
    }

    /// Set up the required database schema
    async fn setup_schema(pool: &SqlitePool) -> Result<()> {
        // Create transactions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transactions (
                id TEXT PRIMARY KEY,
                date TEXT NOT NULL,
                description TEXT NOT NULL,
                amount REAL NOT NULL,
                balance REAL NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for ordering by created_at (for pagination)
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_transactions_created_at 
            ON transactions(created_at DESC);
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }



    /// Store a transaction in the database
    pub async fn store_transaction(&self, transaction: &Transaction) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO transactions (id, date, description, amount, balance)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&transaction.id)
        .bind(&transaction.date)
        .bind(&transaction.description)
        .bind(transaction.amount)
        .bind(transaction.balance)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Get the most recent transaction (for calculating next balance)
    pub async fn get_latest_transaction(&self) -> Result<Option<Transaction>> {
        let row = sqlx::query(
            r#"
            SELECT id, date, description, amount, balance
            FROM transactions
            ORDER BY ROWID DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(Transaction {
                id: r.get("id"),
                child_id: "legacy".to_string(), // Default for legacy SQLite transactions
                date: r.get("date"),
                description: r.get("description"),
                amount: r.get("amount"),
                balance: r.get("balance"),
                transaction_type: if r.get::<f64, _>("amount") >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
            })),
            None => Ok(None),
        }
    }

    /// List transactions with pagination support
    pub async fn list_transactions(
        &self,
        limit: u32,
        after_id: Option<&str>,
    ) -> Result<Vec<Transaction>> {
        let query = if let Some(after_id) = after_id {
            sqlx::query(
                r#"
                SELECT id, date, description, amount, balance
                FROM transactions
                WHERE ROWID < (
                    SELECT ROWID FROM transactions WHERE id = ?
                )
                ORDER BY ROWID DESC
                LIMIT ?
                "#,
            )
            .bind(after_id)
            .bind(limit as i64)
        } else {
            sqlx::query(
                r#"
                SELECT id, date, description, amount, balance
                FROM transactions
                ORDER BY ROWID DESC
                LIMIT ?
                "#,
            )
            .bind(limit as i64)
        };

        let rows = query.fetch_all(&*self.pool).await?;

        let transactions = rows
            .iter()
            .map(|row| Transaction {
                id: row.get("id"),
                child_id: "legacy".to_string(), // Default for legacy SQLite transactions
                date: row.get("date"),
                description: row.get("description"),
                amount: row.get("amount"),
                balance: row.get("balance"),
                transaction_type: if row.get::<f64, _>("amount") >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
            })
            .collect();

        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Setup a new test database for each test
    async fn setup_test() -> DbConnection {
        // Create a unique test database
        DbConnection::init_test().await.expect("Failed to create test database")
    }
    


    #[tokio::test]
    async fn test_store_and_get_latest_transaction() {
        let db = setup_test().await;

        // Initially should have no transactions
        let latest = db.get_latest_transaction().await.expect("Failed to get latest transaction");
        assert!(latest.is_none(), "Should have no transactions initially");

        // Create a test transaction
        let transaction = Transaction {
            id: "transaction::income::1234567890000".to_string(),
            child_id: "test_child".to_string(),
            date: "2025-06-14T10:00:00-04:00".to_string(),
            description: "Test allowance".to_string(),
            amount: 10.0,
            balance: 10.0,
            transaction_type: TransactionType::Income,
        };

        // Store the transaction
        db.store_transaction(&transaction).await.expect("Failed to store transaction");

        // Retrieve the latest transaction
        let latest = db.get_latest_transaction().await.expect("Failed to get latest transaction");
        assert!(latest.is_some(), "Should have one transaction");
        
        let retrieved = latest.unwrap();
        assert_eq!(retrieved.id, transaction.id);
        assert_eq!(retrieved.date, transaction.date);
        assert_eq!(retrieved.description, transaction.description);
        assert_eq!(retrieved.amount, transaction.amount);
        assert_eq!(retrieved.balance, transaction.balance);
    }

    #[tokio::test]
    async fn test_store_multiple_transactions_latest_ordering() {
        let db = setup_test().await;

        let transactions = vec![
            Transaction {
                id: "transaction::income::1234567890000".to_string(),
                child_id: "test_child".to_string(),
                date: "2025-06-14T10:00:00-04:00".to_string(),
                description: "First transaction".to_string(),
                amount: 10.0,
                balance: 10.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: "transaction::expense::1234567891000".to_string(),
                child_id: "test_child".to_string(),
                date: "2025-06-14T11:00:00-04:00".to_string(),
                description: "Second transaction".to_string(),
                amount: -5.0,
                balance: 5.0,
                transaction_type: TransactionType::Expense,
            },
            Transaction {
                id: "transaction::income::1234567892000".to_string(),
                child_id: "test_child".to_string(),
                date: "2025-06-14T12:00:00-04:00".to_string(),
                description: "Third transaction".to_string(),
                amount: 15.0,
                balance: 20.0,
                transaction_type: TransactionType::Income,
            },
        ];

        // Store transactions in insertion order (ROWID will preserve this)
        for transaction in &transactions {
            db.store_transaction(transaction).await.expect("Failed to store transaction");
        }

        // Get latest should return the third (most recently stored) transaction
        let latest = db.get_latest_transaction().await.expect("Failed to get latest transaction");
        assert!(latest.is_some());
        let retrieved = latest.unwrap();
        assert_eq!(retrieved.id, transactions[2].id);
        assert_eq!(retrieved.description, "Third transaction");
        assert_eq!(retrieved.balance, 20.0);
    }

    #[tokio::test]
    async fn test_list_transactions_pagination() {
        let db = setup_test().await;

        // Create test transactions
        let transactions = vec![
            Transaction {
                id: "transaction::income::1234567890000".to_string(),
                child_id: "test_child".to_string(),
                date: "2025-06-14T10:00:00-04:00".to_string(),
                description: "Transaction 1".to_string(),
                amount: 10.0,
                balance: 10.0,
                transaction_type: TransactionType::Income,
            },
            Transaction {
                id: "transaction::expense::1234567891000".to_string(),
                child_id: "test_child".to_string(),
                date: "2025-06-14T11:00:00-04:00".to_string(),
                description: "Transaction 2".to_string(),
                amount: -5.0,
                balance: 5.0,
                transaction_type: TransactionType::Expense,
            },
            Transaction {
                id: "transaction::income::1234567892000".to_string(),
                child_id: "test_child".to_string(),
                date: "2025-06-14T12:00:00-04:00".to_string(),
                description: "Transaction 3".to_string(),
                amount: 15.0,
                balance: 20.0,
                transaction_type: TransactionType::Income,
            },
        ];

        // Store transactions in insertion order (ROWID will preserve this)
        for transaction in &transactions {
            db.store_transaction(transaction).await.expect("Failed to store transaction");
        }

        // Test listing all transactions (limit 10)
        let all_transactions = db.list_transactions(10, None).await.expect("Failed to list transactions");
        assert_eq!(all_transactions.len(), 3);
        
        // Should be in reverse chronological order (newest first)
        assert_eq!(all_transactions[0].description, "Transaction 3");
        assert_eq!(all_transactions[1].description, "Transaction 2");
        assert_eq!(all_transactions[2].description, "Transaction 1");

        // Test pagination with limit
        let first_page = db.list_transactions(2, None).await.expect("Failed to list first page");
        assert_eq!(first_page.len(), 2);
        assert_eq!(first_page[0].description, "Transaction 3");
        assert_eq!(first_page[1].description, "Transaction 2");

        // Test pagination with cursor (after first transaction of previous query)
        let second_page = db.list_transactions(2, Some(&first_page[0].id)).await.expect("Failed to list with cursor");
        assert_eq!(second_page.len(), 2);
        assert_eq!(second_page[0].description, "Transaction 2");
        assert_eq!(second_page[1].description, "Transaction 1");
    }

    #[tokio::test]
    async fn test_list_transactions_empty_database() {
        let db = setup_test().await;

        // List transactions from empty database
        let transactions = db.list_transactions(10, None).await.expect("Failed to list transactions");
        assert!(transactions.is_empty(), "Should return empty vector for empty database");
    }

    #[tokio::test]
    async fn test_list_transactions_with_invalid_cursor() {
        let db = setup_test().await;

        // Store one transaction
        let transaction = Transaction {
            id: "transaction::income::1234567890000".to_string(),
            date: "2025-06-14T10:00:00-04:00".to_string(),
            description: "Test transaction".to_string(),
            amount: 10.0,
            balance: 10.0,
        };
        db.store_transaction(&transaction).await.expect("Failed to store transaction");

        // Try to list with invalid cursor
        let transactions = db.list_transactions(10, Some("invalid_cursor_id")).await.expect("Failed to list transactions");
        // Should return empty when cursor is invalid (no transaction with that ID found)
        assert_eq!(transactions.len(), 0, "Should return empty for invalid cursor");
    }
}
