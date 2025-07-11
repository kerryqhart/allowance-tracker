use serde::{Deserialize, Serialize};
use shared::GoalState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomainGoal {
    pub id: String,
    pub child_id: String,
    pub description: String,
    pub target_amount: f64,
    pub state: GoalState,
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