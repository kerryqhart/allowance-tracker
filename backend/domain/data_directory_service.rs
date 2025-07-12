use anyhow::Result;
use log::info;
use std::sync::Arc;

use crate::backend::storage::csv::CsvConnection;

use crate::backend::domain::child_service::ChildService;
use shared::{GetDataDirectoryResponse, RelocateDataDirectoryRequest, RelocateDataDirectoryResponse, RevertDataDirectoryRequest, RevertDataDirectoryResponse};

/// Service for managing data directory operations
#[derive(Clone)]
pub struct DataDirectoryService {
    csv_connection: Arc<CsvConnection>,
    child_service: Arc<ChildService>,
}

impl DataDirectoryService {
    /// Create a new DataDirectoryService
    pub fn new(csv_conn: Arc<CsvConnection>, child_service: Arc<ChildService>) -> Self {
        Self {
            csv_connection: csv_conn,
            child_service,
        }
    }

    /// Get the current data directory path for a child
    pub fn get_current_directory(&self, child_id: Option<String>) -> Result<GetDataDirectoryResponse> {
        info!("Getting current data directory for child_id: {:?}", child_id);

        let child_id_to_use = if let Some(id) = child_id {
            id.to_string()
        } else {
            let response = self.child_service.get_active_child()?;
            response.active_child.child.ok_or_else(|| anyhow::anyhow!("No active child found"))?.id
        };

        let current_path = self.csv_connection.get_child_directory(&child_id_to_use);
        let path_str = current_path.to_string_lossy().to_string();

        info!("Current data directory for child '{}' (directory: '{}'): {}", child_id_to_use, child_id_to_use, path_str);

        Ok(GetDataDirectoryResponse {
            current_path: path_str,
        })
    }

    /// Relocate child's data directory to a new location
    pub fn relocate_directory(
        &self,
        request: RelocateDataDirectoryRequest,
    ) -> Result<RelocateDataDirectoryResponse> {
        info!("Relocating data directory to: {} for child_id: {:?}", request.new_path, request.child_id);

        let child_id_to_use = if let Some(id) = request.child_id.as_deref() {
            id.to_string()
        } else {
            let response = self.child_service.get_active_child()?;
            response.active_child.child.ok_or_else(|| anyhow::anyhow!("No active child found"))?.id
        };

        info!("🔄 About to call csv_connection.relocate_child_data_directory with child '{}' and path: {}", child_id_to_use, request.new_path);
        match self.csv_connection.relocate_child_data_directory(&child_id_to_use, &request.new_path) {
            Ok(message) => {
                info!("✅ Data directory relocation successful for child '{}'", child_id_to_use);
                Ok(RelocateDataDirectoryResponse {
                    success: true,
                    message,
                    new_path: request.new_path,
                })
            }
            Err(e) => {
                let error_message = format!("Failed to relocate data directory for child '{}': {}", child_id_to_use, e);
                info!("❌ Data directory relocation failed: {}", error_message);
                // Also log the full error chain for debugging
                info!("❌ Full error details: {:?}", e);
                Ok(RelocateDataDirectoryResponse {
                    success: false,
                    message: error_message,
                    new_path: request.new_path,
                })
            }
        }
    }

    /// Revert child's data directory back to the default location
    pub fn revert_directory(
        &self,
        request: RevertDataDirectoryRequest,
    ) -> Result<RevertDataDirectoryResponse> {
        info!("Reverting data directory for child_id: {:?}", request.child_id);

        let child_id_to_use = if let Some(id) = request.child_id.as_deref() {
            id.to_string()
        } else {
            let response = self.child_service.get_active_child()?;
            response.active_child.child.ok_or_else(|| anyhow::anyhow!("No active child found"))?.id
        };

        // Check if there's actually a redirect file
        let base_dir = self.csv_connection.base_directory();
        let default_child_dir = base_dir.join(&child_id_to_use);
        let redirect_file = default_child_dir.join(".allowance_redirect");
        let was_redirected = redirect_file.exists();

        match self.csv_connection.revert_child_data_directory(&child_id_to_use) {
            Ok(message) => {
                info!("Data directory revert successful for child '{}'", child_id_to_use);
                Ok(RevertDataDirectoryResponse {
                    success: true,
                    message,
                    was_redirected,
                })
            }
            Err(e) => {
                let error_message = format!("Failed to revert data directory for child '{}': {}", child_id_to_use, e);
                info!("Data directory revert failed: {}", error_message);
                Ok(RevertDataDirectoryResponse {
                    success: false,
                    message: error_message,
                    was_redirected,
                })
            }
        }
    }
} 