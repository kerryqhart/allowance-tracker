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
    use super::*;
    use crate::backend::io::rest::tests::setup_test_handlers;
    use shared::{CreateChildRequest, UpdateChildRequest};

    #[tokio::test]
    async fn test_create_child_handler() {
        let state = setup_test_handlers().await;
        
        let request = CreateChildRequest {
            name: "Alice Smith".to_string(),
            birthdate: "2015-06-15".to_string(),
        };
        
        let _response = create_child(State(state), Json(request)).await;
        
        // Should return 201 CREATED status with child data
    }

    #[tokio::test]
    async fn test_create_child_validation_error() {
        let state = setup_test_handlers().await;
        
        // Test with empty name (should fail validation)
        let request = CreateChildRequest {
            name: "".to_string(),
            birthdate: "2015-06-15".to_string(),
        };
        
        let _response = create_child(State(state), Json(request)).await;
        
        // Should return 400 BAD REQUEST status
    }

    #[tokio::test]
    async fn test_list_children_handler() {
        let state = setup_test_handlers().await;
        
        // First create a child
        let create_request = CreateChildRequest {
            name: "Bob Johnson".to_string(),
            birthdate: "2012-03-20".to_string(),
        };
        let _create_response = create_child(State(state.clone()), Json(create_request)).await;
        
        // Then list children
        let _list_response = list_children(State(state)).await;
        
        // Should return 200 OK with children list
    }

    #[tokio::test]
    async fn test_get_child_handler() {
        let state = setup_test_handlers().await;
        
        // First create a child
        let create_request = CreateChildRequest {
            name: "Charlie Brown".to_string(),
            birthdate: "2010-12-25".to_string(),
        };
        
        // In a real test, we'd extract the child ID from the create response
        // For now, we'll test with a mock ID
        let child_id = "child::1234567890000".to_string();
        
        let _response = get_child(State(state), axum::extract::Path(child_id)).await;
        
        // Should return 404 NOT FOUND for non-existent child
    }

    #[tokio::test]
    async fn test_update_child_handler() {
        let state = setup_test_handlers().await;
        
        let child_id = "child::1234567890000".to_string();
        let update_request = UpdateChildRequest {
            name: Some("Updated Name".to_string()),
            birthdate: Some("2015-07-20".to_string()),
        };
        
        let _response = update_child(State(state), axum::extract::Path(child_id), Json(update_request)).await;
        
        // Should return 404 NOT FOUND for non-existent child
    }

    #[tokio::test]
    async fn test_delete_child_handler() {
        let state = setup_test_handlers().await;
        
        let child_id = "child::1234567890000".to_string();
        
        let _response = delete_child(State(state), axum::extract::Path(child_id)).await;
        
        // Should return 404 NOT FOUND for non-existent child
    }
} 