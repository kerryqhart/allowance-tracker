use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use shared::{KeyValue, TransactionListRequest, TransactionListResponse};
use crate::domain::{ValueStore, TransactionService};
use serde::Deserialize;
use tracing::info;

/// Application state containing the ValueStore and TransactionService
#[derive(Clone)]
pub struct AppState {
    pub value_store: ValueStore,
    pub transaction_service: TransactionService,
}

impl AppState {
    /// Create new application state with the given ValueStore and TransactionService
    pub fn new(value_store: ValueStore, transaction_service: TransactionService) -> Self {
        Self {
            value_store,
            transaction_service,
        }
    }
}

/// Query parameters for transaction list endpoint
#[derive(Deserialize, Debug)]
pub struct TransactionListQuery {
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// Axum handler function for GET /api/transactions
pub async fn list_transactions(
    State(state): State<AppState>,
    Query(query): Query<TransactionListQuery>,
) -> impl IntoResponse {
    info!("GET /api/transactions - query: {:?}", query);

    let request = TransactionListRequest {
        after: query.after,
        limit: query.limit,
        start_date: query.start_date,
        end_date: query.end_date,
    };

    match state.transaction_service.list_transactions(request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error listing transactions: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error listing transactions").into_response()
        }
    }
}

/// Axum handler function for GET /api/values/:key
pub async fn get_value(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    info!("GET /api/values/{}", key);

    match state.value_store.get_value(&key).await {
        Ok(Some(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(e) => {
            tracing::error!("Error retrieving value: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving value").into_response()
        }
    }
}

/// Axum handler function for POST /api/values
pub async fn put_value(
    State(state): State<AppState>,
    Json(kv): Json<KeyValue>,
) -> impl IntoResponse {
    info!("POST /api/values - key: {}", kv.key);

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
        let transaction_service = TransactionService::new(db);
        AppState::new(value_store, transaction_service)
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
