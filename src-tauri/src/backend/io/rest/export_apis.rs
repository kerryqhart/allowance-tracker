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
use shared::{ExportDataRequest, ExportDataResponse, TransactionListRequest};

#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    file_path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
pub struct ExportToPathRequest {
    child_id: Option<String>,
    custom_path: Option<String>, // Optional custom directory path
}

#[derive(Debug, Serialize)]
pub struct WriteFileResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize)]
pub struct ExportToPathResponse {
    success: bool,
    message: String,
    file_path: String,
    transaction_count: usize,
    child_name: String,
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

    // Get the active child if no child_id specified
    let child_id = if let Some(child_id) = request.child_id {
        child_id
    } else {
        match state.child_service.get_active_child().await {
            Ok(response) => {
                if let Some(child) = response.active_child {
                    child.id
                } else {
                    error!("No active child found for export");
                    return (StatusCode::BAD_REQUEST, "No active child found. Please select a child first.").into_response();
                }
            }
            Err(e) => {
                error!("Failed to get active child for export: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving active child").into_response();
            }
        }
    };

    // Get the child details for the filename
    let child = match state.child_service.get_child(&child_id).await {
        Ok(Some(child)) => child,
        Ok(None) => {
            error!("Child not found: {}", child_id);
            return (StatusCode::NOT_FOUND, "Child not found").into_response();
        }
        Err(e) => {
            error!("Failed to get child details: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving child details").into_response();
        }
    };

    // Get all transactions for the child (no pagination for export)
    let transaction_request = TransactionListRequest {
        after: None,
        limit: Some(10000), // Large limit to get all transactions
        start_date: None,
        end_date: None,
    };

    let transactions = match state.transaction_service.list_transactions(transaction_request).await {
        Ok(response) => response.transactions,
        Err(e) => {
            error!("Failed to get transactions for export: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving transactions").into_response();
        }
    };

    // Sort transactions chronologically (oldest first) for CSV export
    let mut sorted_transactions = transactions;
    sorted_transactions.sort_by(|a, b| a.date.cmp(&b.date));

    // Generate CSV content
    let mut csv_content = String::new();
    csv_content.push_str("transaction_id,transaction_date,description,amount\n");

    for (index, transaction) in sorted_transactions.iter().enumerate() {
        // Parse the date and format as yyyy/mm/dd
        let formatted_date = match DateTime::parse_from_rfc3339(&transaction.date) {
            Ok(dt) => dt.format("%Y/%m/%d").to_string(),
            Err(_) => {
                // Fallback to original date if parsing fails
                transaction.date.clone()
            }
        };

        // Format the CSV row
        let row = format!(
            "{},{},\"{}\",{:.2}\n",
            index + 1, // Simple incrementing integer as requested
            formatted_date,
            transaction.description.replace("\"", "\"\""), // Escape quotes in description
            transaction.amount
        );
        csv_content.push_str(&row);
    }

    // Generate filename with current date
    let now = Utc::now();
    let filename = format!(
        "{}_transactions_{}.csv",
        child.name.replace(" ", "_").to_lowercase(),
        now.format("%Y%m%d")
    );

    let response = ExportDataResponse {
        csv_content,
        filename,
        transaction_count: sorted_transactions.len(),
        child_name: child.name,
    };

    info!("Successfully exported {} transactions for child: {} - generated CSV content ({} bytes) with filename: {}", 
          response.transaction_count, response.child_name, response.csv_content.len(), response.filename);
    (StatusCode::OK, Json(response)).into_response()
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

    // First, get the export data
    let export_request = ExportDataRequest {
        child_id: request.child_id,
    };

