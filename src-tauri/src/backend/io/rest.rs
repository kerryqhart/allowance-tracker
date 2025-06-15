//! # REST API Interface Layer
//!
//! Provides HTTP REST endpoints for the allowance tracker application.
//! This layer handles:
//! - HTTP request/response serialization and deserialization  
//! - Input validation and sanitization
//! - Error translation from domain to HTTP status codes
//! - CORS configuration for frontend integration
//! - Request logging and monitoring
//!
//! ## Key Responsibilities
//!
//! - **API Endpoints**: RESTful HTTP interfaces for all operations
//! - **Error Handling**: Converting domain errors to proper HTTP responses  
//! - **Serialization**: JSON request/response handling
//! - **Input Validation**: Basic input checking before domain layer processing
//! - **Logging**: Request/response logging for debugging and monitoring
//!
//! ## Design Principles
//!
//! - **REST Compliance**: Following RESTful design patterns
//! - **Error Transparency**: Clear error messages for debugging
//! - **Request Logging**: Comprehensive logging for troubleshooting
//! - **Domain Separation**: Pure translation layer without business logic

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;
use log::{info, error};

use crate::backend::AppState;
use shared::{
    TransactionListRequest, CreateTransactionRequest,
    CalendarMonthRequest, TransactionTableResponse,
    ValidationResult, AddMoneyRequest, AddMoneyResponse,
};

// Query parameters for transaction listing API
#[derive(Debug, Deserialize)]
pub struct TransactionListQuery {
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

// Query parameters for calendar month API
#[derive(Debug, Deserialize)]
pub struct CalendarMonthQuery {
    pub month: u32,
    pub year: u32,
}

// Query parameters for transaction table API
#[derive(Debug, Deserialize)]
pub struct TransactionTableQuery {
    pub limit: Option<u32>,
    pub after: Option<String>,
}

// Request body for transaction validation API
#[derive(Debug, Deserialize)]
pub struct ValidateTransactionRequest {
    pub description: String,
    pub amount: String,
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

/// Get calendar month data with transactions
pub async fn get_calendar_month(
    State(state): State<AppState>,
    Query(query): Query<CalendarMonthQuery>,
) -> impl IntoResponse {
    info!("GET /api/calendar/month - query: {:?}", query);

    let request = CalendarMonthRequest {
        month: query.month,
        year: query.year,
    };

    match state.transaction_service.list_transactions(TransactionListRequest {
        after: None,
        limit: Some(1000), // Get enough transactions for calendar calculations
        start_date: None,
        end_date: None,
    }).await {
        Ok(transactions_response) => {
            let calendar_month = state.calendar_service.generate_calendar_month(
                request.month,
                request.year,
                transactions_response.transactions,
            );
            (StatusCode::OK, Json(calendar_month)).into_response()
        }
        Err(e) => {
            error!("Failed to fetch transactions for calendar: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error generating calendar").into_response()
        }
    }
}

/// Get formatted transaction table data
pub async fn get_transaction_table(
    State(state): State<AppState>,
    Query(query): Query<TransactionTableQuery>,
) -> impl IntoResponse {
    info!("GET /api/transactions/table - query: {:?}", query);

    let request = TransactionListRequest {
        after: query.after.clone(),
        limit: query.limit,
        start_date: None,
        end_date: None,
    };

    match state.transaction_service.list_transactions(request).await {
        Ok(transactions_response) => {
            let formatted_transactions = state.transaction_table_service
                .format_transactions_for_table(&transactions_response.transactions);
            
            let table_response = TransactionTableResponse {
                formatted_transactions,
                pagination: transactions_response.pagination,
            };
            
            (StatusCode::OK, Json(table_response)).into_response()
        }
        Err(e) => {
            error!("Failed to get transaction table data: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error getting transaction table").into_response()
        }
    }
}

/// Validate transaction input without creating the transaction
pub async fn validate_transaction(
    State(state): State<AppState>,
    Json(request): Json<ValidateTransactionRequest>,
) -> impl IntoResponse {
    info!("POST /api/transactions/validate - request: {:?}", request);

    let validation_result = state.transaction_table_service
        .validate_transaction_input(&request.description, &request.amount);

    (StatusCode::OK, Json(validation_result)).into_response()
}

/// Validate add money form input
pub async fn validate_add_money_form(
    State(state): State<AppState>,
    Json(request): Json<ValidateAddMoneyRequest>,
) -> impl IntoResponse {
    info!("POST /api/money/validate - request: {:?}", request);

    let validation_result = state.money_management_service
        .validate_add_money_form(&request.description, &request.amount_input);

    (StatusCode::OK, Json(validation_result)).into_response()
}

/// Add money (create a positive transaction)
pub async fn add_money(
    State(state): State<AppState>,
    Json(request): Json<AddMoneyRequest>,
) -> impl IntoResponse {
    info!("POST /api/money/add - request: {:?}", request);

    // First validate the request
    let validation = state.money_management_service
        .validate_add_money_form(&request.description, &request.amount.to_string());

    if !validation.is_valid {
        let error_message = state.money_management_service
            .get_first_error_message(&validation.errors)
            .unwrap_or_else(|| "Invalid input".to_string());
        return (StatusCode::BAD_REQUEST, error_message).into_response();
    }

    // Convert to CreateTransactionRequest
    let create_request = state.money_management_service
        .to_create_transaction_request(request);

    // Create the transaction
    match state.transaction_service.create_transaction(create_request).await {
        Ok(transaction) => {
            let success_message = state.money_management_service
                .generate_success_message(transaction.amount);
            
            let formatted_amount = state.money_management_service
                .format_positive_amount(transaction.amount);

            let response = AddMoneyResponse {
                transaction_id: transaction.id,
                success_message,
                new_balance: transaction.balance,
                formatted_amount,
            };

            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to add money: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to add money").into_response()
        }
    }
}

// Request body for add money form validation API
#[derive(Debug, Deserialize)]
pub struct ValidateAddMoneyRequest {
    pub description: String,
    pub amount_input: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::DbConnection;
    use crate::backend::domain::TransactionService;
    use crate::backend::AppState;

