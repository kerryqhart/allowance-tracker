//! # REST API for Money Management
//!
//! Endpoints for adding and spending money.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use log::{info, error};

use crate::backend::AppState;
use shared::{
    AddMoneyRequest, AddMoneyResponse, SpendMoneyRequest, SpendMoneyResponse,
};

use crate::backend::domain::commands::transactions::CreateTransactionCommand;
use crate::backend::io::rest::mappers::transaction_mapper::TransactionMapper;

/// Add money (create a positive transaction)
pub async fn add_money(
    State(state): State<AppState>,
    Json(request): Json<AddMoneyRequest>,
) -> impl IntoResponse {
    info!("POST /api/money/add - request: {:?}", request);

    // Check for active child first
    info!("🔍 Checking for active child...");
    let active_child_response = match state.child_service.get_active_child().await {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to get active child: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving active child").into_response();
        }
    };
    let active_child = match active_child_response.active_child.child {
        Some(child) => {
            info!("✅ Active child found: {}", child.id);
            child
        },
        None => {
            error!("❌ No active child found for add money operation");
            return (StatusCode::BAD_REQUEST, "No active child found. Please select a child first.").into_response();
        }
    };

    // Enhanced validation that includes date validation if provided
    info!("🔍 Starting validation with date: {:?}", request.date);
    let validation = state.money_management_service
        .validate_add_money_form_with_date(
            &request.description, 
            &request.amount.to_string(),
            request.date.as_deref(),
            Some(&active_child.created_at.to_rfc3339())
        );

    info!("🔍 Validation result - is_valid: {}, errors: {:?}", validation.is_valid, validation.errors);

    if !validation.is_valid {
        let error_message = state.money_management_service
            .get_first_error_message(&validation.errors)
            .unwrap_or_else(|| "Invalid input".to_string());
        error!("❌ Validation failed: {}", error_message);
        return (StatusCode::BAD_REQUEST, error_message).into_response();
    }

    // Convert to CreateTransactionRequest
    info!("🔄 Converting to CreateTransactionRequest...");
    let create_request = state.money_management_service
        .to_create_transaction_request(request.clone());
    info!("✅ CreateTransactionRequest: {:?}", create_request);

    // Convert DTO to domain command and create transaction
    let cmd = CreateTransactionCommand {
        description: create_request.description.clone(),
        amount: create_request.amount,
        date: create_request.date.clone(),
    };

    info!("🚀 Creating transaction via TransactionService...");
    match state.transaction_service.create_transaction_domain(cmd).await {
        Ok(domain_tx) => {
            let transaction = TransactionMapper::to_dto(domain_tx);
            info!("✅ Transaction created successfully: {:?}", transaction);
            
            let success_message = if let Some(date) = &request.date {
                // Check if this was a backdated transaction
                match state.money_management_service.is_backdated_transaction(date) {
                    Ok(true) => {
                        info!("📅 This was a backdated transaction");
                        format!("🎉 {} added successfully (backdated to {})!", 
                                      state.money_management_service.format_positive_amount(transaction.amount),
                                      date)
                    },
                    _ => {
                        info!("📅 This was a current-date transaction");
                        state.money_management_service.generate_success_message(transaction.amount)
                    }
                }
            } else {
                info!("📅 No date provided, using current date");
                state.money_management_service.generate_success_message(transaction.amount)
            };
            
            let formatted_amount = state.money_management_service
                .format_positive_amount(transaction.amount);

            let response = AddMoneyResponse {
                transaction_id: transaction.id,
                success_message: success_message.clone(),
                new_balance: transaction.balance,
                formatted_amount,
            };

            info!("✅ Sending success response: {:?}", response);
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("❌ Failed to add money: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to add money").into_response()
        }
    }
}

