//! # REST API for Goal Management
//!
//! Endpoints for creating, retrieving, updating, and managing goals.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put, delete},
    Router,
};
use log::{info, error};

use crate::backend::AppState;
use crate::backend::domain::commands::goal::{
    CreateGoalCommand, CancelGoalCommand, GetCurrentGoalCommand, GetGoalHistoryCommand, UpdateGoalCommand,
};
use crate::backend::io::rest::mappers::goal_mapper::GoalMapper;
use shared::{
    CreateGoalRequest, UpdateGoalRequest, GetCurrentGoalRequest, 
    GetGoalHistoryRequest, CancelGoalRequest,
};

/// Create a router for goal related APIs
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/current", get(get_current_goal))
        .route("/", post(create_goal).put(update_goal).delete(cancel_goal))
        .route("/history", get(get_goal_history))
}

/// Get current active goal with projection calculations
pub async fn get_current_goal(
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("GET /api/goals/current");

    let command = GetCurrentGoalCommand {
        child_id: None, // Use active child
    };

    match state.goal_service.get_current_goal(command).await {
        Ok(result) => {
            // Convert domain result back to DTO for response
            let response = GoalMapper::to_get_current_goal_response(result.goal, result.calculation);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to get current goal: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving current goal").into_response()
        }
    }
}

