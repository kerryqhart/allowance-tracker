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
    DeleteTransactionsRequest,
    TransactionListResponse,
};

use crate::backend::domain::commands::transactions::{
    CreateTransactionCommand, TransactionListQuery, DeleteTransactionsCommand,
};

use crate::backend::io::rest::mappers::transaction_mapper::TransactionMapper;

// Query parameters for transaction listing API
#[derive(Debug, Deserialize)]
pub struct ListTransactionsQueryParams {
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// List transactions with optional filtering and pagination
pub async fn list_transactions(
    State(state): State<AppState>,
    Query(query): Query<ListTransactionsQueryParams>,
) -> impl IntoResponse {
    info!("GET /api/transactions - query: {:?}", query);

    let domain_query = TransactionListQuery {
        after: query.after.clone(),
        limit: query.limit,
        start_date: query.start_date.clone(),
        end_date: query.end_date.clone(),
    };

    match state.transaction_service.list_transactions_domain(domain_query).await {
        Ok(result) => {
            let dto_response = TransactionListResponse {
                transactions: result
                    .transactions
                    .into_iter()
                    .map(TransactionMapper::to_dto)
                    .collect(),
                pagination: shared::PaginationInfo {
                    has_more: result.pagination.has_more,
                    next_cursor: result.pagination.next_cursor,
                },
            };
            (StatusCode::OK, Json(dto_response)).into_response()
        },
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

    let cmd = CreateTransactionCommand {
        description: request.description,
        amount: request.amount,
        date: request.date,
    };

    match state.transaction_service.create_transaction_domain(cmd).await {
        Ok(domain_tx) => {
            let dto = TransactionMapper::to_dto(domain_tx);
            (StatusCode::CREATED, Json(dto)).into_response()
        },
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

    let cmd = DeleteTransactionsCommand {
        transaction_ids: request.transaction_ids,
    };

    match state.transaction_service.delete_transactions_domain(cmd).await {
        Ok(res) => {
            let dto = shared::DeleteTransactionsResponse {
                deleted_count: res.deleted_count,
                success_message: res.success_message,
                not_found_ids: res.not_found_ids,
            };
            (StatusCode::OK, Json(dto)).into_response()
        },
        Err(e) => {
            error!("Failed to delete transactions: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error deleting transactions").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::csv::CsvConnection;
    use crate::backend::domain::{TransactionService, CalendarService, TransactionTableService, MoneyManagementService, child_service::ChildService};
    use crate::backend::AppState;
    use axum::http::StatusCode;
    use shared::{CreateTransactionRequest, DeleteTransactionsRequest};
    use std::sync::Arc;

    async fn setup_test_state() -> AppState {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        
        // Create a single child_service instance that will be shared by all services
        let child_service = ChildService::new(db.clone());
        
        // Create and set active child BEFORE creating other services
        use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
        
        let create_command = CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        
        let child_result = child_service.create_child(create_command).await.expect("Failed to create test child");
        
        let set_active_command = SetActiveChildCommand {
            child_id: child_result.child.id.clone(),
        };
        
        child_service.set_active_child(set_active_command).await.expect("Failed to set active child");
        
        // Verify the active child is set
        let active_child_result = child_service.get_active_child().await.expect("Failed to get active child");
        assert!(active_child_result.active_child.child.is_some(), "Active child should be set after child creation");
        
        // Now create other services using the same child_service
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db.clone());
        let transaction_service = TransactionService::new(db.clone(), child_service.clone(), allowance_service.clone(), balance_service.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let goal_service = crate::backend::domain::GoalService::new(db.clone(), child_service.clone(), allowance_service.clone(), transaction_service.clone(), balance_service.clone());
        let data_directory_service = crate::backend::domain::DataDirectoryService::new(db.clone(), Arc::new(child_service.clone()));
        
        // Create the AppState
        AppState {
            transaction_service,
            calendar_service,
            transaction_table_service,
            money_management_service,
            child_service,
            parental_control_service,
            allowance_service,
            balance_service,
            goal_service,
            data_directory_service,
            export_service: crate::backend::domain::ExportService::new(),
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
        
        // Create some test transactions with small delays to avoid duplicate IDs
        use crate::backend::domain::commands::transactions::CreateTransactionCommand;
        
        let cmd1 = CreateTransactionCommand {
            description: "Transaction 1".to_string(),
            amount: 10.0,
            date: None,
        };
        let tx1 = state.transaction_service.create_transaction(cmd1).await.unwrap();
        
        // Small delay to ensure different timestamp for next transaction
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        
        let cmd2 = CreateTransactionCommand {
            description: "Transaction 2".to_string(),
            amount: 20.0,
            date: None,
        };
        let _tx2 = state.transaction_service.create_transaction(cmd2).await.unwrap();
        
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