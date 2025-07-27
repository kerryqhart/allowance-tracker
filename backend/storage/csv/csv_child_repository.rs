use anyhow::{Result, Context};
use async_trait::async_trait;
use log::{info, warn, debug};
use std::fs;
use std::path::PathBuf;
use crate::backend::domain::models::child::Child as DomainChild;
use crate::backend::storage::traits::ChildRepository;
use serde::{Deserialize, Serialize};
use super::connection::CsvConnection;
use serde_yaml;

/// CSV-based child repository using filesystem discovery
#[derive(Clone)]
pub struct CsvChildRepository {
    connection: CsvConnection,
}

impl CsvChildRepository {
    /// Create a new CSV child repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { connection }
    }
    
    /// Generate a safe filesystem identifier from a child name
    /// Converts "Emma Smith" -> "Emma_Smith", "José María" -> "Jose_Maria", etc.
    pub fn generate_safe_directory_name(child_name: &str) -> String {
        child_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else if c.is_whitespace() {
                    '_'
                } else {
                    // Replace accented characters and special chars
                    match c {
                        'á' | 'à' | 'ä' | 'â' => 'a',
                        'é' | 'è' | 'ë' | 'ê' => 'e',
                        'í' | 'ì' | 'ï' | 'î' => 'i',
                        'ó' | 'ò' | 'ö' | 'ô' => 'o',
                        'ú' | 'ù' | 'ü' | 'û' => 'u',
                        'ñ' => 'n',
                        'ç' => 'c',
                        _ => '_',
                    }
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }
    
    /// Get the path to a child's YAML configuration file
    fn get_child_yaml_path(&self, directory_name: &str) -> PathBuf {
        self.connection.get_child_directory(directory_name).join("child.yaml")
    }
    
    /// Get the path to the global configuration file
    fn get_global_config_path(&self) -> PathBuf {
        self.connection.base_directory().join("global_config.yaml")
    }
    
    /// Discover all children by scanning directories
    async fn discover_children(&self) -> Result<Vec<DomainChild>> {
        let base_dir = self.connection.base_directory();
        
        if !base_dir.exists() {
            debug!("Base directory doesn't exist, returning empty children list");
            return Ok(Vec::new());
        }
        
        let mut children = Vec::new();
        
        // Read all subdirectories
        for entry in fs::read_dir(base_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            // Skip files, only process directories
            if !path.is_dir() {
                continue;
            }
            
            // Get directory name
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => {
                    warn!("Skipping directory with invalid name: {:?}", path);
                    continue;
                }
            };
            
            // Try to load child from this directory
            match self.load_child_from_directory(dir_name) {
                Ok(Some(child)) => {
                    debug!("Discovered child: {} from directory: {}", child.name, dir_name);
                    children.push(child);
                },
                Ok(None) => {
                    debug!("Directory {} doesn't contain a valid child", dir_name);
                },
                Err(e) => {
                    warn!("Error loading child from directory {}: {}", dir_name, e);
                }
            }
        }
        
        // Sort children by name for consistent ordering
        children.sort_by(|a, b| a.name.cmp(&b.name));
        
        debug!("Discovered {} children", children.len());
        Ok(children)
    }
    
    /// Load a child from a specific directory
    async fn load_child_from_directory(&self, directory_name: &str) -> Result<Option<DomainChild>> {
        let yaml_path = self.get_child_yaml_path(directory_name);
        
        if !yaml_path.exists() {
            return Ok(None);
        }
        
        let yaml_content = fs::read_to_string(&yaml_path)?;
        let shared_child: DomainChild = serde_yaml::from_str(&yaml_content)?;
        
        Ok(Some(shared_child))
    }
    
    /// Save a child to their directory
    async fn save_child_to_directory(&self, child: &DomainChild, directory_name: &str) -> Result<()> {
        // Ensure the child directory exists
        let child_dir = self.connection.get_child_directory(directory_name);
        if !child_dir.exists() {
            fs::create_dir_all(&child_dir)?;
            info!("Created child directory: {:?}", child_dir);
        }
        
        // Write child.yaml
        let yaml_path = self.get_child_yaml_path(directory_name);
        let yaml_content = serde_yaml::to_string(&child)?;
        
        // Atomic write using temp file
        let temp_path = yaml_path.with_extension("tmp");
        fs::write(&temp_path, yaml_content)?;
        fs::rename(&temp_path, &yaml_path)?;
        
        info!("Saved child {} to directory: {}", child.name, directory_name);
        
        Ok(())
    }
    
    /// Get the currently active child directory name from global config
    async fn get_active_child_directory(&self) -> Result<Option<String>> {
        let global_config_path = self.get_global_config_path();
        
        if !global_config_path.exists() {
            return Ok(None);
        }
        
        let yaml_content = fs::read_to_string(&global_config_path)?;
        let config: serde_yaml::Value = serde_yaml::from_str(&yaml_content)?;
        
        Ok(config
            .get("active_child_directory")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()))
    }
    
    /// Set the currently active child directory in global config
    async fn set_active_child_directory(&self, directory_name: &str) -> Result<()> {
        let global_config_path = self.get_global_config_path();
        
        // Create minimal global config
        let mut config = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
        config["active_child_directory"] = serde_yaml::Value::String(directory_name.to_string());
        config["data_format_version"] = serde_yaml::Value::String("1.0".to_string());
        
        let yaml_content = serde_yaml::to_string(&config)?;
        
        // Atomic write using temp file
        let temp_path = global_config_path.with_extension("tmp");
        fs::write(&temp_path, yaml_content)?;
        fs::rename(&temp_path, &global_config_path)?;
        
        info!("Set active child directory to: {}", directory_name);
        Ok(())
    }
    
    /// Find directory name for a child by ID
    async fn find_directory_by_child_id(&self, child_id: &str) -> Result<Option<String>> {
        let children = self.discover_children()?;
        
        for child in children {
            if child.id == child_id {
                // We need to reverse-engineer the directory name from the child name
                // This is a bit hacky, but we'll use the safe directory name generation
                let directory_name = Self::generate_safe_directory_name(&child.name);
                
                // Verify this directory actually exists and contains this child
                if let Ok(Some(loaded_child)) = self.load_child_from_directory(&directory_name) {
                    if loaded_child.id == child_id {
                        return Ok(Some(directory_name));
                    }
                }
            }
        }
        
        Ok(None)
    }
}