/// Create a new goal
pub async fn create_goal(
    State(state): State<AppState>,
    Json(request): Json<CreateGoalRequest>,
) -> impl IntoResponse {
    info!("POST /api/goals - request: {:?}", request);

    let command = CreateGoalCommand {
        child_id: request.child_id,
        description: request.description,
        target_amount: request.target_amount,
    };

    match state.goal_service.create_goal(command).await {
        Ok(result) => {
            // Convert domain result back to DTO for response
            let response = GoalMapper::to_create_goal_response(result.goal, result.calculation, result.success_message);
            (StatusCode::CREATED, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to create goal: {}", e);
            let status = if e.to_string().contains("already has an active goal") {
                StatusCode::CONFLICT
            } else if e.to_string().contains("must be greater than current balance") {
                StatusCode::BAD_REQUEST
            } else if e.to_string().contains("cannot be empty") || e.to_string().contains("must be positive") {
                StatusCode::BAD_REQUEST
            } else if e.to_string().contains("No active child found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, e.to_string()).into_response()
        }
    }
}

/// Update current active goal
pub async fn update_goal(
    State(state): State<AppState>,
    Json(request): Json<UpdateGoalRequest>,
) -> impl IntoResponse {
    info!("PUT /api/goals - request: {:?}", request);

    let command = UpdateGoalCommand {
        child_id: request.child_id,
        description: request.description,
        target_amount: request.target_amount,
    };

    match state.goal_service.update_goal(command).await {
        Ok(result) => {
            // Convert domain result back to DTO for response
            let response = GoalMapper::to_update_goal_response(result.goal, result.calculation, result.success_message);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to update goal: {}", e);
            let status = if e.to_string().contains("No active goal found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("cannot be empty") || e.to_string().contains("must be positive") {
                StatusCode::BAD_REQUEST
            } else if e.to_string().contains("must be greater than current balance") {
                StatusCode::BAD_REQUEST
            } else if e.to_string().contains("No active child found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, e.to_string()).into_response()
        }
    }
}

/// Cancel current active goal
pub async fn cancel_goal(
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("DELETE /api/goals");

    let command = CancelGoalCommand {
        child_id: None, // Use active child
    };

    match state.goal_service.cancel_goal(command).await {
        Ok(result) => {
            // Convert domain result back to DTO for response
            let response = GoalMapper::to_cancel_goal_response(result.goal, result.success_message);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to cancel goal: {}", e);
            let status = if e.to_string().contains("No active goal found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("No active child found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, e.to_string()).into_response()
        }
    }
}

/// Get goal history for current active child
pub async fn get_goal_history(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<GetGoalHistoryRequest>,
) -> impl IntoResponse {
    info!("GET /api/goals/history - query: {:?}", query);

    let command = GetGoalHistoryCommand {
        child_id: query.child_id,
        limit: query.limit,
    };

    match state.goal_service.get_goal_history(command).await {
        Ok(result) => {
            // Convert domain result back to DTO for response
            let response = GoalMapper::to_get_goal_history_response(result.goals);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to get goal history: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving goal history").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::CsvConnection;
    use crate::backend::domain::{GoalService, TransactionService, CalendarService, TransactionTableService, MoneyManagementService, child_service::ChildService, AllowanceService, BalanceService};
    use crate::backend::AppState;
    use axum::http::StatusCode;
    use shared::{CreateGoalRequest, UpdateGoalRequest, CreateChildRequest};
    use std::sync::Arc;
    use tempfile;

    async fn setup_test_app_state() -> AppState {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db = Arc::new(CsvConnection::new(temp_dir.path()).expect("Failed to init test DB"));
        
        // Create all required services
        let child_service = ChildService::new(db.clone());
        let allowance_service = AllowanceService::new(db.clone());
        let balance_service = BalanceService::new(db.clone());
        let transaction_service = TransactionService::new(db.clone(), child_service.clone(), allowance_service.clone(), balance_service.clone());
        let goal_service = GoalService::new(db.clone(), child_service.clone(), allowance_service.clone(), transaction_service.clone(), balance_service.clone());
        let calendar_service = CalendarService::new();
        let transaction_table_service = TransactionTableService::new();
        let money_management_service = MoneyManagementService::new();
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db.clone());
        let data_directory_service = crate::backend::domain::DataDirectoryService::new(db.clone(), std::sync::Arc::new(child_service.clone()));
        let export_service = crate::backend::domain::ExportService::new();
        
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
            export_service,
        }
    }

    async fn create_test_child_with_allowance(app_state: &AppState) -> String {
        // Create a test child
        let child_command = crate::backend::domain::commands::child::CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2010-01-01".to_string(),
        };
        
        let child_result = app_state.child_service.create_child(child_command).await
            .expect("Failed to create test child");
        
        // Set as active child
        let set_active_command = crate::backend::domain::commands::child::SetActiveChildCommand {
            child_id: child_result.child.id.clone(),
        };
        app_state.child_service.set_active_child(set_active_command).await
            .expect("Failed to set active child");
        
        // Set up allowance
        let allowance_command = crate::backend::domain::commands::allowance::UpdateAllowanceConfigCommand {
            child_id: Some(child_result.child.id.clone()),
            amount: 5.0,
            day_of_week: 5, // Friday
            is_active: true,
        };
        
        app_state.allowance_service.update_allowance_config(allowance_command).await
            .expect("Failed to set up allowance");
        
        child_result.child.id
    }

    #[tokio::test]
    async fn test_create_goal_api() {
        let app_state = setup_test_app_state().await;
        let _child_id = create_test_child_with_allowance(&app_state).await;
        
        let request = CreateGoalRequest {
            child_id: None, // Use active child
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        let response = create_goal(State(app_state), Json(request)).await;
        
        // Should return 201 Created
        // Note: In a real test, we'd check the response status and body
        // This is a basic structure test
    }

    #[tokio::test]
    async fn test_get_current_goal_api() {
        let app_state = setup_test_app_state().await;
        let _child_id = create_test_child_with_allowance(&app_state).await;
        
        // First create a goal
        let create_request = CreateGoalRequest {
            child_id: None,
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        let _create_response = create_goal(State(app_state.clone()), Json(create_request)).await;
        
        // Then get current goal
        let response = get_current_goal(State(app_state)).await;
        
        // Should return 200 OK
        // Note: In a real test, we'd check the response status and body
    }

    #[tokio::test]
    async fn test_cancel_goal_api() {
        let app_state = setup_test_app_state().await;
        let _child_id = create_test_child_with_allowance(&app_state).await;
        
        // First create a goal
        let create_request = CreateGoalRequest {
            child_id: None,
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        let _create_response = create_goal(State(app_state.clone()), Json(create_request)).await;
        
        // Then cancel the goal
        let response = cancel_goal(State(app_state)).await;
        
        // Should return 200 OK
        // Note: In a real test, we'd check the response status and body
    }

    #[tokio::test]
    async fn test_update_goal_api() {
        let app_state = setup_test_app_state().await;
        let _child_id = create_test_child_with_allowance(&app_state).await;
        
        // First create a goal
        let create_request = CreateGoalRequest {
            child_id: None,
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        let _create_response = create_goal(State(app_state.clone()), Json(create_request)).await;
        
        // Then update the goal
        let update_request = UpdateGoalRequest {
            child_id: None,
            description: Some("Buy better toy".to_string()),
            target_amount: Some(30.0),
        };
        
        let response = update_goal(State(app_state), Json(update_request)).await;
        
        // Should return 200 OK
        // Note: In a real test, we'd check the response status and body
    }
} 