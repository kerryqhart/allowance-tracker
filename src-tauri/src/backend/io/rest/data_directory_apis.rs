use axum::{
    extract::{State, Query},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::collections::HashMap;

use crate::backend::AppState;
use shared::{GetDataDirectoryResponse, RelocateDataDirectoryRequest, RelocateDataDirectoryResponse, RevertDataDirectoryRequest, RevertDataDirectoryResponse};
use log::{info, error};

/// Create data directory management routes
pub fn create_data_directory_routes() -> Router<AppState> {
    Router::new()
        .route("/current", get(get_current_directory))
        .route("/relocate", post(relocate_directory))
        .route("/revert", post(revert_directory))
}

/// GET /api/data-directory/current?child_id=<optional>
/// Get the current data directory path for a child
pub async fn get_current_directory(
    State(app_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<GetDataDirectoryResponse>, StatusCode> {
    let child_id = params.get("child_id").cloned();
    info!("GET /api/data-directory/current - child_id: {:?}", child_id);

    match app_state.data_directory_service.get_current_directory(child_id).await {
        Ok(response) => {
            info!("Current data directory: {}", response.current_path);
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to get current data directory: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// POST /api/data-directory/relocate
/// Relocate data directory to a new location for a child
pub async fn relocate_directory(
    State(app_state): State<AppState>,
    Json(request): Json<RelocateDataDirectoryRequest>,
) -> Result<Json<RelocateDataDirectoryResponse>, StatusCode> {
    info!("POST /api/data-directory/relocate - new path: {} for child_id: {:?}", request.new_path, request.child_id);

    match app_state.data_directory_service.relocate_directory(request).await {
        Ok(response) => {
            if response.success {
                info!("Data directory relocation successful: {}", response.message);
            } else {
                info!("Data directory relocation failed: {}", response.message);
            }
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to relocate data directory: {}", e);
            let error_response = RelocateDataDirectoryResponse {
                success: false,
                message: format!("Internal server error: {}", e),
                new_path: String::new(),
            };
            Ok(Json(error_response))
        }
    }
}

/// POST /api/data-directory/revert
/// Revert data directory back to the default location for a child
pub async fn revert_directory(
    State(app_state): State<AppState>,
    Json(request): Json<RevertDataDirectoryRequest>,
) -> Result<Json<RevertDataDirectoryResponse>, StatusCode> {
    info!("POST /api/data-directory/revert - child_id: {:?}", request.child_id);

    match app_state.data_directory_service.revert_directory(request).await {
        Ok(response) => {
            if response.success {
                info!("Data directory revert successful: {}", response.message);
            } else {
                info!("Data directory revert failed: {}", response.message);
            }
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to revert data directory: {}", e);
            let error_response = RevertDataDirectoryResponse {
                success: false,
                message: format!("Internal server error: {}", e),
                was_redirected: false,
            };
            Ok(Json(error_response))
        }
    }
} 