use anyhow::Result;
use log::{info, warn, debug};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use shared::Child as SharedChild;
use crate::backend::domain::models::child::Child as DomainChild;
use serde::{Deserialize, Serialize};

/// Intermediate struct for YAML serialization with string date fields
#[derive(Debug, Clone, Serialize, Deserialize)]
struct YamlChild {
    id: String,
    name: String,
    birthdate: String, // String representation for YAML
    created_at: String, // String representation for YAML
    updated_at: String, // String representation for YAML
}
use super::connection::CsvConnection;
use crate::backend::storage::GitManager;
use serde_yaml;

/// CSV-based child repository using filesystem discovery
#[derive(Clone)]
pub struct ChildRepository {
    connection: Arc<CsvConnection>,
    #[allow(dead_code)]
    git_manager: GitManager,
}

impl ChildRepository {
    /// Create a new CSV child repository
    pub fn new(connection: Arc<CsvConnection>) -> Self {
        Self { 
            connection,
            git_manager: GitManager::new(),
        }
    }
    
    /// Generate a safe filesystem identifier from a child name
    /// Converts "Emma Smith" -> "Emma_Smith", "José María" -> "Jose_Maria", etc.
    pub fn generate_safe_directory_name(child_name: &str) -> String {
        let result = child_name
            .chars()
            .map(|c| {
                if c.is_whitespace() {
                    '_'
                } else {
                    // Replace accented characters and special chars first
                    match c {
                        'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
                        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
                        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
                        'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
                        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
                        'ñ' | 'Ñ' => 'n',
                        'ç' | 'Ç' => 'c',
                        c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
                        '#' => '_', // Handle common special chars like #
                        '-' => '_', // Handle dashes
                        _ => '_',
                    }
                }
            })
            .collect::<String>();
        
        // Collapse consecutive underscores into single underscores
        let mut collapsed = String::new();
        let mut last_was_underscore = false;
        
        for c in result.chars() {
            if c == '_' {
                if !last_was_underscore {
                    collapsed.push('_');
                }
                last_was_underscore = true;
            } else {
                collapsed.push(c);
                last_was_underscore = false;
            }
        }
        
        collapsed.trim_matches('_').to_string()
    }
    
    /// Get the path to a child's YAML configuration file
    fn get_child_yaml_path(&self, directory_name: &str) -> PathBuf {
        self.connection.get_child_directory(directory_name).join("child.yaml")
    }
    
    /// Get the path to the global configuration file
    fn get_global_config_path(&self) -> PathBuf {
        self.connection.base_directory().join("global_config.yaml")
    }
    
    /// Discover all children by scanning directories (synchronous version)
    fn discover_children(&self) -> Result<Vec<DomainChild>> {
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
    
    /// Load a child from a specific directory (synchronous version)
    fn load_child_from_directory(&self, directory_name: &str) -> Result<Option<DomainChild>> {
        let yaml_path = self.get_child_yaml_path(directory_name);
        
        if !yaml_path.exists() {
            return Ok(None);
        }
        
        let yaml_content = fs::read_to_string(&yaml_path)?;
        let yaml_child: YamlChild = serde_yaml::from_str(&yaml_content)?;
        
        // Map YAML child to domain child with proper type conversions
        let domain_child = DomainChild {
            id: yaml_child.id,
            name: yaml_child.name,
            birthdate: chrono::NaiveDate::parse_from_str(&yaml_child.birthdate, "%Y-%m-%d")
                .map_err(|e| anyhow::anyhow!("Failed to parse birthdate: {}", e))?,
            created_at: chrono::DateTime::parse_from_rfc3339(&yaml_child.created_at)
                .map_err(|e| anyhow::anyhow!("Failed to parse created_at: {}", e))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&yaml_child.updated_at)
                .map_err(|e| anyhow::anyhow!("Failed to parse updated_at: {}", e))?
                .with_timezone(&chrono::Utc),
        };

        Ok(Some(domain_child))
    }
    
    /// Save a child to their directory (synchronous version)
    fn save_child_to_directory(&self, child: &DomainChild, directory_name: &str) -> Result<()> {
        // Ensure the child directory exists
        let child_dir = self.connection.get_child_directory(directory_name);
        if !child_dir.exists() {
            fs::create_dir_all(&child_dir)?;
            info!("Created child directory: {:?}", child_dir);
        }
        
        // Convert domain child to YAML child
        let yaml_child = YamlChild {
            id: child.id.clone(),
            name: child.name.clone(),
            birthdate: child.birthdate.format("%Y-%m-%d").to_string(),
            created_at: child.created_at.to_rfc3339(),
            updated_at: child.updated_at.to_rfc3339(),
        };
        
        // Write child.yaml
        let yaml_path = self.get_child_yaml_path(directory_name);
        let yaml_content = serde_yaml::to_string(&yaml_child)?;
        
        // Atomic write using temp file
        let temp_path = yaml_path.with_extension("tmp");
        fs::write(&temp_path, yaml_content)?;
        fs::rename(&temp_path, &yaml_path)?;
        
        info!("Saved child {} to directory: {}", child.name, directory_name);
        
        Ok(())
    }
    
