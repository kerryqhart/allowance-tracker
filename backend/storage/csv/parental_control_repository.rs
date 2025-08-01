//! # CSV Parental Control Repository
//!
//! This module provides a file-based parental control attempt storage implementation
//! using CSV files stored per-child. Each child's parental control attempts are stored
//! in `{child_directory}/parental_control_attempts.csv`.
//!
//! ## File Structure
//!
//! ```text
//! data/
//! ├── global_config.yaml
//! └── {child_name}/
//!     ├── child.yaml
//!     ├── allowance_config.yaml
//!     ├── parental_control_attempts.csv    ← This module manages these files
//!     └── transactions.csv
//! ```
//!
//! ## CSV Format
//!
//! CSV files have the following structure:
//! ```csv
//! id,attempted_value,timestamp,success
//! 1,"wrong_answer","2024-01-15T10:30:00Z",false
//! 2,"correct_answer","2024-01-15T10:31:00Z",true
//! ```
//!
//! ## Features
//!
//! - Per-child CSV files for parental control attempts
//! - Atomic file writes with temp files
//! - Auto-incrementing ID generation
//! - Chronological ordering (most recent first)

use anyhow::Result;


use csv::{Reader, Writer};
use log::{info, debug};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use crate::backend::domain::models::parental_control_attempt::ParentalControlAttempt as DomainParentalControlAttempt;
use super::connection::CsvConnection;
use crate::backend::storage::GitManager;

/// CSV record structure for parental control attempts
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParentalControlAttemptRecord {
    id: i64,
    attempted_value: String,
    timestamp: String,
    success: bool,
}

impl From<ParentalControlAttemptRecord> for DomainParentalControlAttempt {
    fn from(record: ParentalControlAttemptRecord) -> Self {
        DomainParentalControlAttempt {
            id: record.id,
            attempted_value: record.attempted_value,
            timestamp: record.timestamp,
            success: record.success,
        }
    }
}

/// CSV-based parental control repository using per-child CSV files
#[derive(Clone)]
pub struct ParentalControlRepository {
    #[allow(dead_code)]
    connection: CsvConnection,
    #[allow(dead_code)]
    git_manager: GitManager,
}

