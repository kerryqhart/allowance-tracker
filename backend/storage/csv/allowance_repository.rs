//! # CSV Allowance Config Repository
//!
//! This module provides a file-based allowance configuration storage implementation
//! using YAML files stored per-child. Each child's allowance configuration is stored
//! in `{child_directory}/allowance_config.yaml`.
//!
//! ## File Structure
//!
//! ```text
//! data/
//! ├── global_config.yaml
//! └── {child_name}/
//!     ├── child.yaml
//!     ├── allowance_config.yaml    ← This module manages these files
//!     └── transactions.csv
//! ```
//!
//! ## Features
//!
//! - Per-child YAML configuration files
//! - Atomic file writes with temp files
//! - Automatic directory discovery
//! - Human-readable YAML format

use anyhow::Result;

use log::{info, warn, debug};

use std::path::{Path, PathBuf};

use shared::ChildId;

use crate::backend::domain::models::allowance::AllowanceConfig as DomainAllowanceConfig;
use super::connection::CsvConnection;
use crate::backend::storage::GitManager;
use serde_yaml;
use serde::{Serialize, Deserialize};

/// YAML representation of an allowance config that omits the redundant child_id.
/// The child_id is implicit from the directory name, so storing it in the file
/// can become stale if the ID ever changes.  We therefore write/read this
/// trimmed struct on disk and inject the child_id in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct YamlAllowanceConfig {
    amount: f64,
    day_of_week: u8,
    is_active: bool,
    #[serde(default)]
    use_age_based_amount: bool,
    created_at: String,
    updated_at: String,
}

/// CSV-based allowance config repository using per-child YAML files
#[derive(Clone)]
pub struct AllowanceRepository {
    connection: CsvConnection,
    git_manager: GitManager,
}

impl AllowanceRepository {
    /// Create a new CSV allowance config repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { 
            connection,
            git_manager: GitManager::new(),
        }
    }
    
    /// Get the allowance config file path for a child, resolved through the registry
    fn get_allowance_config_path(&self, child_id: &ChildId) -> Result<PathBuf> {
        Ok(self
            .connection
            .child_dir(child_id)?
            .join("allowance_config.yaml"))
    }

    /// Save allowance config into an already-resolved child directory.
    ///
    /// No `create_dir_all`: the caller resolved through `child_dir`, which has
    /// already proven the folder is there.
    fn save_allowance_config_to_directory(&self, config: &DomainAllowanceConfig, child_dir: &Path) -> Result<()> {
        let yaml_path = child_dir.join("allowance_config.yaml");

        // Convert to YAML struct without child_id before serialising
        let yaml_model = YamlAllowanceConfig {
            amount: config.amount,
            day_of_week: config.day_of_week,
            is_active: config.is_active,
            use_age_based_amount: config.use_age_based_amount,
            created_at: config.created_at.clone(),
            updated_at: config.updated_at.clone(),
        };

        let yaml_content = serde_yaml::to_string(&yaml_model)?;

        // Use atomic write pattern: write to temp file, then rename
        let temp_path = yaml_path.with_extension("tmp");
        std::fs::write(&temp_path, yaml_content)?;
        std::fs::rename(&temp_path, &yaml_path)?;

        debug!("Saved allowance config to {:?}", yaml_path);

        // Git commit the allowance config change
        let action_description = format!("Updated allowance config (${:.2}/week, active: {})", config.amount, config.is_active);
        let _ = self.git_manager.commit_file_change(
            child_dir,
            "allowance_config.yaml",
            &action_description
        );

        Ok(())
    }

    /// Load allowance config from an already-resolved child directory.
    ///
    /// `child_id` is supplied by the caller from the registry rather than
    /// inferred from the folder's basename, which is no longer the id.
    fn load_allowance_config_from_directory(&self, child_dir: &Path, child_id: &ChildId) -> Result<Option<DomainAllowanceConfig>> {
        let yaml_path = child_dir.join("allowance_config.yaml");

        if !yaml_path.exists() {
            debug!("No allowance config found in {:?}", child_dir);
            return Ok(None);
        }

        let yaml_content = std::fs::read_to_string(&yaml_path)?;
        let yaml_model: YamlAllowanceConfig = serde_yaml::from_str(&yaml_content)?;

        let config = DomainAllowanceConfig {
            child_id: child_id.as_str().to_string(),
            amount: yaml_model.amount,
            day_of_week: yaml_model.day_of_week,
            is_active: yaml_model.is_active,
            use_age_based_amount: yaml_model.use_age_based_amount,
            created_at: yaml_model.created_at,
            updated_at: yaml_model.updated_at,
        };

        debug!("Loaded allowance config for child '{}' from {:?}", child_id, yaml_path);
        Ok(Some(config))
    }
}

