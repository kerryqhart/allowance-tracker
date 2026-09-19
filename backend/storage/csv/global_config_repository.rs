//! # CSV Global Config Repository
//!
//! This module provides a file-based global configuration storage implementation
//! using a single YAML file `global_config.yaml` at the root of the data directory.
//!
//! ## File Structure
//!
//! ```text
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
use chrono::Utc;
use log::{info, debug, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use shared::ChildId;

use super::connection::CsvConnection;

/// Global configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Id of the currently active child (None if no active child).
    ///
    /// This is the authoritative key. The registry migration writes it, and
    /// [`GlobalConfigRepository::active_child_id`] reads it in preference to
    /// the legacy `active_child_directory` below, which on a migrated install
    /// still holds the *folder name* — and a folder name is not an id once
    /// "Add existing child…" can adopt an arbitrary directory.
    ///
    /// `serde(default)` so a pre-migration file (which has only the legacy
    /// key) still loads rather than hard-erroring.
    #[serde(default)]
    pub active_child_id: Option<String>,
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
            active_child_id: None,
            active_child_directory: None,
            data_format_version: "1.0".to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// Storage trait for global configuration operations
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
        crate::backend::storage::atomic::write(&config_path, yaml_content)?;
        
        debug!("Saved global config to {:?}", config_path);
        Ok(())
    }
    
    /// Validate that the named child resolves to a real folder.
    ///
    /// `active_child_directory` now holds the child's **id**; the registry
    /// maps it to a location and `child_dir` proves that location is there.
    fn validate_child_directory(&self, child_id: &str) -> Result<bool> {
        Ok(self.connection.child_dir(&ChildId::from(child_id)).is_ok())
    }

    /// The active child's id, or `None` if none is set.
    ///
    /// Reads `global_config.yaml` only — no child folder is touched — which is
    /// what makes this safe to call while a child's folder is still coming
    /// down from iCloud. Prefers `active_child_id`, falling back to the legacy
    /// `active_child_directory` for a config that predates the migration.
    /// The fallback is correct for a config the migration has rewritten (it
    /// resolves the folder name to a real id and stores it in
    /// `active_child_id`), but on a *pre-migration* config the legacy value is
    /// still a folder name. Where that name is not also the id, this yields an
    /// id matching no registry entry. That fails safe — no roster entry, so
    /// nothing is selected — but it must not fail invisibly, so the fallback
    /// branch warns.
    pub fn active_child_id(&self) -> Result<Option<ChildId>> {
        let config = self.load_or_create_global_config()?;
        if let Some(id) = config.active_child_id {
            return Ok(Some(ChildId::from(id.as_str())));
        }
        let Some(legacy) = config.active_child_directory else {
            return Ok(None);
        };
        warn!(
            "global_config.yaml has no active_child_id; interpreting the legacy \
             active_child_directory '{}' as a child id. If that is a folder name \
             rather than an id, no child will be active until one is selected again.",
            legacy
        );
        Ok(Some(ChildId::from(legacy.as_str())))
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
        // Both keys are written so the file stays self-consistent: the caller
        // passes an id, and leaving the legacy key pointing at a different
        // child would make the two disagree for any reader still on the old
        // key.
        config.active_child_id = child_directory.clone();
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
        // Reconcile the two keys before validating, so this path cannot write a
        // file where they disagree — `active_child_id` is what
        // `Self::active_child_id` prefers, so a divergent pair would silently
        // activate the stale one. `active_child_id` wins when both are set,
        // matching read precedence; either alone fills in the other.
        let mut updated_config = config.clone();
        let active = updated_config
            .active_child_id
            .clone()
            .or_else(|| updated_config.active_child_directory.clone());
        updated_config.active_child_id = active.clone();
        updated_config.active_child_directory = active;

        // Validate the active child if set
        if let Some(ref dir) = updated_config.active_child_directory {
            if !self.validate_child_directory(dir)? {
                return Err(anyhow::anyhow!(
                    "Invalid child directory in config: '{}' does not exist or does not contain a valid child",
                    dir
                ));
            }
        }

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
            id: "test_child".to_string(),  // ID matches directory name
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
            id: "test_child".to_string(),  // ID matches directory name
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

    fn store_child_named(child_repo: &ChildRepository, id: &str) {
        let child = DomainChild {
            id: id.to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        child_repo.store_child(&child).unwrap();
    }

    /// The migration writes the resolved id into `active_child_id` while
    /// *preserving* the legacy `active_child_directory` as a folder name. The
    /// id key must win, or a migrated install activates a name that matches no
    /// registry entry.
    #[test]
    fn active_child_id_prefers_the_id_key_over_a_divergent_legacy_folder_name() {
        let (repo, child_repo, temp_dir) = setup_test_repo();
        store_child_named(&child_repo, "test_child");

        fs::write(
            temp_dir.path().join("global_config.yaml"),
            "active_child_id: test_child\nactive_child_directory: Some Folder Name\n\
             data_format_version: '1.0'\ncreated_at: '2024-01-01T00:00:00Z'\n\
             updated_at: '2024-01-01T00:00:00Z'\n",
        )
        .unwrap();

        assert_eq!(
            repo.active_child_id().unwrap(),
            Some(ChildId::from("test_child"))
        );
    }

    /// A pre-migration config has only the legacy key. Reading it as an id is
    /// the documented fallback (it warns); this pins that the fallback still
    /// happens rather than returning `None`.
    #[test]
    fn active_child_id_falls_back_to_the_legacy_key_when_the_id_key_is_absent() {
        let (repo, child_repo, temp_dir) = setup_test_repo();
        store_child_named(&child_repo, "test_child");

        fs::write(
            temp_dir.path().join("global_config.yaml"),
            "active_child_directory: test_child\ndata_format_version: '1.0'\n\
             created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n",
        )
        .unwrap();

        assert_eq!(
            repo.active_child_id().unwrap(),
            Some(ChildId::from("test_child"))
        );
    }

    /// `update_global_config` must not be a way to write a file whose two
    /// active-child keys disagree: `active_child_id` is what reads prefer, so a
    /// divergent pair silently activates whichever one the caller did not mean.
    #[test]
    fn update_global_config_cannot_leave_the_two_active_child_keys_divergent() {
        let (repo, child_repo, _temp_dir) = setup_test_repo();
        store_child_named(&child_repo, "test_child");
        store_child_named(&child_repo, "other_child");

        // Only the id key set — the legacy key must be filled in to match.
        let mut config = repo.get_global_config().unwrap();
        config.active_child_id = Some("test_child".to_string());
        config.active_child_directory = None;
        repo.update_global_config(&config).unwrap();

        let stored = repo.get_global_config().unwrap();
        assert_eq!(stored.active_child_id.as_deref(), Some("test_child"));
        assert_eq!(stored.active_child_directory.as_deref(), Some("test_child"));

        // Both set but disagreeing — the id key wins and both are rewritten.
        let mut config = repo.get_global_config().unwrap();
        config.active_child_id = Some("other_child".to_string());
        config.active_child_directory = Some("test_child".to_string());
        repo.update_global_config(&config).unwrap();

        let stored = repo.get_global_config().unwrap();
        assert_eq!(stored.active_child_id.as_deref(), Some("other_child"));
        assert_eq!(stored.active_child_directory.as_deref(), Some("other_child"));
        assert_eq!(
            repo.active_child_id().unwrap(),
            Some(ChildId::from("other_child"))
        );
    }

    /// The other direction: a caller that knows only the legacy key still
    /// produces a file the id-preferring read path resolves correctly.
    #[test]
    fn update_global_config_fills_in_the_id_key_from_the_legacy_key() {
        let (repo, child_repo, _temp_dir) = setup_test_repo();
        store_child_named(&child_repo, "test_child");

        let mut config = repo.get_global_config().unwrap();
        config.active_child_id = None;
        config.active_child_directory = Some("test_child".to_string());
        repo.update_global_config(&config).unwrap();

        let stored = repo.get_global_config().unwrap();
        assert_eq!(stored.active_child_id.as_deref(), Some("test_child"));
    }

    #[test]
    fn test_config_persistence() {
        let (repo, child_repo, temp_dir) = setup_test_repo();
        
        // Create a child and set as active
        let child = DomainChild {
            id: "test_child".to_string(),  // ID matches directory name
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