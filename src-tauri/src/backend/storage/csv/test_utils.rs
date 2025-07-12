/// Test utilities module for automatic cleanup and consistent test infrastructure
/// 
/// This module provides RAII-based cleanup that guarantees test data is removed
/// even if tests panic or fail.

use std::path::PathBuf;
use tempfile::TempDir;
use anyhow::Result;
use super::connection::CsvConnection;
use super::transaction_repository::TransactionRepository;
use super::child_repository::ChildRepository;
use super::goal_repository::GoalRepository;
use super::allowance_repository::AllowanceRepository;
use super::parental_control_repository::ParentalControlRepository;
use super::global_config_repository::GlobalConfigRepository;
use crate::backend::domain::models::child::Child as DomainChild;
use crate::backend::storage::ChildStorage;
use std::sync::Arc;
use chrono::Utc;

/// RAII Test Environment that automatically cleans up on drop
/// 
/// This struct ensures that test data is always cleaned up, even if tests panic.
/// The cleanup happens automatically when the TestEnvironment goes out of scope.
pub struct TestEnvironment {
    /// The temporary directory - kept alive to prevent auto-cleanup until drop
    _temp_dir: TempDir,
    /// The CSV connection for the test
    pub connection: CsvConnection,
    /// Base directory path for manual inspection if needed
    pub base_path: PathBuf,
}

impl TestEnvironment {
    /// Create a new test environment with automatic cleanup
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let base_path = temp_dir.path().to_path_buf();
        let connection = CsvConnection::new(&base_path)?;
        
        Ok(TestEnvironment {
            _temp_dir: temp_dir,
            connection,
            base_path,
        })
    }
    
    /// Create a new test environment with a custom prefix for debugging
    pub async fn new_with_prefix(prefix: &str) -> Result<Self> {
        let temp_dir = TempDir::with_prefix(prefix)?;
        let base_path = temp_dir.path().to_path_buf();
        let connection = CsvConnection::new(&base_path)?;
        
        Ok(TestEnvironment {
            _temp_dir: temp_dir,
            connection,
            base_path,
        })
    }
    
    /// Get the base directory path for this test environment
    pub fn base_directory(&self) -> &std::path::Path {
        &self.base_path
    }
}

// Drop implementation ensures cleanup always happens
impl Drop for TestEnvironment {
    fn drop(&mut self) {
        // TempDir automatically cleans up when dropped
        // We could add additional logging here if needed
        #[cfg(test)]
        {
            if std::env::var("ALLOWANCE_TRACKER_DEBUG_TESTS").is_ok() {
                println!("🧹 Cleaning up test environment: {:?}", self.base_path);
            }
        }
    }
}

/// Repository Test Helper with automatic cleanup
/// 
/// Provides easy access to all repositories with guaranteed cleanup
pub struct RepositoryTestHelper {
    pub env: TestEnvironment,
    pub transaction_repo: TransactionRepository,
    pub child_repo: ChildRepository,
    pub goal_repo: GoalRepository,
    pub allowance_repo: AllowanceRepository,
    pub parental_control_repo: ParentalControlRepository,
    pub global_config_repo: GlobalConfigRepository,
}

impl RepositoryTestHelper {
    /// Create a new repository test helper with all repositories
    pub async fn new() -> Result<Self> {
        let env = TestEnvironment::new().await?;
        
        let transaction_repo = TransactionRepository::new(env.connection.clone());
        let child_repo = ChildRepository::new(Arc::new(env.connection.clone()));
        let goal_repo = GoalRepository::new(env.connection.clone());
        let allowance_repo = AllowanceRepository::new(env.connection.clone());
        let parental_control_repo = ParentalControlRepository::new(env.connection.clone());
        let global_config_repo = GlobalConfigRepository::new(env.connection.clone());
        
        Ok(RepositoryTestHelper {
            env,
            transaction_repo,
            child_repo,
            goal_repo,
            allowance_repo,
            parental_control_repo,
            global_config_repo,
        })
    }
    
