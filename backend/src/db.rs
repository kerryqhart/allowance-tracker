use anyhow::Result;
use shared::KeyValue;
use sqlx::{migrate::MigrateDatabase, Pool, Row, Sqlite, SqlitePool};

pub const DATABASE_URL: &str = "sqlite:keyvalue.db";

/// Initialize the database.
/// Creates the database if it doesn't exist and sets up the schema.
pub async fn init_db() -> Result<Pool<Sqlite>> {
    // Create database if it doesn't exist
    if !Sqlite::database_exists(DATABASE_URL).await.unwrap_or(false) {
        Sqlite::create_database(DATABASE_URL).await?;
    }

    // Connect to the database
    let pool = SqlitePool::connect(DATABASE_URL).await?;

    // Setup database schema
    setup_schema(&pool).await?;

    Ok(pool)
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

/// Store a key-value pair in the database.
/// This will overwrite any existing value for the same key.
pub async fn put_value(pool: &SqlitePool, kv: &KeyValue) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO key_values (key, value) VALUES (?, ?)",
    )
    .bind(&kv.key)
    .bind(&kv.value)
    .execute(pool)
    .await?;

    Ok(())
}

/// Retrieve a value by its key
pub async fn get_value(pool: &SqlitePool, key: &str) -> Result<Option<KeyValue>> {
    let result = sqlx::query("SELECT value FROM key_values WHERE key = ?")
        .bind(key)
        .map(|row: sqlx::sqlite::SqliteRow| {
            row.get::<String, _>("value")
        })
        .fetch_optional(pool)
        .await?;

    // Convert the result to a KeyValue if found
    Ok(result.map(|value| KeyValue {
        key: key.to_string(),
        value,
    }))
}

/// Delete a value by its key
pub async fn delete_value(pool: &SqlitePool, key: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM key_values WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    
    // Return whether any row was affected (i.e., key existed)
    Ok(result.rows_affected() > 0)
}

/// List all keys in the database
pub async fn list_keys(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query("SELECT key FROM key_values ORDER BY key")
        .map(|row: sqlx::sqlite::SqliteRow| {
            row.get::<String, _>("key")
        })
        .fetch_all(pool)
        .await?;
    
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    async fn setup_test_db() -> Pool<Sqlite> {
        // Use in-memory SQLite database for testing
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        
        // Set up the schema in the test database
        setup_schema(&pool).await.unwrap();
        
        pool
    }
    
    #[tokio::test]
    async fn test_put_and_get_value() {
        let pool = setup_test_db().await;
        
        // Create a key-value pair
        let test_kv = KeyValue {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
        };
        
        // Put the value in the database
        put_value(&pool, &test_kv).await.expect("Failed to put value");
        
        // Retrieve the value
        let result = get_value(&pool, &test_kv.key).await.expect("Failed to get value");
        
        assert!(result.is_some());
        let retrieved_kv = result.unwrap();
        assert_eq!(retrieved_kv.key, test_kv.key);
        assert_eq!(retrieved_kv.value, test_kv.value);
    }
    
    #[tokio::test]
    async fn test_get_nonexistent_value() {
        let pool = setup_test_db().await;
        
        // Try to get a value that doesn't exist
        let result = get_value(&pool, "nonexistent_key").await.expect("Query failed");
        
        // Should be None
        assert!(result.is_none());
    }
    
    #[tokio::test]
    async fn test_put_replace_value() {
        let pool = setup_test_db().await;
        
        // Create initial key-value pair
        let initial_kv = KeyValue {
            key: "same_key".to_string(),
            value: "initial_value".to_string(),
        };
        
        // Put the value in the database
        put_value(&pool, &initial_kv).await.expect("Failed to put initial value");
        
        // Create updated key-value pair with same key
        let updated_kv = KeyValue {
            key: "same_key".to_string(),
            value: "updated_value".to_string(),
        };
        
        // Replace the value
        put_value(&pool, &updated_kv).await.expect("Failed to update value");
        
        // Retrieve the value
        let result = get_value(&pool, &updated_kv.key).await.expect("Failed to get value");
        
        assert!(result.is_some());
        let retrieved_kv = result.unwrap();
        
        // Should have the updated value
        assert_eq!(retrieved_kv.key, updated_kv.key);
        assert_eq!(retrieved_kv.value, updated_kv.value);
        assert_ne!(retrieved_kv.value, initial_kv.value);
    }
    
    #[tokio::test]
    async fn test_delete_value() {
        let pool = setup_test_db().await;
        
        // Create a key-value pair
        let test_kv = KeyValue {
            key: "key_to_delete".to_string(),
            value: "value_to_delete".to_string(),
        };
        
        // Put the value in the database
        put_value(&pool, &test_kv).await.expect("Failed to put value");
        
        // Verify it exists
        let exists_before = get_value(&pool, &test_kv.key).await.expect("Failed to get value");
        assert!(exists_before.is_some());
        
        // Delete the value
        let deleted = delete_value(&pool, &test_kv.key).await.expect("Failed to delete value");
        assert!(deleted, "Value should have been deleted");
        
        // Verify it's gone
        let exists_after = get_value(&pool, &test_kv.key).await.expect("Failed to check after deletion");
        assert!(exists_after.is_none());
        
        // Try to delete again (should return false - not found)
        let deleted_again = delete_value(&pool, &test_kv.key).await.expect("Failed to re-delete value");
        assert!(!deleted_again, "Value should not exist to be deleted");
    }
    
    #[tokio::test]
    async fn test_list_keys() {
        let pool = setup_test_db().await;
        
        // Initially should be empty
        let empty_keys = list_keys(&pool).await.expect("Failed to list keys");
        assert!(empty_keys.is_empty());
        
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
            put_value(&pool, &kv).await.expect("Failed to put value");
        }
        
        // Get all keys
        let keys = list_keys(&pool).await.expect("Failed to list keys");
        
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
