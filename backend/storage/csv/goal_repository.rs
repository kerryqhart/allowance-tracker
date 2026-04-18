use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};
use crate::backend::storage::GitManager;
use anyhow::Result;
use log::warn;
use serde::{Deserialize, Serialize};
use std::fs::{self};
use std::io::BufWriter;
use super::connection::CsvConnection;

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

/// A CSV-based repository for storing and retrieving goals.
#[derive(Debug, Clone)]
pub struct GoalRepository {
    connection: CsvConnection,
    git_manager: GitManager,
}

impl GoalRepository {
    /// Create a new goal repository
    pub fn new(connection: CsvConnection) -> Self {
        Self {
            connection,
            git_manager: GitManager::new(),
        }
    }

    fn read_goals(&self, child_id: &str) -> Result<Vec<DomainGoal>> {
        let file_path = self.connection.get_goals_file_path(child_id);
        if !file_path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(file_path)?;
        let mut rdr = csv::Reader::from_reader(file);
        let mut goals = Vec::new();
        for result in rdr.deserialize() {
            let record: GoalRecord = result?;
            match DomainGoal::try_from(record) {
                Ok(goal) => goals.push(goal),
                Err(e) => {
                    warn!("Failed to parse goal record: {}. Skipping.", e);
                    continue;
                }
            }
        }
        Ok(goals)
    }

    fn write_goals(&self, child_id: &str, goals: &[DomainGoal]) -> Result<()> {
        let file_path = self.connection.get_goals_file_path(child_id);

        // Ensure the parent directory exists
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = fs::File::create(&file_path)?;
        let mut wtr = csv::Writer::from_writer(BufWriter::new(file));
        for goal in goals {
            let record = GoalRecord::from(goal.clone());
            wtr.serialize(record)?;
        }
        wtr.flush()?;

        // Git commit the goals file change
        if let Some(parent_dir) = file_path.parent() {
            let action_description = format!("Updated goals for child directory: {}", child_id);
            let _ = self.git_manager.commit_file_change(
                parent_dir,
                "goals.csv",
                &action_description
            );
        }

        Ok(())
    }
}

impl GoalRepository {
    /// Store a new goal (append-only - creates new record)
    pub fn store_goal(&self, goal: &DomainGoal) -> Result<()> {
        let mut goals = self.read_goals(&goal.child_id)?;
        goals.push(goal.clone());
        self.write_goals(&goal.child_id, &goals)
    }

    /// Get the current active goal for a specific child
    pub fn get_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        let goals = self.read_goals(child_id)?;
        Ok(goals
            .into_iter()
            .filter(|g| g.state == DomainGoalState::Active)
            .max_by_key(|g| g.updated_at.clone()))
    }

    /// List all goals for a specific child (with optional limit)
    /// Returns goals ordered by created_at descending (most recent first)
    pub fn list_goals(&self, child_id: &str, limit: Option<u32>) -> Result<Vec<DomainGoal>> {
        let mut goals = self.read_goals(child_id)?;
        // Sort by created_at descending (most recent first)
        goals.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        if let Some(limit) = limit {
            goals.truncate(limit as usize);
        }
        
        Ok(goals)
    }

    /// Update an existing goal by creating a new record with updated fields
    /// This maintains the append-only history while updating the current state
    pub fn update_goal(&self, goal: &DomainGoal) -> Result<()> {
        let mut goals = self.read_goals(&goal.child_id)?;
        if let Some(g) = goals.iter_mut().find(|g| g.id == goal.id) {
            *g = goal.clone();
        }
        self.write_goals(&goal.child_id, &goals)
    }

    /// Cancel the current active goal by setting its state to Cancelled
    pub fn cancel_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        let mut goals = self.read_goals(child_id)?;
        if let Some(goal) = goals.iter_mut().find(|g| g.state == DomainGoalState::Active) {
            goal.state = DomainGoalState::Cancelled;
            let cancelled_goal = goal.clone();
            self.write_goals(child_id, &goals)?;
            Ok(Some(cancelled_goal))
        } else {
            Ok(None)
        }
    }

    /// Mark the current active goal as completed
    pub fn complete_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        let mut goals = self.read_goals(child_id)?;
        if let Some(goal) = goals.iter_mut().find(|g| g.state == DomainGoalState::Active) {
            goal.state = DomainGoalState::Completed;
            let completed_goal = goal.clone();
            self.write_goals(child_id, &goals)?;
            Ok(Some(completed_goal))
        } else {
            Ok(None)
        }
    }

    /// Remove a goal record by id. Returns true if a row was removed.
    pub fn delete_goal_by_id(&self, child_id: &str, goal_id: &str) -> Result<bool> {
        let mut goals = self.read_goals(child_id)?;
        let before = goals.len();
        goals.retain(|g| g.id != goal_id);
        if goals.len() == before {
            return Ok(false);
        }
        self.write_goals(child_id, &goals)?;
        Ok(true)
    }

    /// Check if a child has an active goal
    pub fn has_active_goal(&self, child_id: &str) -> Result<bool> {
        let goals = self.read_goals(child_id)?;
        Ok(goals.iter().any(|g| g.state == DomainGoalState::Active))
    }
}