//! Goal service domain logic for the allowance tracker.
//!
//! This module contains the core business logic for goal management,
//! including CRUD operations, goal completion date calculations, and integration
//! with the allowance system for forward-looking projections.
//!
//! ## Key Responsibilities
//!
//! - **Goal CRUD**: Creating, reading, updating, and deleting goals
//! - **Goal Calculations**: Projecting completion dates using future allowances
//! - **State Management**: Handling goal lifecycle (active, cancelled, completed)
//! - **Business Rules**: Enforcing goal validation and business constraints
//! - **Integration**: Working with AllowanceService and TransactionService
//!
//! ## Business Rules
//!
//! - One active goal per child maximum
//! - Goals must have positive target amounts > current balance
//! - Automatic completion when balance meets or exceeds target
//! - Description limits: 1-256 characters
//! - Proper error handling for edge cases

use anyhow::Result;
use chrono::{Utc, NaiveDate, Duration, Local, Datelike};
use log::{info, warn, error};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend::storage::csv::{CsvConnection, GoalRepository};
use crate::backend::storage::GoalStorage;
use crate::backend::domain::{child_service::ChildService, AllowanceService, TransactionService, BalanceService};
use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};
use crate::backend::domain::commands::goal::{
    CreateGoalCommand, UpdateGoalCommand, GetCurrentGoalCommand, GetGoalHistoryCommand, CancelGoalCommand,
    CreateGoalResult, UpdateGoalResult, GetCurrentGoalResult, GetGoalHistoryResult, CancelGoalResult,
};
use crate::backend::domain::commands::transactions::{TransactionListQuery};
use crate::backend::io::rest::mappers::goal_mapper::GoalMapper;
use shared::GoalCalculation;

/// Service for managing goals and goal-related calculations
#[derive(Clone)]
pub struct GoalService {
    goal_repository: GoalRepository,
    child_service: ChildService,
    allowance_service: AllowanceService,
    transaction_service: TransactionService<CsvConnection>,
    balance_service: BalanceService<CsvConnection>,
}

impl GoalService {
    /// Create a new GoalService
    pub fn new(
        csv_conn: Arc<CsvConnection>,
        child_service: ChildService,
        allowance_service: AllowanceService,
        transaction_service: TransactionService<CsvConnection>,
        balance_service: BalanceService<CsvConnection>,
    ) -> Self {
        let goal_repository = GoalRepository::new((*csv_conn).clone());
        Self {
            goal_repository,
            child_service,
            allowance_service,
            transaction_service,
            balance_service,
        }
    }