    // Generate the CSV content using existing logic
    let export_response = match export_transactions_csv_internal(state, export_request).await {
        Ok(response) => response,
        Err(e) => {
            error!("Failed to generate export data: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ExportToPathResponse {
                success: false,
                message: format!("Failed to generate export data: {}", e),
                file_path: String::new(),
                transaction_count: 0,
                child_name: String::new(),
            })).into_response();
        }
    };

    // Determine the export directory
    let export_dir = match request.custom_path {
        Some(custom_path) if !custom_path.trim().is_empty() => {
            // Basic path sanitization: remove quotes, trim whitespace, handle common issues
            let cleaned_path = sanitize_path(&custom_path);
            std::path::PathBuf::from(cleaned_path)
        }
        _ => {
            // Use default location: Documents folder
            match dirs::document_dir() {
                Some(docs_dir) => docs_dir,
                None => {
                    // Fallback to home directory if Documents not available
                    match dirs::home_dir() {
                        Some(home_dir) => home_dir,
                        None => {
                            error!("Could not determine default export directory");
                            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ExportToPathResponse {
                                success: false,
                                message: "Could not determine default export directory".to_string(),
                                file_path: String::new(),
                                transaction_count: 0,
                                child_name: String::new(),
                            })).into_response();
                        }
                    }
                }
            }
        }
    };

    // Create the full file path
    let file_path = export_dir.join(&export_response.filename);
    
    // Ensure the directory exists
    if let Some(parent_dir) = file_path.parent() {
        if let Err(e) = fs::create_dir_all(parent_dir) {
            error!("Failed to create export directory {:?}: {}", parent_dir, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ExportToPathResponse {
                success: false,
                message: format!("Failed to create export directory: {}", e),
                file_path: String::new(),
                transaction_count: 0,
                child_name: String::new(),
            })).into_response();
        }
    }

    // Write the file
    match fs::write(&file_path, &export_response.csv_content) {
        Ok(_) => {
            let file_path_str = file_path.to_string_lossy().to_string();
            info!("Successfully exported {} transactions for {} to: {}", 
                  export_response.transaction_count, export_response.child_name, file_path_str);
            
            (StatusCode::OK, Json(ExportToPathResponse {
                success: true,
                message: format!("Successfully exported {} transactions to {}", 
                               export_response.transaction_count, file_path_str),
                file_path: file_path_str,
                transaction_count: export_response.transaction_count,
                child_name: export_response.child_name,
            })).into_response()
        }
        Err(e) => {
            error!("Failed to write export file to {:?}: {}", file_path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ExportToPathResponse {
                success: false,
                message: format!("Failed to write export file: {}", e),
                file_path: String::new(),
                transaction_count: 0,
                child_name: String::new(),
            })).into_response()
        }
    }
}

// Helper function to extract the main logic for reuse
async fn export_transactions_csv_internal(
    state: AppState,
    request: ExportDataRequest,
) -> Result<ExportDataResponse, String> {
    // Get the active child if no child_id specified
    let child_id = if let Some(child_id) = request.child_id {
        child_id
    } else {
        match state.child_service.get_active_child().await {
            Ok(response) => {
                if let Some(child) = response.active_child {
                    child.id
                } else {
                    return Err("No active child found".to_string());
                }
            }
            Err(e) => return Err(format!("Failed to get active child: {}", e)),
        }
    };

    // Get the child details for the filename
    let child = match state.child_service.get_child(&child_id).await {
        Ok(Some(child)) => child,
        Ok(None) => return Err(format!("Child not found: {}", child_id)),
        Err(e) => return Err(format!("Failed to get child details: {}", e)),
    };

    // Get all transactions for the child (no pagination for export)
    let transaction_request = TransactionListRequest {
        after: None,
        limit: Some(10000), // Large limit to get all transactions
        start_date: None,
        end_date: None,
    };

    let transactions = match state.transaction_service.list_transactions(transaction_request).await {
        Ok(response) => response.transactions,
        Err(e) => return Err(format!("Failed to get transactions: {}", e)),
    };

    // Sort transactions chronologically (oldest first) for CSV export
    let mut sorted_transactions = transactions;
    sorted_transactions.sort_by(|a, b| a.date.cmp(&b.date));

    // Generate CSV content
    let mut csv_content = String::new();
    csv_content.push_str("transaction_id,transaction_date,description,amount\n");

    for (index, transaction) in sorted_transactions.iter().enumerate() {
        // Parse the date and format as yyyy/mm/dd
        let formatted_date = match DateTime::parse_from_rfc3339(&transaction.date) {
            Ok(dt) => dt.format("%Y/%m/%d").to_string(),
            Err(_) => {
                // Fallback to original date if parsing fails
                transaction.date.clone()
            }
        };

        // Format the CSV row
        let row = format!(
            "{},{},\"{}\",{:.2}\n",
            index + 1, // Simple incrementing integer as requested
            formatted_date,
            transaction.description.replace("\"", "\"\""), // Escape quotes in description
            transaction.amount
        );
        csv_content.push_str(&row);
    }

    // Generate filename with current date
    let now = Utc::now();
    let filename = format!(
        "{}_transactions_{}.csv",
        child.name.replace(" ", "_").to_lowercase(),
        now.format("%Y%m%d")
    );

    Ok(ExportDataResponse {
        csv_content,
        filename,
        transaction_count: sorted_transactions.len(),
        child_name: child.name,
    })
}

#[cfg(test)]
mod tests {
    // Tests can be added here for the export functionality
} 