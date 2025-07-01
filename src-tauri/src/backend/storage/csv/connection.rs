use anyhow::Result;
use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use chrono::Utc;
use crate::backend::storage::traits::Connection;
use log::{info, warn, error};
use fs_extra::dir::{CopyOptions, copy as copy_dir};
use std::sync::{Arc, Mutex};

/// CsvConnection manages file paths and ensures CSV files exist for each child
#[derive(Clone)]
pub struct CsvConnection {
    base_directory: Arc<Mutex<PathBuf>>,
}

impl CsvConnection {
    /// Create a new CSV connection with a base directory
    pub fn new<P: AsRef<Path>>(base_directory: P) -> Result<Self> {
        let base_path = base_directory.as_ref().to_path_buf();
        
        // Create the base directory if it doesn't exist
        if !base_path.exists() {
            fs::create_dir_all(&base_path)?;
        }
        
        Ok(Self {
            base_directory: Arc::new(Mutex::new(base_path)),
        })
    }
    
    /// Create a new CSV connection in the default data directory
    /// This uses ~/Documents/Allowance Tracker, but checks for redirect file first
    pub fn new_default() -> Result<Self> {
        // Get the user's home directory and construct the Documents path
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
        
        let documents_dir = PathBuf::from(home_dir).join("Documents");
        let default_data_dir = documents_dir.join("Allowance Tracker");
        
        // Check for redirect file in the default location
        let redirect_file = default_data_dir.join(".allowance_redirect");
        
        let actual_data_dir = if redirect_file.exists() {
            // Read the redirect file to get the actual directory
            match fs::read_to_string(&redirect_file) {
                Ok(redirected_path) => {
                    let redirected_path = redirected_path.trim();
                    let path = PathBuf::from(redirected_path);
                    
                    if path.exists() {
                        info!("Found redirect file, using data directory: {}", path.display());
                        path
                    } else {
                        warn!("Redirect file points to non-existent directory: {}. Using default.", redirected_path);
                        default_data_dir
                    }
                }
                Err(e) => {
                    error!("Failed to read redirect file: {}. Using default directory.", e);
                    default_data_dir
                }
            }
        } else {
            info!("No redirect file found, using default data directory: {}", default_data_dir.display());
            default_data_dir
        };
        
        Self::new(actual_data_dir)
    }
    
    /// Create a new CSV connection for testing with a unique directory
    pub async fn new_for_testing() -> Result<Self> {
        // Create a unique test directory using timestamp
        let timestamp = Utc::now().timestamp_millis();
        let test_dir = PathBuf::from(format!("test_data_{}", timestamp));
        Self::new(test_dir)
    }
    
    /// Get the directory path for a child's data using the child name
    pub fn get_child_directory(&self, child_name: &str) -> PathBuf {
        // debug!("🔍 get_child_directory called for: {}", child_name);
        
        let base_dir = self.base_directory.lock().unwrap();
        // debug!("🔍 Base directory locked: {}", base_dir.display());
        
        // Check for redirect file in the child's directory
        let child_dir = base_dir.join(child_name);
        // debug!("🔍 Child directory path: {}", child_dir.display());
        
        let redirect_file = child_dir.join(".allowance_redirect");
        // debug!("🔍 Redirect file path: {}", redirect_file.display());
        
        // debug!("🔍 Checking if redirect file exists...");
                  if redirect_file.exists() {
              // debug!("📁 Redirect file exists, reading it...");
            // Read the redirect file to get the actual directory
            match fs::read_to_string(&redirect_file) {
                Ok(redirected_path) => {
                    let redirected_path = redirected_path.trim();
                                          // debug!("📄 Redirect file content: {}", redirected_path);
                    let path = PathBuf::from(redirected_path);
                    
                    // debug!("🔍 Checking if redirected path exists: {}", path.display());
                    if path.exists() {
                        info!("✅ Child {} data redirected to: {}", child_name, path.display());
                        return path;
                    } else {
                        warn!("⚠️ Child {} redirect file points to non-existent directory: {}. Using default.", child_name, redirected_path);
                    }
                }
                Err(e) => {
                    error!("❌ Failed to read redirect file for child {}: {}. Using default directory.", child_name, e);
                }
            }
        } else {
            // debug!("📁 No redirect file found");
        }
        
        // No redirect or redirect failed, use default path
        // debug!("✅ Using default child directory: {}", child_dir.display());
        child_dir
    }
    