impl ParentalControlRepository {
    /// Create a new CSV parental control repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { 
            connection,
            git_manager: GitManager::new(),
        }
    }
    
    /// Get the parental control attempts CSV file path for a specific child directory
    fn get_parental_control_file_path(&self, child_directory: &str) -> PathBuf {
        if child_directory == "global" {
            // For global parental control attempts, store at root level
            self.connection
                .base_directory()
                .join("parental_control_attempts.csv")
        } else {
            // For child-specific attempts, store in child directory
            self.connection
                .get_child_directory(child_directory)
                .join("parental_control_attempts.csv")
        }
    }
    
    // NOTE: find_child_directory_by_id method removed - now using centralized version in CsvConnection
    
    /// Get all child directories that exist
    #[allow(dead_code)]
    fn get_all_child_directories(&self) -> Result<Vec<String>> {
        let base_dir = self.connection.base_directory();
        let mut directories = Vec::new();
        
        if !base_dir.exists() {
            return Ok(directories);
        }
        
        for entry in std::fs::read_dir(base_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if !path.is_dir() {
                continue;
            }
            
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };
            
            // Only include directories that have a child.yaml file
            let child_yaml_path = path.join("child.yaml");
            if child_yaml_path.exists() {
                directories.push(dir_name.to_string());
            }
        }
        
        directories.sort();
        Ok(directories)
    }
    
    /// Get the next available ID for a specific child's parental control attempts file
    #[allow(dead_code)]
    fn get_next_id(&self, child_directory: &str) -> Result<i64> {
        let csv_path = self.get_parental_control_file_path(child_directory);
        
        if !csv_path.exists() {
            return Ok(1); // First ID
        }
        
        let file = File::open(&csv_path)?;
        let reader = BufReader::new(file);
        let mut csv_reader = Reader::from_reader(reader);
        
        let mut max_id = 0i64;
        for result in csv_reader.records() {
            let record = result?;
            if record.len() >= 1 {
                if let Ok(id) = record[0].parse::<i64>() {
                    if id > max_id {
                        max_id = id;
                    }
                }
            }
        }
        
        Ok(max_id + 1)
    }
    
    /// Append a parental control attempt to a specific child's CSV file
    #[allow(dead_code)]
    fn append_parental_control_attempt(&self, child_directory: &str, record: &ParentalControlAttemptRecord) -> Result<()> {
        let child_dir = self.connection.get_child_directory(child_directory);
        
        // Ensure the child directory exists
        if !child_dir.exists() {
            std::fs::create_dir_all(&child_dir)?;
            info!("Created child directory for parental control attempts: {:?}", child_dir);
        }
        
        let csv_path = self.get_parental_control_file_path(child_directory);
        let file_exists = csv_path.exists();
        
        // Open file in append mode
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&csv_path)?;
        
        let writer = BufWriter::new(file);
        let mut csv_writer = Writer::from_writer(writer);
        
        // Write header if file is new
        if !file_exists {
            csv_writer.write_record(&["id", "attempted_value", "timestamp", "success"])?;
        }
        
        // Write the record - manually write to handle quoting properly
        csv_writer.write_record(&[
            &record.id.to_string(),
            &record.attempted_value,
            &record.timestamp,
            &record.success.to_string(),
        ])?;
        csv_writer.flush()?;
        
        debug!("Appended parental control attempt to {:?}: ID {}", csv_path, record.id);
        
        // Git integration: commit the parental_control_attempts.csv change
        let action_description = format!("Added parental control attempt (success: {})", record.success);
        
        // This is non-blocking - git errors won't fail the parental control operation
        let _ = self.git_manager.commit_file_change(
            &child_dir,
            "parental_control_attempts.csv", 
            &action_description
        );
        
        Ok(())
    }
    
    /// Load parental control attempts from a specific child's CSV file
    #[allow(dead_code)]
    fn load_parental_control_attempts_from_directory(&self, child_directory: &str, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>> {
        let csv_path = self.get_parental_control_file_path(child_directory);
        
        if !csv_path.exists() {
            debug!("No parental control attempts file found in directory '{}'", child_directory);
            return Ok(Vec::new());
        }
        
        let file = File::open(&csv_path)?;
        let reader = BufReader::new(file);
        let mut csv_reader = Reader::from_reader(reader);
        
        let mut attempts = Vec::new();
        for result in csv_reader.records() {
            let record = result?;
            if record.len() >= 4 {
                let attempt = DomainParentalControlAttempt {
                    id: record[0].parse::<i64>()?,
                    attempted_value: record[1].to_string(),
                    timestamp: record[2].to_string(),
                    success: record[3].parse::<bool>()?,
                };
                attempts.push(attempt);
            }
        }
        
        // Sort by ID descending (most recent first, assuming IDs are incremental)
        attempts.sort_by(|a: &DomainParentalControlAttempt, b: &DomainParentalControlAttempt| b.id.cmp(&a.id));
        
        // Apply limit if specified
        if let Some(limit) = limit {
            attempts.truncate(limit as usize);
        }
        
        debug!("Loaded {} parental control attempts from directory '{}'", attempts.len(), child_directory);
        Ok(attempts)
    }
}

impl crate::backend::storage::ParentalControlStorage for ParentalControlRepository {
    fn record_parental_control_attempt(&self, child_id: &str, attempted_value: &str, success: bool) -> Result<i64> {
        // Handle global parental control attempts specially
        let child_directory = if child_id == "global" {
            // For global attempts, use the root directory directly
            "global".to_string()
        } else {
            // Find the child directory for specific child IDs using centralized logic
            match self.connection.find_child_directory_by_id(child_id)? {
                Some(dir) => dir,
                None => return Err(anyhow::anyhow!("Child not found: {}", child_id)),
            }
        };
        
        // Get the next available ID
        let id = self.get_next_id(&child_directory)?;
        
        // Create the record
        let record = ParentalControlAttemptRecord {
            id,
            attempted_value: attempted_value.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            success,
        };
        
        // Append to the CSV file
        self.append_parental_control_attempt(&child_directory, &record)?;
        
        info!("Recorded parental control attempt for child '{}' with ID {}", child_id, id);
        Ok(id)
    }

