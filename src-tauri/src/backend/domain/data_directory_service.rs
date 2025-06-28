use anyhow::Result;
use log::info;
use std::sync::Arc;

use crate::backend::storage::csv::CsvConnection;
use crate::backend::storage::csv::child_repository::ChildRepository;
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
    pub async fn get_current_directory(&self, child_id: Option<String>) -> Result<GetDataDirectoryResponse> {
        info!("Getting current data directory for child_id: {:?}", child_id);

        // Resolve child
        let child = match child_id {
            Some(id) => {
                self.child_service.get_child(&id).await?
                    .ok_or_else(|| anyhow::anyhow!("Child with ID '{}' not found", id))?
            },
            None => {
                let response = self.child_service.get_active_child().await?;
                response.active_child.ok_or_else(|| anyhow::anyhow!("No active child found"))?
            },
        };

        // Convert display name to filesystem directory name
        let directory_name = ChildRepository::generate_safe_directory_name(&child.name);
        let current_path = self.csv_connection.get_child_directory(&directory_name);
        let path_str = current_path.to_string_lossy().to_string();

        info!("Current data directory for child '{}' (directory: '{}'): {}", child.name, directory_name, path_str);

        Ok(GetDataDirectoryResponse {
            current_path: path_str,
        })
    }

    /// Relocate child's data directory to a new location
    pub async fn relocate_directory(
        &self,
        request: RelocateDataDirectoryRequest,
    ) -> Result<RelocateDataDirectoryResponse> {
        info!("Relocating data directory to: {} for child_id: {:?}", request.new_path, request.child_id);

        // Resolve child
        let child = match request.child_id {
            Some(id) => {
                self.child_service.get_child(&id).await?
                    .ok_or_else(|| anyhow::anyhow!("Child with ID '{}' not found", id))?
            },
            None => {
                let response = self.child_service.get_active_child().await?;
                response.active_child.ok_or_else(|| anyhow::anyhow!("No active child found"))?
            },
        };

        // Convert display name to filesystem directory name
        let directory_name = ChildRepository::generate_safe_directory_name(&child.name);
        info!("🔄 About to call csv_connection.relocate_child_data_directory with child '{}' (directory: '{}') and path: {}", child.name, directory_name, request.new_path);
        match self.csv_connection.relocate_child_data_directory(&directory_name, &request.new_path) {
            Ok(message) => {
                info!("✅ Data directory relocation successful for child '{}'", child.name);
                Ok(RelocateDataDirectoryResponse {
                    success: true,
                    message,
                    new_path: request.new_path,
                })
            }
            Err(e) => {
                let error_message = format!("Failed to relocate data directory for child '{}': {}", child.name, e);
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
    pub async fn revert_directory(
        &self,
        request: RevertDataDirectoryRequest,
    ) -> Result<RevertDataDirectoryResponse> {
        info!("Reverting data directory for child_id: {:?}", request.child_id);

        // Resolve child
        let child = match request.child_id {
            Some(id) => {
                self.child_service.get_child(&id).await?
                    .ok_or_else(|| anyhow::anyhow!("Child with ID '{}' not found", id))?
            },
            None => {
                let response = self.child_service.get_active_child().await?;
                response.active_child.ok_or_else(|| anyhow::anyhow!("No active child found"))?
            },
        };

        // Convert display name to filesystem directory name
        let directory_name = ChildRepository::generate_safe_directory_name(&child.name);
        
        // Check if there's actually a redirect file
        let base_dir = self.csv_connection.base_directory();
        let default_child_dir = base_dir.join(&directory_name);
        let redirect_file = default_child_dir.join(".allowance_redirect");
        let was_redirected = redirect_file.exists();

        match self.csv_connection.revert_child_data_directory(&directory_name) {
            Ok(message) => {
                info!("Data directory revert successful for child '{}'", child.name);
                Ok(RevertDataDirectoryResponse {
                    success: true,
                    message,
                    was_redirected,
                })
            }
            Err(e) => {
                let error_message = format!("Failed to revert data directory for child '{}': {}", child.name, e);
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