    /// Create a new repository test helper with a custom prefix for debugging
    pub async fn new_with_prefix(prefix: &str) -> Result<Self> {
        let env = TestEnvironment::new_with_prefix(prefix).await?;
        
        let transaction_repo = TransactionRepository::new(env.connection.clone());
        let child_repo = ChildRepository::new(Arc::new(env.connection.clone()));
        let goal_repo = GoalRepository::new(env.connection.clone());
        let allowance_repo = AllowanceRepository::new(env.connection.clone());
        let parental_control_repo = ParentalControlRepository::new(env.connection.clone());
        let global_config_repo = GlobalConfigRepository::new(env.connection.clone());
        
        Ok(RepositoryTestHelper {
            env,
            transaction_repo,
            child_repo,
            goal_repo,
            allowance_repo,
            parental_control_repo,
            global_config_repo,
        })
    }
    
    /// Create a test child and return it
    pub async fn create_test_child(&self, name: &str, id_suffix: &str) -> Result<DomainChild> {
        let child = DomainChild {
            id: format!("child::{}", id_suffix),
            name: name.to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2010, 1, 1).unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        self.child_repo.store_child(&child).await?;
        Ok(child)
    }
    
    /// Create a test child with custom birthdate and return it
    pub async fn create_test_child_with_birthdate(
        &self, 
        name: &str, 
        id_suffix: &str, 
        birthdate: chrono::NaiveDate
    ) -> Result<DomainChild> {
        let child = DomainChild {
            id: format!("child::{}", id_suffix),
            name: name.to_string(),
            birthdate,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        self.child_repo.store_child(&child).await?;
        Ok(child)
    }
}

/// Global cleanup function to remove any orphaned test directories
/// 
/// This can be called at the start of test runs to clean up any directories
/// left over from previous test runs that may have crashed.
pub fn cleanup_orphaned_test_directories() -> Result<()> {
    use std::fs;
    
    // Look for test_data_* directories in src-tauri/
    let src_tauri_path = std::env::current_dir()?.join("src-tauri");
    
    if !src_tauri_path.exists() {
        return Ok(());
    }
    
    let entries = fs::read_dir(&src_tauri_path)?;
    let mut cleaned_count = 0;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if dir_name.starts_with("test_data_") {
                    // This is an orphaned test directory
                    if let Err(e) = fs::remove_dir_all(&path) {
                        eprintln!("⚠️  Failed to clean up orphaned test directory {:?}: {}", path, e);
                    } else {
                        cleaned_count += 1;
                        #[cfg(test)]
                        {
                            if std::env::var("ALLOWANCE_TRACKER_DEBUG_TESTS").is_ok() {
                                println!("🧹 Cleaned up orphaned test directory: {:?}", path);
                            }
                        }
                    }
                }
            }
        }
    }
    
    if cleaned_count > 0 {
        println!("🧹 Cleaned up {} orphaned test directories", cleaned_count);
    }
    
    Ok(())
}

/// Macro to create a test with automatic cleanup
/// 
/// Usage:
/// ```rust
/// test_with_cleanup!(test_my_feature, {
///     let helper = RepositoryTestHelper::new().await?;
///     let child = helper.create_test_child("Test Child", "123").await?;
///     // ... test code ...
///     assert_eq!(child.name, "Test Child");
/// });
/// ```
#[macro_export]
macro_rules! test_with_cleanup {
    ($test_name:ident, $test_body:block) => {
        #[tokio::test]
        async fn $test_name() -> anyhow::Result<()> {
            // Clean up any orphaned directories first
            $crate::backend::storage::csv::test_utils::cleanup_orphaned_test_directories()?;
            
            // Run the test
            let result: anyhow::Result<()> = async move $test_body.await;
            
            // Return result (cleanup happens automatically via RAII)
            result
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_environment_cleanup() -> Result<()> {
        let base_path;
        
        // Create and use test environment
        {
            let env = TestEnvironment::new().await?;
            base_path = env.base_directory().to_path_buf();
            
            // Verify directory exists
            assert!(base_path.exists());
            
            // Create some test data
            std::fs::write(base_path.join("test_file.txt"), "test data")?;
            assert!(base_path.join("test_file.txt").exists());
        } // env goes out of scope here, triggering cleanup
        
        // Verify directory was cleaned up
        assert!(!base_path.exists());
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_repository_helper() -> Result<()> {
        let helper = RepositoryTestHelper::new().await?;
        
        // Test child creation
        let child = helper.create_test_child("Test Child", "123").await?;
        assert_eq!(child.name, "Test Child");
        assert_eq!(child.id, "child::123");
        
        // Verify child was stored
        let retrieved = helper.child_repo.get_child(&child.id).await?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Child");
        
        // Environment cleanup happens automatically
        Ok(())
    }
} 