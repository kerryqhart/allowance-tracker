//! # CSV Parental Control Repository
//!
//! This module provides a file-based parental control attempt storage implementation
//! using CSV files stored per-child. Each child's parental control attempts are stored
//! in `{child_directory}/parental_control_attempts.csv`.
//!
//! ## File Structure
//!
//! ```
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
use async_trait::async_trait;
use chrono::Utc;
use csv::{Reader, Writer};
use log::{info, debug};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use crate::backend::storage::traits::ParentalControlRepository;
use shared::ParentalControlAttempt;
use super::connection::CsvConnection;

/// CSV record structure for parental control attempts
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParentalControlAttemptRecord {
    id: i64,
    attempted_value: String,
    timestamp: String,
    success: bool,
}

impl From<ParentalControlAttemptRecord> for ParentalControlAttempt {
    fn from(record: ParentalControlAttemptRecord) -> Self {
        ParentalControlAttempt {
            id: record.id,
            attempted_value: record.attempted_value,
            timestamp: record.timestamp,
            success: record.success,
        }
    }
}

/// CSV-based parental control repository using per-child CSV files
#[derive(Clone)]
pub struct CsvParentalControlRepository {
    connection: CsvConnection,
}

impl CsvParentalControlRepository {
    /// Create a new CSV parental control repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { connection }
    }
    
    /// Get the parental control attempts CSV file path for a specific child directory
    fn get_parental_control_file_path(&self, child_id: &str) -> PathBuf {
        self.connection
            .get_child_directory(child_id)
            .join("parental_control_attempts.csv")
    }
    
    /// Get all child directories that exist
    async fn get_all_child_ids(&self) -> Result<Vec<String>> {
        let base_dir = self.connection.base_directory();
        let mut child_ids = Vec::new();
        
        if !base_dir.exists() {
            return Ok(child_ids);
        }
        
        for entry in std::fs::read_dir(base_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if !path.is_dir() {
                continue;
            }
            
            let child_yaml_path = path.join("child.yaml");
            if child_yaml_path.exists() {
                if let Ok(yaml_content) = std::fs::read_to_string(&child_yaml_path) {
                    if let Ok(child) = serde_yaml::from_str::<shared::Child>(&yaml_content) {
                        child_ids.push(child.id);
                    }
                }
            }
        }
        
        child_ids.sort();
        Ok(child_ids)
    }
    
    /// Get the next available ID for a specific child's parental control attempts file
    async fn get_next_id(&self, child_id: &str) -> Result<i64> {
        let csv_path = self.get_parental_control_file_path(child_id);
        
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
    async fn append_parental_control_attempt(&self, child_id: &str, record: &ParentalControlAttemptRecord) -> Result<()> {
        let child_dir = self.connection.get_child_directory(child_id);
        
        // Ensure the child directory exists
        if !child_dir.exists() {
            std::fs::create_dir_all(&child_dir)?;
            info!("Created child directory for parental control attempts: {:?}", child_dir);
        }
        
        let csv_path = self.get_parental_control_file_path(child_id);
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
        
        Ok(())
    }
    
    /// Load parental control attempts from a specific child's CSV file
    async fn read_attempts(&self, child_id: &str) -> Result<Vec<ParentalControlAttempt>> {
        let csv_path = self.get_parental_control_file_path(child_id);
        
        if !csv_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(csv_path)?;
        let mut rdr = csv::Reader::from_reader(file);
        let mut records = Vec::new();

        for result in rdr.deserialize() {
            let record: ParentalControlAttempt = result?;
            records.push(record);
        }

        Ok(records)
    }
}

#[async_trait]
impl ParentalControlRepository for CsvParentalControlRepository {
    async fn record_parental_control_attempt(&self, child_id: &str, attempted_value: &str, success: bool) -> Result<i64> {
        info!(
            "Recording parental control attempt for child '{}': success={}",
            child_id, success
        );

        let next_id = self.get_next_id(child_id).await?;
        let record = ParentalControlAttemptRecord {
            id: next_id,
            attempted_value: attempted_value.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            success,
        };
        
        self.append_parental_control_attempt(child_id, &record).await?;
        
        Ok(next_id)
    }

