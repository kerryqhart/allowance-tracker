use anyhow::Result;
use shared::KeyValue;
use sqlx::{migrate::MigrateDatabase, Row, Sqlite, SqlitePool};
use std::sync::Arc;

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
        // Create our database table if it doesn't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS key_values (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get the underlying SQLite pool
    pub fn pool(&self) -> &SqlitePool {
        &*self.pool
    }

    /// Store a key-value pair in the database.
    /// This will overwrite any existing value for the same key.
    pub async fn put_value(&self, kv: &KeyValue) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO key_values (key, value) VALUES (?, ?)",
        )
        .bind(&kv.key)
        .bind(&kv.value)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Retrieve a value by its key
    pub async fn get_value(&self, key: &str) -> Result<Option<KeyValue>> {
        let result = sqlx::query("SELECT value FROM key_values WHERE key = ?")
            .bind(key)
            .map(|row: sqlx::sqlite::SqliteRow| {
                row.get::<String, _>("value")
            })
            .fetch_optional(&*self.pool)
            .await?;

        // Convert the result to a KeyValue if found
        Ok(result.map(|value| KeyValue {
            key: key.to_string(),
            value,
        }))
    }

    /// Delete a value by its key
    pub async fn delete_value(&self, key: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM key_values WHERE key = ?")
            .bind(key)
            .execute(&*self.pool)
            .await?;
        
        // Return whether any row was affected (i.e., key existed)
        Ok(result.rows_affected() > 0)
    }

    /// List all keys in the database
    pub async fn list_keys(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT key FROM key_values ORDER BY key")
            .map(|row: sqlx::sqlite::SqliteRow| {
                row.get::<String, _>("key")
            })
            .fetch_all(&*self.pool)
            .await?;
        
        Ok(rows)
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
    async fn test_put_and_get_value() {
        // Each test gets its own database connection
        let db = setup_test().await;
        
        // Create a key-value pair
        let test_kv = KeyValue {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
        };
        
        // Put the value in the database
        db.put_value(&test_kv).await.expect("Failed to put value");
        
        // Retrieve the value
        let result = db.get_value(&test_kv.key).await.expect("Failed to get value");
        
        assert!(result.is_some());
        let retrieved_kv = result.unwrap();
        assert_eq!(retrieved_kv.key, test_kv.key);
        assert_eq!(retrieved_kv.value, test_kv.value);
    }
    
    #[tokio::test]
    async fn test_get_nonexistent_value() {
        let db = setup_test().await;
        
        // Try to get a value that doesn't exist
        let result = db.get_value("nonexistent_key").await.expect("Query failed");
        
        // Should be None
        assert!(result.is_none());
    }
    
    #[tokio::test]
    async fn test_put_replace_value() {
        let db = setup_test().await;
        
        // Create initial key-value pair
        let initial_kv = KeyValue {
            key: "same_key".to_string(),
            value: "initial_value".to_string(),
        };
        
        // Put the value in the database
        db.put_value(&initial_kv).await.expect("Failed to put initial value");
        
        // Create updated key-value pair with same key
        let updated_kv = KeyValue {
            key: "same_key".to_string(),
            value: "updated_value".to_string(),
        };
        
        // Replace the value
        db.put_value(&updated_kv).await.expect("Failed to update value");
        
        // Retrieve the value
        let result = db.get_value(&updated_kv.key).await.expect("Failed to get value");
        
        assert!(result.is_some());
        let retrieved_kv = result.unwrap();
        
        // Should have the updated value
        assert_eq!(retrieved_kv.key, updated_kv.key);
        assert_eq!(retrieved_kv.value, updated_kv.value);
        assert_ne!(retrieved_kv.value, initial_kv.value);
    }
    
    #[tokio::test]
    async fn test_delete_value() {
        let db = setup_test().await;
        
        // Create a key-value pair
        let test_kv = KeyValue {
            key: "key_to_delete".to_string(),
            value: "value_to_delete".to_string(),
        };
        
        // Put the value in the database
        db.put_value(&test_kv).await.expect("Failed to put value");
        
        // Verify it exists
        let exists_before = db.get_value(&test_kv.key).await.expect("Failed to get value");
        assert!(exists_before.is_some());
        
        // Delete the value
        let deleted = db.delete_value(&test_kv.key).await.expect("Failed to delete value");
        assert!(deleted, "Value should have been deleted");
        
        // Verify it's gone
        let exists_after = db.get_value(&test_kv.key).await.expect("Failed to check after deletion");
        assert!(exists_after.is_none());
        
        // Try to delete again (should return false - not found)
        let deleted_again = db.delete_value(&test_kv.key).await.expect("Failed to re-delete value");
        assert!(!deleted_again, "Value should not exist to be deleted");
    }
    
    #[tokio::test]
    async fn test_list_keys() {
        let db = setup_test().await;
        
        // Initially should be empty
        let empty_keys = db.list_keys().await.expect("Failed to list keys");
        assert!(empty_keys.is_empty(), "Database should be empty at test start");
        
        // Add some key-value pairs
        let test_pairs = [
            ("key1", "value1"),
            ("key2", "value2"),
            ("key3", "value3"),
        ];
        
        for (k, v) in &test_pairs {
            let kv = KeyValue {
                key: k.to_string(),
                value: v.to_string(),
            };
            db.put_value(&kv).await.expect("Failed to put value");
        }
        
        // Get all keys
        let keys = db.list_keys().await.expect("Failed to list keys");
        
        // Should have exactly 3 keys
        assert_eq!(keys.len(), 3);
        
        // Check all expected keys exist
        for (k, _) in &test_pairs {
            assert!(keys.contains(&k.to_string()));
        }
        
        // Keys should be in alphabetical order
        let expected_order = ["key1", "key2", "key3"];
        for (i, expected) in expected_order.iter().enumerate() {
            assert_eq!(&keys[i], expected);
        }
    }
}
