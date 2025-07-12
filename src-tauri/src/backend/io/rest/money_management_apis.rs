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

    // Use the new orchestration method from money management service
    match state.money_management_service.add_money_complete(
        request,
        &state.child_service,
        &state.transaction_service,
    ).await {
        Ok(response) => {
            info!("✅ Add money operation completed successfully");
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

    // Use the new orchestration method from money management service
    match state.money_management_service.spend_money_complete(
        request,
        &state.child_service,
        &state.transaction_service,
    ).await {
        Ok(response) => {
            info!("✅ Spend money operation completed successfully");
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("❌ Failed to spend money: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to spend money").into_response()
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
    use shared::{AddMoneyRequest, SpendMoneyRequest};
    use std::sync::Arc;

    async fn setup_test_state() -> AppState {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        
        // Create services with proper dependencies
        let child_service = ChildService::new(db.clone());
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db.clone());
        let transaction_service = TransactionService::new(db.clone(), child_service.clone(), allowance_service.clone(), balance_service.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let goal_service = crate::backend::domain::GoalService::new(db.clone(), child_service.clone(), allowance_service.clone(), transaction_service.clone(), balance_service.clone());
        let data_directory_service = crate::backend::domain::DataDirectoryService::new(db.clone(), Arc::new(child_service.clone()));
        
        // Create a test child and set as active using domain commands
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
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        
        let child_service = ChildService::new(db.clone());
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db.clone());
        let transaction_service = TransactionService::new(db.clone(), child_service.clone(), allowance_service.clone(), balance_service.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let goal_service = crate::backend::domain::GoalService::new(db.clone(), child_service.clone(), allowance_service.clone(), transaction_service.clone(), balance_service.clone());
        let data_directory_service = crate::backend::domain::DataDirectoryService::new(db.clone(), Arc::new(child_service.clone()));
        
        let state = AppState {
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
        let temp_dir = tempfile::tempdir().unwrap();
        let db = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        
        let child_service = ChildService::new(db.clone());
        let allowance_service = crate::backend::domain::AllowanceService::new(db.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db.clone());
        let transaction_service = TransactionService::new(db.clone(), child_service.clone(), allowance_service.clone(), balance_service.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let goal_service = crate::backend::domain::GoalService::new(db.clone(), child_service.clone(), allowance_service.clone(), transaction_service.clone(), balance_service.clone());
        let data_directory_service = crate::backend::domain::DataDirectoryService::new(db.clone(), Arc::new(child_service.clone()));
        
        let state = AppState {
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