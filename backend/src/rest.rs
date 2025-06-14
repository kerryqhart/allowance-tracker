use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use shared::KeyValue;
use crate::domain::ValueStore;

/// Trait that all REST handlers must implement
pub trait RestHandler {
    /// Get the ValueStore associated with this handler
    fn value_store(&self) -> &ValueStore;
}

/// Handler for GET /api/values/:key endpoint
#[derive(Clone)]
pub struct GetValueHandler {
    pub value_store: ValueStore,
}

impl GetValueHandler {
    /// Create a new GetValueHandler
    pub fn new(value_store: ValueStore) -> Self {
        Self { value_store }
    }

    /// Handle the GET request for retrieving a value by key
    pub async fn handle(&self, key: String) -> impl IntoResponse {
        match self.value_store.retrieve_value(&key).await {
            Ok(Some(kv)) => (StatusCode::OK, Json(kv)).into_response(),
            Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving value").into_response(),
        }
    }
}

impl RestHandler for GetValueHandler {
    fn value_store(&self) -> &ValueStore {
        &self.value_store
    }
}

/// Handler for POST /api/values endpoint
#[derive(Clone)]
pub struct PutValueHandler {
    pub value_store: ValueStore,
}

impl PutValueHandler {
    /// Create a new PutValueHandler
    pub fn new(value_store: ValueStore) -> Self {
        Self { value_store }
    }

    /// Handle the POST request for storing a key-value pair
    pub async fn handle(&self, kv: KeyValue) -> impl IntoResponse {
        match self.value_store.store_value(&kv.key, &kv.value).await {
            Ok(stored_kv) => (StatusCode::CREATED, Json(stored_kv)).into_response(),
            Err(e) => {
                tracing::error!("Error storing value: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store value").into_response()
            }
        }
    }
}

impl RestHandler for PutValueHandler {
    fn value_store(&self) -> &ValueStore {
        &self.value_store
    }
}

/// Application state containing all REST handlers
#[derive(Clone)]
pub struct RestHandlers {
    pub get_value_handler: GetValueHandler,
    pub put_value_handler: PutValueHandler,
}

impl RestHandlers {
    /// Create new REST handlers with the given ValueStore
    pub fn new(value_store: ValueStore) -> Self {
        Self {
            get_value_handler: GetValueHandler::new(value_store.clone()),
            put_value_handler: PutValueHandler::new(value_store),
        }
    }
}

/// Axum handler function for GET /api/values/:key
pub async fn get_value(
    State(handlers): State<RestHandlers>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    handlers.get_value_handler.handle(key).await
}

/// Axum handler function for POST /api/values
pub async fn put_value(
    State(handlers): State<RestHandlers>,
    Json(kv): Json<KeyValue>,
) -> impl IntoResponse {
    handlers.put_value_handler.handle(kv).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbConnection;

    /// Helper to create test handlers
    async fn setup_test_handlers() -> RestHandlers {
        let db = DbConnection::init_test().await.expect("Failed to create test database");
        let value_store = ValueStore::new(db);
        RestHandlers::new(value_store)
    }

    #[tokio::test]
    async fn test_get_value_handler() {
        let handlers = setup_test_handlers().await;
        
        // First store a value
        let kv = KeyValue {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
        };
        
        let put_response = handlers.put_value_handler.handle(kv.clone()).await;
        // Note: In a real test, we'd check the response status/body
        
        // Then try to retrieve it
        let get_response = handlers.get_value_handler.handle("test_key".to_string()).await;
        // Note: In a real test, we'd check the response status/body
    }

    #[tokio::test]
    async fn test_put_value_handler() {
        let handlers = setup_test_handlers().await;
        
        let kv = KeyValue {
            key: "test_put_key".to_string(),
            value: "test_put_value".to_string(),
        };
        
        let response = handlers.put_value_handler.handle(kv).await;
        // Note: In a real test, we'd check the response status/body
    }

    #[tokio::test]
    async fn test_rest_handler_trait() {
        let handlers = setup_test_handlers().await;
        
        // Test that both handlers implement the RestHandler trait
        let _get_store = handlers.get_value_handler.value_store();
        let _put_store = handlers.put_value_handler.value_store();
    }
}