    fn get_parental_control_attempts(&self, child_id: &str, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>> {
        // Handle global parental control attempts specially
        let child_directory = if child_id == "global" {
            // For global attempts, use the root directory directly
            "global".to_string()
        } else {
            // Find the child directory for specific child IDs using centralized logic
            match self.connection.find_child_directory_by_id(child_id)? {
                Some(dir) => dir,
                None => return Ok(Vec::new()), // Return empty vector if child not found
            }
        };
        
        // Load attempts from the directory
        self.load_parental_control_attempts_from_directory(&child_directory, limit)
    }

    fn get_all_parental_control_attempts(&self, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>> {
        // Get all child directories
        let child_directories = self.get_all_child_directories()?;
        
        let mut all_attempts = Vec::new();
        
        // Load attempts from each child directory
        for child_directory in child_directories {
            let attempts = self.load_parental_control_attempts_from_directory(&child_directory, None)?;
            all_attempts.extend(attempts);
        }
        
        // Sort by timestamp descending (most recent first)
        all_attempts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Apply limit if specified
        if let Some(limit) = limit {
            all_attempts.truncate(limit as usize);
        }
        
        Ok(all_attempts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::backend::domain::models::child::Child as DomainChild;
    use crate::backend::storage::{ChildStorage, ParentalControlStorage};
    use crate::backend::storage::csv::ChildRepository;
    use std::sync::Arc;
    use chrono::Utc;

    fn setup_test_repo_with_child() -> (ParentalControlRepository, ChildRepository, TempDir, DomainChild) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let connection = CsvConnection::new(temp_dir.path()).expect("Failed to create connection");
        let parental_control_repo = ParentalControlRepository::new(connection.clone());
        let child_repo = ChildRepository::new(Arc::new(connection));
        
        // Create a test child first
        let child = DomainChild {
            id: "child::1234567890".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        child_repo.store_child(&child).expect("Failed to create test child");
        
        (parental_control_repo, child_repo, temp_dir, child)
    }

    #[test]
    fn test_record_and_get_parental_control_attempts() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child();
        
        // Record some parental control attempts
        let id1 = repo.record_parental_control_attempt(&child.id, "wrong_answer", false).unwrap();
        let id2 = repo.record_parental_control_attempt(&child.id, "correct_answer", true).unwrap();
        let id3 = repo.record_parental_control_attempt(&child.id, "another_wrong", false).unwrap();
        
        // IDs should be sequential
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
        
        // Get all attempts
        let attempts = repo.get_parental_control_attempts(&child.id, None).unwrap();
        assert_eq!(attempts.len(), 3);
        
        // Should be ordered by ID descending (most recent first)
        assert_eq!(attempts[0].id, 3);
        assert_eq!(attempts[0].attempted_value, "another_wrong");
        assert_eq!(attempts[0].success, false);
        
        assert_eq!(attempts[1].id, 2);
        assert_eq!(attempts[1].attempted_value, "correct_answer");
        assert_eq!(attempts[1].success, true);
        
        assert_eq!(attempts[2].id, 1);
        assert_eq!(attempts[2].attempted_value, "wrong_answer");
        assert_eq!(attempts[2].success, false);
    }

    #[test]
    fn test_get_parental_control_attempts_with_limit() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child();
        
        // Record several attempts
        for i in 1..=5 {
            repo.record_parental_control_attempt(&child.id, &format!("attempt_{}", i), i % 2 == 0).unwrap();
        }
        
        // Get with limit
        let attempts = repo.get_parental_control_attempts(&child.id, Some(2)).unwrap();
        assert_eq!(attempts.len(), 2);
        
        // Should get the most recent 2
        assert_eq!(attempts[0].attempted_value, "attempt_5");
        assert_eq!(attempts[1].attempted_value, "attempt_4");
    }

    #[test]
    fn test_get_all_parental_control_attempts() {
        let (repo, child_repo, _temp_dir, child1) = setup_test_repo_with_child();
        
        // Create a second child
        let child2 = DomainChild {
            id: "child::2345678901".to_string(),
            name: "Second Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2012-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        child_repo.store_child(&child2).unwrap();
        
        // Record attempts for both children
        repo.record_parental_control_attempt(&child1.id, "child1_attempt1", false).unwrap();
        repo.record_parental_control_attempt(&child2.id, "child2_attempt1", true).unwrap();
        repo.record_parental_control_attempt(&child1.id, "child1_attempt2", true).unwrap();
        
        // Get all attempts
        let all_attempts = repo.get_all_parental_control_attempts(None).unwrap();
        assert_eq!(all_attempts.len(), 3);
        
        // Should be ordered by ID descending across all children
        let attempt_values: Vec<&String> = all_attempts.iter().map(|a| &a.attempted_value).collect();
        assert!(attempt_values.contains(&&"child1_attempt1".to_string()));
        assert!(attempt_values.contains(&&"child2_attempt1".to_string()));
        assert!(attempt_values.contains(&&"child1_attempt2".to_string()));
    }

    #[test]
    fn test_record_attempt_for_nonexistent_child() {
        let (repo, _child_repo, _temp_dir, _child) = setup_test_repo_with_child();
        
        // Try to record attempt for non-existent child
        let result = repo.record_parental_control_attempt("child::nonexistent", "test", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Child not found"));
    }

    #[test]
    fn test_get_attempts_for_nonexistent_child() {
        let (repo, _child_repo, _temp_dir, _child) = setup_test_repo_with_child();
        
        // Try to get attempts for non-existent child
        let attempts = repo.get_parental_control_attempts("child::nonexistent", None).unwrap();
        assert!(attempts.is_empty());
    }
} 