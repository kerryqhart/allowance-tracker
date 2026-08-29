/// Test utilities module for automatic cleanup and consistent test infrastructure
/// 
/// This module provides RAII-based cleanup that guarantees test data is removed
/// even if tests panic or fail.

use tempfile::TempDir;
use anyhow::Result;
use super::connection::CsvConnection;
use super::transaction_repository::TransactionRepository;
use super::child_repository::ChildRepository;
use super::allowance_repository::AllowanceRepository;
use super::parental_control_repository::ParentalControlRepository;
use super::global_config_repository::GlobalConfigRepository;
use super::goal_repository::GoalRepository;
use crate::backend::domain::models::child::Child as DomainChild;
use crate::backend::storage::traits::ChildStorage;
use std::sync::Arc;
use chrono::Utc;

/// Test environment that provides a temporary directory and connection
/// that will be automatically cleaned up when the environment is dropped,
/// even if tests panic or fail.
pub struct TestEnvironment {
    pub connection: CsvConnection,
    /// Base directory path for manual inspection if needed
    pub base_path: std::path::PathBuf,
    _temp_dir: TempDir,  // Keep alive to prevent cleanup
}

/// Test helper that provides repository instances for a test environment
pub struct TestHelper {
    pub env: TestEnvironment,
    pub transaction_repo: TransactionRepository,
    pub child_repo: ChildRepository,
    pub goal_repo: GoalRepository,
    pub allowance_repo: AllowanceRepository,
    pub parental_control_repo: ParentalControlRepository,
    pub global_config_repo: GlobalConfigRepository,
}

impl TestEnvironment {
    /// Create a new test environment with a temporary directory
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let connection = CsvConnection::new(temp_dir.path())?;
        Ok(Self {
            connection,
            base_path: temp_dir.path().to_path_buf(),
            _temp_dir: temp_dir,
        })
    }
}

impl TestHelper {
    /// Create a new test helper with a fresh environment
    pub fn new() -> Result<Self> {
        let env = TestEnvironment::new()?;
        let transaction_repo = TransactionRepository::new(env.connection.clone());
        let child_repo = ChildRepository::new(Arc::new(env.connection.clone()));
        let goal_repo = GoalRepository::new(env.connection.clone());
        let allowance_repo = AllowanceRepository::new(env.connection.clone());
        let parental_control_repo = ParentalControlRepository::new(env.connection.clone());
        let global_config_repo = GlobalConfigRepository::new(env.connection.clone());

        Ok(Self {
            env,
            transaction_repo,
            child_repo,
            goal_repo,
            allowance_repo,
            parental_control_repo,
            global_config_repo,
        })
    }

    /// Create a new test helper with an existing environment
    pub fn from_env(env: TestEnvironment) -> Result<Self> {
        let transaction_repo = TransactionRepository::new(env.connection.clone());
        let child_repo = ChildRepository::new(Arc::new(env.connection.clone()));
        let goal_repo = GoalRepository::new(env.connection.clone());
        let allowance_repo = AllowanceRepository::new(env.connection.clone());
        let parental_control_repo = ParentalControlRepository::new(env.connection.clone());
        let global_config_repo = GlobalConfigRepository::new(env.connection.clone());

        Ok(Self {
            env,
            transaction_repo,
            child_repo,
            goal_repo,
            allowance_repo,
            parental_control_repo,
            global_config_repo,
        })
    }

    /// Create a test child whose id deliberately differs from its sanitized
    /// display name.
    ///
    /// This is the default on purpose. When id, folder name, and sanitized
    /// name are all the same string, a resolver that uses the wrong one still
    /// passes — which is how the rename bug survived in a green suite.
    pub fn create_test_child(&self) -> Result<DomainChild> {
        self.create_test_child_with_distinct_id("Test Child", "child_fixture_001")
    }

    /// Create a test child with a specific display name and a distinct id.
    pub fn create_test_child_with_name(&self, name: &str) -> Result<DomainChild> {
        let safe = CsvConnection::generate_safe_directory_name(name);
        self.create_test_child_with_distinct_id(name, &format!("id_{safe}"))
    }

    /// Create a test child with an explicitly chosen id.
    pub fn create_test_child_with_distinct_id(&self, name: &str, id: &str) -> Result<DomainChild> {
        debug_assert_ne!(
            id,
            CsvConnection::generate_safe_directory_name(name),
            "fixtures must keep id and sanitized name distinct"
        );
        let child = DomainChild {
            id: id.to_string(),
            name: name.to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.child_repo.store_child(&child)?;
        Ok(child)
    }

    /// Verify that a child exists in storage
    pub fn verify_child_exists(&self, child: &DomainChild) -> Result<()> {
        let retrieved = self.child_repo.get_child(&child.id)?;
        assert!(retrieved.is_some(), "Child not found in storage");
        assert_eq!(retrieved.unwrap().name, child.name);
        Ok(())
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
                        eprintln!(" Failed to clean up orphaned test directory {:?}: {}", path, e);
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
///     let helper = RepositoryTestHelper::new()?;
///     let child = helper.create_test_child("Test Child", "123")?;
///     // ... test code ...
///     assert_eq!(child.name, "Test Child");
/// });
/// ```
#[macro_export]
macro_rules! test_with_cleanup {
    ($test_name:ident, $test_body:block) => {
        #[test]
        async fn $test_name() -> anyhow::Result<()> {
            // Clean up any orphaned directories first
            $crate::backend::storage::csv::test_utils::cleanup_orphaned_test_directories()?;
            
            // Run the test
            let result: anyhow::Result<()> = async move $test_body;
            
            // Return result (cleanup happens automatically via RAII)
            result
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_environment_cleanup() -> Result<()> {
        let base_path;
        {
            let env = TestEnvironment::new()?;
            base_path = env.base_path.clone();
            assert!(base_path.exists());
            // Environment dropped here
        }
        assert!(!base_path.exists());
        Ok(())
    }

    #[test]
    fn test_repository_helper() -> Result<()> {
        let helper = TestHelper::new()?;

        // Test child creation
        let child = helper.create_test_child()?;
        assert_eq!(child.name, "Test Child");
        assert_eq!(child.id, "child_fixture_001");

        // The whole point of the fixture: the id must NOT be the sanitized
        // display name, so a resolver that uses the wrong one is caught.
        assert_ne!(
            child.id,
            CsvConnection::generate_safe_directory_name(&child.name),
            "fixture must keep id and sanitized name distinct"
        );

        // Verify child was stored
        helper.verify_child_exists(&child)?;

        Ok(())
    }

    #[test]
    fn create_test_child_with_name_mints_a_distinct_id() -> Result<()> {
        let helper = TestHelper::new()?;

        let child = helper.create_test_child_with_name("Keiko Hart")?;
        assert_eq!(child.name, "Keiko Hart");
        assert_ne!(
            child.id,
            CsvConnection::generate_safe_directory_name(&child.name),
            "fixture must keep id and sanitized name distinct"
        );

        helper.verify_child_exists(&child)?;

        Ok(())
    }
} 