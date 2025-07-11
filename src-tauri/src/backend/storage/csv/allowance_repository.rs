//! # CSV Allowance Config Repository
//!
//! This module provides a file-based allowance configuration storage implementation
//! using YAML files stored per-child. Each child's allowance configuration is stored
//! in `{child_directory}/allowance_config.yaml`.
//!
//! ## File Structure
//!
//! ```
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
use async_trait::async_trait;
use log::{info, warn, debug};
use std::fs;
use std::path::PathBuf;
use crate::backend::domain::models::allowance::AllowanceConfig as DomainAllowanceConfig;
use super::connection::CsvConnection;
use crate::backend::storage::GitManager;
use serde_yaml;

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
    
    /// Get the allowance config file path for a specific child directory
    fn get_allowance_config_path(&self, child_directory: &str) -> PathBuf {
        self.connection
            .get_child_directory(child_directory)
            .join("allowance_config.yaml")
    }
    
    /// Save allowance config to a specific child directory
    async fn save_allowance_config_to_directory(&self, config: &DomainAllowanceConfig, child_directory: &str) -> Result<()> {
        let child_dir = self.connection.get_child_directory(child_directory);
        
        // Ensure the child directory exists
        if !child_dir.exists() {
            fs::create_dir_all(&child_dir)?;
            info!("Created child directory for allowance config: {:?}", child_dir);
        }
        
        let yaml_path = self.get_allowance_config_path(child_directory);
        let yaml_content = serde_yaml::to_string(config)?;
        
        // Use atomic write pattern: write to temp file, then rename
        let temp_path = yaml_path.with_extension("tmp");
        fs::write(&temp_path, yaml_content)?;
        fs::rename(&temp_path, &yaml_path)?;
        
        debug!("Saved allowance config for child directory '{}' to {:?}", child_directory, yaml_path);
        
        // Git integration: commit the allowance_config.yaml change
        let action_description = format!("Updated allowance configuration (amount: ${:.2})", config.amount);
        
        // This is non-blocking - git errors won't fail the allowance operation
        let _ = self.git_manager.commit_file_change(
            &child_dir,
            "allowance_config.yaml", 
            &action_description
        ).await;
        
        Ok(())
    }
    
    /// Load allowance config from a specific child directory
    async fn load_allowance_config_from_directory(&self, child_directory: &str) -> Result<Option<DomainAllowanceConfig>> {
        let yaml_path = self.get_allowance_config_path(child_directory);
        
        if !yaml_path.exists() {
            debug!("No allowance config found in directory '{}'", child_directory);
            return Ok(None);
        }
        
        let yaml_content = fs::read_to_string(&yaml_path)?;
        let config: DomainAllowanceConfig = serde_yaml::from_str(&yaml_content)?;
        
        debug!("Loaded allowance config for child directory '{}' from {:?}", child_directory, yaml_path);
        Ok(Some(config))
    }
    
    /// Find the child directory that contains a child with the given child_id
    async fn find_child_directory_by_id(&self, child_id: &str) -> Result<Option<String>> {
        let base_dir = self.connection.base_directory();
        
        if !base_dir.exists() {
            return Ok(None);
        }
        
        // Search through all child directories
        for entry in fs::read_dir(base_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if !path.is_dir() {
                continue;
            }
            
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };
            
            // Check if this directory has a child.yaml with the matching child_id
            let child_yaml_path = path.join("child.yaml");
            if child_yaml_path.exists() {
                if let Ok(yaml_content) = fs::read_to_string(&child_yaml_path) {
                    if let Ok(child) = serde_yaml::from_str::<shared::Child>(&yaml_content) {
                        if child.id == child_id {
                            debug!("Found child directory '{}' for child ID '{}'", dir_name, child_id);
                            return Ok(Some(dir_name.to_string()));
                        }
                    }
                }
            }
        }
        
        debug!("No child directory found for child ID '{}'", child_id);
        Ok(None)
    }
    
    /// Get all child directories that have allowance configs
    async fn get_all_child_directories_with_allowance_configs(&self) -> Result<Vec<String>> {
        let base_dir = self.connection.base_directory();
        let mut directories = Vec::new();
        
        if !base_dir.exists() {
            return Ok(directories);
        }
        
        for entry in fs::read_dir(base_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if !path.is_dir() {
                continue;
            }
            
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };
            
            // Check if this directory has an allowance config
            let allowance_config_path = path.join("allowance_config.yaml");
            if allowance_config_path.exists() {
                directories.push(dir_name.to_string());
            }
        }
        
        directories.sort();
        Ok(directories)
    }
}

