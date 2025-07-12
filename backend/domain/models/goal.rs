use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DomainGoalState {
    Active,
    Cancelled,
    Completed,
}

impl DomainGoalState {
    /// Convert to string for CSV storage
    pub fn to_string(&self) -> String {
        match self {
            DomainGoalState::Active => "active".to_string(),
            DomainGoalState::Cancelled => "cancelled".to_string(),
            DomainGoalState::Completed => "completed".to_string(),
        }
    }

    /// Parse from string for CSV loading
    pub fn from_string(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "active" => Ok(DomainGoalState::Active),
            "cancelled" => Ok(DomainGoalState::Cancelled),
            "completed" => Ok(DomainGoalState::Completed),
            _ => Err(format!("Invalid goal state: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainGoal {
    pub id: String,
    pub child_id: String,
    pub description: String,
    pub target_amount: f64,
    pub state: DomainGoalState,
    pub created_at: String,
    pub updated_at: String,
}

impl DomainGoal {
    pub fn generate_id(child_id: &str, now_millis: u64) -> String {
        format!("goal::{}_{}", child_id, now_millis)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GoalValidationError {
    #[error("Description cannot be empty")]
    EmptyDescription,
    #[error("Description is too long")]
    DescriptionTooLong,
    #[error("Target amount must be positive")]
    NonPositiveTargetAmount,
    #[error("Target amount must be greater than current balance")]
    TargetAmountNotGreaterThanBalance,
    #[error("Child already has an active goal")]
    ActiveGoalAlreadyExists,
} 