    /// Get the file path for a child's transactions using the child name
    pub fn get_transactions_file_path(&self, child_name: &str) -> PathBuf {
        let child_dir = self.get_child_directory(child_name);
        child_dir.join("transactions.csv")
    }
    
    /// Ensure a CSV file exists with proper header for the child using the child name
    pub fn ensure_transactions_file_exists(&self, child_name: &str) -> Result<()> {
        let child_dir = self.get_child_directory(child_name);
        
        // Create the child directory if it doesn't exist
        if !child_dir.exists() {
            fs::create_dir_all(&child_dir)?;
        }
        
        let file_path = child_dir.join("transactions.csv");
        
        if !file_path.exists() {
            // Create the file with CSV header
            let header = "id,child_id,date,description,amount,balance\n";
            fs::write(&file_path, header)?;
        }
        
        Ok(())
    }
    
    /// Get the current data directory path
    pub fn get_current_data_directory(&self) -> PathBuf {
        let base_dir = self.base_directory.lock().unwrap();
        base_dir.clone()
    }
    
    /// Get the base directory path
    pub fn base_directory(&self) -> PathBuf {
        let base_dir = self.base_directory.lock().unwrap();
        base_dir.clone()
    }
    
    /// Relocate a child's data directory to a new location
    pub fn relocate_child_data_directory<P: AsRef<Path>>(&self, child_name: &str, new_path: P) -> Result<String> {
        info!("🔄 Original input path: {:?}", new_path.as_ref().as_os_str());
        
        // Convert to string and unescape common shell escape sequences
        let path_str = new_path.as_ref().to_string_lossy().to_string();
        info!("🔍 Path as string: {}", path_str);
        
        // Unescape shell escape sequences
        let unescaped_path = path_str
            .replace("\\ ", " ")              // Escaped spaces
            .replace("\\~", "~")              // Escaped tildes
            .replace("\\'", "'")              // Escaped single quotes
            .replace("\\\"", "\"")            // Escaped double quotes
            .replace("\\\\", "\\");           // Escaped backslashes (do this last)
        
        info!("🔍 Unescaped path: {}", unescaped_path);
        let new_path = PathBuf::from(unescaped_path);
        info!("🔄 Starting child data directory relocation for '{}' to: {}", child_name, new_path.display());
        info!("🔍 Final path components: {:?}", new_path.components().collect::<Vec<_>>());
        
        let current_child_dir = {
            let base_dir = self.base_directory.lock().unwrap();
            base_dir.join(child_name)
        };
        info!("🔍 Current child dir (base + name): {}", current_child_dir.display());
        
        // Check if it's the same path (no-op)
        info!("🔍 About to call get_child_directory (lock released)...");
        let current_actual_dir = self.get_child_directory(child_name);
        info!("🔍 Current actual dir (resolved): {}", current_actual_dir.display());
        info!("🔍 Comparing paths: new={:?} vs current={:?}", new_path, current_actual_dir);
        
        if new_path == current_actual_dir {
            info!("✅ Paths are the same - no operation needed");
            return Ok(format!("Child '{}' data directory is already at the specified location", child_name));
        }
        
        // Validate that source directory exists and has data
        info!("🔍 Checking if source directory exists: {}", current_actual_dir.display());
        if !current_actual_dir.exists() {
            let error_msg = format!("Child '{}' directory does not exist: {}", child_name, current_actual_dir.display());
            info!("❌ {}", error_msg);
            return Err(anyhow::anyhow!(error_msg));
        }
        info!("✅ Source directory exists");
        
        // Create parent directories for the target path if needed
        if let Some(parent) = new_path.parent() {
            info!("🔍 Checking parent directory: {}", parent.display());
            if !parent.exists() {
                info!("🔨 Creating parent directories...");
                match fs::create_dir_all(parent) {
                    Ok(_) => info!("✅ Created parent directories: {}", parent.display()),
                    Err(e) => {
                        let error_msg = format!("Failed to create parent directories: {}", e);
                        info!("❌ {}", error_msg);
                        return Err(anyhow::anyhow!(error_msg));
                    }
                }
            } else {
                info!("✅ Parent directories already exist");
            }
        }
        
        // Validate target directory - it should not exist or be empty
        info!("🔍 Checking if target directory exists: {}", new_path.display());
        if new_path.exists() {
            info!("📁 Target directory exists, checking if empty...");
            let entries: Result<Vec<_>, _> = fs::read_dir(&new_path)?.collect();
            match entries {
                Ok(entries) => {
                    if !entries.is_empty() {
                        let error_msg = format!("Target directory is not empty: {}", new_path.display());
                        info!("❌ {}", error_msg);
                        return Err(anyhow::anyhow!(error_msg));
                    }
                    info!("✅ Target directory is empty");
                }
                Err(e) => {
                    let error_msg = format!("Failed to read target directory: {}", e);
                    info!("❌ {}", error_msg);
                    return Err(anyhow::anyhow!(error_msg));
                }
            }
        } else {
            info!("✅ Target directory does not exist (will be created)");
        }
        
        // Copy child directory contents to new location
        info!("🔄 Starting copy operation from {} to {}", current_actual_dir.display(), new_path.display());
        match Self::copy_directory_contents(&current_actual_dir, &new_path) {
            Ok(_) => info!("✅ Successfully copied child '{}' data to new location", child_name),
            Err(e) => {
                let error_msg = format!("Failed to copy directory contents: {}", e);
                info!("❌ {}", error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        }
        
        // Verify the copy was successful
        info!("🔍 Verifying copy was successful...");
        let key_files = ["child.yaml", "transactions.csv"];
        for file in &key_files {
            let source_file = current_actual_dir.join(file);
            let dest_file = new_path.join(file);
            info!("🔍 Checking file: {}", file);
            
            if source_file.exists() {
                info!("  📄 Source file exists: {}", source_file.display());
                if !dest_file.exists() {
                    let error_msg = format!("File '{}' was not copied successfully - missing at destination", file);
                    info!("❌ {}", error_msg);
                    return Err(anyhow::anyhow!(error_msg));
                }
                info!("  ✅ Destination file exists: {}", dest_file.display());
            } else {
                info!("  ⚠️ Source file does not exist: {}", source_file.display());
            }
        }
        info!("✅ All key files verified successfully");
        
        // If this was the first move (no redirect file yet), we need to handle the original directory
        let redirect_file = current_child_dir.join(".allowance_redirect");
        if !redirect_file.exists() {
            // This is the first move - preserve .git directory and create redirect file
            
            // Remove all files and directories except .git
            for entry in fs::read_dir(&current_child_dir)? {
                let entry = entry?;
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default();
                
                if file_name != ".git" {
                    if path.is_dir() {
                        fs::remove_dir_all(&path)?;
                    } else {
                        fs::remove_file(&path)?;
                    }
                }
            }
            
            info!("Cleaned original child directory, preserving .git");
        }
        
        // Create/update redirect file in original location
        fs::write(&redirect_file, new_path.to_string_lossy().as_bytes())?;
        info!("Created redirect file for child '{}' pointing to: {}", child_name, new_path.display());
        
        // If there's a .git directory in the original location, commit the redirect file
        let git_dir = current_child_dir.join(".git");
        if git_dir.exists() {
            self.commit_redirect_file(&current_child_dir, child_name)?;
        }
        
        info!("Child '{}' data directory successfully relocated to: {}", child_name, new_path.display());
        Ok(format!("Child '{}' data directory successfully relocated to: {}", child_name, new_path.display()))
    }
    
    /// Commit the redirect file to git for recovery purposes
    fn commit_redirect_file(&self, child_dir: &Path, child_name: &str) -> Result<()> {
        use std::process::Command;
        
        // Add the redirect file to git
        let output = Command::new("git")
            .current_dir(child_dir)
            .args(&["add", ".allowance_redirect"])
            .output();
            
        match output {
            Ok(output) if output.status.success() => {
                info!("Added redirect file to git for child '{}'", child_name);
            }
            Ok(output) => {
                warn!("Git add failed for child '{}': {}", child_name, String::from_utf8_lossy(&output.stderr));
                return Ok(()); // Don't fail the whole operation
            }
            Err(e) => {
                warn!("Failed to run git add for child '{}': {}", child_name, e);
                return Ok(()); // Don't fail the whole operation
            }
        }
        
        // Commit the redirect file
        let commit_message = format!("Data directory relocated for {}", child_name);
        let output = Command::new("git")
            .current_dir(child_dir)
            .args(&["commit", "-m", &commit_message])
            .output();
            
        match output {
            Ok(output) if output.status.success() => {
                info!("Committed redirect file to git for child '{}'", child_name);
            }
            Ok(output) => {
                warn!("Git commit failed for child '{}': {}", child_name, String::from_utf8_lossy(&output.stderr));
            }
            Err(e) => {
                warn!("Failed to run git commit for child '{}': {}", child_name, e);
            }
        }
        
        Ok(())
    }

    /// Commit the removal of redirect file to git
    fn commit_redirect_removal(&self, child_dir: &Path, child_name: &str) -> Result<()> {
        use std::process::Command;
        
        // Add all changes (removal of redirect file, addition of reverted files)
        let output = Command::new("git")
            .current_dir(child_dir)
            .args(&["add", "-A"])
            .output();
            
        match output {
            Ok(output) if output.status.success() => {
                info!("Added changes to git for child '{}'", child_name);
            }
            Ok(output) => {
                warn!("Git add failed for child '{}': {}", child_name, String::from_utf8_lossy(&output.stderr));
                return Ok(()); // Don't fail the whole operation
            }
            Err(e) => {
                warn!("Failed to run git add for child '{}': {}", child_name, e);
                return Ok(()); // Don't fail the whole operation
            }
        }
        
        // Commit the revert
        let commit_message = format!("Data directory reverted to default for {}", child_name);
        let output = Command::new("git")
            .current_dir(child_dir)
            .args(&["commit", "-m", &commit_message])
            .output();
            
        match output {
            Ok(output) if output.status.success() => {
                info!("Committed data directory revert to git for child '{}'", child_name);
            }
            Ok(output) => {
                warn!("Git commit failed for child '{}': {}", child_name, String::from_utf8_lossy(&output.stderr));
            }
            Err(e) => {
                warn!("Failed to run git commit for child '{}': {}", child_name, e);
            }
        }
        
        Ok(())
    }

    /// Copy directory contents recursively, preserving structure and metadata
    fn copy_directory_contents(source: &Path, destination: &Path) -> Result<()> {
        info!("📂 copy_directory_contents: {} → {}", source.display(), destination.display());
        
        // Create destination directory if it doesn't exist
        if !destination.exists() {
            info!("🔨 Creating destination directory: {}", destination.display());
            match fs::create_dir_all(destination) {
                Ok(_) => info!("✅ Created destination directory"),
                Err(e) => {
                    let error_msg = format!("Failed to create destination directory: {}", e);
                    info!("❌ {}", error_msg);
                    return Err(anyhow::anyhow!(error_msg));
                }
            }
        } else {
            info!("✅ Destination directory already exists");
        }

        // Read source directory
        info!("🔍 Reading source directory entries...");
        let entries = match fs::read_dir(source) {
            Ok(entries) => entries,
            Err(e) => {
                let error_msg = format!("Failed to read source directory: {}", e);
                info!("❌ {}", error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        let mut file_count = 0;
        let mut dir_count = 0;
        
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    let error_msg = format!("Failed to read directory entry: {}", e);
                    info!("❌ {}", error_msg);
                    return Err(anyhow::anyhow!(error_msg));
                }
            };
            
            let source_path = entry.path();
            let file_name = entry.file_name();
            let dest_path = destination.join(&file_name);
            
            info!("📁 Processing: {} → {}", source_path.display(), dest_path.display());

            if source_path.is_dir() {
                // Recursively copy subdirectories
                info!("📂 Copying subdirectory...");
                Self::copy_directory_contents(&source_path, &dest_path)?;
                dir_count += 1;
            } else {
                // Copy individual files
                info!("📄 Copying file...");
                match fs::copy(&source_path, &dest_path) {
                    Ok(bytes) => {
                        info!("✅ Copied {} bytes", bytes);
                        file_count += 1;
                    },
                    Err(e) => {
                        // Check if this is a permission error in a git objects directory
                        let is_git_object = source_path.ancestors()
                            .any(|p| p.file_name().map_or(false, |name| name == "objects"));
                        
                        if is_git_object && dest_path.exists() && e.kind() == io::ErrorKind::PermissionDenied {
                            // Git objects are content-addressed, so if the file already exists 
                            // with the same name, it should be identical. Skip with a warning.
                            info!("⚠️ Skipping git object (permission denied, file already exists): {}", dest_path.display());
                            file_count += 1; // Count as successful since we're intentionally skipping
                        } else {
                            let error_msg = format!("Failed to copy file {}: {}", source_path.display(), e);
                            info!("❌ {}", error_msg);
                            return Err(anyhow::anyhow!(error_msg));
                        }
                    }
                }
            }
        }

        info!("✅ Copy completed: {} files, {} directories", file_count, dir_count);
        Ok(())
    }
    
    /// Get the default data directory path
    fn get_default_data_directory() -> Result<PathBuf> {
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
        
        let documents_dir = PathBuf::from(home_dir).join("Documents");
        Ok(documents_dir.join("Allowance Tracker"))
    }
    
    /// Clean up test data (useful for tests)
    #[cfg(test)]
    pub fn cleanup(&self) -> Result<()> {
        let base_dir = self.base_directory.lock().unwrap();
        if base_dir.exists() {
            fs::remove_dir_all(&*base_dir)?;
        }
        Ok(())
    }
    
    /// Test helper to create a test directory structure with sample files
    #[cfg(test)]
    pub fn create_test_child_data(&self, child_name: &str) -> Result<()> {
        use std::fs;
        
        let child_dir = self.get_child_directory(child_name);
        fs::create_dir_all(&child_dir)?;
        
        // Create child.yaml
        let child_yaml = child_dir.join("child.yaml");
        fs::write(&child_yaml, format!("name: {}\nage: 10\nbirthdate: '2014-01-01'\n", child_name))?;
        
        // Create transactions.csv with header
        let transactions_csv = child_dir.join("transactions.csv");
        fs::write(&transactions_csv, "id,date,amount,description,balance\n")?;
        
        // Create goals.csv with header  
        let goals_csv = child_dir.join("goals.csv");
        fs::write(&goals_csv, "id,name,target_amount,target_date,current_amount\n")?;
        
        Ok(())
    }
    
    /// Test helper to verify that the expected files exist in a directory
    #[cfg(test)]
    pub fn verify_child_data_exists(&self, path: &Path) -> Result<()> {
        let expected_files = ["child.yaml", "transactions.csv", "goals.csv"];
        
        for file_name in &expected_files {
            let file_path = path.join(file_name);
            if !file_path.exists() {
                return Err(anyhow::anyhow!("Expected file '{}' does not exist at path: {}", file_name, file_path.display()));
            }
        }
        
        Ok(())
    }

    /// Revert a child's data directory back to the default location
    pub fn revert_child_data_directory(&self, child_name: &str) -> Result<String> {
        info!("Starting child data directory revert for '{}'", child_name);
        
        let base_dir = self.base_directory.lock().unwrap();
        let default_child_dir = base_dir.join(child_name);
        let redirect_file = default_child_dir.join(".allowance_redirect");
        
        // Check if there's a redirect file
        if !redirect_file.exists() {
            return Ok(format!("Child '{}' data directory is already at the default location", child_name));
        }
        
        // Read the redirect location
        let redirected_path = match fs::read_to_string(&redirect_file) {
            Ok(path) => PathBuf::from(path.trim()),
            Err(e) => return Err(anyhow::anyhow!("Failed to read redirect file: {}", e)),
        };
        
        info!("Found redirect pointing to: {}", redirected_path.display());
        
        // Validate that redirected directory exists
        if !redirected_path.exists() {
            return Err(anyhow::anyhow!("Redirected directory does not exist: {}", redirected_path.display()));
        }
        
        // Remove all files and directories in default location except .git and .allowance_redirect
        for entry in fs::read_dir(&default_child_dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default();
            
            if file_name != ".git" && file_name != ".allowance_redirect" {
                if path.is_dir() {
                    fs::remove_dir_all(&path)?;
                    info!("Removed directory: {}", path.display());
                } else {
                    fs::remove_file(&path)?;
                    info!("Removed file: {}", path.display());
                }
            }
        }
        
        // Copy data from redirected location back to default location
        Self::copy_directory_contents(&redirected_path, &default_child_dir)?;
        info!("Successfully copied child '{}' data back to default location", child_name);
        
        // Verify the copy was successful
        let key_files = ["child.yaml", "transactions.csv"];
        for file in &key_files {
            if redirected_path.join(file).exists() && !default_child_dir.join(file).exists() {
                return Err(anyhow::anyhow!("File '{}' was not copied successfully during revert", file));
            }
        }
        
        // Remove the redirect file
        fs::remove_file(&redirect_file)?;
        info!("Removed redirect file for child '{}'", child_name);
        
        // If there's a .git directory, commit the removal of redirect file
        let git_dir = default_child_dir.join(".git");
        if git_dir.exists() {
            self.commit_redirect_removal(&default_child_dir, child_name)?;
        }
        
        info!("Child '{}' data directory successfully reverted to default location", child_name);
        Ok(format!("Child '{}' data directory successfully reverted to default location: {}", child_name, default_child_dir.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    /// Helper to create a test connection with a temporary directory
    async fn create_test_connection() -> Result<(CsvConnection, TempDir)> {
        let temp_dir = TempDir::new()?;
        let connection = CsvConnection::new(temp_dir.path())?;
        Ok((connection, temp_dir))
    }
    
    /// Helper to create a destination directory with complex path (including spaces and special chars)
    fn create_test_destination_with_complex_path(base_dir: &Path) -> Result<PathBuf> {
        // Create a path with spaces and special characters similar to real-world scenarios
        let complex_path = base_dir.join("Test Documents").join("com~apple~CloudDocs").join("Kids Data").join("Child's Money");
        fs::create_dir_all(&complex_path)?;
        Ok(complex_path)
    }
    
    #[tokio::test]
    async fn test_path_unescaping_functionality() -> Result<()> {
        // Test the path unescaping logic that's built into relocate_child_data_directory
        let (_connection, temp_dir) = create_test_connection().await?;
        
        // Create a test file to verify the unescaping works
        let escaped_input = format!("{}{}Documents{}com\\~apple\\~CloudDocs{}Kids{}Child\\'s\\ Money", 
            temp_dir.path().to_string_lossy(),
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR, 
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        );
        
        // The expected unescaped path
        let expected_path = temp_dir.path()
            .join("Documents")
            .join("com~apple~CloudDocs") 
            .join("Kids")
            .join("Child's Money");
        
        // Create the expected directory structure
        fs::create_dir_all(&expected_path)?;
        
        // Test that the path unescaping works by testing the components
        let test_path = PathBuf::from(&escaped_input);
        let path_str = test_path.to_string_lossy().to_string();
        
        // Apply the same unescaping logic as in relocate_child_data_directory
        let unescaped_path_str = path_str
            .replace("\\ ", " ")      // Unescape spaces
            .replace("\\~", "~")      // Unescape tildes  
            .replace("\\'", "'")      // Unescape apostrophes
            .replace("\\\"", "\"");   // Unescape quotes
        
        let unescaped_path = PathBuf::from(unescaped_path_str);
        
        // Verify the path components are correct
        let components: Vec<_> = unescaped_path.components()
            .filter_map(|c| match c {
                std::path::Component::Normal(name) => Some(name.to_string_lossy().to_string()),
                _ => None,
            })
            .collect();
        
        assert!(components.contains(&"Documents".to_string()));
        assert!(components.contains(&"com~apple~CloudDocs".to_string()));
        assert!(components.contains(&"Kids".to_string()));
        assert!(components.contains(&"Child's Money".to_string()));
        
        println!("✅ Path unescaping test passed");
        Ok(())
    }
    
    #[tokio::test] 
    async fn test_relocate_child_data_directory_with_escaped_path() -> Result<()> {
        let (connection, temp_dir) = create_test_connection().await?;
        let child_name = "test_child";
        
        // Create test child data
        connection.create_test_child_data(child_name)?;
        let original_dir = connection.get_child_directory(child_name);
        
        // Verify original data exists
        connection.verify_child_data_exists(&original_dir)?;
        
        // Create destination with escaped path (simulating user input)
        let dest_base = create_test_destination_with_complex_path(temp_dir.path())?;
        let escaped_dest_path = format!("{}{}Test\\ Documents{}com\\~apple\\~CloudDocs{}Kids\\ Data{}Child\\'s\\ Money", 
            temp_dir.path().to_string_lossy(),
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR, 
            std::path::MAIN_SEPARATOR
        );
        
        // Test the relocation with escaped path
        let result = connection.relocate_child_data_directory(child_name, &escaped_dest_path)?;
        assert!(result.contains("successfully relocated"));
        
        // Verify redirect file was created
        let redirect_file = original_dir.join(".allowance_redirect");
        assert!(redirect_file.exists());
        
        // Verify redirect file points to unescaped path
        let redirect_content = fs::read_to_string(&redirect_file)?;
        let redirect_path = PathBuf::from(redirect_content.trim());
        
        // Should not contain escape characters in the stored path
        let path_str = redirect_path.to_string_lossy();
        assert!(!path_str.contains("\\ "));   // No escaped spaces
        assert!(!path_str.contains("\\~"));   // No escaped tildes  
        assert!(!path_str.contains("\\'"));   // No escaped apostrophes
        
        // Verify data was copied to the unescaped path location
        connection.verify_child_data_exists(&redirect_path)?;
        
        println!("✅ Escaped path relocation test passed");
        Ok(())
    }
    
    #[tokio::test]
    async fn test_revert_after_escaped_path_relocation() -> Result<()> {
        let (connection, temp_dir) = create_test_connection().await?;
        let child_name = "test_child";
        
        // Create test child data
        connection.create_test_child_data(child_name)?;
        let original_dir = connection.get_child_directory(child_name);
        
        // Create destination with escaped path
        let dest_base = create_test_destination_with_complex_path(temp_dir.path())?;
        let escaped_dest_path = format!("{}{}Test\\ Documents{}com\\~apple\\~CloudDocs{}Kids\\ Data{}Child\\'s\\ Money",
            temp_dir.path().to_string_lossy(),
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        );
        
        // Relocate first
        connection.relocate_child_data_directory(child_name, &escaped_dest_path)?;
        
        // Now test revert
        let revert_result = connection.revert_child_data_directory(child_name)?;
        assert!(revert_result.contains("successfully reverted"));
        
        // Verify redirect file was removed
        let redirect_file = original_dir.join(".allowance_redirect");
        assert!(!redirect_file.exists());
        
        // Verify data is back in original location
        connection.verify_child_data_exists(&original_dir)?;
        
        println!("✅ Revert after escaped path relocation test passed");
        Ok(())
    }
    
    #[tokio::test]
    async fn test_complex_escaped_path_like_real_scenario() -> Result<()> {
        let (connection, temp_dir) = create_test_connection().await?;
        let child_name = "test_child";
        
        // Create test child data
        connection.create_test_child_data(child_name)?;
        let original_dir = connection.get_child_directory(child_name);
        
        // Create the exact path structure from the real scenario
        let mobile_docs = temp_dir.path().join("Library").join("Mobile Documents");
        let icloud_path = mobile_docs.join("com~apple~CloudDocs"); 
        let hart_root = icloud_path.join("HartRoot").join("Kids").join("Keiko");
        let final_dest = hart_root.join("Keiko's money");
        fs::create_dir_all(&final_dest)?;
        
        // Create the escaped version exactly as it appeared in the logs
        let escaped_path = format!("{}{}Library{}Mobile\\ Documents{}com\\~apple\\~CloudDocs{}HartRoot{}Kids{}Keiko{}Keiko\\'s\\ money",
            temp_dir.path().to_string_lossy(),
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR,
            std::path::MAIN_SEPARATOR
        );
        
        // Test relocation with the real-world escaped path
        let result = connection.relocate_child_data_directory(child_name, &escaped_path)?;
        assert!(result.contains("successfully relocated"));
        
        // Verify files were copied to the correct unescaped location
        connection.verify_child_data_exists(&final_dest)?;
        
        // Verify redirect file contains unescaped path
        let redirect_file = original_dir.join(".allowance_redirect");
        let redirect_content = fs::read_to_string(&redirect_file)?;
        let stored_path = redirect_content.trim(); 
        
        // Should not contain any escape characters
        assert!(!stored_path.contains("\\"));
        assert!(stored_path.contains("Mobile Documents"));  // Space unescaped
        assert!(stored_path.contains("com~apple~CloudDocs")); // Tilde unescaped 
        assert!(stored_path.contains("Keiko's money"));     // Apostrophe unescaped
        
        // Test that subsequent operations work with the redirect
        let current_dir = connection.get_child_directory(child_name);
        assert_eq!(current_dir, final_dest);
        
        // Test revert also works
        let revert_result = connection.revert_child_data_directory(child_name)?;
        assert!(revert_result.contains("successfully reverted"));
        assert!(!redirect_file.exists());
        
        println!("✅ Real-world complex escaped path test passed");
        Ok(())
    }
    
    #[tokio::test]
    async fn test_no_op_when_same_path_with_escapes() -> Result<()> {
        let (connection, temp_dir) = create_test_connection().await?;
        let child_name = "test_child";
        
        // Create test child data
        connection.create_test_child_data(child_name)?;
        let original_dir = connection.get_child_directory(child_name);
        
        // First relocate to a different location
        let dest_dir = temp_dir.path().join("new_location");
        fs::create_dir_all(&dest_dir)?;
        connection.relocate_child_data_directory(child_name, &dest_dir)?;
        
        // Now try to "relocate" to the same location but with escaped path
        let escaped_same_path = dest_dir.to_string_lossy().replace(" ", "\\ ");
        let result = connection.relocate_child_data_directory(child_name, &escaped_same_path)?;
        
        // Should be a no-op
        assert!(result.contains("already at the specified location"));
        
        println!("✅ No-op test with escaped paths passed");
        Ok(())
    }
    
    #[tokio::test]
    async fn test_error_handling_with_invalid_escaped_path() -> Result<()> {
        let (connection, _temp_dir) = create_test_connection().await?;
        let child_name = "test_child";
        
        // Create test child data
        connection.create_test_child_data(child_name)?;
        
        // Try to relocate to a path that doesn't exist and can't be created (invalid)
        let invalid_escaped_path = "/invalid\\root/path\\ that/cannot\\ exist/Keiko\\'s\\ money";
        
        let result = connection.relocate_child_data_directory(child_name, invalid_escaped_path);
        assert!(result.is_err());
        
        // Verify original data is still intact
        let original_dir = connection.get_child_directory(child_name);
        connection.verify_child_data_exists(&original_dir)?;
        
        println!("✅ Error handling test with invalid escaped paths passed");
        Ok(())
    }
}

impl Connection for CsvConnection {
    type TransactionRepository = super::transaction_repository::TransactionRepository;
    
    fn create_transaction_repository(&self) -> Self::TransactionRepository {
        super::transaction_repository::TransactionRepository::new(self.clone())
    }
} 