#[async_trait]
impl crate::backend::storage::AllowanceStorage for AllowanceRepository {
    async fn store_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()> {
        // Find the child directory for this child_id
        let child_directory = match self.find_child_directory_by_id(&config.child_id).await? {
            Some(dir) => dir,
            None => {
                return Err(anyhow::anyhow!(
                    "Cannot store allowance config: child with ID '{}' not found. Create the child first.",
                    config.child_id
                ));
            }
        };
        
        self.save_allowance_config_to_directory(config, &child_directory).await?;
        info!("Stored allowance config for child ID '{}'", config.child_id);
        Ok(())
    }
    
    async fn get_allowance_config(&self, child_id: &str) -> Result<Option<DomainAllowanceConfig>> {
        // Find the child directory for this child_id
        let child_directory = match self.find_child_directory_by_id(child_id).await? {
            Some(dir) => dir,
            None => {
                debug!("Child with ID '{}' not found when getting allowance config", child_id);
                return Ok(None);
            }
        };
        
        self.load_allowance_config_from_directory(&child_directory).await
    }
    
    async fn update_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()> {
        // Update is the same as store for YAML files
        self.store_allowance_config(config).await
    }
    
    async fn delete_allowance_config(&self, child_id: &str) -> Result<bool> {
        // Find the child directory for this child_id
        let child_directory = match self.find_child_directory_by_id(child_id).await? {
            Some(dir) => dir,
            None => {
                debug!("Child with ID '{}' not found when deleting allowance config", child_id);
                return Ok(false);
            }
        };
        
        let yaml_path = self.get_allowance_config_path(&child_directory);
        
        if yaml_path.exists() {
            fs::remove_file(&yaml_path)?;
            info!("Deleted allowance config for child ID '{}' from {:?}", child_id, yaml_path);
            Ok(true)
        } else {
            debug!("No allowance config found to delete for child ID '{}'", child_id);
            Ok(false)
        }
    }
    