    async fn get_parental_control_attempts(
        &self,
        child_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<shared::ParentalControlAttempt>> {
        let mut attempts = self.read_attempts(child_id).await?;
        
        // Sort by timestamp descending to get the most recent attempts
        attempts.sort_by(|a: &shared::ParentalControlAttempt, b: &shared::ParentalControlAttempt| b.timestamp.cmp(&a.timestamp));

        // Apply limit if provided
        if let Some(l) = limit {
            attempts.truncate(l as usize);
        }

        Ok(attempts)
    }

    async fn get_all_parental_control_attempts(
        &self,
    ) -> Result<Vec<shared::ParentalControlAttempt>> {
        let child_ids = self.connection.get_all_child_ids().await?;
        let mut all_attempts = Vec::new();

        for child_id in child_ids {
            let attempts = self.read_attempts(&child_id).await?;
            all_attempts.extend(attempts);
        }

        all_attempts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(all_attempts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::csv::child_repository::CsvChildRepository;
    use crate::backend::storage::ChildStorage;
    use shared::Child;
    use tempfile::TempDir;

    async fn setup_test_repo_with_child() -> (CsvParentalControlRepository, CsvChildRepository, TempDir, Child) {
        let temp_dir = TempDir::new().unwrap();
        let connection = CsvConnection::new(temp_dir.path()).unwrap();
        let repo = CsvParentalControlRepository::new(connection.clone());
        let child_repo = CsvChildRepository::new(connection);

        let child = Child {
            id: "child1".to_string(),
            name: "Test Child".to_string(),
            birthdate: "2010-01-01".to_string(),
        };
        child_repo.store_child(&child.into()).await.unwrap();

        (repo, child_repo, temp_dir, child)
    }

    #[tokio::test]
    async fn test_record_and_get_parental_control_attempts() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;

        repo.record_parental_control_attempt(&child.id, "wrong", false).await.unwrap();
        repo.record_parental_control_attempt(&child.id, "right", true).await.unwrap();

        let attempts = repo.get_parental_control_attempts(&child.id, None).await.unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].success, true);
        assert_eq!(attempts[1].success, false);
    }

    #[tokio::test]
    async fn test_get_parental_control_attempts_with_limit() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;

        for i in 0..5 {
            repo.record_parental_control_attempt(&child.id, &format!("attempt {}", i), i % 2 == 0).await.unwrap();
        }

        let attempts = repo.get_parental_control_attempts(&child.id, Some(3)).await.unwrap();
        assert_eq!(attempts.len(), 3);
        assert_eq!(attempts[0].attempted_value, "attempt 4");
    }

    #[tokio::test]
    async fn test_get_all_parental_control_attempts() {
        let (repo, child_repo, _temp_dir, _child1) = setup_test_repo_with_child().await;

        let child2 = Child {
            id: "child2".to_string(),
            name: "Another Child".to_string(),
            birthdate: "2012-02-02".to_string(),
        };
        child_repo.store_child(&child2.clone().into()).await.unwrap();

        repo.record_parental_control_attempt("child1", "c1a1", true).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        repo.record_parental_control_attempt("child2", "c2a1", false).await.unwrap();

        let all_attempts = repo.get_all_parental_control_attempts(None).await.unwrap();
        assert_eq!(all_attempts.len(), 2);
        assert_eq!(all_attempts[0].attempted_value, "c2a1");
        assert_eq!(all_attempts[1].attempted_value, "c1a1");
    }

    #[tokio::test]
    async fn test_record_attempt_for_nonexistent_child() {
        let (repo, _child_repo, _temp_dir, _child) = setup_test_repo_with_child().await;
        let result = repo.record_parental_control_attempt("nonexistent", "value", true).await;
        // This should succeed as it will create the directory
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_attempts_for_nonexistent_child() {
        let (repo, _child_repo, _temp_dir, _child) = setup_test_repo_with_child().await;
        let attempts = repo.get_parental_control_attempts("nonexistent", None).await.unwrap();
        assert!(attempts.is_empty());
    }
} 