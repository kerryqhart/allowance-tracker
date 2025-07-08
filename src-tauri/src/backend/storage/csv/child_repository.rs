use anyhow::{Result, Context};
use async_trait::async_trait;
use log::{info, warn, debug};
use std::fs;
use std::path::PathBuf;
use shared::Child as SharedChild;
use crate::backend::domain::models::child::Child as DomainChild;
use super::connection::CsvConnection;
use crate::backend::storage::GitManager;
use serde_yaml;

/// CSV-based child repository using filesystem discovery
#[derive(Clone)]
pub struct ChildRepository {
    connection: CsvConnection,
    git_manager: GitManager,
}

impl ChildRepository {
    /// Create a new CSV child repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { 
            connection,
            git_manager: GitManager::new(),
        }
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
            match self.load_child_from_directory(dir_name).await {
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
        
        info!("Discovered {} children", children.len());
        Ok(children)
    }
    
    /// Load a child from a specific directory
    async fn load_child_from_directory(&self, directory_name: &str) -> Result<Option<DomainChild>> {
        let yaml_path = self.get_child_yaml_path(directory_name);
        
        if !yaml_path.exists() {
            return Ok(None);
        }
        
        let yaml_content = fs::read_to_string(&yaml_path)?;
        let shared_child: SharedChild = serde_yaml::from_str(&yaml_content)?;
        
        let domain_child = crate::backend::io::rest::mappers::child_mapper::ChildMapper::to_domain(shared_child)
            .context("Failed to map shared child to domain child")?;

        Ok(Some(domain_child))
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
        let shared_child = crate::backend::io::rest::mappers::child_mapper::ChildMapper::to_dto(child.clone());
        let yaml_content = serde_yaml::to_string(&shared_child)?;
        
        // Atomic write using temp file
        let temp_path = yaml_path.with_extension("tmp");
        fs::write(&temp_path, yaml_content)?;
        fs::rename(&temp_path, &yaml_path)?;
        
        info!("Saved child {} to directory: {}", child.name, directory_name);
        
        // Git integration: commit the child.yaml change
        let action_description = format!("Updated child configuration for {}", child.name);
        
        // This is non-blocking - git errors won't fail the child operation
        let _ = self.git_manager.commit_file_change(
            &child_dir,
            "child.yaml", 
            &action_description
        ).await;
        
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
        let children = self.discover_children().await?;
        
        for child in children {
            if child.id == child_id {
                // We need to reverse-engineer the directory name from the child name
                // This is a bit hacky, but we'll use the safe directory name generation
                let directory_name = Self::generate_safe_directory_name(&child.name);
                
                // Verify this directory actually exists and contains this child
                if let Ok(Some(loaded_child)) = self.load_child_from_directory(&directory_name).await {
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
impl crate::backend::storage::ChildStorage for ChildRepository {
    /// Store a new child
    async fn store_child(&self, child: &DomainChild) -> Result<()> {
        let dir_name = Self::generate_safe_directory_name(&child.name);
        self.save_child_to_directory(child, &dir_name).await
    }
    
    /// Retrieve a specific child by ID
    async fn get_child(&self, child_id: &str) -> Result<Option<DomainChild>> {
        let children = self.discover_children().await?;
        Ok(children.into_iter().find(|c| c.id == child_id))
    }
    
    /// List all children ordered by name
    async fn list_children(&self) -> Result<Vec<DomainChild>> {
        self.discover_children().await
    }
    
    /// Update an existing child
    async fn update_child(&self, child: &DomainChild) -> Result<()> {
        let dir_name = self.find_directory_by_child_id(&child.id).await?
            .ok_or_else(|| anyhow::anyhow!("Could not find directory for child ID: {}", child.id))?;
        
        self.save_child_to_directory(child, &dir_name).await
    }
    
    /// Delete a child by ID
    async fn delete_child(&self, child_id: &str) -> Result<()> {
        let directory_name = match self.find_directory_by_child_id(child_id).await? {
            Some(dir) => dir,
            None => return Err(anyhow::anyhow!("Child not found: {}", child_id)),
        };
        
        let child_dir = self.connection.get_child_directory(&directory_name);
        
        if child_dir.exists() {
            fs::remove_dir_all(&child_dir)?;
            info!("Deleted child directory: {:?}", child_dir);
        }
        
        Ok(())
    }
    
    /// Get the currently active child
    async fn get_active_child(&self) -> Result<Option<String>> {
        let directory_name = match self.get_active_child_directory().await? {
            Some(dir) => dir,
            None => return Ok(None),
        };
        
        // Load the child from this directory and return their ID
        match self.load_child_from_directory(&directory_name).await? {
            Some(child) => Ok(Some(child.id)),
            None => {
                warn!("Active child directory {} doesn't contain a valid child", directory_name);
                Ok(None)
            }
        }
    }
    
    /// Set the currently active child
    async fn set_active_child(&self, child_id: &str) -> Result<()> {
        // First verify the child exists and find their directory
        let directory_name = match self.find_directory_by_child_id(child_id).await? {
            Some(dir) => dir,
            None => return Err(anyhow::anyhow!("Child not found: {}", child_id)),
        };
        
        self.set_active_child_directory(&directory_name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::backend::storage::ChildStorage;
    
    async fn setup_test_repo() -> (ChildRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let connection = CsvConnection::new(temp_dir.path()).unwrap();
        let repo = ChildRepository::new(connection);
        (repo, temp_dir)
    }
    
    #[tokio::test]
    async fn test_generate_safe_directory_name() {
        assert_eq!(ChildRepository::generate_safe_directory_name("Emma Smith"), "emma_smith");
        assert_eq!(ChildRepository::generate_safe_directory_name("José María"), "jose_maria");
        assert_eq!(ChildRepository::generate_safe_directory_name("Kid #1"), "kid_1");
        assert_eq!(ChildRepository::generate_safe_directory_name("Test-Child"), "test_child");
    }
    
    #[tokio::test]
    async fn test_store_and_discover_child() {
        let (repo, _temp_dir) = setup_test_repo().await;
        
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
        repo.store_child(&child).await.expect("Failed to store child");
        
        // Discover children
        let children = repo.list_children().await.expect("Failed to list children");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "Test Child");
        assert_eq!(children[0].id, "child::123");
        
        // Get the specific child
        let retrieved_child = repo.get_child("child::123").await.expect("Failed to get child");
        assert!(retrieved_child.is_some());
        assert_eq!(retrieved_child.unwrap().name, "Test Child");
    }
    
    #[tokio::test]
    async fn test_active_child_management() {
        let (repo, _temp_dir) = setup_test_repo().await;
        
        // Initially, no active child
        let active_child_id = repo.get_active_child().await.expect("Failed to get active child");
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
        repo.store_child(&child).await.expect("Failed to store child");
        
        // Set active child
        repo.set_active_child("child::456").await.expect("Failed to set active child");
        
        // Get active child
        let active_child_id = repo.get_active_child().await.expect("Failed to get active child");
        assert_eq!(active_child_id, Some("child::456".to_string()));
    }
} 