    /// Create a new goal
    pub async fn create_goal(&self, command: CreateGoalCommand) -> Result<CreateGoalResult> {
        info!("Creating goal: {:?}", command);

        // Validate description
        if command.description.trim().is_empty() {
            return Err(anyhow::anyhow!("Goal description cannot be empty"));
        }
        if command.description.len() > 256 {
            return Err(anyhow::anyhow!("Goal description cannot exceed 256 characters"));
        }

        // Validate target amount
        if command.target_amount <= 0.0 {
            return Err(anyhow::anyhow!("Goal target amount must be positive"));
        }

        // Get child ID
        let child_id = match command.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.active_child.child {
                    Some(c) => c.id,
                    None => return Err(anyhow::anyhow!("No active child found")),
                };
                child
            }
        };

        // Check if child already has an active goal
        if self.goal_repository.has_active_goal(&child_id).await? {
            return Err(anyhow::anyhow!("Child already has an active goal. Cancel or complete the existing goal first."));
        }

        // Get current balance to validate goal is achievable
        let current_balance = self.get_current_balance(&child_id).await?;
        if current_balance >= command.target_amount {
            return Err(anyhow::anyhow!("Target amount (${:.2}) must be greater than current balance (${:.2})", 
                                     command.target_amount, current_balance));
        }

        // Generate goal ID
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;
        let goal_id = shared::Goal::generate_id(&child_id, now_millis);

        // Create domain goal
        let now_rfc3339 = Utc::now().to_rfc3339();
        let domain_goal = DomainGoal {
            id: goal_id,
            child_id: child_id.clone(),
            description: command.description.trim().to_string(),
            target_amount: command.target_amount,
            state: DomainGoalState::Active,
            created_at: now_rfc3339.clone(),
            updated_at: now_rfc3339,
        };

        // Store goal directly as domain model
        self.goal_repository.store_goal(&domain_goal).await?;

        // Calculate completion projection
        let calculation = self.calculate_goal_completion(&child_id, command.target_amount).await?;

        info!("Successfully created goal: {}", domain_goal.id);

        Ok(CreateGoalResult {
            goal: domain_goal,
            calculation,
            success_message: "Goal created successfully".to_string(),
        })
    }

    /// Get current active goal with calculations
    pub async fn get_current_goal(&self, command: GetCurrentGoalCommand) -> Result<GetCurrentGoalResult> {
        info!("Getting current goal: {:?}", command);

        // Get child ID
        let child_id = match command.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.active_child.child {
                    Some(c) => c.id,
                    None => {
                        info!("No active child found for goal request");
                        return Ok(GetCurrentGoalResult {
                            goal: None,
                            calculation: None,
                        });
                    }
                };
                child
            }
        };

        // Get current goal (returns domain Goal)
        let current_goal_domain = self.goal_repository.get_current_goal(&child_id).await?;
        
        // Calculate completion projection if goal exists
        let calculation = if let Some(ref goal) = current_goal_domain {
            Some(self.calculate_goal_completion(&child_id, goal.target_amount).await?)
        } else {
            None
        };

        Ok(GetCurrentGoalResult {
            goal: current_goal_domain,
            calculation,
        })
    }

    /// Update current active goal
    pub async fn update_goal(&self, command: UpdateGoalCommand) -> Result<UpdateGoalResult> {
        info!("Updating goal: {:?}", command);

        // Get child ID
        let child_id = match command.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.active_child.child {
                    Some(c) => c.id,
                    None => return Err(anyhow::anyhow!("No active child found to update goal")),
                };
                child
            }
        };

        // Get current active goal (returns domain Goal)
        let mut current_goal_domain = match self.goal_repository.get_current_goal(&child_id).await? {
            Some(goal) => goal,
            None => return Err(anyhow::anyhow!("No active goal found to update")),
        };

        // Update fields if provided
        if let Some(description) = command.description {
            if description.trim().is_empty() {
                return Err(anyhow::anyhow!("Goal description cannot be empty"));
            }
            if description.len() > 256 {
                return Err(anyhow::anyhow!("Goal description cannot exceed 256 characters"));
            }
            current_goal_domain.description = description.trim().to_string();
        }

        if let Some(target_amount) = command.target_amount {
            if target_amount <= 0.0 {
                return Err(anyhow::anyhow!("Goal target amount must be positive"));
            }
            
            // Validate target amount is greater than current balance
            let current_balance = self.get_current_balance(&child_id).await?;
            if current_balance >= target_amount {
                return Err(anyhow::anyhow!("Target amount (${:.2}) must be greater than current balance (${:.2})", 
                                         target_amount, current_balance));
            }
            
            current_goal_domain.target_amount = target_amount;
        }

        // Update timestamp
        current_goal_domain.updated_at = Utc::now().to_rfc3339();

        // Store updated goal (append-only)
        self.goal_repository.update_goal(&current_goal_domain).await?;

        // Calculate new completion projection
        let calculation = self.calculate_goal_completion(&child_id, current_goal_domain.target_amount).await?;

        info!("Successfully updated goal: {}", current_goal_domain.id);

        Ok(UpdateGoalResult {
            goal: current_goal_domain,
            calculation,
            success_message: "Goal updated successfully".to_string(),
        })
    }

    /// Cancel current active goal
    pub async fn cancel_goal(&self, command: CancelGoalCommand) -> Result<CancelGoalResult> {
        info!("Cancelling goal: {:?}", command);

        // Get child ID
        let child_id = match command.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.active_child.child {
                    Some(c) => c.id,
                    None => return Err(anyhow::anyhow!("No active child found to cancel goal")),
                };
                child
            }
        };

        // Cancel the goal (returns domain Goal)
        let cancelled_goal_domain = match self.goal_repository.cancel_current_goal(&child_id).await? {
            Some(goal) => goal,
            None => return Err(anyhow::anyhow!("No active goal found to cancel")),
        };

        info!("Successfully cancelled goal: {}", cancelled_goal_domain.id);

        Ok(CancelGoalResult {
            goal: cancelled_goal_domain,
            success_message: "Goal cancelled successfully".to_string(),
        })
    }

    /// Get goal history for a child
    pub async fn get_goal_history(&self, command: GetGoalHistoryCommand) -> Result<GetGoalHistoryResult> {
        info!("Getting goal history: {:?}", command);

        // Get child ID
        let child_id = match command.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.active_child.child {
                    Some(c) => c.id,
                    None => {
                        info!("No active child found for goal history request");
                        return Ok(GetGoalHistoryResult {
                            goals: Vec::new(),
                        });
                    }
                };
                child
            }
        };

        // Get goal history (returns domain Goals)
        let goals_domain = self.goal_repository.list_goals(&child_id, command.limit).await?;

        Ok(GetGoalHistoryResult { goals: goals_domain })
    }

    /// Check if current balance meets any active goal and auto-complete if so
    pub async fn check_and_complete_goals(&self, child_id: &str) -> Result<Option<DomainGoal>> {
        info!("Checking for goal completion for child: {}", child_id);

        // Get current active goal (returns domain Goal)
        let current_goal_domain = match self.goal_repository.get_current_goal(child_id).await? {
            Some(goal) => goal,
            None => {
                info!("No active goal found for child: {}", child_id);
                return Ok(None);
            }
        };

        // Get current balance
        let current_balance = self.get_current_balance(child_id).await?;

        // Check if goal is completed
        if current_balance >= current_goal_domain.target_amount {
            info!("Goal {} completed! Current balance: ${:.2}, Target: ${:.2}", 
                  current_goal_domain.id, current_balance, current_goal_domain.target_amount);
            
            // Mark goal as completed (returns domain Goal)
            let completed_goal_domain = self.goal_repository.complete_current_goal(child_id).await?;
            
            return Ok(completed_goal_domain);
        }

        Ok(None)
    }

    /// Calculate goal completion projection
    async fn calculate_goal_completion(&self, child_id: &str, target_amount: f64) -> Result<GoalCalculation> {
        info!("Calculating goal completion for child: {}, target: ${:.2}", child_id, target_amount);

        // Get current balance
        let current_balance = self.get_current_balance(child_id).await?;
        let amount_needed = target_amount - current_balance;

        // If already at target, return immediately
        if amount_needed <= 0.0 {
            return Ok(GoalCalculation {
                current_balance,
                amount_needed: 0.0,
                projected_completion_date: Some(Utc::now().to_rfc3339()),
                allowances_needed: 0,
                is_achievable: true,
                exceeds_time_limit: false,
            });
        }

        // Get allowance configuration
        let allowance_config = self.allowance_service
            .get_allowance_config(crate::backend::domain::commands::allowance::GetAllowanceConfigCommand {
                child_id: Some(child_id.to_string()),
            })
            .await?;

        let config = match allowance_config.allowance_config {
            Some(config) if config.is_active && config.amount > 0.0 => config,
            _ => {
                // No allowance configured or inactive
                return Ok(GoalCalculation {
                    current_balance,
                    amount_needed,
                    projected_completion_date: None,
                    allowances_needed: 0,
                    is_achievable: false,
                    exceeds_time_limit: false,
                });
            }
        };

        // Calculate how many allowances are needed
        let allowances_needed = (amount_needed / config.amount).ceil() as u32;

        // Calculate projected completion date
        let current_date = Local::now().date_naive();
        let mut projected_date = current_date;
        let mut allowances_counted = 0;

        // Find the next allowance dates
        for _ in 0..365 { // Limit to 1 year
            if projected_date > current_date {
                let day_of_week = projected_date.weekday().num_days_from_sunday() as u8;
                if day_of_week == config.day_of_week {
                    allowances_counted += 1;
                    if allowances_counted >= allowances_needed {
                        break;
                    }
                }
            }
            
            projected_date = projected_date.succ_opt().unwrap_or(projected_date);
            
            // Check if we've exceeded 1 year
            if projected_date > current_date + Duration::days(365) {
                return Ok(GoalCalculation {
                    current_balance,
                    amount_needed,
                    projected_completion_date: None,
                    allowances_needed,
                    is_achievable: false,
                    exceeds_time_limit: true,
                });
            }
        }

        // Convert to RFC 3339 format
        let projected_completion_date = projected_date
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();

        let exceeds_time_limit = projected_date > current_date + Duration::days(365);

        Ok(GoalCalculation {
            current_balance,
            amount_needed,
            projected_completion_date: Some(projected_completion_date),
            allowances_needed,
            is_achievable: !exceeds_time_limit,
            exceeds_time_limit,
        })
    }

    /// Get current balance for a child
    async fn get_current_balance(&self, _child_id: &str) -> Result<f64> {
        // Get the latest transaction to get current balance
        let query = TransactionListQuery {
            after: None,
            limit: Some(1),
            start_date: None,
            end_date: None,
        };

        let result = self.transaction_service.list_transactions(query).await?;

        match result.transactions.first() {
            Some(tx) => Ok(tx.balance),
            None => Ok(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::backend::storage::csv::CsvConnection;
    use crate::backend::domain::{ChildService, AllowanceService, TransactionService, BalanceService};

use crate::backend::domain::commands::goal::CreateGoalCommand;
use crate::backend::domain::commands::goal::CancelGoalCommand;
use crate::backend::domain::commands::goal::GetCurrentGoalCommand;
    use chrono::Utc;

    async fn create_test_service() -> GoalService {
        let db = Arc::new(CsvConnection::new_default().expect("Failed to init test DB"));
        
        // Create required service dependencies
        let child_service = ChildService::new(db.clone());
        let allowance_service = AllowanceService::new(db.clone());
        let transaction_service = TransactionService::new(db.clone(), child_service.clone(), allowance_service.clone(), BalanceService::new(db.clone()));
        let balance_service = BalanceService::new(db.clone());
        
        GoalService::new(db, child_service, allowance_service, transaction_service, balance_service)
    }

    async fn create_test_child_and_allowance(service: &GoalService) -> String {
        // Create a test child
        let child_request = CreateChildRequest {
            name: "Test Child".to_string(),
            birthdate: "2010-01-01".to_string(),
        };
        
        let child_response = service.child_service.create_child(child_request).await
            .expect("Failed to create test child");
        
        // Set up allowance
        let allowance_command = crate::backend::domain::commands::allowance::UpdateAllowanceConfigCommand {
            child_id: Some(child_response.child.id.clone()),
            amount: 5.0,
            day_of_week: 5, // Friday
            is_active: true,
        };
        
        service.allowance_service.update_allowance_config(allowance_command).await
            .expect("Failed to set up allowance");
        
        child_response.child.id
    }

    #[tokio::test]
    async fn test_create_goal() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        let command = CreateGoalCommand {
            child_id: Some(child_id.clone()),
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        let result = service.create_goal(command).await.expect("Failed to create goal");
        
        assert_eq!(result.goal.description, "Buy new toy");
        assert_eq!(result.goal.target_amount, 25.0);
        assert_eq!(result.goal.state, DomainGoalState::Active);
        assert!(result.calculation.is_achievable);
    }

    #[tokio::test]
    async fn test_create_goal_validation() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        // Empty description should fail
        let command = CreateGoalCommand {
            child_id: Some(child_id.clone()),
            description: "".to_string(),
            target_amount: 25.0,
        };
        
        let result = service.create_goal(command).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
        
        // Negative amount should fail
        let command = CreateGoalCommand {
            child_id: Some(child_id.clone()),
            description: "Test goal".to_string(),
            target_amount: -10.0,
        };
        
        let result = service.create_goal(command).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));
    }

    #[tokio::test]
    async fn test_cancel_goal() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        // Create a goal first
        let create_command = CreateGoalCommand {
            child_id: Some(child_id.clone()),
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        service.create_goal(create_command).await.expect("Failed to create goal");
        
        // Cancel the goal
        let cancel_command = CancelGoalCommand {
            child_id: Some(child_id.clone()),
        };
        
        let result = service.cancel_goal(cancel_command).await.expect("Failed to cancel goal");
        
        assert_eq!(result.goal.state, DomainGoalState::Cancelled);
        
        // Should no longer have an active goal
        let current_goal_command = GetCurrentGoalCommand {
            child_id: Some(child_id),
        };
        
        let current_result = service.get_current_goal(current_goal_command).await
            .expect("Failed to get current goal");
        
        assert!(current_result.goal.is_none());
    }

    #[tokio::test]
    async fn test_goal_calculation() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        // Create a goal that requires multiple allowances
        let command = CreateGoalCommand {
            child_id: Some(child_id),
            description: "Buy expensive toy".to_string(),
            target_amount: 30.0, // With $5 allowances, should need 6 allowances
        };
        
        let result = service.create_goal(command).await.expect("Failed to create goal");
        
        assert_eq!(result.calculation.allowances_needed, 6);
        assert!(result.calculation.is_achievable);
        assert!(result.calculation.projected_completion_date.is_some());
        assert!(!result.calculation.exceeds_time_limit);
    }
} 