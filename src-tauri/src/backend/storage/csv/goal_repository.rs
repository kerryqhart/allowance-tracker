//! # CSV Goal Repository
//!
//! This module provides a file-based goal storage implementation
//! using CSV files stored per-child. Each child's goals are stored
//! in `{child_directory}/goals.csv`.
//!
//! ## File Structure
//!
//! ```
//! data/
//! ├── global_config.yaml
//! └── {child_name}/
//!     ├── child.yaml
//!     ├── allowance_config.yaml
//!     ├── parental_control_attempts.csv
//!     ├── transactions.csv
//!     └── goals.csv    ← This module manages these files
//! ```
//!
//! ## CSV Format
//!
//! CSV files have the following structure:
//! ```csv
//! id,child_id,description,target_amount,state,created_at,updated_at
//! goal::1234567890,child::abc,"Buy new lego set",40.0,active,2025-01-20T10:00:00Z,2025-01-20T10:00:00Z
//! goal::1234567891,child::abc,"Buy new lego set",40.0,completed,2025-01-20T10:00:00Z,2025-02-15T14:30:00Z
//! ```
//!
//! ## Features
//!
//! - Per-child CSV files for goals
//! - Append-only approach with state tracking
//! - Atomic file writes with temp files
//! - Chronological ordering (most recent first)
//! - Full goal history preservation

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use csv::{Reader, Writer};
use log::{info, debug, warn};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};
use super::connection::CsvConnection;
use super::child_repository::ChildRepository;
use crate::backend::storage::{ChildStorage, GitManager};

/// CSV record structure for goals
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoalRecord {
    id: String,
    child_id: String,
    description: String,
    target_amount: f64,
    state: String,
    created_at: String,
    updated_at: String,
}

impl From<DomainGoal> for GoalRecord {
    fn from(goal: DomainGoal) -> Self {
        GoalRecord {
            id: goal.id,
            child_id: goal.child_id,
            description: goal.description,
            target_amount: goal.target_amount,
            state: goal.state.to_string(),
            created_at: goal.created_at,
            updated_at: goal.updated_at,
        }
    }
}

impl TryFrom<GoalRecord> for DomainGoal {
    type Error = anyhow::Error;

