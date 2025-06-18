use anyhow::Result;
use sqlx::{migrate::MigrateDatabase, Row, Sqlite, SqlitePool};
use std::sync::Arc;
use shared::{Transaction, Child, ParentalControlAttempt};

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
                child_id TEXT NOT NULL,
                date TEXT NOT NULL,
                description TEXT NOT NULL,
                amount REAL NOT NULL,
                balance REAL NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (child_id) REFERENCES children (id) ON DELETE CASCADE
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

        // Create index for child_id filtering
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_transactions_child_id 
            ON transactions(child_id);
            "#,
        )
        .execute(pool)
        .await?;

        // Create children table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS children (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                birthdate TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for ordering children by name
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_children_name 
            ON children(name);
            "#,
        )
        .execute(pool)
        .await?;

        // Create parental_control_attempts table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS parental_control_attempts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                attempted_value TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL DEFAULT FALSE
            );
            "#,
        )
        .execute(pool)
        .await?;

        // Create index for ordering attempts by id
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_parental_control_attempts_id 
            ON parental_control_attempts(id DESC);
            "#,
        )
        .execute(pool)
        .await?;

        // Create active_child table (single row to track the currently active child)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS active_child (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                child_id TEXT NOT NULL,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (child_id) REFERENCES children (id) ON DELETE CASCADE
            );
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
        .execute(&*self.pool)
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
        .fetch_optional(&*self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(Transaction {
                id: r.get("id"),
                child_id: r.get("child_id"),
                date: r.get("date"),
                description: r.get("description"),
                amount: r.get("amount"),
                balance: r.get("balance"),
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

        let rows = query.fetch_all(&*self.pool).await?;

        let transactions = rows
            .iter()
            .map(|row| Transaction {
                id: row.get("id"),
                child_id: row.get("child_id"),
                date: row.get("date"),
                description: row.get("description"),
                amount: row.get("amount"),
                balance: row.get("balance"),
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

        let result = query.execute(&*self.pool).await?;
        Ok(result.rows_affected() as usize)
    }

    /// Delete a single transaction by ID for a specific child
    pub async fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM transactions WHERE child_id = ? AND id = ?"
        )
        .bind(child_id)
        .bind(transaction_id)
        .execute(&*self.pool)
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

        let rows = query.fetch_all(&*self.pool).await?;
        let existing_ids: Vec<String> = rows.iter().map(|row| row.get("id")).collect();
        
        Ok(existing_ids)
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
        .execute(&*self.pool)
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
        .fetch_optional(&*self.pool)
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
        .fetch_all(&*self.pool)
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
        .execute(&*self.pool)
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
        .execute(&*self.pool)
        .await?;
        Ok(())
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
        .execute(&*self.pool)
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

        let rows = query.fetch_all(&*self.pool).await?;

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

    /// Get the currently active child ID
    pub async fn get_active_child(&self) -> Result<Option<String>> {
        let row = sqlx::query(
            r#"
            SELECT child_id
            FROM active_child
            WHERE id = 1
            "#,
        )
        .fetch_optional(&*self.pool)
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
        .fetch_optional(&*self.pool)
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
        .execute(&*self.pool)
        .await?;

        Ok(())
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

        let child_id = "test_child_id";

        // Initially should have no transactions for this child
        let latest = db.get_latest_transaction(child_id).await.expect("Failed to get latest transaction");
        assert!(latest.is_none(), "Should have no transactions initially");

        // Create a test transaction
        let transaction = Transaction {
            id: "transaction::income::1234567890000".to_string(),
            child_id: child_id.to_string(),
            date: "2025-06-14T10:00:00-04:00".to_string(),
            description: "Test allowance".to_string(),
            amount: 10.0,
            balance: 10.0,
        };

        // Store the transaction
        db.store_transaction(&transaction).await.expect("Failed to store transaction");

        // Retrieve the latest transaction
        let latest = db.get_latest_transaction(child_id).await.expect("Failed to get latest transaction");
        assert!(latest.is_some(), "Should have one transaction");
        
        let retrieved = latest.unwrap();
        assert_eq!(retrieved.id, transaction.id);
        assert_eq!(retrieved.child_id, transaction.child_id);
        assert_eq!(retrieved.date, transaction.date);
        assert_eq!(retrieved.description, transaction.description);
        assert_eq!(retrieved.amount, transaction.amount);
        assert_eq!(retrieved.balance, transaction.balance);
    }

    #[tokio::test]
    async fn test_store_multiple_transactions_latest_ordering() {
        let db = setup_test().await;

        let child_id = "test_child_id";

        let transactions = vec![
            Transaction {
                id: "transaction::income::1234567890000".to_string(),
                child_id: child_id.to_string(),
                date: "2025-06-14T10:00:00-04:00".to_string(),
                description: "First transaction".to_string(),
                amount: 10.0,
                balance: 10.0,
            },
            Transaction {
                id: "transaction::expense::1234567891000".to_string(),
                child_id: child_id.to_string(),
                date: "2025-06-14T11:00:00-04:00".to_string(),
                description: "Second transaction".to_string(),
                amount: -5.0,
                balance: 5.0,
            },
            Transaction {
                id: "transaction::income::1234567892000".to_string(),
                child_id: child_id.to_string(),
                date: "2025-06-14T12:00:00-04:00".to_string(),
                description: "Third transaction".to_string(),
                amount: 15.0,
                balance: 20.0,
            },
        ];

        // Store transactions in insertion order (ROWID will preserve this)
        for transaction in &transactions {
            db.store_transaction(transaction).await.expect("Failed to store transaction");
        }

        // Get latest should return the third (most recently stored) transaction
        let latest = db.get_latest_transaction(child_id).await.expect("Failed to get latest transaction");
        assert!(latest.is_some());
        let retrieved = latest.unwrap();
        assert_eq!(retrieved.id, transactions[2].id);
        assert_eq!(retrieved.description, "Third transaction");
        assert_eq!(retrieved.balance, 20.0);
    }

    #[tokio::test]
    async fn test_list_transactions_pagination() {
        let db = setup_test().await;

        let child_id = "test_child_id";

        // Create test transactions
        let transactions = vec![
            Transaction {
                id: "transaction::income::1234567890000".to_string(),
                child_id: child_id.to_string(),
                date: "2025-06-14T10:00:00-04:00".to_string(),
                description: "Transaction 1".to_string(),
                amount: 10.0,
                balance: 10.0,
            },
            Transaction {
                id: "transaction::expense::1234567891000".to_string(),
                child_id: child_id.to_string(),
                date: "2025-06-14T11:00:00-04:00".to_string(),
                description: "Transaction 2".to_string(),
                amount: -5.0,
                balance: 5.0,
            },
            Transaction {
                id: "transaction::income::1234567892000".to_string(),
                child_id: child_id.to_string(),
                date: "2025-06-14T12:00:00-04:00".to_string(),
                description: "Transaction 3".to_string(),
                amount: 15.0,
                balance: 20.0,
            },
        ];

        // Store transactions in insertion order (ROWID will preserve this)
        for transaction in &transactions {
            db.store_transaction(transaction).await.expect("Failed to store transaction");
        }

        // Test listing all transactions (limit 10)
        let all_transactions = db.list_transactions(child_id, 10, None).await.expect("Failed to list transactions");
        assert_eq!(all_transactions.len(), 3);
        
        // Should be in reverse chronological order (newest first)
        assert_eq!(all_transactions[0].description, "Transaction 3");
        assert_eq!(all_transactions[1].description, "Transaction 2");
        assert_eq!(all_transactions[2].description, "Transaction 1");

        // Test pagination with limit
        let first_page = db.list_transactions(child_id, 2, None).await.expect("Failed to list first page");
        assert_eq!(first_page.len(), 2);
        assert_eq!(first_page[0].description, "Transaction 3");
        assert_eq!(first_page[1].description, "Transaction 2");

        // Test pagination with cursor (after first transaction of previous query)
        let second_page = db.list_transactions(child_id, 2, Some(&first_page[0].id)).await.expect("Failed to list with cursor");
        assert_eq!(second_page.len(), 2);
        assert_eq!(second_page[0].description, "Transaction 2");
        assert_eq!(second_page[1].description, "Transaction 1");
    }

    #[tokio::test]
    async fn test_list_transactions_empty_database() {
        let db = setup_test().await;

        let child_id = "test_child_id";

        // List transactions from empty database
        let transactions = db.list_transactions(child_id, 10, None).await.expect("Failed to list transactions");
        assert!(transactions.is_empty(), "Should return empty vector for empty database");
    }

    #[tokio::test]
    async fn test_list_transactions_with_invalid_cursor() {
        let db = setup_test().await;

        let child_id = "test_child_id";

        // Store one transaction
        let transaction = Transaction {
            id: "transaction::income::1234567890000".to_string(),
            child_id: child_id.to_string(),
            date: "2025-06-14T10:00:00-04:00".to_string(),
            description: "Test transaction".to_string(),
            amount: 10.0,
            balance: 10.0,
        };
        db.store_transaction(&transaction).await.expect("Failed to store transaction");

        // Try to list with invalid cursor
        let transactions = db.list_transactions(child_id, 10, Some("invalid_cursor_id")).await.expect("Failed to list transactions");
        // Should return empty when cursor is invalid (no transaction with that ID found)
        assert_eq!(transactions.len(), 0, "Should return empty for invalid cursor");
    }

    #[tokio::test]
    async fn test_store_and_get_child() {
        let db = setup_test().await;

        // Create a test child
        let child = Child {
            id: "child::1702516122000".to_string(),
            name: "Alice Smith".to_string(),
            birthdate: "2015-06-15".to_string(),
            created_at: "2023-12-14T01:02:02.000Z".to_string(),
            updated_at: "2023-12-14T01:02:02.000Z".to_string(),
        };

        // Store the child
        db.store_child(&child).await.expect("Failed to store child");

        // Retrieve the child
        let retrieved_child = db.get_child(&child.id).await.expect("Failed to get child");
        assert!(retrieved_child.is_some(), "Child should exist");
        
        let retrieved_child = retrieved_child.unwrap();
        assert_eq!(retrieved_child.id, child.id);
        assert_eq!(retrieved_child.name, child.name);
        assert_eq!(retrieved_child.birthdate, child.birthdate);
        assert_eq!(retrieved_child.created_at, child.created_at);
        assert_eq!(retrieved_child.updated_at, child.updated_at);
    }

    #[tokio::test]
    async fn test_get_nonexistent_child() {
        let db = setup_test().await;

        // Try to get a child that doesn't exist
        let child = db.get_child("child::nonexistent").await.expect("Failed to query child");
        assert!(child.is_none(), "Should return None for nonexistent child");
    }

    #[tokio::test]
    async fn test_list_children() {
        let db = setup_test().await;

        // Initially should have no children
        let children = db.list_children().await.expect("Failed to list children");
        assert_eq!(children.len(), 0, "Should have no children initially");

        // Create test children
        let child1 = Child {
            id: "child::1702516122000".to_string(),
            name: "Bob Johnson".to_string(),
            birthdate: "2012-03-20".to_string(),
            created_at: "2023-12-14T01:02:02.000Z".to_string(),
            updated_at: "2023-12-14T01:02:02.000Z".to_string(),
        };

        let child2 = Child {
            id: "child::1702516125000".to_string(),
            name: "Alice Smith".to_string(),
            birthdate: "2015-06-15".to_string(),
            created_at: "2023-12-14T01:02:05.000Z".to_string(),
            updated_at: "2023-12-14T01:02:05.000Z".to_string(),
        };

        // Store both children
        db.store_child(&child1).await.expect("Failed to store child1");
        db.store_child(&child2).await.expect("Failed to store child2");

        // List children (should be ordered by name)
        let children = db.list_children().await.expect("Failed to list children");
        assert_eq!(children.len(), 2, "Should have 2 children");
        
        // Should be ordered by name: Alice, Bob
        assert_eq!(children[0].name, "Alice Smith");
        assert_eq!(children[1].name, "Bob Johnson");
    }

    #[tokio::test]
    async fn test_update_child() {
        let db = setup_test().await;

        // Create and store a child
        let mut child = Child {
            id: "child::1702516122000".to_string(),
            name: "Original Name".to_string(),
            birthdate: "2015-06-15".to_string(),
            created_at: "2023-12-14T01:02:02.000Z".to_string(),
            updated_at: "2023-12-14T01:02:02.000Z".to_string(),
        };

        db.store_child(&child).await.expect("Failed to store child");

        // Update the child
        child.name = "Updated Name".to_string();
        child.birthdate = "2015-07-20".to_string();
        child.updated_at = "2023-12-14T02:00:00.000Z".to_string();

        db.update_child(&child).await.expect("Failed to update child");

        // Retrieve and verify the update
        let updated_child = db.get_child(&child.id).await.expect("Failed to get child").unwrap();
        assert_eq!(updated_child.name, "Updated Name");
        assert_eq!(updated_child.birthdate, "2015-07-20");
        assert_eq!(updated_child.updated_at, "2023-12-14T02:00:00.000Z");
        assert_eq!(updated_child.created_at, child.created_at); // Should remain unchanged
    }

    #[tokio::test]
    async fn test_delete_child() {
        let db = setup_test().await;

        // Create and store a child
        let child = Child {
            id: "child::1702516122000".to_string(),
            name: "Test Child".to_string(),
            birthdate: "2015-06-15".to_string(),
            created_at: "2023-12-14T01:02:02.000Z".to_string(),
            updated_at: "2023-12-14T01:02:02.000Z".to_string(),
        };

        db.store_child(&child).await.expect("Failed to store child");

        // Verify child exists
        let retrieved_child = db.get_child(&child.id).await.expect("Failed to get child");
        assert!(retrieved_child.is_some(), "Child should exist before deletion");

        // Delete the child
        db.delete_child(&child.id).await.expect("Failed to delete child");

        // Verify child no longer exists
        let deleted_child = db.get_child(&child.id).await.expect("Failed to query child");
        assert!(deleted_child.is_none(), "Child should not exist after deletion");
    }

    #[tokio::test]
    async fn test_delete_single_transaction() {
        let db = setup_test().await;
        
        let child_id = "test_child_id";
        
        // Create a test transaction
        let transaction = Transaction {
            id: "tx123".to_string(),
            child_id: child_id.to_string(),
            date: "2025-01-01T10:00:00-05:00".to_string(),
            description: "Test transaction".to_string(),
            amount: 10.0,
            balance: 10.0,
        };
        
        // Store the transaction
        db.store_transaction(&transaction).await.unwrap();
        
        // Verify it exists
        let transactions = db.list_transactions(child_id, 10, None).await.unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(transactions[0].id, "tx123");
        
        // Delete the transaction
        let deleted = db.delete_transaction(child_id, "tx123").await.unwrap();
        assert!(deleted);
        
        // Verify it's gone
        let transactions = db.list_transactions(child_id, 10, None).await.unwrap();
        assert_eq!(transactions.len(), 0);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_transaction() {
        let db = setup_test().await;
        
        let child_id = "test_child_id";
        
        // Try to delete a non-existent transaction
        let deleted = db.delete_transaction(child_id, "nonexistent").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_multiple_transactions() {
        let db = setup_test().await;
        
        let child_id = "test_child_id";
        
        // Create multiple test transactions
        let transactions = vec![
            Transaction {
                id: "tx1".to_string(),
                child_id: child_id.to_string(),
                date: "2025-01-01T10:00:00-05:00".to_string(),
                description: "Transaction 1".to_string(),
                amount: 10.0,
                balance: 10.0,
            },
            Transaction {
                id: "tx2".to_string(),
                child_id: child_id.to_string(),
                date: "2025-01-01T11:00:00-05:00".to_string(),
                description: "Transaction 2".to_string(),
                amount: 20.0,
                balance: 30.0,
            },
            Transaction {
                id: "tx3".to_string(),
                child_id: child_id.to_string(),
                date: "2025-01-01T12:00:00-05:00".to_string(),
                description: "Transaction 3".to_string(),
                amount: -5.0,
                balance: 25.0,
            },
        ];
        
        // Store all transactions
        for tx in &transactions {
            db.store_transaction(tx).await.unwrap();
        }
        
        // Verify they exist
        let stored = db.list_transactions(child_id, 10, None).await.unwrap();
        assert_eq!(stored.len(), 3);
        
        // Delete two transactions
        let ids_to_delete = vec!["tx1".to_string(), "tx3".to_string()];
        let deleted_count = db.delete_transactions(child_id, &ids_to_delete).await.unwrap();
        assert_eq!(deleted_count, 2);
        
        // Verify only one remains
        let remaining = db.list_transactions(child_id, 10, None).await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "tx2");
    }

    #[tokio::test]
    async fn test_delete_empty_transaction_list() {
        let db = setup_test().await;
        
        let child_id = "test_child_id";
        
        // Try to delete empty list
        let deleted_count = db.delete_transactions(child_id, &[]).await.unwrap();
        assert_eq!(deleted_count, 0);
    }

    #[tokio::test]
    async fn test_delete_transactions_with_invalid_ids() {
        let db = setup_test().await;
        
        let child_id = "test_child_id";
        
        // Create one test transaction
        let transaction = Transaction {
            id: "tx1".to_string(),
            child_id: child_id.to_string(),
            date: "2025-01-01T10:00:00-05:00".to_string(),
            description: "Transaction 1".to_string(),
            amount: 10.0,
            balance: 10.0,
        };
        db.store_transaction(&transaction).await.unwrap();
        
        // Try to delete mix of valid and invalid IDs
        let ids_to_delete = vec!["tx1".to_string(), "invalid".to_string(), "also_invalid".to_string()];
        let deleted_count = db.delete_transactions(child_id, &ids_to_delete).await.unwrap();
        assert_eq!(deleted_count, 1); // Only tx1 should be deleted
        
        // Verify the transaction is gone
        let remaining = db.list_transactions(child_id, 10, None).await.unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[tokio::test]
    async fn test_check_transactions_exist() {
        let db = setup_test().await;
        
        let child_id = "test_child_id";
        
        // Create test transactions
        let transactions = vec![
            Transaction {
                id: "tx1".to_string(),
                child_id: child_id.to_string(),
                date: "2025-01-01T10:00:00-05:00".to_string(),
                description: "Transaction 1".to_string(),
                amount: 10.0,
                balance: 10.0,
            },
            Transaction {
                id: "tx2".to_string(),
                child_id: child_id.to_string(),
                date: "2025-01-01T11:00:00-05:00".to_string(),
                description: "Transaction 2".to_string(),
                amount: 20.0,
                balance: 30.0,
            },
        ];
        
        // Store transactions
        for tx in &transactions {
            db.store_transaction(tx).await.unwrap();
        }
        
        // Check which transactions exist
        let ids_to_check = vec!["tx1".to_string(), "tx2".to_string(), "tx3".to_string()];
        let existing_ids = db.check_transactions_exist(child_id, &ids_to_check).await.unwrap();
        
        assert_eq!(existing_ids.len(), 2);
        assert!(existing_ids.contains(&"tx1".to_string()));
        assert!(existing_ids.contains(&"tx2".to_string()));
        assert!(!existing_ids.contains(&"tx3".to_string()));
    }

    #[tokio::test]
    async fn test_check_transactions_exist_empty_list() {
        let db = setup_test().await;
        
                 let child_id = "test_child_id";
         
         // Check empty list
         let existing_ids = db.check_transactions_exist(child_id, &[]).await.unwrap();
         assert_eq!(existing_ids.len(), 0);
    }

    #[tokio::test]
    async fn test_record_parental_control_attempt() {
        let db = setup_test().await;
        
        // Record a successful attempt
        let attempt_id = db.record_parental_control_attempt("ice cold", true).await.unwrap();
        assert!(attempt_id > 0);
        
        // Record a failed attempt
        let failed_attempt_id = db.record_parental_control_attempt("wrong answer", false).await.unwrap();
        assert!(failed_attempt_id > 0);
        assert_ne!(attempt_id, failed_attempt_id);
    }

    #[tokio::test]
    async fn test_get_parental_control_attempts() {
        let db = setup_test().await;
        
        // Initially should have no attempts
        let attempts = db.get_parental_control_attempts(None).await.unwrap();
        assert_eq!(attempts.len(), 0);
        
        // Record some attempts with small delays to ensure different timestamps
        db.record_parental_control_attempt("ice cold", true).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.record_parental_control_attempt("wrong answer", false).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.record_parental_control_attempt("another wrong", false).await.unwrap();
        
        // Get all attempts
        let attempts = db.get_parental_control_attempts(None).await.unwrap();
        assert_eq!(attempts.len(), 3);
        
        // Should be ordered by id DESC (newest first)
        assert_eq!(attempts[0].attempted_value, "another wrong");
        assert_eq!(attempts[0].success, false);
        assert_eq!(attempts[1].attempted_value, "wrong answer");
        assert_eq!(attempts[1].success, false);
        assert_eq!(attempts[2].attempted_value, "ice cold");
        assert_eq!(attempts[2].success, true);
        
        // Test with limit
        let limited_attempts = db.get_parental_control_attempts(Some(2)).await.unwrap();
        assert_eq!(limited_attempts.len(), 2);
        assert_eq!(limited_attempts[0].attempted_value, "another wrong");
        assert_eq!(limited_attempts[1].attempted_value, "wrong answer");
    }

    #[tokio::test]
    async fn test_parental_control_attempt_properties() {
        let db = setup_test().await;
        
        // Record an attempt
        let attempt_id = db.record_parental_control_attempt("test answer", true).await.unwrap();
        
        // Retrieve it
        let attempts = db.get_parental_control_attempts(Some(1)).await.unwrap();
        assert_eq!(attempts.len(), 1);
        
        let attempt = &attempts[0];
        assert_eq!(attempt.id, attempt_id);
        assert_eq!(attempt.attempted_value, "test answer");
        assert_eq!(attempt.success, true);
        // Timestamp should be a valid datetime string
        assert!(!attempt.timestamp.is_empty());
        // Should be able to parse as datetime
        assert!(chrono::DateTime::parse_from_rfc3339(&attempt.timestamp).is_ok() || 
                chrono::NaiveDateTime::parse_from_str(&attempt.timestamp, "%Y-%m-%d %H:%M:%S").is_ok());
    }

    #[tokio::test]
    async fn test_get_active_child_when_none_set() {
        let db = setup_test().await;
        
        let active_child = db.get_active_child().await.expect("Failed to get active child");
        assert!(active_child.is_none(), "Should have no active child initially");
    }

    #[tokio::test]
    async fn test_set_and_get_active_child() {
        let db = setup_test().await;
        
        // Create a test child first
        let child = Child {
            id: "child::1234567890000".to_string(),
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        };
        db.store_child(&child).await.expect("Failed to store child");
        
        // Set as active child
        db.set_active_child(&child.id).await.expect("Failed to set active child");
        
        // Verify it's set correctly
        let active_child = db.get_active_child().await.expect("Failed to get active child");
        assert!(active_child.is_some());
        assert_eq!(active_child.unwrap(), child.id);
    }

    #[tokio::test]
    async fn test_set_active_child_with_nonexistent_child() {
        let db = setup_test().await;
        
        let result = db.set_active_child("nonexistent::child::id").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Child not found"));
    }

    #[tokio::test]
    async fn test_update_active_child() {
        let db = setup_test().await;
        
        // Create two test children
        let child1 = Child {
            id: "child::1234567890000".to_string(),
            name: "First Child".to_string(),
            birthdate: "2015-01-01".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let child2 = Child {
            id: "child::1234567891000".to_string(),
            name: "Second Child".to_string(),
            birthdate: "2016-01-01".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        };
        
        db.store_child(&child1).await.expect("Failed to store first child");
        db.store_child(&child2).await.expect("Failed to store second child");
        
        // Set first child as active
        db.set_active_child(&child1.id).await.expect("Failed to set first child as active");
        
        let active_child = db.get_active_child().await.expect("Failed to get active child");
        assert_eq!(active_child.unwrap(), child1.id);
        
        // Update to second child
        db.set_active_child(&child2.id).await.expect("Failed to set second child as active");
        
        let active_child = db.get_active_child().await.expect("Failed to get active child");
        assert_eq!(active_child.unwrap(), child2.id);
    }

    #[tokio::test]
    async fn test_active_child_cascade_delete() {
        let db = setup_test().await;
        
        // Create a test child
        let child = Child {
            id: "child::1234567890000".to_string(),
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        };
        db.store_child(&child).await.expect("Failed to store child");
        
        // Set as active child
        db.set_active_child(&child.id).await.expect("Failed to set active child");
        
        // Verify it's set
        let active_child = db.get_active_child().await.expect("Failed to get active child");
        assert!(active_child.is_some());
        
        // Delete the child
        db.delete_child(&child.id).await.expect("Failed to delete child");
        
        // Active child should be cleared due to CASCADE DELETE
        let active_child = db.get_active_child().await.expect("Failed to get active child");
        assert!(active_child.is_none());
    }
}
