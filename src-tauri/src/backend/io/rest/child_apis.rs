//! # REST API for Child Management
//!
//! Endpoints for creating, retrieving, updating, and deleting children.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use log::{info, error};

use crate::backend::AppState;
use shared::{
    CreateChildRequest, UpdateChildRequest,
};


/// Create a new child
pub async fn create_child(
    State(state): State<AppState>,
    Json(request): Json<CreateChildRequest>,
) -> impl IntoResponse {
    info!("POST /api/children - request: {:?}", request);

    match state.child_service.create_child(request).await {
        Ok(response) => (StatusCode::CREATED, Json(response)).into_response(),
        Err(e) => {
            error!("Failed to create child: {}", e);
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

/// Get a child by ID
pub async fn get_child(
    State(state): State<AppState>,
    axum::extract::Path(child_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    info!("GET /api/children/{}", child_id);

    match state.child_service.get_child(&child_id).await {
        Ok(Some(child)) => (StatusCode::OK, Json(child)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Child not found").into_response(),
        Err(e) => {
            error!("Failed to get child: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving child").into_response()
        }
    }
}

/// List all children
pub async fn list_children(
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("GET /api/children");

    match state.child_service.list_children().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            error!("Failed to list children: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error listing children").into_response()
        }
    }
}

/// Update a child
pub async fn update_child(
    State(state): State<AppState>,
    axum::extract::Path(child_id): axum::extract::Path<String>,
    Json(request): Json<UpdateChildRequest>,
) -> impl IntoResponse {
    info!("PUT /api/children/{} - request: {:?}", child_id, request);

    match state.child_service.update_child(&child_id, request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            error!("Failed to update child: {}", e);
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, e.to_string()).into_response()
        }
    }
}

/// Delete a child
pub async fn delete_child(
    State(state): State<AppState>,
    axum::extract::Path(child_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    info!("DELETE /api/children/{}", child_id);

    match state.child_service.delete_child(&child_id).await {
        Ok(()) => (StatusCode::NO_CONTENT, "").into_response(),
        Err(e) => {
            error!("Failed to delete child: {}", e);
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, e.to_string()).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests temporarily disabled due to missing test infrastructure
    // TODO: Re-enable after fixing test setup
} 