    fn try_from(record: GoalRecord) -> Result<Self> {
        let state = DomainGoalState::from_string(&record.state)
            .map_err(|e| anyhow::anyhow!("Failed to parse goal state: {}", e))?;

        Ok(DomainGoal {
            id: record.id,
            child_id: record.child_id,
            description: record.description,
            target_amount: record.target_amount,
            state,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }
}

/// CSV-based goal repository using per-child CSV files
#[derive(Clone)]
pub struct GoalRepository {
    connection: CsvConnection,
    child_repository: ChildRepository,
    git_manager: GitManager,
}

impl GoalRepository {
    /// Create a new CSV goal repository
    pub fn new(connection: CsvConnection) -> Self {
        let child_repository = ChildRepository::new(connection.clone());
        Self { 
            connection,
            child_repository,
            git_manager: GitManager::new(),
        }
    }
    
    /// Get the goals CSV file path for a specific child directory
    fn get_goals_file_path(&self, child_directory: &str) -> PathBuf {
        self.connection
            .get_child_directory(child_directory)
            .join("goals.csv")
    }
    
    /// Find the child directory that contains a child with the given child_id
    async fn find_child_directory(&self, child_id: &str) -> Result<Option<String>> {
        let children = self.child_repository.list_children().await?;
        
        for child in children {
            if child.id == child_id {
                return Ok(Some(ChildRepository::generate_safe_directory_name(&child.name)));
            }
        }
        
        Ok(None)
    }
    
    /// Ensure the goals CSV file exists for a child directory
    fn ensure_goals_file_exists(&self, child_directory: &str) -> Result<()> {
        let child_dir = self.connection.get_child_directory(child_directory);
        
        // Create the child directory if it doesn't exist
        if !child_dir.exists() {
            std::fs::create_dir_all(&child_dir)?;
        }
        
        let goals_file_path = self.get_goals_file_path(child_directory);
        
        if !goals_file_path.exists() {
            // Create the file with CSV header
            let header = "id,child_id,description,target_amount,state,created_at,updated_at\n";
            std::fs::write(&goals_file_path, header)?;
            debug!("Created goals CSV file: {:?}", goals_file_path);
        }
        
        Ok(())
    }
    
    /// Read all goals for a child from their CSV file
    async fn read_goals(&self, child_directory: &str) -> Result<Vec<DomainGoal>> {
        self.ensure_goals_file_exists(child_directory)?;
        
        let goals_file_path = self.get_goals_file_path(child_directory);
        let file = File::open(&goals_file_path)?;
        let reader = BufReader::new(file);
        let mut csv_reader = Reader::from_reader(reader);
        
        let mut goals = Vec::new();
        
        for result in csv_reader.records() {
            let record = result?;
            
            // Parse CSV record into GoalRecord
            let goal_record = GoalRecord {
                id: record.get(0).unwrap_or("").to_string(),
                child_id: record.get(1).unwrap_or("").to_string(),
                description: record.get(2).unwrap_or("").to_string(),
                target_amount: record.get(3).unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                state: record.get(4).unwrap_or("active").to_string(),
                created_at: record.get(5).unwrap_or("").to_string(),
                updated_at: record.get(6).unwrap_or("").to_string(),
            };
            
            // Convert to DomainGoal
            match DomainGoal::try_from(goal_record) {
                Ok(goal) => goals.push(goal),
                Err(e) => {
                    warn!("Failed to parse goal record: {}. Skipping.", e);
                    continue;
                }
            }
        }
        
        Ok(goals)
    }
    
    /// Write all goals for a child to their CSV file
    async fn write_goals(&self, child_directory: &str, goals: &[DomainGoal]) -> Result<()> {
        let goals_file_path = self.get_goals_file_path(child_directory);
        let temp_file_path = goals_file_path.with_extension("csv.tmp");
        
        // Write to temporary file first (atomic operation)
        {
            let temp_file = File::create(&temp_file_path)?;
            let writer = BufWriter::new(temp_file);
            let mut csv_writer = Writer::from_writer(writer);
            
            // Write all goal records
            for goal in goals {
                let record = GoalRecord::from(goal.clone());
                csv_writer.serialize(record)?;
            }
            
            csv_writer.flush()?;
        }
        
        // Atomically replace the original file
        std::fs::rename(&temp_file_path, &goals_file_path)?;
        
        // Git commit if enabled
        if let Err(e) = self.git_manager.commit_file_change(&self.connection.get_child_directory(child_directory), "goals.csv", &format!("Updated goals for child directory: {}", child_directory)).await {
            debug!("Git commit failed (this is OK in development): {}", e);
        }
        
        debug!("Successfully wrote {} goals to {:?}", goals.len(), goals_file_path);
        Ok(())
    }
    
    /// Append a new goal to the CSV file (more efficient than rewriting entire file)
    async fn append_goal(&self, child_directory: &str, goal: &DomainGoal) -> Result<()> {
        self.ensure_goals_file_exists(child_directory)?;
        
        let goals_file_path = self.get_goals_file_path(child_directory);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&goals_file_path)?;
        
        let mut csv_writer = Writer::from_writer(file);
        // Don't write headers when appending to existing file
        csv_writer.write_record(&[
            &goal.id,
            &goal.child_id, 
            &goal.description,
            &goal.target_amount.to_string(),
            &goal.state.to_string(),
            &goal.created_at,
            &goal.updated_at,
        ])?;
        csv_writer.flush()?;
        
        // Git commit if enabled
        if let Err(e) = self.git_manager.commit_file_change(&self.connection.get_child_directory(child_directory), "goals.csv", &format!("Added new goal for child directory: {}", child_directory)).await {
            debug!("Git commit failed (this is OK in development): {}", e);
        }
        
        debug!("Successfully appended goal {} to {:?}", goal.id, goals_file_path);
        Ok(())
    }
}

#[async_trait]
impl crate::backend::storage::GoalStorage for GoalRepository {
    /// Store a new goal (append-only - creates new record)
    async fn store_goal(&self, goal: &DomainGoal) -> Result<()> {
        info!("Storing goal in CSV: {}", goal.id);
        
        let child_directory = match self.find_child_directory(&goal.child_id).await? {
            Some(dir) => dir,
            None => return Err(anyhow::anyhow!("Child not found: {}", goal.child_id)),
        };
        
        self.append_goal(&child_directory, goal).await?;
        
        info!("Successfully stored goal: {}", goal.id);
        Ok(())
    }
    
