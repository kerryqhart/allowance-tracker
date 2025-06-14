use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use shared::KeyValue;
use crate::domain::ValueStore;

/// Application state containing the ValueStore
#[derive(Clone)]
pub struct AppState {
    pub value_store: ValueStore,
}

impl AppState {
    /// Create new application state with the given ValueStore
    pub fn new(value_store: ValueStore) -> Self {
        Self {
            value_store,
        }
    }
}

/// Axum handler function for GET /api/values/:key
pub async fn get_value(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    match state.value_store.get_value(&key).await {
        Ok(Some(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving value").into_response(),
    }
}

/// Axum handler function for POST /api/values
pub async fn put_value(
    State(state): State<AppState>,
    Json(kv): Json<KeyValue>,
) -> impl IntoResponse {
    match state.value_store.put_value(&kv.key, &kv.value).await {
        Ok(()) => (StatusCode::CREATED, Json(kv)).into_response(),
        Err(e) => {
            tracing::error!("Error storing value: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store value").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbConnection;

    /// Helper to create test handlers
    async fn setup_test_handlers() -> AppState {
        let db = DbConnection::init_test().await.expect("Failed to create test database");
        let value_store = ValueStore::new(db);
        AppState::new(value_store)
    }

    #[tokio::test]
    async fn test_get_value_handler() {
        let state = setup_test_handlers().await;
        
        // First store a value
        let kv = KeyValue {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
        };
        
        let put_response = put_value(State(state.clone()), Json(kv.clone())).await;
        // Note: In a real test, we'd check the response status/body
        
        // Then try to retrieve it
        let get_response = get_value(State(state), Path("test_key".to_string())).await;
        // Note: In a real test, we'd check the response status/body
    }

    #[tokio::test]
    async fn test_put_value_handler() {
        let state = setup_test_handlers().await;
        
        let kv = KeyValue {
            key: "test_put_key".to_string(),
            value: "test_put_value".to_string(),
        };
        
        let response = put_value(State(state), Json(kv)).await;
        // Note: In a real test, we'd check the response status/body
    }
}
