use axum::{extract::State, http::StatusCode, response::Json, routing::post, Router};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::backend::AppState;

#[derive(Debug, Deserialize)]
pub struct LogRequest {
    pub level: String,
    pub message: String,
    pub component: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogResponse {
    pub success: bool,
}

pub async fn log_message(
    State(_app_state): State<AppState>,
    Json(request): Json<LogRequest>,
) -> Result<Json<LogResponse>, StatusCode> {
    let component = request.component.as_deref().unwrap_or("frontend");
    let message = format!("[{}] {}", component, request.message);
    
    match request.level.to_lowercase().as_str() {
        "debug" => debug!("{}", message),
        "info" => info!("{}", message),
        "warn" => warn!("{}", message),
        "error" => error!("{}", message),
        _ => info!("{}", message), // Default to info for unknown levels
    }
    
    Ok(Json(LogResponse { success: true }))
} 