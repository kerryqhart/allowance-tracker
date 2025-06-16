//! # REST API for Transactions
//!
//! Endpoints for listing and creating transactions.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;
use log::{info, error};

use crate::backend::AppState;
use shared::{
    CreateTransactionRequest,
    TransactionListRequest,
};

// Query parameters for transaction listing API
#[derive(Debug, Deserialize)]
pub struct TransactionListQuery {
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// List transactions with optional filtering and pagination
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
            error!("Failed to list transactions: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error listing transactions").into_response()
        }
    }
}

/// Create a new transaction
pub async fn create_transaction(
    State(state): State<AppState>,
    Json(request): Json<CreateTransactionRequest>,
) -> impl IntoResponse {
    info!("POST /api/transactions - request: {:?}", request);

    match state.transaction_service.create_transaction(request).await {
        Ok(transaction) => (StatusCode::CREATED, Json(transaction)).into_response(),
        Err(e) => {
            error!("Failed to create transaction: {}", e);
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::io::rest::tests::setup_test_handlers;
    use axum::{Router, TestServer, http::StatusCode, routing::get};
    use serde_json::Value;
    use shared::CreateTransactionRequest;

    #[tokio::test]
    async fn test_create_transaction_handler() {
        let state = setup_test_handlers().await;
        
        let request = CreateTransactionRequest {
            description: "Test transaction".to_string(),
            amount: 15.0,
            date: None,
            child_id: None,
        };
        
        let response = create_transaction(State(state), Json(request)).await;
        
        assert_eq!(response.into_response().status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_create_transaction_validation_error() {
        let state = setup_test_handlers().await;
        
        // Test with empty description (should fail validation)
        let request = CreateTransactionRequest {
            description: "".to_string(),
            amount: 10.0,
            date: None,
            child_id: None,
        };
        
        let response = create_transaction(State(state), Json(request)).await;
        
        assert_eq!(response.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_transaction_with_custom_date() {
        let state = setup_test_handlers().await;
        
        let request = CreateTransactionRequest {
            description: "Custom date transaction".to_string(),
            amount: -5.0,
            date: Some("2025-06-14T10:30:00-04:00".to_string()),
            child_id: None,
        };
        
        let response = create_transaction(State(state), Json(request)).await;

        assert_eq!(response.into_response().status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_list_transactions_handler() {
        let app_state = setup_test_handlers().await;
        let app = Router::new()
            .route("/transactions", get(list_transactions))
            .with_state(app_state.clone());

        let server = TestServer::new(app).unwrap();

        // Create a transaction to be fetched
        let _ = app_state
            .transaction_service
            .create_transaction(CreateTransactionRequest {
                description: "Test".to_string(),
                amount: 10.0,
                date: Some("2025-06-16".to_string()),
                child_id: None,
            })
            .await;

        let response = server.get("/transactions?limit=5").await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body: Value = response.json();
        assert_eq!(body["transactions"].as_array().unwrap().len(), 1);
        assert_eq!(body["pagination"]["has_more"], false);
    }
} 