    /// Helper to create test handlers
    async fn setup_test_handlers() -> AppState {
        let db = DbConnection::init_test().await.expect("Failed to create test database");
        let transaction_service = TransactionService::new(db);
        let calendar_service = crate::backend::domain::CalendarService::new();
        let transaction_table_service = crate::backend::domain::TransactionTableService::new();
        
        AppState {
            transaction_service,
            calendar_service,
            transaction_table_service,
        }
    }

    #[tokio::test]
    async fn test_create_transaction_handler() {
        let state = setup_test_handlers().await;
        
        let request = CreateTransactionRequest {
            description: "Test transaction".to_string(),
            amount: 15.0,
            date: None,
        };
        
        let _response = create_transaction(State(state), Json(request)).await;
        
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
        
        let _response = create_transaction(State(state), Json(request)).await;
        
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
        
        let _response = create_transaction(State(state), Json(request)).await;
        
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
        
        let _list_response = list_transactions(State(state), Query(list_query)).await;
        
        // Should return 200 OK with transaction list
        // The created transaction should appear in the list
    }

    #[tokio::test]
    async fn test_validate_transaction_handler() {
        let state = setup_test_handlers().await;
        
        let request = ValidateTransactionRequest {
            description: "Valid transaction".to_string(),
            amount: "10.50".to_string(),
        };
        
        let _response = validate_transaction(State(state), Json(request)).await;
        
        // Should return validation result
    }

    #[tokio::test]
    async fn test_get_transaction_table_handler() {
        let state = setup_test_handlers().await;
        
        let query = TransactionTableQuery {
            limit: Some(10),
            after: None,
        };
        
        let _response = get_transaction_table(State(state), Query(query)).await;
        
        // Should return formatted transaction table data
    }
}
