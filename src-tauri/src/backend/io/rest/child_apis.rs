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
    CreateChildRequest, UpdateChildRequest, SetActiveChildRequest,
};
use crate::backend::io::rest::mappers::child_mapper::ChildMapper;


/// Create a new child
pub async fn create_child(
    State(state): State<AppState>,
    Json(request): Json<CreateChildRequest>,
) -> impl IntoResponse {
    info!("POST /api/children - request: {:?}", request);

    match state.child_service.create_child(request).await {
        Ok(domain_child) => {
            let response = ChildMapper::to_child_response_dto(domain_child, "Child created successfully");
            (StatusCode::CREATED, Json(response)).into_response()
        },
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
        Ok(Some(domain_child)) => {
            let response = ChildMapper::to_dto(domain_child);
            (StatusCode::OK, Json(response)).into_response()
        },
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
        Ok(domain_children) => {
            let response = ChildMapper::to_child_list_dto(domain_children);
            (StatusCode::OK, Json(response)).into_response()
        },
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
        Ok(domain_child) => {
            let response = ChildMapper::to_child_response_dto(domain_child, "Child updated successfully");
            (StatusCode::OK, Json(response)).into_response()
        },
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

/// Get the currently active child
pub async fn get_active_child(
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("GET /api/active-child");

    match state.child_service.get_active_child().await {
        Ok(domain_active_child) => {
            let response = ChildMapper::to_active_child_dto(domain_active_child);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to get active child: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving active child").into_response()
        }
    }
}

/// Set the active child
pub async fn set_active_child(
    State(state): State<AppState>,
    Json(request): Json<SetActiveChildRequest>,
) -> impl IntoResponse {
    info!("POST /api/active-child - request: {:?}", request);

    match state.child_service.set_active_child(&request.child_id).await {
        Ok(domain_child) => {
            let response = ChildMapper::to_set_active_child_dto(domain_child);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => {
            error!("Failed to set active child: {}", e);
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
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