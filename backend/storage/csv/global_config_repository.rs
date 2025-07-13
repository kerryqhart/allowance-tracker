//! # CSV Global Config Repository
//!
//! This module provides a file-based global configuration storage implementation
//! using a single YAML file `global_config.yaml` at the root of the data directory.
//!
//! ## File Structure
//!
//! ```
//! data/
//! ├── global_config.yaml    ← This module manages this file
//! └── {child_name}/
//!     ├── child.yaml
//!     ├── allowance_config.yaml
//!     ├── parental_control_attempts.csv
//!     └── transactions.csv
//! ```
//!
//! ## YAML Format
//!
//! ```yaml
//! active_child_directory: "child_name"
//! data_format_version: "1.0"
//! created_at: "2025-01-21T19:30:00Z"
//! updated_at: "2025-01-21T19:35:00Z"
//! ```
//!
//! ## Features
//!
//! - Single global configuration file
//! - Active child directory tracking
//! - Data format versioning for future migrations
//! - Atomic file writes with temp files

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use log::{info, debug};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::connection::CsvConnection;

/// Global configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Directory name of the currently active child (None if no active child)
    pub active_child_directory: Option<String>,
    /// Data format version for future migrations
    pub data_format_version: String,
    /// When the global config was first created
    pub created_at: String,
    /// When the global config was last updated
    pub updated_at: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            active_child_directory: None,
            data_format_version: "1.0".to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// Storage trait for global configuration operations
#[async_trait]
pub trait GlobalConfigStorage: Send + Sync {
    /// Get the global configuration
    fn get_global_config(&self) -> Result<GlobalConfig>;
    
    /// Set the active child directory
    fn set_active_child_directory(&self, child_directory: Option<String>) -> Result<()>;
    
    /// Update the global configuration
    fn update_global_config(&self, config: &GlobalConfig) -> Result<()>;
}

/// CSV-based global config repository using a single YAML file
#[derive(Clone)]
pub struct GlobalConfigRepository {
    connection: CsvConnection,
}

impl GlobalConfigRepository {
    /// Create a new global config repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { connection }
    }
    
    /// Get the global config file path
    fn get_global_config_path(&self) -> PathBuf {
        self.connection.base_directory().join("global_config.yaml")
    }
    
    /// Load global config from file, creating default if it doesn't exist
    fn load_or_create_global_config(&self) -> Result<GlobalConfig> {
        let config_path = self.get_global_config_path();
        
        if config_path.exists() {
            let yaml_content = fs::read_to_string(&config_path)?;
            let config: GlobalConfig = serde_yaml::from_str(&yaml_content)?;
            debug!("Loaded global config from {:?}", config_path);
            Ok(config)
        } else {
            // Create default config
            let config = GlobalConfig::default();
            self.save_global_config(&config)?;
            info!("Created default global config at {:?}", config_path);
            Ok(config)
        }
    }
    
    /// Save global config to file
    fn save_global_config(&self, config: &GlobalConfig) -> Result<()> {
        let config_path = self.get_global_config_path();
        let base_dir = self.connection.base_directory();
        
        // Ensure base directory exists
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir)?;
            info!("Created base data directory: {:?}", base_dir);
        }
        
        let yaml_content = serde_yaml::to_string(config)?;
        
        // Use atomic write pattern: write to temp file, then rename
        let temp_path = config_path.with_extension("tmp");
        fs::write(&temp_path, yaml_content)?;
        fs::rename(&temp_path, &config_path)?;
        
        debug!("Saved global config to {:?}", config_path);
        Ok(())
    }
    
    /// Validate that a child directory exists
    fn validate_child_directory(&self, child_directory: &str) -> Result<bool> {
        let child_dir_path = self.connection.get_child_directory(child_directory);
        let child_yaml_path = child_dir_path.join("child.yaml");
        Ok(child_yaml_path.exists())
    }
}

impl GlobalConfigStorage for GlobalConfigRepository {
    fn get_global_config(&self) -> Result<GlobalConfig> {
        self.load_or_create_global_config()
    }
    
    fn set_active_child_directory(&self, child_directory: Option<String>) -> Result<()> {
        // Validate child directory exists if provided
        if let Some(ref dir) = child_directory {
            if !self.validate_child_directory(dir)? {
                return Err(anyhow::anyhow!(
                    "Cannot set active child: directory '{}' does not exist or does not contain a valid child",
                    dir
                ));
            }
        }
        
        let mut config = self.load_or_create_global_config()?;
        config.active_child_directory = child_directory.clone();
        config.updated_at = Utc::now().to_rfc3339();
        
        self.save_global_config(&config)?;
        
        match child_directory {
            Some(dir) => info!("Set active child directory to '{}'", dir),
            None => info!("Cleared active child directory"),
        }
        
        Ok(())
    }
    
