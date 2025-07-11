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
use shared::{
    Goal, GoalState, GoalCalculation, CreateGoalRequest, CreateGoalResponse,
    UpdateGoalRequest, UpdateGoalResponse, GetCurrentGoalRequest, GetCurrentGoalResponse,
    GetGoalHistoryRequest, GetGoalHistoryResponse, CancelGoalRequest, CancelGoalResponse,
};
use crate::backend::domain::commands::transactions::{TransactionListQuery};

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
    pub async fn create_goal(&self, request: CreateGoalRequest) -> Result<CreateGoalResponse> {
        info!("Creating goal: {:?}", request);

        // Validate description
        if request.description.trim().is_empty() {
            return Err(anyhow::anyhow!("Goal description cannot be empty"));
        }
        if request.description.len() > 256 {
            return Err(anyhow::anyhow!("Goal description cannot exceed 256 characters"));
        }

        // Validate target amount
        if request.target_amount <= 0.0 {
            return Err(anyhow::anyhow!("Goal target amount must be positive"));
        }

        // Get child ID
        let child_id = match request.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.child {
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
        if current_balance >= request.target_amount {
            return Err(anyhow::anyhow!("Target amount (${:.2}) must be greater than current balance (${:.2})", 
                                     request.target_amount, current_balance));
        }

        // Generate goal ID
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;
        let goal_id = Goal::generate_id(&child_id, now_millis);

        // Create goal
        let now_rfc3339 = Utc::now().to_rfc3339();
        let goal = Goal {
            id: goal_id,
            child_id: child_id.clone(),
            description: request.description.trim().to_string(),
            target_amount: request.target_amount,
            state: GoalState::Active,
            created_at: now_rfc3339.clone(),
            updated_at: now_rfc3339,
        };

        // Store goal
        self.goal_repository.store_goal(&goal).await?;

        // Calculate completion projection
        let calculation = self.calculate_goal_completion(&child_id, request.target_amount).await?;

        info!("Successfully created goal: {}", goal.id);

        Ok(CreateGoalResponse {
            goal,
            calculation,
            success_message: "Goal created successfully".to_string(),
        })
    }

    /// Get current active goal with calculations
    pub async fn get_current_goal(&self, request: GetCurrentGoalRequest) -> Result<GetCurrentGoalResponse> {
        info!("Getting current goal: {:?}", request);

        // Get child ID
        let child_id = match request.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.child {
                    Some(c) => c.id,
                    None => {
                        info!("No active child found for goal request");
                        return Ok(GetCurrentGoalResponse {
                            goal: None,
                            calculation: None,
                        });
                    }
                };
                child
            }
        };

        // Get current goal
        let current_goal = self.goal_repository.get_current_goal(&child_id).await?;
        
        // Calculate completion projection if goal exists
        let calculation = if let Some(ref goal) = current_goal {
            Some(self.calculate_goal_completion(&child_id, goal.target_amount).await?)
        } else {
            None
        };

        Ok(GetCurrentGoalResponse {
            goal: current_goal,
            calculation,
        })
    }

    /// Update current active goal
    pub async fn update_goal(&self, request: UpdateGoalRequest) -> Result<UpdateGoalResponse> {
        info!("Updating goal: {:?}", request);

        // Get child ID
        let child_id = match request.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.child {
                    Some(c) => c.id,
                    None => return Err(anyhow::anyhow!("No active child found to update goal")),
                };
                child
            }
        };

        // Get current active goal
        let mut current_goal = match self.goal_repository.get_current_goal(&child_id).await? {
            Some(goal) => goal,
            None => return Err(anyhow::anyhow!("No active goal found to update")),
        };

        // Update fields if provided
        if let Some(description) = request.description {
            if description.trim().is_empty() {
                return Err(anyhow::anyhow!("Goal description cannot be empty"));
            }
            if description.len() > 256 {
                return Err(anyhow::anyhow!("Goal description cannot exceed 256 characters"));
            }
            current_goal.description = description.trim().to_string();
        }

        if let Some(target_amount) = request.target_amount {
            if target_amount <= 0.0 {
                return Err(anyhow::anyhow!("Goal target amount must be positive"));
            }
            
            // Validate target amount is greater than current balance
            let current_balance = self.get_current_balance(&child_id).await?;
            if current_balance >= target_amount {
                return Err(anyhow::anyhow!("Target amount (${:.2}) must be greater than current balance (${:.2})", 
                                         target_amount, current_balance));
            }
            
            current_goal.target_amount = target_amount;
        }

        // Update timestamp
        current_goal.updated_at = Utc::now().to_rfc3339();

        // Store updated goal (append-only)
        self.goal_repository.update_goal(&current_goal).await?;

        // Calculate new completion projection
        let calculation = self.calculate_goal_completion(&child_id, current_goal.target_amount).await?;

        info!("Successfully updated goal: {}", current_goal.id);

        Ok(UpdateGoalResponse {
            goal: current_goal,
            calculation,
            success_message: "Goal updated successfully".to_string(),
        })
    }

    /// Cancel current active goal
    pub async fn cancel_goal(&self, request: CancelGoalRequest) -> Result<CancelGoalResponse> {
        info!("Cancelling goal: {:?}", request);

        // Get child ID
        let child_id = match request.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.child {
                    Some(c) => c.id,
                    None => return Err(anyhow::anyhow!("No active child found to cancel goal")),
                };
                child
            }
        };

        // Cancel the goal
        let cancelled_goal = match self.goal_repository.cancel_current_goal(&child_id).await? {
            Some(goal) => goal,
            None => return Err(anyhow::anyhow!("No active goal found to cancel")),
        };

        info!("Successfully cancelled goal: {}", cancelled_goal.id);

        Ok(CancelGoalResponse {
            goal: cancelled_goal,
            success_message: "Goal cancelled successfully".to_string(),
        })
    }

    /// Get goal history for a child
    pub async fn get_goal_history(&self, request: GetGoalHistoryRequest) -> Result<GetGoalHistoryResponse> {
        info!("Getting goal history: {:?}", request);

        // Get child ID
        let child_id = match request.child_id {
            Some(id) => id,
            None => {
                let active_child_response = self.child_service.get_active_child().await?;
                let child = match active_child_response.child {
                    Some(c) => c.id,
                    None => {
                        info!("No active child found for goal history request");
                        return Ok(GetGoalHistoryResponse {
                            goals: Vec::new(),
                        });
                    }
                };
                child
            }
        };

        // Get goal history
        let goals = self.goal_repository.list_goals(&child_id, request.limit).await?;

        Ok(GetGoalHistoryResponse { goals })
    }

    /// Check if current balance meets any active goal and auto-complete if so
    pub async fn check_and_complete_goals(&self, child_id: &str) -> Result<Option<Goal>> {
        info!("Checking for goal completion for child: {}", child_id);

        // Get current active goal
        let current_goal = match self.goal_repository.get_current_goal(child_id).await? {
            Some(goal) => goal,
            None => {
                info!("No active goal found for child: {}", child_id);
                return Ok(None);
            }
        };

        // Get current balance
        let current_balance = self.get_current_balance(child_id).await?;

        // Check if goal is completed
        if current_balance >= current_goal.target_amount {
            info!("Goal {} completed! Current balance: ${:.2}, Target: ${:.2}", 
                  current_goal.id, current_balance, current_goal.target_amount);
            
            // Mark goal as completed
            let completed_goal = self.goal_repository.complete_current_goal(child_id).await?;
            
            return Ok(completed_goal);
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

        let result = self.transaction_service.list_transactions_domain(query).await?;

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
    use shared::{Child, AllowanceConfig, CreateChildRequest};
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
        let allowance_request = shared::UpdateAllowanceConfigRequest {
            child_id: Some(child_response.child.id.clone()),
            amount: 5.0,
            day_of_week: 5, // Friday
            is_active: true,
        };
        
        service.allowance_service.update_allowance_config(allowance_request).await
            .expect("Failed to set up allowance");
        
        child_response.child.id
    }

    #[tokio::test]
    async fn test_create_goal() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        let request = CreateGoalRequest {
            child_id: Some(child_id.clone()),
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        let response = service.create_goal(request).await.expect("Failed to create goal");
        
        assert_eq!(response.goal.description, "Buy new toy");
        assert_eq!(response.goal.target_amount, 25.0);
        assert_eq!(response.goal.state, GoalState::Active);
        assert!(response.calculation.is_achievable);
    }

    #[tokio::test]
    async fn test_create_goal_validation() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        // Empty description should fail
        let request = CreateGoalRequest {
            child_id: Some(child_id.clone()),
            description: "".to_string(),
            target_amount: 25.0,
        };
        
        let result = service.create_goal(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
        
        // Negative amount should fail
        let request = CreateGoalRequest {
            child_id: Some(child_id.clone()),
            description: "Test goal".to_string(),
            target_amount: -10.0,
        };
        
        let result = service.create_goal(request).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));
    }

    #[tokio::test]
    async fn test_cancel_goal() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        // Create a goal first
        let create_request = CreateGoalRequest {
            child_id: Some(child_id.clone()),
            description: "Buy new toy".to_string(),
            target_amount: 25.0,
        };
        
        service.create_goal(create_request).await.expect("Failed to create goal");
        
        // Cancel the goal
        let cancel_request = CancelGoalRequest {
            child_id: Some(child_id.clone()),
        };
        
        let response = service.cancel_goal(cancel_request).await.expect("Failed to cancel goal");
        
        assert_eq!(response.goal.state, GoalState::Cancelled);
        
        // Should no longer have an active goal
        let current_goal_request = GetCurrentGoalRequest {
            child_id: Some(child_id),
        };
        
        let current_response = service.get_current_goal(current_goal_request).await
            .expect("Failed to get current goal");
        
        assert!(current_response.goal.is_none());
    }

    #[tokio::test]
    async fn test_goal_calculation() {
        let service = create_test_service().await;
        let child_id = create_test_child_and_allowance(&service).await;
        
        // Create a goal that requires multiple allowances
        let request = CreateGoalRequest {
            child_id: Some(child_id),
            description: "Buy expensive toy".to_string(),
            target_amount: 30.0, // With $5 allowances, should need 6 allowances
        };
        
        let response = service.create_goal(request).await.expect("Failed to create goal");
        
        assert_eq!(response.calculation.allowances_needed, 6);
        assert!(response.calculation.is_achievable);
        assert!(response.calculation.projected_completion_date.is_some());
        assert!(!response.calculation.exceeds_time_limit);
    }
} 