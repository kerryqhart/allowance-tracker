//! # REST API for Child Management
//!
//! Endpoints for creating, retrieving, updating, and deleting children.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};
use log::{error, info};

use crate::backend::AppState;
use crate::backend::io::rest::mappers::child_mapper::ChildMapper;
use crate::backend::domain::commands::child::{
    CreateChildCommand, UpdateChildCommand, GetChildCommand, 
    SetActiveChildCommand, DeleteChildCommand
};
use shared::{
    ChildListResponse, ChildResponse, CreateChildRequest, SetActiveChildRequest, UpdateChildRequest,
    ActiveChildResponse,
};

/// Create a new child
pub async fn create_child(
    State(state): State<AppState>,
    Json(request): Json<CreateChildRequest>,
) -> impl IntoResponse {
    info!("POST /api/children - request: {:?}", request);
    
    let command = CreateChildCommand {
        name: request.name,
        birthdate: request.birthdate,
    };
    
    match state.child_service.create_child(command).await {
        Ok(result) => {
            let response = ChildMapper::to_child_response_dto(result.child, "Child created successfully");
            info!("✅ Child created successfully: {:?}", response);
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to create child: {}", e);
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

/// Get a child by ID
pub async fn get_child_by_id(State(state): State<AppState>, Path(child_id): Path<String>) -> impl IntoResponse {
    info!("GET /api/children/{} - request", child_id);
    
    let command = GetChildCommand { child_id: child_id.clone() };
    
    match state.child_service.get_child(command).await {
        Ok(result) => match result.child {
            Some(domain_child) => {
                let response = ChildMapper::to_child_response_dto(domain_child, "Child found");
                info!("✅ Child found: {}", child_id);
                (StatusCode::OK, Json(response)).into_response()
            }
            None => {
                info!("Child not found: {}", child_id);
                (StatusCode::NOT_FOUND, "Child not found").into_response()
            }
        },
        Err(e) => {
            error!("Failed to get child: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response()
        }
    }
}

/// List all children
pub async fn list_children(State(state): State<AppState>) -> impl IntoResponse {
    info!("GET /api/children - request");
    
    match state.child_service.list_children().await {
        Ok(result) => {
            let response = ChildMapper::to_child_list_dto(result.children);
            info!("✅ Children listed: {} children", response.children.len());
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to list children: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to list children").into_response()
        }
    }
}

/// Update a child
pub async fn update_child(
    State(state): State<AppState>,
    Path(child_id): Path<String>,
    Json(request): Json<UpdateChildRequest>,
) -> impl IntoResponse {
    info!("PUT /api/children/{} - request: {:?}", child_id, request);
    
    let command = UpdateChildCommand {
        child_id: child_id.clone(),
        name: request.name,
        birthdate: request.birthdate,
    };
    
    match state.child_service.update_child(command).await {
        Ok(result) => {
            let response = ChildMapper::to_child_response_dto(result.child, "Child updated successfully");
            info!("✅ Child updated successfully: {}", child_id);
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to update child: {}", e);
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

/// Delete a child
pub async fn delete_child(State(state): State<AppState>, Path(child_id): Path<String>) -> impl IntoResponse {
    info!("DELETE /api/children/{} - request", child_id);
    
    let command = DeleteChildCommand { child_id: child_id.clone() };
    
    match state.child_service.delete_child(command).await {
        Ok(_result) => {
            info!("✅ Child deleted successfully: {}", child_id);
            (StatusCode::NO_CONTENT, "").into_response()
        }
        Err(e) => {
            error!("Failed to delete child: {}", e);
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

/// Get the currently active child
pub async fn get_active_child(State(state): State<AppState>) -> impl IntoResponse {
    info!("GET /api/children/active - request");
    
    match state.child_service.get_active_child().await {
        Ok(result) => {
            let response = ChildMapper::to_active_child_dto(result.active_child);
            info!("✅ Active child retrieved");
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to get active child: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get active child").into_response()
        }
    }
}

/// Set the active child
pub async fn set_active_child(
    State(state): State<AppState>,
    Json(request): Json<SetActiveChildRequest>,
) -> impl IntoResponse {
    info!("POST /api/children/active - request: {:?}", request);
    
    let command = SetActiveChildCommand {
        child_id: request.child_id.clone(),
    };
    
    match state.child_service.set_active_child(command).await {
        Ok(result) => {
            let response = ChildMapper::to_set_active_child_dto(result.child);
            info!("✅ Active child set successfully: {}", request.child_id);
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to set active child: {}", e);
            (StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    }
}

pub fn create_child_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_child))
        .route("/", get(list_children))
        .route("/:id", get(get_child_by_id))
        .route("/:id", put(update_child))
        .route("/:id", delete(delete_child))
        .route("/active", get(get_active_child))
        .route("/active", post(set_active_child))
}

#[cfg(test)]
mod tests {
    // Tests temporarily disabled due to missing test infrastructure
    // TODO: Re-enable after fixing test setup
} 