    /// Get the current active goal for a specific child
    async fn get_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        let child_directory = match self.find_child_directory(child_id).await? {
            Some(dir) => dir,
            None => return Ok(None),
        };
        
        let mut goals = self.read_goals(&child_directory).await?;
        
        // Sort by updated_at descending (most recent first)
        goals.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        
        // Get the most recent goal entry and check if it's active
        match goals.first() {
            Some(goal) if goal.state == DomainGoalState::Active => Ok(Some(goal.clone())),
            _ => Ok(None), // No goals or latest goal is cancelled/completed
        }
    }
    
    /// List all goals for a specific child (with optional limit)
    /// Returns goals ordered by created_at descending (most recent first)
    async fn list_goals(&self, child_id: &str, limit: Option<u32>) -> Result<Vec<DomainGoal>> {
        let child_directory = match self.find_child_directory(child_id).await? {
            Some(dir) => dir,
            None => return Ok(Vec::new()),
        };
        
        let mut goals = self.read_goals(&child_directory).await?;
        
        // Sort by created_at descending (most recent first)
        goals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        // Apply limit if specified
        if let Some(limit) = limit {
            goals.truncate(limit as usize);
        }
        
        Ok(goals)
    }
    
    /// Update an existing goal by creating a new record with updated fields
    /// This maintains the append-only history while updating the current state
    async fn update_goal(&self, goal: &DomainGoal) -> Result<()> {
        info!("Updating goal in CSV: {}", goal.id);
        
        // Simply append the updated goal - the append-only nature preserves history
        self.store_goal(goal).await?;
        
        info!("Successfully updated goal: {}", goal.id);
        Ok(())
    }
    
    /// Cancel the current active goal by setting its state to Cancelled
    async fn cancel_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        info!("Cancelling current goal for child: {}", child_id);
        
        let current_goal = match self.get_current_goal(child_id).await? {
            Some(goal) => goal,
            None => {
                info!("No active goal found to cancel for child: {}", child_id);
                return Ok(None);
            }
        };
        
        // Create cancelled version
        let mut cancelled_goal = current_goal.clone();
        cancelled_goal.state = DomainGoalState::Cancelled;
        cancelled_goal.updated_at = Utc::now().to_rfc3339();
        
        // Store the cancelled goal (append-only)
        self.store_goal(&cancelled_goal).await?;
        