impl crate::backend::storage::AllowanceStorage for AllowanceRepository {
    fn store_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()> {
        let id = ChildId::from(config.child_id.as_str());
        let child_dir = self.connection.child_dir(&id).map_err(|e| {
            anyhow::anyhow!(
                "Cannot store allowance config: child with ID '{}' not found. Create the child first. ({})",
                config.child_id,
                e
            )
        })?;

        self.save_allowance_config_to_directory(config, &child_dir)?;
        info!("Stored allowance config for child ID '{}'", config.child_id);
        Ok(())
    }

    fn get_allowance_config(&self, child_id: &str) -> Result<Option<DomainAllowanceConfig>> {
        let id = ChildId::from(child_id);
        let child_dir = match self.connection.child_dir(&id) {
            Ok(dir) => dir,
            Err(e) => {
                debug!("Child with ID '{}' not resolvable when getting allowance config: {}", child_id, e);
                return Ok(None);
            }
        };

        self.load_allowance_config_from_directory(&child_dir, &id)
    }

    fn update_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()> {
        // Update is the same as store for YAML files
        self.store_allowance_config(config)
    }

    fn delete_allowance_config(&self, child_id: &str) -> Result<bool> {
        let id = ChildId::from(child_id);
        let yaml_path = match self.get_allowance_config_path(&id) {
            Ok(path) => path,
            Err(e) => {
                debug!("Child with ID '{}' not resolvable when deleting allowance config: {}", child_id, e);
                return Ok(false);
            }
        };

        if yaml_path.exists() {
            std::fs::remove_file(&yaml_path)?;
            info!("Deleted allowance config for child ID '{}' from {:?}", child_id, yaml_path);
            Ok(true)
        } else {
            debug!("No allowance config found to delete for child ID '{}'", child_id);
            Ok(false)
        }
    }

