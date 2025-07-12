//! # REST API for Data Export
//!
//! Endpoints for exporting transaction data as CSV files.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use log::{info, error, warn};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf, Component};

use crate::backend::AppState;
use shared::{ExportDataRequest, ExportDataResponse, ExportToPathRequest, ExportToPathResponse, Transaction};

use crate::backend::domain::commands::transactions::TransactionListQuery;
use crate::backend::io::rest::mappers::transaction_mapper::TransactionMapper;

#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    file_path: String,
    content: String,
}

#[derive(Debug, Serialize)]
pub struct WriteFileResponse {
    success: bool,
    message: String,
}

/// Basic path sanitization to handle common user input issues
fn sanitize_path(path: &str) -> String {
    let mut cleaned = path.trim().to_string();
    
    // Remove surrounding quotes (single or double)
    if (cleaned.starts_with('"') && cleaned.ends_with('"')) ||
       (cleaned.starts_with('\'') && cleaned.ends_with('\'')) {
        cleaned = cleaned[1..cleaned.len()-1].to_string();
    }
    
    // Trim again after quote removal
    cleaned = cleaned.trim().to_string();
    
    // Handle escaped spaces (common on some systems)
    cleaned = cleaned.replace("\\ ", " ");
    
    // Remove any trailing slashes/backslashes
    while cleaned.ends_with('/') || cleaned.ends_with('\\') {
        cleaned.pop();
    }
    
    // Handle tilde expansion for home directory
    if cleaned.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            if cleaned == "~" {
                cleaned = home.to_string_lossy().to_string();
            } else if cleaned.starts_with("~/") || cleaned.starts_with("~\\") {
                cleaned = home.join(&cleaned[2..]).to_string_lossy().to_string();
            }
        }
    }
    
    cleaned
}

/// Create a router for export related APIs
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/csv", post(export_transactions_csv))
        .route("/write-file", post(write_export_file))
        .route("/to-path", post(export_to_path))
}

/// Export transactions as CSV data
pub async fn export_transactions_csv(
    State(state): State<AppState>,
    Json(request): Json<ExportDataRequest>,
) -> impl IntoResponse {
    info!("POST /api/export/csv - request: {:?}", request);

    // Use the new orchestration method from export service
    match state.export_service.export_transactions_csv(
        request,
        &state.child_service,
        &state.transaction_service,
    ).await {
        Ok(response) => {
            info!("✅ Export CSV operation completed successfully");
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("❌ Failed to export transactions: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to export transactions").into_response()
        }
    }
}

/// Write exported data to a file
pub async fn write_export_file(
    Json(request): Json<WriteFileRequest>,
) -> impl IntoResponse {
    info!("POST /api/export/write-file - writing to: {}", request.file_path);

    match fs::write(&request.file_path, &request.content) {
        Ok(_) => {
            info!("Successfully wrote {} bytes to file: {}", request.content.len(), request.file_path);
            let response = WriteFileResponse {
                success: true,
                message: format!("File written successfully to: {}", request.file_path),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to write file {}: {}", request.file_path, e);
            let response = WriteFileResponse {
                success: false,
                message: format!("Failed to write file: {}", e),
            };
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response()
        }
    }
}

/// Export data directly to a specified path (or default location)
pub async fn export_to_path(
    State(state): State<AppState>,
    Json(request): Json<ExportToPathRequest>,
) -> impl IntoResponse {
    info!("POST /api/export/to-path - custom_path: {:?}", request.custom_path);

    // Use the new orchestration method from export service
    match state.export_service.export_to_path(
        request,
        &state.child_service,
        &state.transaction_service,
    ).await {
        Ok(response) => {
            info!("✅ Export to path operation completed successfully");
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("❌ Failed to export to path: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ExportToPathResponse {
                success: false,
                message: format!("Failed to export to path: {}", e),
                file_path: String::new(),
                transaction_count: 0,
                child_name: String::new(),
            })).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests can be added here for the export functionality
} 