        info!("Successfully cancelled goal: {}", cancelled_goal.id);
        Ok(Some(cancelled_goal))
    }
    
    /// Mark the current active goal as completed
    async fn complete_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        info!("Completing current goal for child: {}", child_id);
        
        let current_goal = match self.get_current_goal(child_id).await? {
            Some(goal) => goal,
            None => {
                info!("No active goal found to complete for child: {}", child_id);
                return Ok(None);
            }
        };
        
        // Create completed version
        let mut completed_goal = current_goal.clone();
        completed_goal.state = DomainGoalState::Completed;
        completed_goal.updated_at = Utc::now().to_rfc3339();
        
        // Store the completed goal (append-only)
        self.store_goal(&completed_goal).await?;
        
        info!("Successfully completed goal: {}", completed_goal.id);
        Ok(Some(completed_goal))
    }
    
    /// Check if a child has an active goal
    async fn has_active_goal(&self, child_id: &str) -> Result<bool> {
        let current_goal = self.get_current_goal(child_id).await?;
        Ok(current_goal.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use chrono::Utc;
    use crate::backend::domain::models::child::Child as DomainChild;
    use crate::backend::storage::GoalStorage;

    async fn setup_test_repo_with_child() -> (GoalRepository, ChildRepository, TempDir, DomainChild) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let connection = CsvConnection::new(temp_dir.path()).expect("Failed to create connection");
        let goal_repo = GoalRepository::new(connection.clone());
        let child_repo = ChildRepository::new(connection);
        
        // Create a test child first
        let child = DomainChild {
            id: "child::1234567890".to_string(),
            name: "Test Child".to_string(),
            birthdate: chrono::NaiveDate::parse_from_str("2010-01-01", "%Y-%m-%d").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        child_repo.store_child(&child).await.expect("Failed to create test child");
        
        (goal_repo, child_repo, temp_dir, child)
    }

    #[tokio::test]
    async fn test_store_and_get_goal() {
        let (goal_repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;
        
        let goal = DomainGoal {
            id: "goal::test".to_string(),
            child_id: child.id.clone(),
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
            state: DomainGoalState::Active,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        // Store goal
        goal_repo.store_goal(&goal).await.expect("Failed to store goal");
        
        // Retrieve goal
        let retrieved_goal = goal_repo.get_current_goal(&child.id).await
            .expect("Failed to get goal")
            .expect("Goal should exist");
        
        assert_eq!(retrieved_goal.id, goal.id);
        assert_eq!(retrieved_goal.description, goal.description);
        assert_eq!(retrieved_goal.target_amount, goal.target_amount);
        assert_eq!(retrieved_goal.state, DomainGoalState::Active);
    }

    #[tokio::test]
    async fn test_cancel_goal() {
        let (goal_repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;
        
        let goal = DomainGoal {
            id: "goal::test".to_string(),
            child_id: child.id.clone(),
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
            state: DomainGoalState::Active,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        // Store active goal
        goal_repo.store_goal(&goal).await.expect("Failed to store goal");
        
        // Cancel goal
        let cancelled_goal = goal_repo.cancel_current_goal(&child.id).await
            .expect("Failed to cancel goal")
            .expect("Cancelled goal should be returned");
        
        assert_eq!(cancelled_goal.state, DomainGoalState::Cancelled);
        
        // Should no longer have an active goal
        let current_goal = goal_repo.get_current_goal(&child.id).await
            .expect("Failed to get current goal");
        assert!(current_goal.is_none());
    }

    #[tokio::test]
    async fn test_goal_history() {
        let (goal_repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;
        
        // Create multiple goals over time
        let goal1 = DomainGoal {
            id: "goal::1".to_string(),
            child_id: child.id.clone(),
            description: "First goal".to_string(),
            target_amount: 10.0,
            state: DomainGoalState::Active,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        };
        
        let goal2 = DomainGoal {
            id: "goal::2".to_string(),
            child_id: child.id.clone(),
            description: "Second goal".to_string(),
            target_amount: 20.0,
            state: DomainGoalState::Completed,
            created_at: "2025-01-02T00:00:00Z".to_string(),
            updated_at: "2025-01-02T00:00:00Z".to_string(),
        };
        
        goal_repo.store_goal(&goal1).await.expect("Failed to store goal1");
        goal_repo.store_goal(&goal2).await.expect("Failed to store goal2");
        
        // Get history
        let history = goal_repo.list_goals(&child.id, None).await
            .expect("Failed to get goal history");
        
        assert_eq!(history.len(), 2);
        // Should be sorted by created_at descending (most recent first)
        assert_eq!(history[0].id, "goal::2");
        assert_eq!(history[1].id, "goal::1");
    }

    #[tokio::test]
    async fn test_has_active_goal() {
        let (goal_repo, _child_repo, _temp_dir, child) = setup_test_repo_with_child().await;
        
        // Initially no active goal
        assert!(!goal_repo.has_active_goal(&child.id).await.expect("Failed to check active goal"));
        
        let goal = DomainGoal {
            id: "goal::test".to_string(),
            child_id: child.id.clone(),
            description: "Test goal".to_string(),
            target_amount: 25.0,
            state: DomainGoalState::Active,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        };
        
        // Store active goal
        goal_repo.store_goal(&goal).await.expect("Failed to store goal");
        
        // Now should have active goal
        assert!(goal_repo.has_active_goal(&child.id).await.expect("Failed to check active goal"));
        
        // Cancel the goal
        goal_repo.cancel_current_goal(&child.id).await.expect("Failed to cancel goal");
        
        // Should no longer have active goal
        assert!(!goal_repo.has_active_goal(&child.id).await.expect("Failed to check active goal"));
    }
} 