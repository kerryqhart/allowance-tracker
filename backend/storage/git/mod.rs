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
//! ```rust,no_run
//! use allowance_tracker_egui::backend::storage::git::GitManager;
//!
//! fn example() -> anyhow::Result<()> {
//!     let git_manager = GitManager::new();
//!
//!     // Ensure repository exists
//!     git_manager.ensure_repo_exists("/path/to/child/directory")?;
//!
//!     // Commit a file change
//!     git_manager.commit_file_change(
//!         "/path/to/child/directory",
//!         "transactions.csv",
//!         "Added $5.00 allowance transaction"
//!     )?;
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use git2::{Repository, Signature, IndexAddOption};
use log::{info, warn, debug};
use std::path::{Path, PathBuf};

/// Git manager for handling local repository operations
#[derive(Clone, Debug)]
pub struct GitManager {
    /// Default author name for commits
    author_name: String,
    /// Default author email for commits
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

    /// Initialize a git repository in the specified directory
    pub fn init_repo<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Initializing git repository at: {:?}", repo_path);
        Repository::init(repo_path)?;
        Ok(())
    }

    /// Ensure a git repository exists at the specified path
    pub fn ensure_repo_exists<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        if !self.is_git_repository(repo_path) {
            debug!("Repository doesn't exist, initializing at: {:?}", repo_path);
            self.init_repo(repo_path)?;
        }
        Ok(())
    }

    /// Stage a specific file for commit
    pub fn add_file<P: AsRef<Path>>(&self, repo_path: P, file_path: &str) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Staging file '{}' in repository: {:?}", file_path, repo_path);
        let repo = Repository::open(repo_path)?;
        let mut index = repo.index()?;
        index.add_path(Path::new(file_path))?;
        index.write()?;
        Ok(())
    }

    /// Stage all changes in the repository
    pub fn add_all<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        debug!("Staging all changes in repository: {:?}", repo_path);
        let repo = Repository::open(repo_path)?;
        let mut index = repo.index()?;
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    /// Create a commit with the staged changes
    pub fn commit<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<String> {
        let repo_path = repo_path.as_ref();
        debug!("Creating commit in repository: {:?} with message: {}", repo_path, message);

        let repo = Repository::open(repo_path)?;
        let signature = Signature::now(&self.author_name, &self.author_email)?;

        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;

        // Get the parent commit (HEAD) if it exists
        let parent_commit = match repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit()?;
                Some(commit)
            }
            Err(_) => None, // No HEAD yet (initial commit)
        };

        let commit_id = match &parent_commit {
            Some(parent) => {
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[parent],
                )?
            }
            None => {
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[],
                )?
            }
        };

        Ok(commit_id.to_string())
    }

    /// Check if repository has uncommitted changes
    pub fn has_uncommitted_changes<P: AsRef<Path>>(&self, repo_path: P) -> Result<bool> {
        let repo_path = repo_path.as_ref();
        debug!("Checking for uncommitted changes in repository: {:?}", repo_path);

        let repo = Repository::open(repo_path)?;
        let statuses = repo.statuses(None)?;

        Ok(!statuses.is_empty())
    }

    /// Commit file changes with staging (convenience method)
    pub fn commit_file_change<P: AsRef<Path>>(
        &self,
        repo_path: P,
        filename: &str,
        action_description: &str
    ) -> Result<()> {
        let repo_path = repo_path.as_ref();
        info!("Committing file change to {}: {} in repository: {:?}",
              filename, action_description, repo_path);

        // Ensure repo exists
        self.ensure_repo_exists(repo_path)?;

        // Stage the file
        if let Err(e) = self.add_file(repo_path, filename) {
            warn!("Failed to stage file {}: {}. Trying add_all instead.", filename, e);
            self.add_all(repo_path)?;
        }

        // Create commit message
        let message = format!("Update {}: {}", filename, action_description);

        // Only commit if there are changes
        if self.has_uncommitted_changes(repo_path)? {
            match self.commit(repo_path, &message) {
                Ok(commit_id) => {
                    info!("Created commit {} for {}", commit_id, filename);
                }
                Err(e) => {
                    warn!("Failed to create commit for {}: {}", filename, e);
                }
            }
        } else {
            debug!("No changes to commit for {}", filename);
        }

        Ok(())
    }

    /// Get the git directory path for a given repository path
    pub fn get_git_dir<P: AsRef<Path>>(&self, repo_path: P) -> PathBuf {
        repo_path.as_ref().join(".git")
    }

    /// Check if a directory is a git repository
    pub fn is_git_repository<P: AsRef<Path>>(&self, repo_path: P) -> bool {
        let repo_path = repo_path.as_ref();
        Repository::open(repo_path).is_ok()
    }

    // ========== SYNCHRONOUS VERSIONS FOR EGUI FRONTEND ==========
    // These are aliases since git2 is already synchronous

    /// Initialize a git repository in the specified directory (synchronous)
    pub fn init_repo_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        self.init_repo(repo_path)
    }

    /// Ensure a git repository exists at the specified path (synchronous)
    pub fn ensure_repo_exists_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        self.ensure_repo_exists(repo_path)
    }

    /// Stage all changes in the repository (synchronous)
    pub fn add_all_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        self.add_all(repo_path)
    }

    /// Create a commit with the staged changes (synchronous)
    pub fn commit_sync<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<String> {
        self.commit(repo_path, message)
    }

    /// Check if repository has uncommitted changes (synchronous)
    pub fn has_uncommitted_changes_sync<P: AsRef<Path>>(&self, repo_path: P) -> Result<bool> {
        self.has_uncommitted_changes(repo_path)
    }

    /// Commit file changes with staging (synchronous version)
    pub fn commit_file_change_sync<P: AsRef<Path>>(
        &self,
        repo_path: P,
        filename: &str,
        action_description: &str
    ) -> Result<()> {
        self.commit_file_change(repo_path, filename, action_description)
    }
}

impl Default for GitManager {
    fn default() -> Self {
        Self::new()
    }
}

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
    fn test_is_git_repository_nonexistent() {
        let git_manager = GitManager::new();
        assert!(!git_manager.is_git_repository("/nonexistent/path"));
    }

    #[test]
    fn test_init_and_commit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let git_manager = GitManager::new();

        // Initialize repo
        git_manager.init_repo(temp_dir.path()).unwrap();
        assert!(git_manager.is_git_repository(temp_dir.path()));

        // Create a test file
        std::fs::write(temp_dir.path().join("test.txt"), "hello").unwrap();

        // Commit it
        git_manager.add_file(temp_dir.path(), "test.txt").unwrap();
        let commit_id = git_manager.commit(temp_dir.path(), "Initial commit").unwrap();
        assert!(!commit_id.is_empty());
    }
}
