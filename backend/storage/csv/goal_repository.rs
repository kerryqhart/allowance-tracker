use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};
use crate::backend::storage::GitManager;
use anyhow::Result;
use log::warn;
use serde::{Deserialize, Serialize};
use std::fs::{self};
use shared::ChildId;
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
        let file_path = self.connection.goals_path(&ChildId::from(child_id))?;
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
        let file_path = self.write_goals_internal(child_id, goals)?;

        // Git commit the goals file change
        if let Some(parent_dir) = file_path.parent() {
            let action_description = format!("Updated goals for child directory: {}", child_id);
            if let Err(e) = self.git_manager.commit_file_change(
                parent_dir,
                "goals.csv",
                &action_description
            ) {
                // Deliberately non-fatal: the data is already on disk, and
                // the sync guard commits a tracked file left dirty on its
                // next cycle — unless that cycle is refused outright, which
                // happens only when transactions.csv is found emptied (see
                // `DirtyTreeError::WouldEmptyLedger`). Logged rather than
                // discarded so this is visible.
                warn!("git commit for goals.csv did not complete: {e}");
            }
        }

        Ok(())
    }

    /// Write goals WITHOUT creating a git commit. Returns the file path so
    /// the committing `write_goals` above can find its parent directory
    /// without re-resolving it.
    ///
    /// Used exclusively by the AWS-apply path (`ApplyRemoteEntity`, applied
    /// on the UI thread in `egui-frontend/src/ui/app_coordinator.rs`). See
    /// `TransactionRepository::upsert_transaction_no_commit` for the full
    /// rationale: two transports now write this same file, and if the AWS
    /// path also committed here, one MCP-server write would produce a
    /// separate, divergent git commit on every machine running the MCP
    /// server. The commit for this change is produced later, by whatever
    /// ordinary local edit or lgs merge next touches `goals.csv`.
    fn write_goals_internal(&self, child_id: &str, goals: &[DomainGoal]) -> Result<std::path::PathBuf> {
        // No `create_dir_all` here: `goals_path` resolves through the registry
        // and has already proven the child's folder is present. Creating it
        // would be manufacturing a folder for a child whose data is elsewhere.
        let file_path = self.connection.goals_path(&ChildId::from(child_id))?;

        let mut wtr = csv::Writer::from_writer(Vec::new());
        for goal in goals {
            let record = GoalRecord::from(goal.clone());
            wtr.serialize(record)?;
        }
        let bytes = wtr.into_inner()?;
        crate::backend::storage::atomic::write(&file_path, &bytes)?;

        Ok(file_path)
    }

    /// Upsert a goal WITHOUT creating a git commit — the AWS-apply
    /// equivalent of `store_goal`/`update_goal` combined. Used exclusively
    /// by `GoalService::upsert_goal_from_sync`.
    pub(crate) fn upsert_goal_no_commit(&self, goal: &DomainGoal) -> Result<()> {
        let mut goals = self.read_goals(&goal.child_id)?;
        if let Some(existing) = goals.iter_mut().find(|g| g.id == goal.id) {
            *existing = goal.clone();
        } else {
            goals.push(goal.clone());
        }
        self.write_goals_internal(&goal.child_id, &goals)?;
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

    /// Remove a goal record by id WITHOUT creating a git commit.
    ///
    /// Used exclusively by the AWS-apply path (`DeleteLocalEntity`, the
    /// sibling of `ApplyRemoteEntity` — see
    /// `TransactionRepository::upsert_transaction_no_commit`'s doc comment
    /// for the shared rationale). Returns `true` if a row was found and
    /// removed, `false` if it was already absent (idempotent, same as the
    /// committing `delete_goal_by_id`).
    pub(crate) fn delete_goal_no_commit(&self, child_id: &str, goal_id: &str) -> Result<bool> {
        let mut goals = self.read_goals(child_id)?;
        let before = goals.len();
        goals.retain(|g| g.id != goal_id);
        if goals.len() == before {
            return Ok(false);
        }
        self.write_goals_internal(child_id, &goals)?;
        Ok(true)
    }

    /// Check if a child has an active goal
    pub fn has_active_goal(&self, child_id: &str) -> Result<bool> {
        let goals = self.read_goals(child_id)?;
        Ok(goals.iter().any(|g| g.state == DomainGoalState::Active))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
    use crate::backend::Backend;

    /// One child, with its folder registered and present — the state
    /// `write_goals_internal` assumes via `goals_path` (see its doc comment:
    /// "the child's folder is present"). Built through the same
    /// `Backend::with_data_dir` → `create_child` → `set_active_child`
    /// sequence `app_with_git_backed_child` uses in
    /// `egui-frontend/src/ui/app_coordinator.rs`, rather than constructing a
    /// `GoalRepository` directly against a bare `CsvConnection` — that would
    /// skip the registry state this write path depends on.
    fn repo_with_child() -> (GoalRepository, String, tempfile::TempDir) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None).expect("backend");
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");

        let repo = GoalRepository::new((*backend.csv_connection).clone());
        (repo, child.id, temp)
    }

    fn sample_goal(child_id: &str, id: &str) -> DomainGoal {
        DomainGoal {
            id: id.to_string(),
            child_id: child_id.to_string(),
            description: "Save for a bike".to_string(),
            target_amount: 100.0,
            state: DomainGoalState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn write_goals_internal_produces_the_same_bytes_as_a_streaming_writer() {
        let (repo, child_id, _temp) = repo_with_child();
        let goals = vec![sample_goal(&child_id, "g-1"), sample_goal(&child_id, "g-2")];

        let path = repo.write_goals_internal(&child_id, &goals).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();

        let mut expected = csv::Writer::from_writer(Vec::new());
        for goal in &goals {
            expected.serialize(GoalRecord::from(goal.clone())).unwrap();
        }
        expected.flush().unwrap();
        let expected = String::from_utf8(expected.into_inner().unwrap()).unwrap();

        assert_eq!(written, expected, "switching to a buffered render must not change a byte");
    }
}