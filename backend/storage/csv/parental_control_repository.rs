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


use csv::Writer;
use log::{info, debug, warn};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use shared::ChildId;

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
    connection: CsvConnection,
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
    
    /// Resolve the directory that holds a parental-control attempts file.
    ///
    /// The pseudo-id `global` names the base directory; every other id
    /// resolves through the registry.
    fn attempts_dir(&self, child_id: &str) -> Result<PathBuf> {
        if child_id == "global" {
            Ok(self.connection.base_directory().to_path_buf())
        } else {
            self.connection.child_dir(&ChildId::from(child_id))
        }
    }

    /// Get the next available ID for an attempts file in the given directory
    fn get_next_id(&self, dir: &Path) -> Result<i64> {
        let csv_path = dir.join("parental_control_attempts.csv");

        if !csv_path.exists() {
            return Ok(1); // First ID
        }
        
        let file = File::open(&csv_path)?;
        let reader = BufReader::new(file);
        let mut csv_reader = csv::ReaderBuilder::new().flexible(true).from_reader(reader);

        let mut max_id = 0i64;
        for result in csv_reader.records() {
            // A torn trailing line from an interrupted append must cost that
            // record and nothing else. Propagating here would make the log
            // permanently unwritable, since every future append calls this.
            let Ok(record) = result else { continue };
            // A torn trailing line can still parse as a short, syntactically
            // valid record under a flexible reader (e.g. "99,partial"). Only
            // a full record's first field is trustworthy as an id — anything
            // shorter is leftover from an interrupted append, not a real row.
            if record.len() >= 4 {
                if let Ok(id) = record[0].parse::<i64>() {
                    if id > max_id {
                        max_id = id;
                    }
                }
            }
        }

        Ok(max_id + 1)
    }
    
    /// Whether an existing, non-empty file's last byte is not a newline.
    ///
    /// An interrupted append leaves exactly this: a trailing line with no
    /// terminator. Appending straight onto that would glue the next record
    /// onto the torn bytes instead of starting a new line, corrupting the
    /// new write too — so this is checked before every append.
    fn file_missing_trailing_newline(path: &Path) -> Result<bool> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Ok(false);
        }
        file.seek(SeekFrom::End(-1))?;
        let mut last_byte = [0u8; 1];
        file.read_exact(&mut last_byte)?;
        Ok(last_byte[0] != b'\n')
    }

    /// Append a parental control attempt to an already-resolved directory.
    ///
    /// No `create_dir_all`: the directory came from `attempts_dir`, which
    /// resolves through the registry and has already proven it is there.
    fn append_parental_control_attempt(&self, dir: &Path, record: &ParentalControlAttemptRecord) -> Result<()> {
        let csv_path = dir.join("parental_control_attempts.csv");
        let file_exists = csv_path.exists();

        // Restore the line boundary a torn trailing line left missing. This
        // is still an append — one newline byte at EOF — never a rewrite.
        if file_exists && Self::file_missing_trailing_newline(&csv_path)? {
            let mut newline_fixup = OpenOptions::new().append(true).open(&csv_path)?;
            newline_fixup.write_all(b"\n")?;
        }

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
        if let Err(e) = self.git_manager.commit_file_change(
            dir,
            "parental_control_attempts.csv",
            &action_description
        ) {
            // Deliberately non-fatal: the data is already on disk, and the
            // sync guard commits a tracked file left dirty on its next
            // cycle — unless that cycle is refused outright, which happens
            // only when transactions.csv is found emptied (see
            // `DirtyTreeError::WouldEmptyLedger`). Logged rather than
            // discarded so this is visible.
            warn!("git commit for parental_control_attempts.csv did not complete: {e}");
        }

        Ok(())
    }

    /// Load parental control attempts from an already-resolved directory
    fn load_parental_control_attempts_from_directory(&self, dir: &Path, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>> {
        let csv_path = dir.join("parental_control_attempts.csv");

        if !csv_path.exists() {
            debug!("No parental control attempts file found in {:?}", dir);
            return Ok(Vec::new());
        }

        let file = File::open(&csv_path)?;
        let reader = BufReader::new(file);
        let mut csv_reader = csv::ReaderBuilder::new().flexible(true).from_reader(reader);

        let mut attempts = Vec::new();
        for result in csv_reader.records() {
            // A torn trailing line from an interrupted append must cost that
            // record and nothing else, not the whole file.
            let Ok(record) = result else { continue };
            // A torn write can also land mid-field with the full field
            // count intact (e.g. `success` truncated to "tr"). The count
            // guard above doesn't catch that, so the content parses below
            // must skip rather than propagate too — otherwise a single
            // corrupt-but-complete record still kills the whole read.
            if record.len() >= 4 {
                let Ok(id) = record[0].parse::<i64>() else { continue };
                let Ok(success) = record[3].parse::<bool>() else { continue };
                let attempt = DomainParentalControlAttempt {
                    id,
                    attempted_value: record[1].to_string(),
                    timestamp: record[2].to_string(),
                    success,
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
        
        debug!("Loaded {} parental control attempts from {:?}", attempts.len(), dir);
        Ok(attempts)
    }
}

impl crate::backend::storage::ParentalControlStorage for ParentalControlRepository {
    fn record_parental_control_attempt(&self, child_id: &str, attempted_value: &str, success: bool) -> Result<i64> {
        let dir = self
            .attempts_dir(child_id)
            .map_err(|e| anyhow::anyhow!("Child not found: {} ({})", child_id, e))?;

        // Get the next available ID
        let id = self.get_next_id(&dir)?;

        // Create the record
        let record = ParentalControlAttemptRecord {
            id,
            attempted_value: attempted_value.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            success,
        };

        // Append to the CSV file
        self.append_parental_control_attempt(&dir, &record)?;

        info!("Recorded parental control attempt for child '{}' with ID {}", child_id, id);
        Ok(id)
    }

    fn get_parental_control_attempts(&self, child_id: &str, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>> {
        let dir = match self.attempts_dir(child_id) {
            Ok(dir) => dir,
            // An unresolvable child has no attempts to show.
            Err(_) => return Ok(Vec::new()),
        };

        self.load_parental_control_attempts_from_directory(&dir, limit)
    }

    /// Every registered child's attempts, from the registry rather than a
    /// base-directory scan.
    fn get_all_parental_control_attempts(&self, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>> {
        let registry = self.connection.registry();

        let mut all_attempts = Vec::new();

        for entry in registry.entries() {
            let attempts = self.load_parental_control_attempts_from_directory(&entry.path, None)?;
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

    /// An interrupted append leaves a partial trailing line. That must cost
    /// the trailing record and nothing else — not the whole log, and not the
    /// ability to append ever again.
    #[test]
    fn a_truncated_trailing_line_costs_only_that_record() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child();
        repo.record_parental_control_attempt(&child.id, "1234", false).unwrap();
        repo.record_parental_control_attempt(&child.id, "5678", false).unwrap();

        let dir = repo.attempts_dir(&child.id).unwrap();
        let path = dir.join("parental_control_attempts.csv");

        // Simulate the interrupted append: a trailing line with too few fields.
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("99,partial");
        std::fs::write(&path, text).unwrap();

        let attempts = repo.get_parental_control_attempts(&child.id, None).unwrap();
        assert_eq!(attempts.len(), 2, "every prior record must still be readable");

        // And the log must still be appendable — `get_next_id` is the path
        // that would otherwise be permanently blocked.
        repo.record_parental_control_attempt(&child.id, "0000", true).unwrap();
        let after = repo.get_parental_control_attempts(&child.id, None).unwrap();
        assert_eq!(after.len(), 3, "a torn line must not block future appends");
    }

    /// A crash can also land mid-field with the field *count* intact — e.g.
    /// `success` truncated from "true" to "tr" partway through the write.
    /// The field-count guard doesn't catch this shape at all, since the
    /// record has all four fields; only the content is corrupt. That must
    /// still cost only the trailing record, not the whole read or future
    /// appends.
    #[test]
    fn a_content_corrupted_trailing_line_costs_only_that_record() {
        let (repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child();
        repo.record_parental_control_attempt(&child.id, "1234", false).unwrap();
        repo.record_parental_control_attempt(&child.id, "5678", false).unwrap();

        let dir = repo.attempts_dir(&child.id).unwrap();
        let path = dir.join("parental_control_attempts.csv");

        // Simulate a crash partway through writing the last field: a full
        // 4-field record whose `success` value is truncated mid-word.
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("3,9999,2024-01-01T00:00:00Z,tr");
        std::fs::write(&path, text).unwrap();

        let attempts = repo.get_parental_control_attempts(&child.id, None).unwrap();
        assert_eq!(attempts.len(), 2, "every prior record must still be readable");

        // And the log must still be appendable afterwards.
        repo.record_parental_control_attempt(&child.id, "0000", true).unwrap();
        let after = repo.get_parental_control_attempts(&child.id, None).unwrap();
        assert_eq!(
            after.len(),
            3,
            "a content-corrupted line must not block future appends"
        );
    }
}