    async fn list_allowance_configs(&self) -> Result<Vec<DomainAllowanceConfig>> {
        let directories = self.get_all_child_directories_with_allowance_configs().await?;
        let mut configs = Vec::new();
        
        for directory in directories {
            if let Ok(Some(config)) = self.load_allowance_config_from_directory(&directory).await {
                configs.push(config);
            } else {
                warn!("Failed to load allowance config from directory '{}'", directory);
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
    use shared::Child;
    use crate::backend::storage::{ChildStorage, AllowanceStorage};
    use crate::backend::storage::csv::ChildRepository;

    async fn setup_test_repo_with_child() -> (AllowanceRepository, ChildRepository, TempDir, Child) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let connection = CsvConnection::new(temp_dir.path()).expect("Failed to create connection");
        let allowance_repo = AllowanceRepository::new(connection.clone());
        let child_repo = ChildRepository::new(connection);
        
        // Create a test child first
        let child = Child {
            id: "child::1234567890".to_string(),
            name: "Test Child".to_string(),
            birthdate: "2010-01-01".to_string(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        child_repo.store_child(&child).await.expect("Failed to create test child");
        
        (allowance_repo, child_repo, temp_dir, child)
    }

    #[tokio::test]
    async fn test_store_and_get_allowance_config() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;
        
        let config = AllowanceConfig {
            id: "allowance::1234567890::98765".to_string(),
            child_id: child.id.clone(),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        // Store the config
        repo.store_allowance_config(&config).await.unwrap();
        
        // Retrieve the config
        let retrieved = repo.get_allowance_config(&child.id).await.unwrap();
        assert!(retrieved.is_some());
        
        let retrieved_config = retrieved.unwrap();
        assert_eq!(retrieved_config.id, config.id);
        assert_eq!(retrieved_config.child_id, config.child_id);
        assert_eq!(retrieved_config.amount, config.amount);
        assert_eq!(retrieved_config.day_of_week, config.day_of_week);
        assert_eq!(retrieved_config.is_active, config.is_active);
    }

    #[tokio::test]
    async fn test_update_allowance_config() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;
        
        let mut config = AllowanceConfig {
            id: "allowance::1234567890::98765".to_string(),
            child_id: child.id.clone(),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        // Store the initial config
        repo.store_allowance_config(&config).await.unwrap();
        
        // Update the config
        config.amount = 15.0;
        config.day_of_week = 5; // Friday
        config.updated_at = Utc::now().to_rfc3339();
        
        repo.update_allowance_config(&config).await.unwrap();
        
        // Verify the update
        let retrieved = repo.get_allowance_config(&child.id).await.unwrap().unwrap();
        assert_eq!(retrieved.amount, 15.0);
        assert_eq!(retrieved.day_of_week, 5);
    }

    #[tokio::test]
    async fn test_delete_allowance_config() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;
        
        let config = AllowanceConfig {
            id: "allowance::1234567890::98765".to_string(),
            child_id: child.id.clone(),
            amount: 10.0,
            day_of_week: 1, // Monday
            is_active: true,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        // Store the config
        repo.store_allowance_config(&config).await.unwrap();
        
        // Verify it exists
        assert!(repo.get_allowance_config(&child.id).await.unwrap().is_some());
        
        // Delete the config
        let deleted = repo.delete_allowance_config(&child.id).await.unwrap();
        assert!(deleted);
        
        // Verify it's gone
        assert!(repo.get_allowance_config(&child.id).await.unwrap().is_none());
        
        // Deleting again should return false
        let deleted_again = repo.delete_allowance_config(&child.id).await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_list_allowance_configs() {
        let (repo, child_repo, _temp_dir, child1) = setup_test_repo_with_child().await;
        
        // Create a second child
        let child2 = Child {
            id: "child::2345678901".to_string(),
            name: "Second Child".to_string(),
            birthdate: "2012-01-01".to_string(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        child_repo.store_child(&child2).await.unwrap();
        
        // Create configs for both children
        let config1 = AllowanceConfig {
            id: "allowance::1234567890::98765".to_string(),
            child_id: child1.id.clone(),
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        let config2 = AllowanceConfig {
            id: "allowance::2345678901::87654".to_string(),
            child_id: child2.id.clone(),
            amount: 15.0,
            day_of_week: 5,
            is_active: false,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        repo.store_allowance_config(&config1).await.unwrap();
        repo.store_allowance_config(&config2).await.unwrap();
        
        // List all configs
        let configs = repo.list_allowance_configs().await.unwrap();
        assert_eq!(configs.len(), 2);
        
        // Verify both configs are present
        let child_ids: Vec<&String> = configs.iter().map(|c| &c.child_id).collect();
        assert!(child_ids.contains(&&child1.id));
        assert!(child_ids.contains(&&child2.id));
    }

    #[tokio::test]
    async fn test_store_config_for_nonexistent_child() {
        let (repo, _child_repo, _temp_dir, _child) = setup_test_repo_with_child().await;
        
        let config = AllowanceConfig {
            id: "allowance::9999999999::98765".to_string(),
            child_id: "child::nonexistent".to_string(),
            amount: 10.0,
            day_of_week: 1,
            is_active: true,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        // Storing config for nonexistent child should fail
        let result = repo.store_allowance_config(&config).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("child with ID"));
    }
} 