//! # Storage Traits
//!
//! This module defines the storage abstraction traits that allow different
//! storage backends to be used interchangeably in the domain layer.

use anyhow::Result;
use async_trait::async_trait;
use crate::backend::domain::models::child::Child as DomainChild;
use crate::backend::domain::models::transaction::Transaction as DomainTransaction;
use crate::backend::domain::models::allowance::AllowanceConfig as DomainAllowanceConfig;
use crate::backend::domain::models::parental_control_attempt::ParentalControlAttempt as DomainParentalControlAttempt;
use crate::backend::domain::models::goal::DomainGoal;

/// Trait defining the interface for transaction storage operations
/// 
/// This trait abstracts away the specific storage implementation details,
/// allowing the domain layer to work with different storage backends
/// (SQL databases, CSV files, etc.) without modification.
#[async_trait]
pub trait TransactionStorage: Send + Sync {
    /// Store a new transaction
    async fn store_transaction(&self, transaction: &DomainTransaction) -> Result<()>;
    
    /// Retrieve a specific transaction by ID
    async fn get_transaction(&self, child_id: &str, transaction_id: &str) -> Result<Option<DomainTransaction>>;
    
    /// List transactions with pagination support
    /// Returns transactions ordered by date descending (most recent first)
    async fn list_transactions(&self, child_id: &str, limit: Option<u32>, after: Option<String>) -> Result<Vec<DomainTransaction>>;
    
    /// List transactions in chronological order with optional date filtering
    /// Returns transactions ordered by date ascending (oldest first)
    async fn list_transactions_chronological(&self, child_id: &str, start_date: Option<String>, end_date: Option<String>) -> Result<Vec<DomainTransaction>>;
    
    /// Update an existing transaction
    async fn update_transaction(&self, transaction: &DomainTransaction) -> Result<()>;
    
    /// Delete a single transaction
    /// Returns true if the transaction was found and deleted, false otherwise
    async fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool>;
    
    /// Delete multiple transactions
    /// Returns the number of transactions actually deleted
    async fn delete_transactions(&self, child_id: &str, transaction_ids: &[String]) -> Result<u32>;
    
    /// Get the most recent transaction for a specific child (for calculating next balance)
    async fn get_latest_transaction(&self, child_id: &str) -> Result<Option<DomainTransaction>>;
    
    /// Get all transactions after a specific date (inclusive) for balance recalculation
    /// Returns transactions in chronological order (oldest first)
    async fn get_transactions_since(&self, child_id: &str, date: &str) -> Result<Vec<DomainTransaction>>;
    
    /// Get the most recent transaction before a specific date
    /// This is useful for finding the starting balance when inserting backdated transactions
    async fn get_latest_transaction_before_date(&self, child_id: &str, date: &str) -> Result<Option<DomainTransaction>>;
    
    /// Update the balance of a specific transaction
    /// Used during balance recalculation after backdated transactions
    async fn update_transaction_balance(&self, transaction_id: &str, new_balance: f64) -> Result<()>;
    
    /// Update multiple transaction balances atomically
    /// Used for bulk balance recalculation after backdated transactions
    async fn update_transaction_balances(&self, updates: &[(String, f64)]) -> Result<()>;
    
    /// Check if transactions exist by their IDs for a specific child
    async fn check_transactions_exist(&self, child_id: &str, transaction_ids: &[String]) -> Result<Vec<String>>;
}

/// Trait defining the interface for child storage operations
#[async_trait]
pub trait ChildStorage: Send + Sync {
    /// Store a new child
    async fn store_child(&self, child: &DomainChild) -> Result<()>;
    
    /// Retrieve a specific child by ID
    async fn get_child(&self, child_id: &str) -> Result<Option<DomainChild>>;
    
    /// List all children ordered by name
    async fn list_children(&self) -> Result<Vec<DomainChild>>;
    
    /// Update an existing child
    async fn update_child(&self, child: &DomainChild) -> Result<()>;
    
    /// Delete a child by ID
    async fn delete_child(&self, child_id: &str) -> Result<()>;
    
    /// Get the currently active child ID
    async fn get_active_child(&self) -> Result<Option<String>>;
    
    /// Set the currently active child
    async fn set_active_child(&self, child_id: &str) -> Result<()>;
}

/// Trait defining the interface for allowance config storage operations
#[async_trait]
pub trait AllowanceStorage: Send + Sync {
    /// Store a new allowance config for a child
    async fn store_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()>;
    
    /// Retrieve allowance config for a specific child by child ID
    async fn get_allowance_config(&self, child_id: &str) -> Result<Option<DomainAllowanceConfig>>;
    
    /// Update an existing allowance config for a child
    async fn update_allowance_config(&self, config: &DomainAllowanceConfig) -> Result<()>;
    
    /// Delete allowance config for a specific child
    async fn delete_allowance_config(&self, child_id: &str) -> Result<bool>;
    
    /// List all allowance configs (for admin purposes)
    async fn list_allowance_configs(&self) -> Result<Vec<DomainAllowanceConfig>>;
}

/// Trait defining the interface for parental control attempt storage operations
#[async_trait]
pub trait ParentalControlStorage: Send + Sync {
    /// Record a parental control validation attempt for a specific child
    async fn record_parental_control_attempt(&self, child_id: &str, attempted_value: &str, success: bool) -> Result<i64>;
    
    /// Get parental control attempts for a specific child with optional limit
    async fn get_parental_control_attempts(&self, child_id: &str, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>>;
    
    /// Get all parental control attempts across all children (for admin/debugging)
    async fn get_all_parental_control_attempts(&self, limit: Option<u32>) -> Result<Vec<DomainParentalControlAttempt>>;
}

/// Trait defining the interface for goal storage operations
/// 
/// This trait abstracts away the specific storage implementation details,
/// allowing the domain layer to work with different storage backends
/// (SQL databases, CSV files, etc.) without modification.
#[async_trait]
pub trait GoalStorage: Send + Sync {
    /// Store a new goal (append-only - creates new record)
    async fn store_goal(&self, goal: &DomainGoal) -> Result<()>;
    
    /// Get the current active goal for a specific child
    async fn get_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>>;
    
    /// List all goals for a specific child (with optional limit)
    /// Returns goals ordered by created_at descending (most recent first)
    async fn list_goals(&self, child_id: &str, limit: Option<u32>) -> Result<Vec<DomainGoal>>;
    
    /// Update an existing goal by creating a new record with updated fields
    /// This maintains the append-only history while updating the current state
    async fn update_goal(&self, goal: &DomainGoal) -> Result<()>;
    
    /// Cancel the current active goal by setting its state to Cancelled
    async fn cancel_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>>;
    
    /// Mark the current active goal as completed
    async fn complete_current_goal(&self, child_id: &str) -> Result<Option<DomainGoal>>;
    
    /// Check if a child has an active goal
    async fn has_active_goal(&self, child_id: &str) -> Result<bool>;
}

/// Trait defining the interface for storage connections
/// 
/// This trait abstracts away the specific connection type (database, CSV, etc.)
/// and provides factory methods for creating repositories. This allows the domain
/// layer to work with any storage backend without knowing the implementation details.
pub trait Connection: Send + Sync + Clone {
    /// The type of TransactionStorage this connection creates
    type TransactionRepository: TransactionStorage;
    
    /// Create a new transaction repository for this connection
    fn create_transaction_repository(&self) -> Self::TransactionRepository;
} 