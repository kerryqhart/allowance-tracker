//! # Data Directory Service (vestigial)
//!
//! This service used to move a child's folder elsewhere and leave a
//! `.allowance_redirect` marker behind, then follow that marker on every
//! resolution. The child registry replaces that entirely: a child's location
//! is a registry entry, and moving one is a `repoint`, not a copy plus a
//! marker file.
//!
//! The redirect/relocate/revert machinery it called has been deleted from
//! `CsvConnection`. What remains here is the minimum that keeps the settings
//! UI compiling and honest: the current directory can still be reported, and
//! every relocation entry point returns an unsuccessful response saying the
//! operation is no longer available. A later task removes this service and its
//! modal outright.

use anyhow::Result;
use log::info;
use std::sync::Arc;

use crate::backend::storage::csv::CsvConnection;

use crate::backend::domain::child_service::ChildService;
use shared::{
    ChildId,
    GetDataDirectoryResponse, RelocateDataDirectoryRequest, RelocateDataDirectoryResponse,
    RevertDataDirectoryRequest, RevertDataDirectoryResponse,
    CheckDataDirectoryConflictRequest, CheckDataDirectoryConflictResponse,
    RelocateWithConflictResolutionRequest, RelocateWithConflictResolutionResponse,
    ReturnToDefaultLocationRequest, ReturnToDefaultLocationResponse,
};

/// Explanation handed back to the UI wherever a relocation is requested.
const RELOCATION_RETIRED: &str = "Moving a child's data folder from this screen is no longer \
available. A child's location now lives in the machine-local child registry \
(children.yaml) rather than in redirect files.";

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

    /// Resolve the child to act on: the one named, or the active one.
    fn resolve_child_id(&self, child_id: Option<&str>) -> Result<String> {
        match child_id {
            Some(id) => Ok(id.to_string()),
            None => {
                let response = self.child_service.get_active_child()?;
                Ok(response
                    .active_child
                    .child
                    .ok_or_else(|| anyhow::anyhow!("No active child found"))?
                    .id)
            }
        }
    }

    /// Get the current data directory path for a child.
    ///
    /// Resolution is a registry lookup. `is_redirected` is always false —
    /// redirect files are gone, and a registered path is simply the path.
    pub fn get_current_directory(&self, child_id: Option<String>) -> Result<GetDataDirectoryResponse> {
        info!("Getting current data directory for child_id: {:?}", child_id);

        let child_id_to_use = self.resolve_child_id(child_id.as_deref())?;

        let current_path = self
            .csv_connection
            .child_dir(&ChildId::from(child_id_to_use.as_str()))?;
        let path_str = current_path.to_string_lossy().to_string();

        info!("Current data directory for child '{}': {}", child_id_to_use, path_str);

        Ok(GetDataDirectoryResponse {
            current_path: path_str,
            is_redirected: false,
        })
    }

    /// Relocation is retired; see the module docs.
    pub fn relocate_directory(
        &self,
        request: RelocateDataDirectoryRequest,
    ) -> Result<RelocateDataDirectoryResponse> {
        Ok(RelocateDataDirectoryResponse {
            success: false,
            message: RELOCATION_RETIRED.to_string(),
            new_path: request.new_path,
        })
    }

    /// Reverting a relocation is retired; see the module docs.
    pub fn revert_directory(
        &self,
        _request: RevertDataDirectoryRequest,
    ) -> Result<RevertDataDirectoryResponse> {
        Ok(RevertDataDirectoryResponse {
            success: false,
            message: RELOCATION_RETIRED.to_string(),
            was_redirected: false,
        })
    }

    /// Check if relocating to a path would cause conflicts.
    ///
    /// This inspects only the target path, so it survives the registry
    /// cutover untouched.
    pub fn check_relocation_conflicts(
        &self,
        request: CheckDataDirectoryConflictRequest,
    ) -> Result<CheckDataDirectoryConflictResponse> {
        info!("Checking data directory conflicts for path: {}", request.new_path);

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
        if self.directory_contains_child_data(&new_path) {
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

    /// Relocation with conflict resolution is retired; see the module docs.
    pub fn relocate_with_conflict_resolution(
        &self,
        request: RelocateWithConflictResolutionRequest,
    ) -> Result<RelocateWithConflictResolutionResponse> {
        Ok(RelocateWithConflictResolutionResponse {
            success: false,
            message: RELOCATION_RETIRED.to_string(),
            new_path: request.new_path,
            archived_to: None,
        })
    }

    /// Returning to the default location is retired; see the module docs.
    pub fn return_to_default_location(
        &self,
        request: ReturnToDefaultLocationRequest,
    ) -> Result<ReturnToDefaultLocationResponse> {
        let child_id_to_use = self.resolve_child_id(request.child_id.as_deref())?;
        let default_path = self
            .csv_connection
            .child_dir(&ChildId::from(child_id_to_use.as_str()))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(ReturnToDefaultLocationResponse {
            success: false,
            message: RELOCATION_RETIRED.to_string(),
            default_path,
        })
    }

    /// Check if a directory contains valid child data
    fn directory_contains_child_data(&self, path: &std::path::Path) -> bool {
        let child_file = path.join("child.yaml");
        let transactions_file = path.join("transactions.csv");

        // Consider it child data if it has either the child config or transactions
        child_file.exists() || transactions_file.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::domain::models::child::Child as DomainChild;
    use crate::backend::storage::csv::ChildRepository;
    use crate::backend::storage::traits::ChildStorage;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Build a `DataDirectoryService` over a fresh temp base directory with one
    /// registered child whose *display name* differs from its folder name.
    fn service_with_child(display_name: &str) -> (DataDirectoryService, TempDir, String) {
        let temp_dir = TempDir::new().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let child_service = Arc::new(ChildService::new(connection.clone(), None));
        let service = DataDirectoryService::new(connection.clone(), child_service);

        let child_id = CsvConnection::generate_safe_directory_name(display_name);
        let child = DomainChild {
            id: child_id.clone(),
            name: display_name.to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        ChildRepository::new(connection).store_child(&child).unwrap();

        (service, temp_dir, child_id)
    }

    #[test]
    fn get_current_directory_reports_the_registered_folder() {
        let (service, temp_dir, child_id) = service_with_child("Keiko Hart");

        let resp = service
            .get_current_directory(Some(child_id.clone()))
            .unwrap();

        let expected = temp_dir.path().join(&child_id);
        assert_eq!(resp.current_path, expected.to_string_lossy());
        assert!(!resp.is_redirected);
    }

    /// Registration made the folder resolvable, so removing the folder must
    /// make resolution fail rather than report a path nothing will honour.
    #[test]
    fn get_current_directory_errors_when_the_folder_is_gone() {
        let (service, temp_dir, child_id) = service_with_child("Keiko Hart");
        std::fs::remove_dir_all(temp_dir.path().join(&child_id)).unwrap();

        assert!(service.get_current_directory(Some(child_id)).is_err());
    }

    #[test]
    fn relocation_reports_that_it_is_no_longer_available() {
        let (service, temp_dir, child_id) = service_with_child("Keiko Hart");
        let target = temp_dir.path().join("relocated");

        let resp = service
            .relocate_directory(RelocateDataDirectoryRequest {
                child_id: Some(child_id),
                new_path: target.to_string_lossy().to_string(),
            })
            .unwrap();

        assert!(!resp.success);
        assert!(!target.exists(), "a retired relocation must not touch the filesystem");
    }
}