    /// Get the currently active child directory name from global config (synchronous version)
    fn get_active_child_directory(&self) -> Result<Option<String>> {
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
    
    /// Set the currently active child directory in global config (synchronous version)
    fn set_active_child_directory(&self, directory_name: &str) -> Result<()> {
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
    
    /// Find directory name for a child by ID (synchronous version)
    fn find_directory_by_child_id(&self, child_id: &str) -> Result<Option<String>> {
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

impl crate::backend::storage::ChildStorage for ChildRepository {
    /// Store a new child
    fn store_child(&self, child: &DomainChild) -> Result<()> {
        let dir_name = Self::generate_safe_directory_name(&child.name);
        self.save_child_to_directory(child, &dir_name)
    }
    
    /// Retrieve a specific child by ID
    fn get_child(&self, child_id: &str) -> Result<Option<DomainChild>> {
        let children = self.discover_children()?;
        Ok(children.into_iter().find(|c| c.id == child_id))
    }
    
    /// List all children ordered by name
    fn list_children(&self) -> Result<Vec<DomainChild>> {
        self.discover_children()
    }
    
    /// Update an existing child
    fn update_child(&self, child: &DomainChild) -> Result<()> {
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
    fn delete_child(&self, child_id: &str) -> Result<()> {
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
    
    /// Get the currently active child
    fn get_active_child(&self) -> Result<Option<String>> {
        // Get the active child directory name
        if let Some(directory_name) = self.get_active_child_directory()? {
            // Load the child from that directory to get their ID
            if let Some(child) = self.load_child_from_directory(&directory_name)? {
                return Ok(Some(child.id));
            }
        }
        Ok(None)
    }
    
    /// Set the currently active child
    fn set_active_child(&self, child_id: &str) -> Result<()> {
        // Find the directory name for this child ID
        if let Some(directory_name) = self.find_directory_by_child_id(child_id)? {
            self.set_active_child_directory(&directory_name)
        } else {
            Err(anyhow::anyhow!("Child not found: {}", child_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::backend::storage::ChildStorage;
    
    fn setup_test_repo() -> (ChildRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let connection = CsvConnection::new(temp_dir.path()).unwrap();
        let repo = ChildRepository::new(Arc::new(connection));
        (repo, temp_dir)
    }
    
    #[test]
    fn test_generate_safe_directory_name() {
        assert_eq!(ChildRepository::generate_safe_directory_name("Emma Smith"), "emma_smith");
        assert_eq!(ChildRepository::generate_safe_directory_name("José María"), "jose_maria");
        assert_eq!(ChildRepository::generate_safe_directory_name("Kid #1"), "kid_1");
        assert_eq!(ChildRepository::generate_safe_directory_name("Test-Child"), "test_child");
    }
    
    #[test]
    fn test_store_and_discover_child() {
        let (repo, _temp_dir) = setup_test_repo();
        
        // Create a child
        let now = chrono::Utc::now();
        let child = DomainChild {
            id: "child::123".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2015, 5, 15).unwrap(),
            created_at: now,
            updated_at: now,
        };
        
        // Store the child
        repo.store_child(&child).expect("Failed to store child");
        
        // Discover children
        let children = repo.list_children().expect("Failed to list children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "Test Child");
        assert_eq!(children[0].id, "child::123");
        
        // Get the specific child
        let retrieved_child = repo.get_child("child::123").expect("Failed to get child");
        assert!(retrieved_child.is_some());
        assert_eq!(retrieved_child.unwrap().name, "Test Child");
    }
    
    #[test]
    fn test_active_child_management() {
        let (repo, _temp_dir) = setup_test_repo();
        
        // Initially, no active child
        let active_child_id = repo.get_active_child().expect("Failed to get active child");
        assert!(active_child_id.is_none());
        
        // Create and store a child
        let now = chrono::Utc::now();
        let child = DomainChild {
            id: "child::456".to_string(),
            name: "Active Child".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2018, 8, 8).unwrap(),
            created_at: now,
            updated_at: now,
        };
        repo.store_child(&child).expect("Failed to store child");
        
        // Set active child
        repo.set_active_child("child::456").expect("Failed to set active child");
        
        // Get active child
        let active_child_id = repo.get_active_child().expect("Failed to get active child");
        assert_eq!(active_child_id, Some("child::456".to_string()));
    }
} 