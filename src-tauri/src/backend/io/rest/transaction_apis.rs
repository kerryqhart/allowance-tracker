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
    DeleteTransactionsRequest,
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

/// Delete multiple transactions
pub async fn delete_transactions(
    State(state): State<AppState>,
    Json(request): Json<DeleteTransactionsRequest>,
) -> impl IntoResponse {
    info!("DELETE /api/transactions - request: {:?}", request);

    match state.transaction_service.delete_transactions(request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            error!("Failed to delete transactions: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error deleting transactions").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::DbConnection;
    use crate::backend::domain::{TransactionService, CalendarService, TransactionTableService, MoneyManagementService, child_service::ChildService};
    use crate::backend::AppState;
    use axum::http::StatusCode;
    use shared::{CreateTransactionRequest, DeleteTransactionsRequest};
    use std::sync::Arc;

    async fn setup_test_state() -> AppState {
        let db = Arc::new(DbConnection::init_test().await.unwrap());
        let transaction_service = TransactionService::new(db.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let child_service = ChildService::new(db);
        
        AppState {
            transaction_service,
            calendar_service,
            transaction_table_service,
            money_management_service,
            child_service,
        }
    }

    #[tokio::test]
    async fn test_create_transaction_handler() {
        let state = setup_test_state().await;
        
        let request = CreateTransactionRequest {
            description: "Test transaction".to_string(),
            amount: 15.0,
            date: None,
        };
        
        let response = create_transaction(State(state), Json(request)).await;
        
        assert_eq!(response.into_response().status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_create_transaction_validation_error() {
        let state = setup_test_state().await;
        
        // Test with empty description (should fail validation)
        let request = CreateTransactionRequest {
            description: "".to_string(),
            amount: 10.0,
            date: None,
        };
        
        let response = create_transaction(State(state), Json(request)).await;
        
        assert_eq!(response.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_transaction_with_custom_date() {
        let state = setup_test_state().await;
        
        let request = CreateTransactionRequest {
            description: "Custom date transaction".to_string(),
            amount: -5.0,
            date: Some("2025-06-14T10:30:00-04:00".to_string()),
        };
        
        let response = create_transaction(State(state), Json(request)).await;

        assert_eq!(response.into_response().status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_delete_transactions_handler() {
        let state = setup_test_state().await;
        
        // Create some test transactions
        let tx1 = state.transaction_service.create_transaction(CreateTransactionRequest {
            description: "Transaction 1".to_string(),
            amount: 10.0,
            date: None,
        }).await.unwrap();
        
        let _tx2 = state.transaction_service.create_transaction(CreateTransactionRequest {
            description: "Transaction 2".to_string(),
            amount: 20.0,
            date: None,
        }).await.unwrap();
        
        // Delete one transaction
        let delete_request = DeleteTransactionsRequest {
            transaction_ids: vec![tx1.id.clone()],
        };
        
        let response = delete_transactions(State(state), Json(delete_request)).await;
        assert_eq!(response.into_response().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_transactions_empty_list() {
        let state = setup_test_state().await;
        
        let delete_request = DeleteTransactionsRequest {
            transaction_ids: vec![],
        };
        
        let response = delete_transactions(State(state), Json(delete_request)).await;
        assert_eq!(response.into_response().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_transactions_not_found() {
        let state = setup_test_state().await;
        
        let delete_request = DeleteTransactionsRequest {
            transaction_ids: vec!["nonexistent".to_string()],
        };
        
        let response = delete_transactions(State(state), Json(delete_request)).await;
        assert_eq!(response.into_response().status(), StatusCode::OK);
    }
} 