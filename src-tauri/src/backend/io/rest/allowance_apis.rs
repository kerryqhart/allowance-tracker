//! # REST API for Allowance Configuration
//!
//! Endpoints for managing allowance configurations.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use log::{info, error};

use crate::backend::AppState;
use shared::{
    GetAllowanceConfigRequest, UpdateAllowanceConfigRequest,
    GetAllowanceConfigResponse, UpdateAllowanceConfigResponse,
};
use crate::backend::domain::commands::allowance::{GetAllowanceConfigCommand, UpdateAllowanceConfigCommand};
use crate::backend::io::rest::mappers::allowance_mapper::AllowanceMapper;

/// Create a router for allowance related APIs
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_allowance_config).post(update_allowance_config))
}

/// Get allowance configuration
pub async fn get_allowance_config(
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("GET /api/allowance");

    let command = GetAllowanceConfigCommand {
        child_id: None, // Use active child
    };

    match state.allowance_service.get_allowance_config(command).await {
        Ok(result) => {
            // Convert domain result back to DTO for response
            let dto_config = result.allowance_config.map(AllowanceMapper::to_dto);
            let response = GetAllowanceConfigResponse { 
                allowance_config: dto_config 
            };
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to get allowance config: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving allowance configuration").into_response()
        }
    }
}

/// Update allowance configuration
pub async fn update_allowance_config(
    State(state): State<AppState>,
    Json(request): Json<UpdateAllowanceConfigRequest>,
) -> impl IntoResponse {
    info!("POST /api/allowance - request: {:?}", request);

    let command = UpdateAllowanceConfigCommand {
        child_id: request.child_id,
        amount: request.amount,
        day_of_week: request.day_of_week,
        is_active: request.is_active,
    };

    match state.allowance_service.update_allowance_config(command).await {
        Ok(result) => {
            // Convert domain result back to DTO for response
            let dto_config = AllowanceMapper::to_dto(result.allowance_config);
            let response = UpdateAllowanceConfigResponse {
                allowance_config: dto_config,
                success_message: result.success_message,
            };
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to update allowance config: {}", e);
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("Invalid") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, e.to_string()).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::DbConnection;
    use crate::backend::domain::{AllowanceService, child_service::ChildService};
    use crate::backend::AppState;
    use axum::http::StatusCode;
    use shared::{CreateChildRequest, UpdateAllowanceConfigRequest};
    use std::sync::Arc;

    async fn create_test_app_state() -> AppState {
        let db_conn = Arc::new(DbConnection::init_test().await.expect("Failed to init test DB"));
        
        // Create dummy services (only allowance_service and child_service are used in tests)
        let transaction_service = crate::backend::domain::TransactionService::new(db_conn.clone());
        let calendar_service = crate::backend::domain::CalendarService::new();
        let transaction_table_service = crate::backend::domain::TransactionTableService::new();
        let money_management_service = crate::backend::domain::MoneyManagementService::new();
        let child_service = ChildService::new(db_conn.clone());
        let parental_control_service = crate::backend::domain::ParentalControlService::new(db_conn.clone());
        let allowance_service = AllowanceService::new(db_conn.clone());
        let balance_service = crate::backend::domain::BalanceService::new(db_conn);

        AppState {
            transaction_service,
            calendar_service,
            transaction_table_service,
            money_management_service,
            child_service,
            parental_control_service,
            allowance_service,
            balance_service,
        }
    }

    #[tokio::test]
    async fn test_get_allowance_config_no_active_child() {
        let app_state = create_test_app_state().await;
        
        let result = get_allowance_config(State(app_state)).await;
        
        // Should return OK even when no active child (returns None in response)
        // This is handled by the service layer
        match result.into_response().status() {
            StatusCode::OK => {},
            status => panic!("Expected OK, got {:?}", status),
        }
    }

    #[tokio::test]
    async fn test_update_allowance_config_no_active_child() {
        let app_state = create_test_app_state().await;
        
        let request = UpdateAllowanceConfigRequest {
            child_id: None,
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
        };

        let result = update_allowance_config(State(app_state), Json(request)).await;
        
        // Should return error when no active child
        match result.into_response().status() {
            StatusCode::INTERNAL_SERVER_ERROR => {},
            status => panic!("Expected INTERNAL_SERVER_ERROR, got {:?}", status),
        }
    }

    #[tokio::test]
    async fn test_update_allowance_config_with_active_child() {
        let app_state = create_test_app_state().await;
        
        // Create a child first
        let child_request = CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        let child_response = app_state.child_service.create_child(child_request).await.expect("Failed to create child");
        
        // Set as active child
        use shared::SetActiveChildRequest;
        let set_active_request = SetActiveChildRequest {
            child_id: child_response.child.id,
        };
        app_state.child_service.set_active_child(set_active_request).await.expect("Failed to set active child");

        let request = UpdateAllowanceConfigRequest {
            child_id: None, // Use active child
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
        };

        let result = update_allowance_config(State(app_state), Json(request)).await;
        
        // Should succeed
        match result.into_response().status() {
            StatusCode::OK => {},
            status => panic!("Expected OK, got {:?}", status),
        }
    }

    #[tokio::test]
    async fn test_update_allowance_config_invalid_day() {
        let app_state = create_test_app_state().await;
        
        // Create a child first
        let child_request = CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        };
        let child_response = app_state.child_service.create_child(child_request).await.expect("Failed to create child");

        let request = UpdateAllowanceConfigRequest {
            child_id: Some(child_response.child.id),
            amount: 10.0,
            day_of_week: 7, // Invalid day
            is_active: true,
        };

        let result = update_allowance_config(State(app_state), Json(request)).await;
        
        // Should return bad request
        match result.into_response().status() {
            StatusCode::BAD_REQUEST => {},
            status => panic!("Expected BAD_REQUEST, got {:?}", status),
        }
    }
} 