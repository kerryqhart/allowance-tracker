use anyhow::Result;
use log::{info, warn};
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::backend::storage::csv::CsvConnection;

use crate::backend::domain::child_service::ChildService;
use shared::{
    GetDataDirectoryResponse, RelocateDataDirectoryRequest, RelocateDataDirectoryResponse, 
    RevertDataDirectoryRequest, RevertDataDirectoryResponse,
    CheckDataDirectoryConflictRequest, CheckDataDirectoryConflictResponse,
    RelocateWithConflictResolutionRequest, RelocateWithConflictResolutionResponse,
    ReturnToDefaultLocationRequest, ReturnToDefaultLocationResponse,
    ConflictResolution
};

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

        // Get the child's name (CSV connection expects name, not ID)
        let child = self.child_service.get_child(crate::backend::domain::commands::child::GetChildCommand {
            child_id: child_id_to_use.clone(),
        })?;
        
        let child_name = match child.child {
            Some(child) => child.name,
            None => return Err(anyhow::anyhow!("Child not found: {}", child_id_to_use)),
        };

        let current_path = self.csv_connection.get_child_directory(&child_name);
        let path_str = current_path.to_string_lossy().to_string();

        // Check if this location is via a redirect file
        let base_dir = self.csv_connection.base_directory();
        let default_child_dir = base_dir.join(&child_name);
        let redirect_file = default_child_dir.join(".allowance_redirect");
        let is_redirected = redirect_file.exists();

        info!("Current data directory for child '{}' (name: '{}'): {} (redirected: {})", child_id_to_use, child_name, path_str, is_redirected);

        Ok(GetDataDirectoryResponse {
            current_path: path_str,
            is_redirected,
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

        // Get the child's name (CSV connection expects name, not ID)
        let child = self.child_service.get_child(crate::backend::domain::commands::child::GetChildCommand {
            child_id: child_id_to_use.clone(),
        })?;
        
        let child_name = match child.child {
            Some(child) => child.name,
            None => return Err(anyhow::anyhow!("Child not found: {}", child_id_to_use)),
        };

        info!("🔄 About to call csv_connection.relocate_child_data_directory with child '{}' (name: '{}') and path: {}", child_id_to_use, child_name, request.new_path);
        match self.csv_connection.relocate_child_data_directory(&child_name, &request.new_path) {
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

        // Get the child's name (CSV connection expects name, not ID)
        let child = self.child_service.get_child(crate::backend::domain::commands::child::GetChildCommand {
            child_id: child_id_to_use.clone(),
        })?;
        
        let child_name = match child.child {
            Some(child) => child.name,
            None => return Err(anyhow::anyhow!("Child not found: {}", child_id_to_use)),
        };

        // Check if there's actually a redirect file
        let base_dir = self.csv_connection.base_directory();
        let default_child_dir = base_dir.join(&child_name);
        let redirect_file = default_child_dir.join(".allowance_redirect");
        let was_redirected = redirect_file.exists();

        match self.csv_connection.revert_child_data_directory(&child_name) {
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

    /// Check if relocating to a path would cause conflicts
    pub fn check_relocation_conflicts(
        &self,
        request: CheckDataDirectoryConflictRequest,
    ) -> Result<CheckDataDirectoryConflictResponse> {
        info!("Checking data directory conflicts for path: {}", request.new_path);

        // Note: This method only checks the target path for conflicts, so it doesn't need child info
        let new_path = std::path::PathBuf::from(&request.new_path);
        
        // Check if target directory exists and has child data
        if !new_path.exists() {
            info!("Target directory does not exist - no conflicts");
            return Ok(CheckDataDirectoryConflictResponse {
                has_conflict: false,
                conflict_details: None,
                can_proceed_safely: true,
            });
        }

        // Check if directory is empty
        let entries: Result<Vec<_>, _> = std::fs::read_dir(&new_path)?.collect();
        match entries {
            Ok(entries) => {
                if entries.is_empty() {
                    info!("Target directory is empty - no conflicts");
                    return Ok(CheckDataDirectoryConflictResponse {
                        has_conflict: false,
                        conflict_details: None,
                        can_proceed_safely: true,
                    });
                }
            }
            Err(e) => {
                return Ok(CheckDataDirectoryConflictResponse {
                    has_conflict: true,
                    conflict_details: Some(format!("Cannot read target directory: {}", e)),
                    can_proceed_safely: false,
                });
            }
        }

        // Check if target contains valid child data
        let has_child_data = self.directory_contains_child_data(&new_path);
        
        if has_child_data {
            info!("Target directory contains child data - conflict detected");
            Ok(CheckDataDirectoryConflictResponse {
                has_conflict: true,
                conflict_details: Some("Target directory contains existing child data (child.yaml and/or transactions.csv)".to_string()),
                can_proceed_safely: false,
            })
        } else {
            info!("Target directory contains files but no child data - conflict detected");
            Ok(CheckDataDirectoryConflictResponse {
                has_conflict: true,
                conflict_details: Some("Target directory is not empty".to_string()),
                can_proceed_safely: false,
            })
        }
    }

    /// Relocate data directory with conflict resolution
    pub fn relocate_with_conflict_resolution(
        &self,
        request: RelocateWithConflictResolutionRequest,
    ) -> Result<RelocateWithConflictResolutionResponse> {
        info!("Relocating with conflict resolution: {:?}", request.resolution);

        let child_id_to_use = if let Some(id) = request.child_id.as_deref() {
            id.to_string()
        } else {
            let response = self.child_service.get_active_child()?;
            response.active_child.child.ok_or_else(|| anyhow::anyhow!("No active child found"))?.id
        };

        match request.resolution {
            ConflictResolution::Cancel => {
                info!("User cancelled relocation");
                Ok(RelocateWithConflictResolutionResponse {
                    success: false,
                    message: "Operation cancelled by user".to_string(),
                    new_path: request.new_path,
                    archived_to: None,
                })
            }
            ConflictResolution::OverwriteTarget => {
                info!("Overwriting target with current child data");
                // Use existing relocate method but clear target directory first
                let new_path = std::path::PathBuf::from(&request.new_path);
                if new_path.exists() {
                    std::fs::remove_dir_all(&new_path)?;
                }
                
                let relocate_request = RelocateDataDirectoryRequest {
                    child_id: Some(child_id_to_use),
                    new_path: request.new_path.clone(),
                };
                
                let result = self.relocate_directory(relocate_request)?;
                
                Ok(RelocateWithConflictResolutionResponse {
                    success: result.success,
                    message: result.message,
                    new_path: result.new_path,
                    archived_to: None,
                })
            }
            ConflictResolution::UseTargetData => {
                info!("Using target data and archiving current data");
                
                // Get the child's name first (needed for directory operations)
                let child = self.child_service.get_child(crate::backend::domain::commands::child::GetChildCommand {
                    child_id: child_id_to_use.clone(),
                })?;
                
                let child_name = match child.child {
                    Some(child) => child.name,
                    None => return Err(anyhow::anyhow!("Child not found: {}", child_id_to_use)),
                };
                
                // Archive current data first
                let archive_path = self.archive_current_data(&child_id_to_use)?;
                
                // Create redirect to target location
                let base_dir = self.csv_connection.base_directory();
                let default_child_dir = base_dir.join(&child_name);
                let redirect_file = default_child_dir.join(".allowance_redirect");
                
                // Clear default directory (except .git)
                if default_child_dir.exists() {
                    for entry in std::fs::read_dir(&default_child_dir)? {
                        let entry = entry?;
                        let path = entry.path();
                        let file_name = path.file_name().unwrap_or_default();
                        
                        if file_name != ".git" {
                            if path.is_dir() {
                                std::fs::remove_dir_all(&path)?;
                            } else {
                                std::fs::remove_file(&path)?;
                            }
                        }
                    }
                } else {
                    std::fs::create_dir_all(&default_child_dir)?;
                }
                
                // Create redirect file
                std::fs::write(&redirect_file, request.new_path.as_bytes())?;
                
                info!("Successfully redirected to target location and archived original data");
                
                Ok(RelocateWithConflictResolutionResponse {
                    success: true,
                    message: format!("Now using data from target location. Original data archived to: {}", archive_path),
                    new_path: request.new_path,
                    archived_to: Some(archive_path),
                })
            }
        }
    }

    /// Check if a directory contains valid child data
    fn directory_contains_child_data(&self, path: &std::path::Path) -> bool {
        let child_file = path.join("child.yaml");
        let transactions_file = path.join("transactions.csv");
        
        // Consider it child data if it has either the child config or transactions
        child_file.exists() || transactions_file.exists()
    }

    /// Archive current child data to archive/child_name folder in default location
    fn archive_current_data(&self, child_id: &str) -> Result<String> {
        info!("Archiving current data for child: {}", child_id);
        
        // Get the child's name (CSV connection expects name, not ID)
        let child = self.child_service.get_child(crate::backend::domain::commands::child::GetChildCommand {
            child_id: child_id.to_string(),
        })?;
        
        let child_name = match child.child {
            Some(child) => child.name,
            None => return Err(anyhow::anyhow!("Child not found: {}", child_id)),
        };
        
        let current_data_dir = self.csv_connection.get_child_directory(&child_name);
        let base_dir = self.csv_connection.base_directory();
        let archive_base = base_dir.join("archive");
        
        // Create unique archive folder with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let archive_dir = archive_base.join(format!("{}_{}", child_name, timestamp));
        
        info!("Creating archive directory: {}", archive_dir.display());
        std::fs::create_dir_all(&archive_dir)?;
        
        // Copy all current data to archive (excluding .git and .allowance_redirect)
        for entry in std::fs::read_dir(&current_data_dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default();
            
            // Skip .git and .allowance_redirect files
            if file_name == ".git" || file_name == ".allowance_redirect" {
                continue;
            }
            
            let dest_path = archive_dir.join(file_name);
            
            if path.is_dir() {
                self.copy_directory_recursive(&path, &dest_path)?;
            } else {
                std::fs::copy(&path, &dest_path)?;
            }
        }
        
        let archive_path = archive_dir.to_string_lossy().to_string();
        info!("Successfully archived data to: {}", archive_path);
        Ok(archive_path)
    }

    /// Recursively copy directory contents
    fn copy_directory_recursive(&self, source: &std::path::Path, dest: &std::path::Path) -> Result<()> {
        info!("Creating destination directory: {}", dest.display());
        std::fs::create_dir_all(dest).map_err(|e| {
            anyhow::anyhow!("Failed to create directory {}: {}", dest.display(), e)
        })?;
        
        // Ensure destination directory is writable
        #[cfg(unix)]
        {
            if let Ok(mut perms) = std::fs::metadata(dest).map(|m| m.permissions()) {
                perms.set_mode(perms.mode() | 0o700); // Ensure owner can read, write, execute
                let _ = std::fs::set_permissions(dest, perms);
            }
        }
        
        info!("Reading source directory: {}", source.display());
        for entry in std::fs::read_dir(source).map_err(|e| {
            anyhow::anyhow!("Failed to read source directory {}: {}", source.display(), e)
        })? {
            let entry = entry.map_err(|e| anyhow::anyhow!("Error reading directory entry in {}: {}", source.display(), e))?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());
            
            info!("Processing: {} -> {}", path.display(), dest_path.display());
            
            if path.is_dir() {
                self.copy_directory_recursive(&path, &dest_path)?;
            } else {
                // If destination file exists and might be read-only (like git objects), make it writable first
                if dest_path.exists() {
                    #[cfg(unix)]
                    {
                        if let Ok(mut perms) = std::fs::metadata(&dest_path).map(|m| m.permissions()) {
                            perms.set_mode(perms.mode() | 0o600); // Make sure owner can read and write
                            let _ = std::fs::set_permissions(&dest_path, perms);
                        }
                    }
                }
                
                // Copy the file
                std::fs::copy(&path, &dest_path).map_err(|e| {
                    anyhow::anyhow!("Failed to copy file {} to {}: {}", path.display(), dest_path.display(), e)
                })?;
                
                // Set appropriate permissions on the copied file
                #[cfg(unix)]
                {
                    if let Ok(source_perms) = std::fs::metadata(&path).map(|m| m.permissions()) {
                        // For git objects, preserve original permissions but ensure owner can write
                        let mut new_mode = source_perms.mode();
                        if path.to_string_lossy().contains(".git/objects/") {
                            new_mode |= 0o600; // Ensure git objects are readable and writable by owner
                        } else {
                            new_mode |= 0o200; // Ensure regular files are writable by owner
                        }
                        
                        let new_perms = std::fs::Permissions::from_mode(new_mode);
                        if let Err(_) = std::fs::set_permissions(&dest_path, new_perms) {
                            // If setting permissions fails, at least try to make it writable
                            if let Ok(mut perms) = std::fs::metadata(&dest_path).map(|m| m.permissions()) {
                                perms.set_mode(perms.mode() | 0o200);
                                let _ = std::fs::set_permissions(&dest_path, perms);
                            }
                        }
                    }
                }
                
                info!("Successfully copied file: {} -> {}", path.display(), dest_path.display());
            }
        }
        
        Ok(())
    }

    /// Return data to default location (copy from redirect location)
    pub fn return_to_default_location(
        &self,
        request: ReturnToDefaultLocationRequest,
    ) -> Result<ReturnToDefaultLocationResponse> {
        info!("Returning data to default location for child_id: {:?}", request.child_id);

        let child_id_to_use = if let Some(id) = request.child_id.as_deref() {
            id.to_string()
        } else {
            let response = self.child_service.get_active_child()?;
            response.active_child.child.ok_or_else(|| anyhow::anyhow!("No active child found"))?.id
        };

        // Get the child's name (CSV connection expects name, not ID)
        let child = self.child_service.get_child(crate::backend::domain::commands::child::GetChildCommand {
            child_id: child_id_to_use.clone(),
        })?;
        
        let child_name = match child.child {
            Some(child) => child.name,
            None => return Err(anyhow::anyhow!("Child not found: {}", child_id_to_use)),
        };

        let base_dir = self.csv_connection.base_directory();
        let default_child_dir = base_dir.join(&child_name);
        let redirect_file = default_child_dir.join(".allowance_redirect");

        // Check if there's actually a redirect file
        if !redirect_file.exists() {
            return Ok(ReturnToDefaultLocationResponse {
                success: false,
                message: "Data is already at the default location".to_string(),
                default_path: default_child_dir.to_string_lossy().to_string(),
            });
        }

        // Read the redirect location
        let redirected_path = match std::fs::read_to_string(&redirect_file) {
            Ok(path) => std::path::PathBuf::from(path.trim()),
            Err(e) => return Err(anyhow::anyhow!("Failed to read redirect file: {}", e)),
        };

        info!("Found redirect pointing to: {}", redirected_path.display());

        // Validate that redirected directory exists
        if !redirected_path.exists() {
            return Err(anyhow::anyhow!("Redirected directory does not exist: {}", redirected_path.display()));
        }

        // Remove all files and directories in default location except .git and .allowance_redirect
        info!("Cleaning default directory: {}", default_child_dir.display());
        
        // Check if we can write to the directory first
        if let Ok(metadata) = std::fs::metadata(&default_child_dir) {
            if metadata.permissions().readonly() {
                return Err(anyhow::anyhow!("Default directory is read-only: {}", default_child_dir.display()));
            }
        }
        
        for entry in std::fs::read_dir(&default_child_dir).map_err(|e| {
            anyhow::anyhow!("Cannot read default directory {}: {}", default_child_dir.display(), e)
        })? {
            let entry = entry.map_err(|e| anyhow::anyhow!("Error reading directory entry: {}", e))?;
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default();
            
            if file_name != ".git" && file_name != ".allowance_redirect" {
                info!("Attempting to remove: {}", path.display());
                
                if path.is_dir() {
                    // For directories, make sure they're writable before trying to remove
                    #[cfg(unix)]
                    {
                        fn make_writable_recursive(dir: &std::path::Path) -> std::io::Result<()> {
                            for entry in std::fs::read_dir(dir)? {
                                let entry = entry?;
                                let path = entry.path();
                                if path.is_dir() {
                                    if let Ok(mut perms) = std::fs::metadata(&path).map(|m| m.permissions()) {
                                        perms.set_mode(perms.mode() | 0o700);
                                        let _ = std::fs::set_permissions(&path, perms);
                                    }
                                    make_writable_recursive(&path)?;
                                } else {
                                    if let Ok(mut perms) = std::fs::metadata(&path).map(|m| m.permissions()) {
                                        perms.set_mode(perms.mode() | 0o600);
                                        let _ = std::fs::set_permissions(&path, perms);
                                    }
                                }
                            }
                            Ok(())
                        }
                        let _ = make_writable_recursive(&path);
                    }
                    
                    std::fs::remove_dir_all(&path).map_err(|e| {
                        anyhow::anyhow!("Failed to remove directory {}: {}", path.display(), e)
                    })?;
                    info!("Successfully removed directory: {}", path.display());
                } else {
                    // Make file writable before trying to remove it
                    #[cfg(unix)]
                    {
                        if let Ok(mut perms) = std::fs::metadata(&path).map(|m| m.permissions()) {
                            perms.set_mode(perms.mode() | 0o600);
                            let _ = std::fs::set_permissions(&path, perms);
                        }
                    }
                    
                    std::fs::remove_file(&path).map_err(|e| {
                        anyhow::anyhow!("Failed to remove file {}: {}", path.display(), e)
                    })?;
                    info!("Successfully removed file: {}", path.display());
                }
            }
        }

        // Copy data from redirected location to default location (without removing source)
        info!("Copying data from {} to {}", redirected_path.display(), default_child_dir.display());
        self.copy_directory_recursive(&redirected_path, &default_child_dir).map_err(|e| {
            anyhow::anyhow!("Failed to copy data from {} to {}: {}", redirected_path.display(), default_child_dir.display(), e)
        })?;
        info!("Successfully copied child '{}' data to default location", child_name);

        // Verify the copy was successful
        let key_files = ["child.yaml", "transactions.csv"];
        for file in &key_files {
            if redirected_path.join(file).exists() && !default_child_dir.join(file).exists() {
                return Err(anyhow::anyhow!("File '{}' was not copied successfully during return to default", file));
            }
        }

        // Remove the redirect file
        info!("Removing redirect file: {}", redirect_file.display());
        std::fs::remove_file(&redirect_file).map_err(|e| {
            anyhow::anyhow!("Failed to remove redirect file {}: {}", redirect_file.display(), e)
        })?;
        info!("Successfully removed redirect file for child '{}'", child_name);

        // If there's a .git directory, commit the removal of redirect file
        let git_dir = default_child_dir.join(".git");
        if git_dir.exists() {
            // Use the CSV connection's commit method
            match self.csv_connection.commit_redirect_removal(&default_child_dir, &child_name) {
                Ok(_) => info!("Git commit successful for return to default"),
                Err(e) => warn!("Git commit failed for return to default: {}", e),
            }
        }

        let default_path = default_child_dir.to_string_lossy().to_string();
        info!("Child '{}' data successfully returned to default location: {}", child_name, default_path);
        
        Ok(ReturnToDefaultLocationResponse {
            success: true,
            message: format!("Data successfully returned to default location. Redirected data remains at: {}", redirected_path.display()),
            default_path,
        })
    }
} 