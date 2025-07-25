// src-tauri/src/backend/domain/commands.rs

//! Domain-level command and query types
//! These structs are used by services inside the domain layer and are **not**
//! exposed over the public API. The REST (or Tauri) layer is responsible for
//! mapping the public DTOs defined in the `shared` crate to these internal
//! types.

pub mod transactions {
    use super::super::models::transaction::Transaction as DomainTransaction;

    /// Input for creating a new transaction.
    #[derive(Debug, Clone)]
    pub struct CreateTransactionCommand {
        pub description: String,
        pub amount: f64,
        pub date: Option<chrono::DateTime<chrono::FixedOffset>>,
    }

    /// Query parameters for listing transactions.
    #[derive(Debug, Clone, Default)]
    pub struct TransactionListQuery {
        pub after: Option<String>,
        pub limit: Option<u32>,
        pub start_date: Option<String>,
        pub end_date: Option<String>,
    }

    /// Query parameters for getting transactions for calendar display.
    #[derive(Debug, Clone)]
    pub struct CalendarTransactionsQuery {
        pub month: u32,
        pub year: u32,
    }

    /// Command for deleting multiple transactions.
    #[derive(Debug, Clone)]
    pub struct DeleteTransactionsCommand {
        pub transaction_ids: Vec<String>,
    }

    /// Generic pagination info returned by list queries.
    #[derive(Debug, Clone)]
    pub struct PaginationInfo {
        pub has_more: bool,
        pub next_cursor: Option<String>,
    }

    /// Result of listing transactions.
    #[derive(Debug, Clone)]
    pub struct TransactionListResult {
        pub transactions: Vec<DomainTransaction>,
        pub pagination: PaginationInfo,
    }

    /// Result of getting transactions for calendar display.
    #[derive(Debug, Clone)]
    pub struct CalendarTransactionsResult {
        pub transactions: Vec<DomainTransaction>,
    }

    /// Result of deleting transactions.
    #[derive(Debug, Clone)]
    pub struct DeleteTransactionsResult {
        pub deleted_count: usize,
        pub not_found_ids: Vec<String>,
        pub success_message: String,
    }
}

pub mod allowance {
    use crate::backend::domain::models::allowance::AllowanceConfig;

    /// Input for getting allowance configuration.
    #[derive(Debug, Clone)]
    pub struct GetAllowanceConfigCommand {
        pub child_id: Option<String>,
    }

    /// Input for updating allowance configuration.
    #[derive(Debug, Clone)]
    pub struct UpdateAllowanceConfigCommand {
        pub child_id: Option<String>,
        pub amount: f64,
        pub day_of_week: u8,
        pub is_active: bool,
    }

    /// Result of getting allowance configuration.
    #[derive(Debug, Clone)]
    pub struct GetAllowanceConfigResult {
        pub allowance_config: Option<AllowanceConfig>,
    }

    /// Result of updating allowance configuration.
    #[derive(Debug, Clone)]
    pub struct UpdateAllowanceConfigResult {
        pub allowance_config: AllowanceConfig,
        pub success_message: String,
    }
}

pub mod goal {
    use crate::backend::domain::models::goal::DomainGoal;
    use shared::GoalCalculation;

    /// Input for creating a new goal.
    #[derive(Debug, Clone)]
    pub struct CreateGoalCommand {
        pub child_id: Option<String>,
        pub description: String,
        pub target_amount: f64,
    }

    /// Input for updating a goal.
    #[derive(Debug, Clone)]
    pub struct UpdateGoalCommand {
        pub child_id: Option<String>,
        pub description: Option<String>,
        pub target_amount: Option<f64>,
    }

    /// Input for getting current goal.
    #[derive(Debug, Clone)]
    pub struct GetCurrentGoalCommand {
        pub child_id: Option<String>,
    }

    /// Input for getting goal history.
    #[derive(Debug, Clone)]
    pub struct GetGoalHistoryCommand {
        pub child_id: Option<String>,
        pub limit: Option<u32>,
    }

    /// Input for canceling a goal.
    #[derive(Debug, Clone)]
    pub struct CancelGoalCommand {
        pub child_id: Option<String>,
    }

    /// Result of creating a goal.
    #[derive(Debug, Clone)]
    pub struct CreateGoalResult {
        pub goal: DomainGoal,
        pub calculation: GoalCalculation,
        pub success_message: String,
    }

    /// Result of updating a goal.
    #[derive(Debug, Clone)]
    pub struct UpdateGoalResult {
        pub goal: DomainGoal,
        pub calculation: GoalCalculation,
        pub success_message: String,
    }

    /// Result of getting current goal.
    #[derive(Debug, Clone)]
    pub struct GetCurrentGoalResult {
        pub goal: Option<DomainGoal>,
        pub calculation: Option<GoalCalculation>,
    }

    /// Result of getting goal history.
    #[derive(Debug, Clone)]
    pub struct GetGoalHistoryResult {
        pub goals: Vec<DomainGoal>,
    }

    /// Result of canceling a goal.
    #[derive(Debug, Clone)]
    pub struct CancelGoalResult {
        pub goal: DomainGoal,
        pub success_message: String,
    }
}

pub mod child {
    use crate::backend::domain::models::child::{ActiveChild, Child as DomainChild};

    /// Input for creating a new child.
    #[derive(Debug, Clone)]
    pub struct CreateChildCommand {
        pub name: String,
        pub birthdate: String, // Format: YYYY-MM-DD
    }

    /// Input for updating a child.
    #[derive(Debug, Clone)]
    pub struct UpdateChildCommand {
        pub child_id: String,
        pub name: Option<String>,
        pub birthdate: Option<String>, // Format: YYYY-MM-DD
    }

    /// Input for getting a child by ID.
    #[derive(Debug, Clone)]
    pub struct GetChildCommand {
        pub child_id: String,
    }

    /// Input for setting the active child.
    #[derive(Debug, Clone)]
    pub struct SetActiveChildCommand {
        pub child_id: String,
    }

    /// Input for deleting a child.
    #[derive(Debug, Clone)]
    pub struct DeleteChildCommand {
        pub child_id: String,
    }

    /// Result of creating a child.
    #[derive(Debug, Clone)]
    pub struct CreateChildResult {
        pub child: DomainChild,
    }

    /// Result of updating a child.
    #[derive(Debug, Clone)]
    pub struct UpdateChildResult {
        pub child: DomainChild,
    }

    /// Result of getting a child.
    #[derive(Debug, Clone)]
    pub struct GetChildResult {
        pub child: Option<DomainChild>,
    }

    /// Result of getting active child.
    #[derive(Debug, Clone)]
    pub struct GetActiveChildResult {
        pub active_child: ActiveChild,
    }

    /// Result of listing children.
    #[derive(Debug, Clone)]
    pub struct ListChildrenResult {
        pub children: Vec<DomainChild>,
    }

    /// Result of setting active child.
    #[derive(Debug, Clone)]
    pub struct SetActiveChildResult {
        pub child: DomainChild,
    }

    /// Result of deleting a child.
    #[derive(Debug, Clone)]
    pub struct DeleteChildResult {
        pub success_message: String,
    }
}

pub mod parental_control {
    /// Input for validating parental control answer.
    #[derive(Debug, Clone)]
    pub struct ValidateParentalControlCommand {
        pub answer: String,
    }

    /// Result of validating parental control answer.
    #[derive(Debug, Clone)]
    pub struct ValidateParentalControlResult {
        pub success: bool,
        pub message: String,
    }
} 