/// Spend money (create a negative transaction)
pub async fn spend_money(
    State(state): State<AppState>,
    Json(request): Json<SpendMoneyRequest>,
) -> impl IntoResponse {
    info!("POST /api/money/spend - request: {:?}", request);

    // Check for active child first
    info!("🔍 Checking for active child...");
    let active_child_response = match state.child_service.get_active_child().await {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to get active child: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving active child").into_response();
        }
    };
    let active_child = match active_child_response.active_child.child {
        Some(child) => {
            info!("✅ Active child found: {}", child.id);
            child
        },
        None => {
            error!("❌ No active child found for spend money operation");
            return (StatusCode::BAD_REQUEST, "No active child found. Please select a child first.").into_response();
        }
    };

    // Enhanced validation that includes date validation if provided
    info!("🔍 Starting validation with date: {:?}", request.date);
    let validation = state.money_management_service
        .validate_spend_money_form_with_date(
            &request.description, 
            &request.amount.to_string(),
            request.date.as_deref(),
            Some(&active_child.created_at.to_rfc3339())
        );

    info!("🔍 Validation result - is_valid: {}, errors: {:?}", validation.is_valid, validation.errors);

    if !validation.is_valid {
        let error_message = state.money_management_service
            .get_first_error_message(&validation.errors)
            .unwrap_or_else(|| "Invalid input".to_string());
        error!("❌ Validation failed: {}", error_message);
        return (StatusCode::BAD_REQUEST, error_message).into_response();
    }

    // Convert to CreateTransactionRequest (this will make the amount negative)
    info!("🔄 Converting to CreateTransactionRequest...");
    let create_request = state.money_management_service
        .spend_to_create_transaction_request(request.clone());
    info!("✅ CreateTransactionRequest: {:?}", create_request);

    let cmd = CreateTransactionCommand {
        description: create_request.description.clone(),
        amount: create_request.amount,
        date: create_request.date.clone(),
    };

    info!("🚀 Creating transaction via TransactionService...");
    match state.transaction_service.create_transaction_domain(cmd).await {
        Ok(domain_tx) => {
            let transaction = TransactionMapper::to_dto(domain_tx);
            info!("✅ Transaction created successfully: {:?}", transaction);
            
            let success_message = if let Some(date) = &request.date {
                // Check if this was a backdated transaction
                match state.money_management_service.is_backdated_transaction(date) {
                    Ok(true) => {
                        info!("📅 This was a backdated transaction");
                        format!("💸 {} spent successfully (backdated to {})!", 
                                      state.money_management_service.format_amount(request.amount.abs()),
                                      date)
                    },
                    _ => {
                        info!("📅 This was a current-date transaction");
                        state.money_management_service.generate_spend_success_message(request.amount)
                    }
                }
            } else {
                info!("📅 No date provided, using current date");
                state.money_management_service.generate_spend_success_message(request.amount)
            };
            
            let formatted_amount = state.money_management_service
                .format_negative_amount(request.amount);

            let response = SpendMoneyResponse {
                transaction_id: transaction.id,
                success_message: success_message.clone(),
                new_balance: transaction.balance,
                formatted_amount,
            };

            info!("✅ Sending success response: {:?}", response);
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("❌ Failed to spend money: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to record spending").into_response()
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
    use shared::{AddMoneyRequest, SpendMoneyRequest};
    use std::sync::Arc;

    async fn setup_test_state() -> AppState {
        let db = Arc::new(DbConnection::init_test().await.unwrap());
        let transaction_service = TransactionService::new(db.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let child_service = ChildService::new(db.clone());
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db);
        
        // Create a test child and set as active using ChildService
        use shared::{CreateChildRequest, SetActiveChildRequest};
        
        let create_request = CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        
        let child_response = child_service.create_child(create_request).await.expect("Failed to create test child");
        
        let set_active_request = SetActiveChildRequest {
            child_id: child_response.child.id.clone(),
        };
        
        child_service.set_active_child(set_active_request).await.expect("Failed to set active child");
        
        AppState {
            transaction_service: Arc::new(transaction_service),
            calendar_service: Arc::new(calendar_service),
            transaction_table_service: Arc::new(transaction_table_service),
            money_management_service: Arc::new(money_management_service),
            child_service: Arc::new(child_service),
            parental_control_service: Arc::new(parental_control_service),
            allowance_service: Arc::new(allowance_service),
            balance_service: Arc::new(balance_service),
        }
    }

    #[tokio::test]
    async fn test_add_money_success() {
        let state = setup_test_state().await;
        let request = AddMoneyRequest {
            amount: 50.0,
            description: "Test deposit".to_string(),
            date: None,
        };
        let response = add_money(State(state), Json(request)).await.into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_add_money_no_active_child() {
        // Create an app state without an active child
        let db = Arc::new(DbConnection::init_test().await.unwrap());
        let transaction_service = TransactionService::new(db.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let child_service = ChildService::new(db.clone());
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db);
        let state = AppState {
            transaction_service: Arc::new(transaction_service),
            calendar_service: Arc::new(calendar_service),
            transaction_table_service: Arc::new(transaction_table_service),
            money_management_service: Arc::new(money_management_service),
            child_service: Arc::new(child_service),
            parental_control_service: Arc::new(parental_control_service),
            allowance_service: Arc::new(allowance_service),
            balance_service: Arc::new(balance_service),
        };

        let request = AddMoneyRequest {
            amount: 50.0,
            description: "Test deposit".to_string(),
            date: None,
        };
        let response = add_money(State(state), Json(request)).await.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_spend_money_success() {
        let state = setup_test_state().await;
        let request = SpendMoneyRequest {
            amount: 20.0,
            description: "Test withdrawal".to_string(),
            date: None,
        };
        let response = spend_money(State(state), Json(request)).await.into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_spend_money_no_active_child() {
        // Create an app state without an active child
        let db = Arc::new(DbConnection::init_test().await.unwrap());
        let transaction_service = TransactionService::new(db.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let child_service = ChildService::new(db.clone());
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db);
        let state = AppState {
            transaction_service: Arc::new(transaction_service),
            calendar_service: Arc::new(calendar_service),
            transaction_table_service: Arc::new(transaction_table_service),
            money_management_service: Arc::new(money_management_service),
            child_service: Arc::new(child_service),
            parental_control_service: Arc::new(parental_control_service),
            allowance_service: Arc::new(allowance_service),
            balance_service: Arc::new(balance_service),
        };

        let request = SpendMoneyRequest {
            amount: 20.0,
            description: "Test withdrawal".to_string(),
            date: None,
        };
        let response = spend_money(State(state), Json(request)).await.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    
    #[tokio::test]
    async fn test_add_money_invalid_input() {
        let state = setup_test_state().await;
        let request = AddMoneyRequest {
            amount: -10.0, // Invalid amount
            description: "".to_string(), // Invalid description
            date: None,
        };
        let response = add_money(State(state), Json(request)).await.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_spend_money_invalid_input() {
        let state = setup_test_state().await;
        let request = SpendMoneyRequest {
            amount: 0.0, // Invalid amount
            description: " ".to_string(), // Invalid description
            date: None,
        };
        let response = spend_money(State(state), Json(request)).await.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
} 