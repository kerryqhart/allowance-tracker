use crate::backend::{
    domain::models::goal::DomainGoal,
    storage::traits::GoalRepository,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use shared::{Goal, GoalState};
use super::connection::CsvConnection;

/// A CSV-based repository for storing and retrieving goals.
#[derive(Debug, Clone)]
pub struct CsvGoalRepository {
    connection: CsvConnection,
}

impl CsvGoalRepository {
    /// Create a new goal repository
    pub fn new(connection: CsvConnection) -> Self {
        Self { connection }
    }

    async fn read_goals(&self, child_id: &str) -> Result<Vec<DomainGoal>> {
        let file_path = self.connection.get_goals_file_path(child_id);
        if !file_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(file_path)?;
        let mut rdr = csv::Reader::from_reader(file);
        let mut goals = Vec::new();
        for result in rdr.deserialize() {
            let goal: DomainGoal = result?;
            goals.push(goal);
        }
        Ok(goals)
    }

    async fn write_goals(&self, child_id: &str, goals: &[DomainGoal]) -> Result<()> {
        let file_path = self.connection.get_goals_file_path(child_id);
        let mut wtr = csv::Writer::from_path(file_path)?;
        for goal in goals {
            wtr.serialize(goal)?;
        }
        wtr.flush()?;
        Ok(())
    }
}

#[async_trait]
impl GoalRepository for CsvGoalRepository {
    async fn store_goal(&self, goal: &DomainGoal) -> Result<()> {
        let mut goals = self.read_goals(&goal.child_id)?;
        goals.push(goal.clone());
        self.write_goals(&goal.child_id, &goals)
    }

    async fn get_goal_by_id(&self, child_id: &str, goal_id: &str) -> Result<Option<DomainGoal>> {
        let goals = self.read_goals(child_id)?;
        Ok(goals.into_iter().find(|g| g.id == goal_id))
    }

    async fn get_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        let goals = self.read_goals(child_id)?;
        Ok(goals
            .into_iter()
            .filter(|g| g.state == GoalState::Active)
            .max_by_key(|g| g.updated_at.clone()))
    }

    async fn get_all_goals(&self, child_id: &str) -> Result<Vec<DomainGoal>> {
        self.read_goals(child_id)
    }

    async fn update_goal(&self, goal: &DomainGoal) -> Result<()> {
        let mut goals = self.read_goals(&goal.child_id)?;
        if let Some(g) = goals.iter_mut().find(|g| g.id == goal.id) {
            *g = goal.clone();
        }
        self.write_goals(&goal.child_id, &goals)
    }

    async fn has_active_goal(&self, child_id: &str) -> Result<bool> {
        let goals = self.read_goals(child_id)?;
        Ok(goals.iter().any(|g| g.state == GoalState::Active))
    }
}