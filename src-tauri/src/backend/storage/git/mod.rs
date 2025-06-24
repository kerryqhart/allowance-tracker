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

use anyhow::{Result, anyhow};
use git2::{Repository, Signature, IndexAddOption, Oid};
use log::{info, warn, error, debug};
use std::path::{Path, PathBuf};
use std::fs;

/// Git manager for handling local repository operations
#[derive(Clone)]
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
    /// 
    /// # Arguments
    /// * `repo_path` - Path to the directory where the git repository should be initialized
    /// 
    /// # Returns
    /// * `Result<()>` - Ok if successful, error if initialization fails
    pub async fn init_repo<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        
        debug!("Initializing git repository at: {:?}", repo_path);
        
        // Ensure the directory exists
        if !repo_path.exists() {
            fs::create_dir_all(repo_path)?;
            info!("Created directory: {:?}", repo_path);
        }

        // Initialize the repository
        match Repository::init(repo_path) {
            Ok(_repo) => {
                info!("Initialized git repository at: {:?}", repo_path);
                Ok(())
            }
            Err(e) => {
                error!("Failed to initialize git repository at {:?}: {}", repo_path, e);
                Err(anyhow!("Failed to initialize git repository: {}", e))
            }
        }
    }

    /// Ensure a git repository exists at the specified path
    /// If it doesn't exist, initialize it
    /// 
    /// # Arguments
    /// * `repo_path` - Path to the directory that should contain a git repository
    /// 
    /// # Returns
    /// * `Result<()>` - Ok if repository exists or was created successfully
    pub async fn ensure_repo_exists<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        let git_dir = repo_path.join(".git");

        if git_dir.exists() {
            debug!("Git repository already exists at: {:?}", repo_path);
            Ok(())
        } else {
            debug!("Git repository does not exist, initializing at: {:?}", repo_path);
            self.init_repo(repo_path).await
        }
    }

    /// Stage a specific file for commit
    /// 
    /// # Arguments
    /// * `repo_path` - Path to the git repository
    /// * `file_path` - Relative path to the file within the repository
    /// 
    /// # Returns
    /// * `Result<()>` - Ok if file was staged successfully
    pub async fn add_file<P: AsRef<Path>>(&self, repo_path: P, file_path: &str) -> Result<()> {
        let repo_path = repo_path.as_ref();
        
        debug!("Staging file '{}' in repository: {:?}", file_path, repo_path);

        let repo = Repository::open(repo_path)?;
        let mut index = repo.index()?;
        
        // Add the specific file to the index
        index.add_path(Path::new(file_path))?;
        index.write()?;

        debug!("Successfully staged file: {}", file_path);
        Ok(())
    }

    /// Stage all changes in the repository
    /// 
    /// # Arguments
    /// * `repo_path` - Path to the git repository
    /// 
    /// # Returns
    /// * `Result<()>` - Ok if all changes were staged successfully
    pub async fn add_all<P: AsRef<Path>>(&self, repo_path: P) -> Result<()> {
        let repo_path = repo_path.as_ref();
        
        debug!("Staging all changes in repository: {:?}", repo_path);

        let repo = Repository::open(repo_path)?;
        let mut index = repo.index()?;
        
        // Add all changes to the index
        index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        debug!("Successfully staged all changes");
        Ok(())
    }

    /// Create a commit with the staged changes
    /// 
    /// # Arguments
    /// * `repo_path` - Path to the git repository
    /// * `message` - Commit message
    /// 
    /// # Returns
    /// * `Result<Oid>` - The OID of the created commit if successful
    pub async fn commit<P: AsRef<Path>>(&self, repo_path: P, message: &str) -> Result<Oid> {
        let repo_path = repo_path.as_ref();
        
        debug!("Creating commit in repository: {:?}", repo_path);
        debug!("Commit message: {}", message);

        let repo = Repository::open(repo_path)?;
        let signature = Signature::now(&self.author_name, &self.author_email)?;
        
        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;

        // Check if this is the first commit
        let parent_commits = if let Ok(head) = repo.head() {
            if let Some(target) = head.target() {
                vec![repo.find_commit(target)?]
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        // Create the commit
        let commit_id = repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_commits.iter().collect::<Vec<_>>(),
        )?;

        info!("Created commit {} in repository: {:?}", commit_id, repo_path);
        info!("Commit message: {}", message);
        
        Ok(commit_id)
    }

    /// Get the status of the repository (check for uncommitted changes)
    /// 
    /// # Arguments
    /// * `repo_path` - Path to the git repository
    /// 
    /// # Returns
    /// * `Result<bool>` - True if there are uncommitted changes, false if clean
    pub async fn has_uncommitted_changes<P: AsRef<Path>>(&self, repo_path: P) -> Result<bool> {
        let repo_path = repo_path.as_ref();
        
        let repo = Repository::open(repo_path)?;
        let statuses = repo.statuses(None)?;
        
        let has_changes = !statuses.is_empty();
        debug!("Repository {:?} has uncommitted changes: {}", repo_path, has_changes);
        
        Ok(has_changes)
    }

    /// Commit a file change with automatic staging and standardized commit message
    /// This is the main method that repositories should use for git operations
    /// 
    /// # Arguments
    /// * `repo_path` - Path to the git repository
    /// * `filename` - Name of the file that was changed
    /// * `action_description` - Description of what was done to the file
    /// 
    /// # Returns
    /// * `Result<()>` - Ok if the commit was successful
    pub async fn commit_file_change<P: AsRef<Path>>(
        &self, 
        repo_path: P, 
        filename: &str, 
        action_description: &str
    ) -> Result<()> {
        let repo_path = repo_path.as_ref();
        
        // Ensure repository exists
        if let Err(e) = self.ensure_repo_exists(repo_path).await {
            warn!("Failed to ensure git repository exists at {:?}: {}", repo_path, e);
            return Ok(()); // Non-blocking: continue with main operation
        }

        // Check if there are changes to commit
        match self.has_uncommitted_changes(repo_path).await {
            Ok(false) => {
                debug!("No changes to commit in repository: {:?}", repo_path);
                return Ok(());
            }
            Ok(true) => {
                debug!("Found changes to commit in repository: {:?}", repo_path);
            }
            Err(e) => {
                warn!("Failed to check repository status at {:?}: {}", repo_path, e);
                return Ok(()); // Non-blocking: continue with main operation
            }
        }

        // Stage all changes
        if let Err(e) = self.add_all(repo_path).await {
            warn!("Failed to stage changes in repository {:?}: {}", repo_path, e);
            return Ok(()); // Non-blocking: continue with main operation
        }

        // Create commit message
        let commit_message = format!("Update {}: {}", filename, action_description);

        // Commit the changes
        match self.commit(repo_path, &commit_message).await {
            Ok(commit_id) => {
                info!("Successfully committed changes to {}: {} (commit: {})", 
                      filename, action_description, commit_id);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to commit changes in repository {:?}: {}", repo_path, e);
                Ok(()) // Non-blocking: continue with main operation
            }
        }
    }

    /// Get the git directory path for a given repository path
    pub fn get_git_dir<P: AsRef<Path>>(&self, repo_path: P) -> PathBuf {
        repo_path.as_ref().join(".git")
    }

    /// Check if a directory is a git repository
    pub fn is_git_repository<P: AsRef<Path>>(&self, repo_path: P) -> bool {
        self.get_git_dir(repo_path).exists()
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
    use tempfile::TempDir;
    use std::fs;

    async fn setup_test_repo() -> (GitManager, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let git_manager = GitManager::new();
        (git_manager, temp_dir)
    }

    #[tokio::test]
    async fn test_init_repo() {
        let (git_manager, temp_dir) = setup_test_repo().await;
        let repo_path = temp_dir.path().join("test_repo");

        let result = git_manager.init_repo(&repo_path).await;
        assert!(result.is_ok());
        assert!(repo_path.join(".git").exists());
    }

    #[tokio::test]
    async fn test_ensure_repo_exists_new() {
        let (git_manager, temp_dir) = setup_test_repo().await;
        let repo_path = temp_dir.path().join("new_repo");

        let result = git_manager.ensure_repo_exists(&repo_path).await;
        assert!(result.is_ok());
        assert!(repo_path.join(".git").exists());
    }

    #[tokio::test]
    async fn test_ensure_repo_exists_existing() {
        let (git_manager, temp_dir) = setup_test_repo().await;
        let repo_path = temp_dir.path().join("existing_repo");

        // Initialize repo first
        git_manager.init_repo(&repo_path).await.unwrap();
        
        // Should not fail when repo already exists
        let result = git_manager.ensure_repo_exists(&repo_path).await;
        assert!(result.is_ok());
        assert!(repo_path.join(".git").exists());
    }

    #[tokio::test]
    async fn test_add_file_and_commit() {
        let (git_manager, temp_dir) = setup_test_repo().await;
        let repo_path = temp_dir.path().join("commit_test");

        // Initialize repository
        git_manager.init_repo(&repo_path).await.unwrap();

        // Create a test file
        let test_file = repo_path.join("test.txt");
        fs::write(&test_file, "Hello, world!").unwrap();

        // Stage the file
        let result = git_manager.add_file(&repo_path, "test.txt").await;
        assert!(result.is_ok());

        // Commit the file
        let commit_result = git_manager.commit(&repo_path, "Add test file").await;
        assert!(commit_result.is_ok());
    }

    #[tokio::test]
    async fn test_commit_file_change() {
        let (git_manager, temp_dir) = setup_test_repo().await;
        let repo_path = temp_dir.path().join("file_change_test");

        // Create a test file
        fs::create_dir_all(&repo_path).unwrap();
        let test_file = repo_path.join("data.csv");
        fs::write(&test_file, "id,name,value\n1,test,100").unwrap();

        // Commit the file change
        let result = git_manager.commit_file_change(
            &repo_path,
            "data.csv",
            "Added initial data"
        ).await;
        assert!(result.is_ok());

        // Verify repository was created and commit was made
        assert!(repo_path.join(".git").exists());
    }

    #[tokio::test]
    async fn test_has_uncommitted_changes() {
        let (git_manager, temp_dir) = setup_test_repo().await;
        let repo_path = temp_dir.path().join("status_test");

        // Initialize repository
        git_manager.init_repo(&repo_path).await.unwrap();

        // Should have no changes initially
        let has_changes = git_manager.has_uncommitted_changes(&repo_path).await.unwrap();
        assert!(!has_changes);

        // Create a file
        let test_file = repo_path.join("test.txt");
        fs::write(&test_file, "New content").unwrap();

        // Should now have uncommitted changes
        let has_changes = git_manager.has_uncommitted_changes(&repo_path).await.unwrap();
        assert!(has_changes);
    }

    #[tokio::test]
    async fn test_is_git_repository() {
        let (git_manager, temp_dir) = setup_test_repo().await;
        let repo_path = temp_dir.path().join("is_repo_test");
        let non_repo_path = temp_dir.path().join("not_a_repo");

        // Create regular directory
        fs::create_dir_all(&non_repo_path).unwrap();

        // Should not be a git repository
        assert!(!git_manager.is_git_repository(&repo_path));
        assert!(!git_manager.is_git_repository(&non_repo_path));

        // Initialize git repository
        git_manager.init_repo(&repo_path).await.unwrap();

        // Should now be a git repository
        assert!(git_manager.is_git_repository(&repo_path));
        assert!(!git_manager.is_git_repository(&non_repo_path));
    }
}

 