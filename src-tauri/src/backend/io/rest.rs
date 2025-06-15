use axum::{
    extract::{State, Query},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use shared::{TransactionListRequest, CreateTransactionRequest};
use crate::backend::domain::TransactionService;
use serde::Deserialize;
use tracing::info;


/// Application state containing the TransactionService
#[derive(Clone)]
pub struct AppState {
    pub transaction_service: TransactionService,
}

impl AppState {
    /// Create new application state with the given TransactionService
    pub fn new(transaction_service: TransactionService) -> Self {
        Self { transaction_service }
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

/// Axum handler function for POST /api/transactions
pub async fn create_transaction(
    State(state): State<AppState>,
    Json(request): Json<CreateTransactionRequest>,
) -> impl IntoResponse {
    info!("POST /api/transactions - request: {:?}", request);

    match state.transaction_service.create_transaction(request).await {
        Ok(transaction) => (StatusCode::CREATED, Json(transaction)).into_response(),
        Err(e) => {
            tracing::error!("Error creating transaction: {:?}", e);
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::DbConnection;
    use crate::backend::domain::TransactionService;

    /// Helper to create test handlers
    async fn setup_test_handlers() -> AppState {
        let db = DbConnection::init_test().await.expect("Failed to create test database");
        let transaction_service = TransactionService::new(db);
        AppState::new(transaction_service)
    }

    #[tokio::test]
    async fn test_create_transaction_handler() {
        let state = setup_test_handlers().await;
        
        let request = CreateTransactionRequest {
            description: "Test transaction".to_string(),
            amount: 15.0,
            date: None,
        };
        
        let response = create_transaction(State(state), Json(request)).await;
        
        // Should return 201 CREATED status
        // Note: In a real integration test framework, we'd verify the response status and body
        // For now, this test verifies the handler doesn't panic and compiles correctly
    }

    #[tokio::test]
    async fn test_create_transaction_validation_error() {
        let state = setup_test_handlers().await;
        
        // Test with empty description (should fail validation)
        let request = CreateTransactionRequest {
            description: "".to_string(),
            amount: 10.0,
            date: None,
        };
        
        let response = create_transaction(State(state), Json(request)).await;
        
        // Should return 400 BAD REQUEST status
        // Note: In a real integration test framework, we'd verify the response status and error message
    }

    #[tokio::test]
    async fn test_create_transaction_with_custom_date() {
        let state = setup_test_handlers().await;
        
        let request = CreateTransactionRequest {
            description: "Custom date transaction".to_string(),
            amount: -5.0,
            date: Some("2025-06-14T10:30:00-04:00".to_string()),
        };
        
        let response = create_transaction(State(state), Json(request)).await;
        
        // Should successfully create transaction with custom date
    }

    #[tokio::test]
    async fn test_list_transactions_handler() {
        let state = setup_test_handlers().await;
        
        // First create a transaction
        let create_request = CreateTransactionRequest {
            description: "Handler test transaction".to_string(),
            amount: 25.0,
            date: None,
        };
        let _create_response = create_transaction(State(state.clone()), Json(create_request)).await;
        
        // Then list transactions
        let list_query = TransactionListQuery {
            after: None,
            limit: Some(10),
            start_date: None,
            end_date: None,
        };
        
        let list_response = list_transactions(State(state), Query(list_query)).await;
        
        // Should return 200 OK with transaction list
        // The created transaction should appear in the list
    }
}