    fn update_global_config(&self, config: &GlobalConfig) -> Result<()> {
        // Validate child directory if set
        if let Some(ref dir) = config.active_child_directory {
            if !self.validate_child_directory(dir)? {
                return Err(anyhow::anyhow!(
                    "Invalid child directory in config: '{}' does not exist or does not contain a valid child",
                    dir
                ));
            }
        }
        
        let mut updated_config = config.clone();
        updated_config.updated_at = Utc::now().to_rfc3339();
        
        self.save_global_config(&updated_config)?;
        info!("Updated global config");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::backend::domain::models::child::Child as DomainChild;
    use crate::backend::storage::ChildStorage;
    use crate::backend::storage::csv::ChildRepository;
    use std::sync::Arc;

    fn setup_test_repo() -> (GlobalConfigRepository, ChildRepository, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let connection = CsvConnection::new(temp_dir.path()).expect("Failed to create connection");
        let global_config_repo = GlobalConfigRepository::new(connection.clone());
        let child_repo = ChildRepository::new(Arc::new(connection));
        
        (global_config_repo, child_repo, temp_dir)
    }

    #[test]
    fn test_get_global_config_creates_default() {
        let (repo, _child_repo, _temp_dir) = setup_test_repo();
        
        let config = repo.get_global_config().unwrap();
        assert_eq!(config.active_child_directory, None);
        assert_eq!(config.data_format_version, "1.0");
        assert!(!config.created_at.is_empty());
        assert!(!config.updated_at.is_empty());
    }

    #[test]
    fn test_set_active_child_directory() {
        let (repo, child_repo, _temp_dir) = setup_test_repo();
        
        // Create a test child first
        let child = DomainChild {
            id: "child::1234567890".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        child_repo.store_child(&child).unwrap();
        
        // Set active child directory
        repo.set_active_child_directory(Some("test_child".to_string())).unwrap();
        
        // Verify it was set
        let config = repo.get_global_config().unwrap();
        assert_eq!(config.active_child_directory, Some("test_child".to_string()));
    }

    #[test]
    fn test_clear_active_child_directory() {
        let (repo, child_repo, _temp_dir) = setup_test_repo();
        
        // Create and set a child first
        let child = DomainChild {
            id: "child::1234567890".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        child_repo.store_child(&child).unwrap();
        repo.set_active_child_directory(Some("test_child".to_string())).unwrap();
        
        // Clear the active child
        repo.set_active_child_directory(None).unwrap();
        
        // Verify it was cleared
        let config = repo.get_global_config().unwrap();
        assert_eq!(config.active_child_directory, None);
    }

    #[test]
    fn test_set_invalid_child_directory() {
        let (repo, _child_repo, _temp_dir) = setup_test_repo();
        
        // Try to set non-existent child directory
        let result = repo.set_active_child_directory(Some("nonexistent_child".to_string()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_update_global_config() {
        let (repo, _child_repo, _temp_dir) = setup_test_repo();
        
        // Get initial config
        let mut config = repo.get_global_config().unwrap();
        let initial_updated_at = config.updated_at.clone();
        
        // Modify and update
        config.data_format_version = "2.0".to_string();
        
        repo.update_global_config(&config).unwrap();
        
        // Verify update
        let updated_config = repo.get_global_config().unwrap();
        assert_eq!(updated_config.data_format_version, "2.0");
        assert_ne!(updated_config.updated_at, initial_updated_at);
    }

    #[test]
    fn test_config_persistence() {
        let (repo, child_repo, temp_dir) = setup_test_repo();
        
        // Create a child and set as active
        let child = DomainChild {
            id: "child::1234567890".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        child_repo.store_child(&child).unwrap();
        repo.set_active_child_directory(Some("test_child".to_string())).unwrap();
        
        // Create a new repository instance (simulating app restart)
        let connection2 = CsvConnection::new(temp_dir.path()).unwrap();
        let repo2 = GlobalConfigRepository::new(connection2);
        
        // Verify config persisted
        let config = repo2.get_global_config().unwrap();
        assert_eq!(config.active_child_directory, Some("test_child".to_string()));
    }
} 