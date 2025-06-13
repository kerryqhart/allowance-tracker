use anyhow::Result;
use shared::KeyValue;
use crate::db::DbConnection;

/// ValueStore provides domain operations for working with key-value data
#[derive(Clone)]
pub struct ValueStore {
    db: DbConnection,
}

impl ValueStore {
    /// Create a new ValueStore with the given database connection
    pub fn new(db: DbConnection) -> Self {
        Self { db }
    }

    /// Store a value with the given key
    /// 
    /// This will overwrite any existing value for the same key.
    /// Returns the stored key-value pair on success.
    pub async fn store_value(&self, key: &str, value: &str) -> Result<KeyValue> {
        let kv = KeyValue {
            key: key.to_string(),
            value: value.to_string(),
        };
        
        // Store the key-value pair in the database
        self.db.put_value(&kv).await?;
        
        Ok(kv)
    }

    /// Retrieve a value by its key
    ///
    /// Returns None if the key doesn't exist
    pub async fn retrieve_value(&self, key: &str) -> Result<Option<KeyValue>> {
        self.db.get_value(key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Helper to create a test ValueStore with an in-memory database
    async fn setup_test_store() -> ValueStore {
        let db = DbConnection::init_test().await.expect("Failed to create test database");
        ValueStore::new(db)
    }
    
    #[tokio::test]
    async fn test_store_and_retrieve_value() {
        // Create a new store for this test
        let store = setup_test_store().await;
        
        // Store a test value
        let key = "test_domain_key";
        let value = "test_domain_value";
        let stored = store.store_value(key, value).await.expect("Failed to store value");
        
        // Check the returned value from store_value
        assert_eq!(stored.key, key);
        assert_eq!(stored.value, value);
        
        // Retrieve the value and check it matches
        let retrieved = store.retrieve_value(key).await.expect("Failed to retrieve value");
        assert!(retrieved.is_some());
        
        let retrieved_kv = retrieved.unwrap();
        assert_eq!(retrieved_kv.key, key);
        assert_eq!(retrieved_kv.value, value);
    }
    
    #[tokio::test]
    async fn test_retrieve_nonexistent_value() {
        // Create a new store for this test
        let store = setup_test_store().await;
        
        // Try to retrieve a value that doesn't exist
        let result = store.retrieve_value("nonexistent_domain_key").await.expect("Query failed");
        
        // Should be None
        assert!(result.is_none());
    }
    
    #[tokio::test]
    async fn test_overwrite_existing_value() {
        // Create a new store for this test
        let store = setup_test_store().await;
        
        // Store an initial value
        let key = "overwrite_test_key";
        let initial_value = "initial_value";
        store.store_value(key, initial_value).await.expect("Failed to store initial value");
        
        // Store a new value with the same key
        let new_value = "updated_value";
        let updated = store.store_value(key, new_value).await.expect("Failed to update value");
        
        // Check the returned value reflects the update
        assert_eq!(updated.key, key);
        assert_eq!(updated.value, new_value);
        
        // Retrieve the value and verify it's the updated one
        let retrieved = store.retrieve_value(key).await.expect("Failed to retrieve value").unwrap();
        assert_eq!(retrieved.value, new_value);
        assert_ne!(retrieved.value, initial_value);
    }
}
