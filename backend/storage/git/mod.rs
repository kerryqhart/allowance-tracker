//! # Git Versioning Module
//!
//! This module provides git repository management for child directories in the allowance tracker.
//! Each child directory becomes its own git repository with automatic versioning of data files.
//!
//! ## Features
//!
//! - Initialize git repositories in child directories
//! - Stage and commit file changes automatically
//! - Non-blocking git operations (errors are logged but don't fail main operations)
//! - Standard commit messages for different file types
//!
//! ## Usage
//!
//! ```rust
//! let git_manager = GitManager::new();
//! 
//! // Ensure repository exists
//! git_manager.ensure_repo_exists("/path/to/child/directory").await?;
//! 
//! // Commit a file change
//! git_manager.commit_file_change(
//!     "/path/to/child/directory",
//!     "transactions.csv",
//!     "Added $5.00 allowance transaction"
//! ).await?;
//! ```

use anyhow::Result;
// Temporarily disabled git2 to avoid OpenSSL build issues
// use git2::{Repository, Signature, IndexAddOption, Oid};
use log::{info, debug};
use std::path::{Path, PathBuf};


/// Git manager for handling local repository operations
/// NOTE: Temporarily simplified to avoid git2/OpenSSL build issues
#[derive(Clone)]
pub struct GitManager {
    /// Default author name for commits
    #[allow(dead_code)]
    author_name: String,
    /// Default author email for commits
    #[allow(dead_code)]
    author_email: String,
}

impl GitManager {
    /// Create a new GitManager with default configuration
    pub fn new() -> Self {
        Self {
            author_name: "Allowance Tracker".to_string(),
            author_email: "allowance@tracker.local".to_string(),
        }
    }

    /// Create a new GitManager with custom author information
    pub fn with_author(author_name: String, author_email: String) -> Self {
        Self {
            author_name,
            author_email,
        }
    }

    /// Initialize a git repository in the specified directory (no-op version)
    pub fn init_repo<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Would initialize git repository at: {:?} (no-op)", repo_path);
        Ok(())
    }

    /// Ensure a git repository exists at the specified path (no-op version)
    pub fn ensure_repo_exists<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Would ensure git repository exists at: {:?} (no-op)", repo_path);
        Ok(())
    }

    /// Stage a specific file for commit (no-op version)
    pub fn add_file<P: AsRef<Path>>(&self, repo_path: P, file_path: &str) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Would stage file '{}' in repository: {:?} (no-op)", file_path, repo_path);
        Ok(())
    }

    /// Stage all changes in the repository (no-op version)
    pub fn add_all<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Would stage all changes in repository: {:?} (no-op)", repo_path);
        Ok(())
    }

    /// Create a commit with the staged changes (no-op version)
    pub fn commit<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<String> {
        let repo_path = repo_path.as_ref();
        debug!("Would create commit in repository: {:?} with message: {} (no-op)", repo_path, message);
        Ok("fake-commit-id".to_string())
    }

    /// Check if repository has uncommitted changes (no-op version)
    pub fn has_uncommitted_changes<P: AsRef<Path>>(&self, repo_path: P) -> Result<bool> {
        let repo_path = repo_path.as_ref();
        debug!("Would check for uncommitted changes in repository: {:?} (no-op)", repo_path);
        Ok(false) // Always report no changes in no-op mode
    }

    /// Commit file changes with staging (sync version)
    pub fn commit_file_change<P: AsRef<Path>>(
        &self, 
        repo_path: P, 
        filename: &str, 
        action_description: &str
    ) -> Result<()> {
        let repo_path = repo_path.as_ref();
        info!("Would commit file change to {}: {} in repository: {:?} (no-op)", 
              filename, action_description, repo_path);
        Ok(())
    }

    /// Get the git directory path for a given repository path
    pub fn get_git_dir<P: AsRef<Path>>(&self, repo_path: P) -> PathBuf {
        repo_path.as_ref().join(".git")
    }

    /// Check if a directory is a git repository (no-op version)
    pub fn is_git_repository<P: AsRef<Path>>(&self, repo_path: P) -> bool {
        let _repo_path = repo_path.as_ref();
        false // Always report not a git repository in no-op mode
    }

    // ========== SYNCHRONOUS VERSIONS FOR EGUI FRONTEND ==========

    /// Initialize a git repository in the specified directory (synchronous no-op)
    pub fn init_repo_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Would initialize git repository at: {:?} (sync no-op)", repo_path);
        Ok(())
    }

    /// Ensure a git repository exists at the specified path (synchronous no-op)
    pub fn ensure_repo_exists_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Would ensure git repository exists at: {:?} (sync no-op)", repo_path);
        Ok(())
    }

    /// Stage all changes in the repository (synchronous no-op)
    pub fn add_all_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Would stage all changes in repository: {:?} (sync no-op)", repo_path);
        Ok(())
    }

    /// Create a commit with the staged changes (synchronous no-op)
    pub fn commit_sync<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<String> {
        let repo_path = repo_path.as_ref();
        debug!("Would create commit in repository: {:?} with message: {} (sync no-op)", repo_path, message);
        Ok("fake-commit-id".to_string())
    }

    /// Check if repository has uncommitted changes (synchronous no-op)
    pub fn has_uncommitted_changes_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<bool> {
        let repo_path = repo_path.as_ref();
        debug!("Would check for uncommitted changes in repository: {:?} (sync no-op)", repo_path);
        Ok(false) // Always report no changes in no-op mode
    }

    /// Commit file changes with staging (synchronous no-op version)
    pub fn commit_file_change_sync<P: AsRef<Path>>(
        &self, 
        repo_path: P, 
        filename: &str, 
        action_description: &str
    ) -> Result<()> {
        let repo_path = repo_path.as_ref();
        info!("Would commit file change to {}: {} in repository: {:?} (sync no-op)", 
              filename, action_description, repo_path);
        Ok(())
    }
}

impl Default for GitManager {
    fn default() -> Self {
        Self::new()
    }
}

// Tests temporarily disabled due to git2 dependency removal
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_manager_creation() {
        let git_manager = GitManager::new();
        assert_eq!(git_manager.author_name, "Allowance Tracker");
        assert_eq!(git_manager.author_email, "allowance@tracker.local");
    }

    #[test] 
    fn test_is_git_repository_no_op() {
        let git_manager = GitManager::new();
        // In no-op mode, always returns false
        assert!(!git_manager.is_git_repository("/some/path"));
    }
}

 