#[async_trait]
impl ChildRepository for CsvChildRepository {
    /// Store a new child
    async fn store_child(&self, child: &DomainChild) -> Result<()> {
        let dir_name = Self::generate_safe_directory_name(&child.name);
        self.save_child_to_directory(child, &dir_name)
    }
    
    /// Retrieve a specific child by ID
    async fn get_child(&self, child_id: &str) -> Result<Option<DomainChild>> {
        let children = self.discover_children()?;
        Ok(children.into_iter().find(|c| c.id == child_id))
    }
    
    /// List all children ordered by name
    async fn list_children(&self) -> Result<Vec<DomainChild>> {
        self.discover_children()
    }
    
    /// Update an existing child
    async fn update_child(&self, child: &DomainChild) -> Result<()> {
        // Find existing child directory to handle name changes
        if let Some(dir_name) = self.find_directory_by_child_id(&child.id)? {
            self.save_child_to_directory(child, &dir_name)
        } else {
            // This is an update, so the child should exist
            warn!("Attempted to update a non-existent child: {}", child.id);
            Err(anyhow::anyhow!("Child not found for update"))
        }
    }

    /// Delete a child by ID
    async fn delete_child(&self, child_id: &str) -> Result<()> {
        if let Some(dir_name) = self.find_directory_by_child_id(child_id)? {
            let child_dir = self.connection.get_child_directory(&dir_name);
            if child_dir.exists() {
                fs::remove_dir_all(&child_dir)?;
                info!("Deleted child directory: {:?}", child_dir);
            }
        } else {
            warn!("Attempted to delete a non-existent child: {}", child_id);
        }
        Ok(())
    }

    /// Get the currently active child ID
    async fn get_active_child(&self) -> Result<Option<String>> {
        if let Some(dir_name) = self.get_active_child_directory()? {
            if let Ok(Some(child)) = self.load_child_from_directory(&dir_name) {
                Ok(Some(child.id))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Set the currently active child
    async fn set_active_child(&self, child_id: &str) -> Result<()> {
        if let Some(dir_name) = self.find_directory_by_child_id(child_id)? {
            self.set_active_child_directory(&dir_name)
        } else {
            Err(anyhow::anyhow!("Child not found to set as active"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use chrono::NaiveDate;
    use crate::backend::storage::traits::ChildRepository;

    fn setup_test_repo() -> (CsvChildRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let connection = CsvConnection::new(temp_dir.path()).unwrap();
        (CsvChildRepository::new(connection), temp_dir)
    }

    #[test]
    fn test_generate_safe_directory_name() {
        assert_eq!(CsvChildRepository::generate_safe_directory_name("Emma Smith"), "emma_smith");
        assert_eq!(CsvChildRepository::generate_safe_directory_name("José María"), "jose_maria");
        assert_eq!(CsvChildRepository::generate_safe_directory_name("  Test-Name "), "test_name");
    }

    #[test]
    fn test_store_and_discover_child() {
        let (repo, _temp_dir) = setup_test_repo();
        let child = DomainChild {
            id: "child1".to_string(),
            name: "Test Child".to_string(),
            birthdate: NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        repo.store_child(&child).unwrap();

        let children = repo.discover_children().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, "child1");
    }

    #[test]
    fn test_active_child_management() {
        let (repo, _temp_dir) = setup_test_repo();
        let child = DomainChild {
            id: "child1".to_string(),
            name: "Test Child".to_string(),
            birthdate: NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        repo.store_child(&child).unwrap();

        repo.set_active_child("child1").unwrap();
        let active_id = repo.get_active_child().unwrap();
        assert_eq!(active_id, Some("child1".to_string()));
    }
} 