    /// List every registered child's allowance config.
    ///
    /// Registry-backed rather than a base-directory scan, so `child_id` comes
    /// from the registry entry instead of being inferred from a folder name.
    fn list_allowance_configs(&self) -> Result<Vec<DomainAllowanceConfig>> {
        let registry = self.connection.registry();
        let mut configs = Vec::new();

        for entry in registry.entries() {
            match self.load_allowance_config_from_directory(&entry.path, &entry.id) {
                Ok(Some(config)) => configs.push(config),
                Ok(None) => {}
                Err(e) => warn!(
                    "Failed to load allowance config for child '{}' at {}: {}",
                    entry.id,
                    entry.path.display(),
                    e
                ),
            }
        }

        // Sort by updated_at timestamp (most recent first)
        configs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        
        info!("Listed {} allowance configs", configs.len());
        Ok(configs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use chrono::Utc;
    use crate::backend::domain::models::child::Child as DomainChild;
    use crate::backend::storage::{ChildStorage, AllowanceStorage};
    use crate::backend::storage::csv::ChildRepository;
    use std::sync::Arc;

    fn setup_test_repo_with_child() -> (AllowanceRepository, ChildRepository, TempDir, DomainChild) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let connection = CsvConnection::new(temp_dir.path()).expect("Failed to create connection");
        let allowance_repo = AllowanceRepository::new(connection.clone());
        let child_repo = ChildRepository::new(Arc::new(connection));
        
        // Create a test child first
        let child = DomainChild {
            id: "test_child".to_string(),  // ID matches directory name
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        child_repo.store_child(&child).expect("Failed to create test child");
        
        (allowance_repo, child_repo, temp_dir, child)
    }

    #[test]
    fn test_store_and_get_allowance_config() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child();
        
        let config = DomainAllowanceConfig {
            child_id: child.id.clone(),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
            use_age_based_amount: false,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Store the config
        repo.store_allowance_config(&config).unwrap();

        // Retrieve the config
        let retrieved = repo.get_allowance_config(&child.id).unwrap();
        assert!(retrieved.is_some());
        
        let retrieved_config = retrieved.unwrap();
        assert_eq!(retrieved_config.child_id, config.child_id);
        assert_eq!(retrieved_config.amount, config.amount);
        assert_eq!(retrieved_config.day_of_week, config.day_of_week);
        assert_eq!(retrieved_config.is_active, config.is_active);
    }

    #[test]
    fn test_update_allowance_config() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child();
        
        let mut config = DomainAllowanceConfig {
            child_id: child.id.clone(),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
            use_age_based_amount: false,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Store the initial config
        repo.store_allowance_config(&config).unwrap();

        // Update the config
        config.amount = 15.0;
        config.day_of_week = 5; // Friday
        config.updated_at = Utc::now().to_rfc3339();
        
        repo.update_allowance_config(&config).unwrap();
        
        // Verify the update
        let retrieved = repo.get_allowance_config(&child.id).unwrap().unwrap();
        assert_eq!(retrieved.amount, 15.0);
        assert_eq!(retrieved.day_of_week, 5);
    }

    #[test]
    fn test_delete_allowance_config() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child();

        let config = DomainAllowanceConfig {
            child_id: child.id.clone(),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
            use_age_based_amount: false,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Store the config
        repo.store_allowance_config(&config).unwrap();
        
        // Verify it exists
        assert!(repo.get_allowance_config(&child.id).unwrap().is_some());
        
        // Delete the config
        let deleted = repo.delete_allowance_config(&child.id).unwrap();
        assert!(deleted);
        
        // Verify it's gone
        assert!(repo.get_allowance_config(&child.id).unwrap().is_none());
        
        // Deleting again should return false
        let deleted_again = repo.delete_allowance_config(&child.id).unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_list_allowance_configs() {
        let (repo, child_repo, _temp_dir, child1) = setup_test_repo_with_child();
        
        // Create a second child
        let child2 = DomainChild {
            id: "test_child_2".to_string(),  // ID matches directory name
            name: "Second Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2012-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        child_repo.store_child(&child2).unwrap();
        
        // Create configs for both children
        let config1 = DomainAllowanceConfig {
            child_id: child1.id.clone(),
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
            use_age_based_amount: false,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        let config2 = DomainAllowanceConfig {
            child_id: child2.id.clone(),
            amount: 15.0,
            day_of_week: 5,
            is_active: false,
            use_age_based_amount: false,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        repo.store_allowance_config(&config1).unwrap();
        repo.store_allowance_config(&config2).unwrap();
        
        // List all configs
        let configs = repo.list_allowance_configs().unwrap();
        assert_eq!(configs.len(), 2);
        
        // Verify both configs are present by checking child_ids
        let child_ids: Vec<&String> = configs.iter().map(|c| &c.child_id).collect();
        assert!(child_ids.contains(&&child1.id), "Child1 ID not found in configs");
        assert!(child_ids.contains(&&child2.id), "Child2 ID not found in configs");
    }

    #[test]
    fn test_store_config_for_nonexistent_child() {
        let (repo, _child_repo, _temp_dir, _child) = setup_test_repo_with_child();
        
        let config = DomainAllowanceConfig {
            child_id: "nonexistent_child".to_string(),  // ID matches directory name
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
            use_age_based_amount: false,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };

        // Storing config for nonexistent child should fail
        let result = repo.store_allowance_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("child with ID"));
    }
} 