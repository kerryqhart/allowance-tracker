//! # Storage Traits
//!
//! This module defines the storage abstraction traits that allow different
//! storage backends to be used interchangeably in the domain layer.

use anyhow::Result;
use async_trait::async_trait;
use shared::Transaction;

/// Trait defining the interface for transaction storage operations
/// 
/// This trait abstracts away the specific storage implementation details,
/// allowing the domain layer to work with different storage backends
/// (SQL databases, CSV files, etc.) without modification.
#[async_trait]
pub trait TransactionStorage: Send + Sync {
    /// Store a new transaction
    async fn store_transaction(&self, transaction: &Transaction) -> Result<()>;
    
    /// Retrieve a specific transaction by ID
    async fn get_transaction(&self, child_id: &str, transaction_id: &str) -> Result<Option<Transaction>>;
    
    /// List transactions with pagination support
    /// Returns transactions ordered by date descending (most recent first)
    async fn list_transactions(&self, child_id: &str, limit: Option<u32>, after: Option<String>) -> Result<Vec<Transaction>>;
    
    /// List transactions in chronological order with optional date filtering
    /// Returns transactions ordered by date ascending (oldest first)
    async fn list_transactions_chronological(&self, child_id: &str, start_date: Option<String>, end_date: Option<String>) -> Result<Vec<Transaction>>;
    
    /// Update an existing transaction
    async fn update_transaction(&self, transaction: &Transaction) -> Result<()>;
    
    /// Delete a single transaction
    /// Returns true if the transaction was found and deleted, false otherwise
    async fn delete_transaction(&self, child_id: &str, transaction_id: &str) -> Result<bool>;
    
    /// Delete multiple transactions
    /// Returns the number of transactions actually deleted
    async fn delete_transactions(&self, child_id: &str, transaction_ids: &[String]) -> Result<u32>;
    
    /// Get the most recent transaction for a specific child (for calculating next balance)
    async fn get_latest_transaction(&self, child_id: &str) -> Result<Option<Transaction>>;
    
    /// Get all transactions after a specific date (inclusive) for balance recalculation
    /// Returns transactions in chronological order (oldest first)
    async fn get_transactions_after_date(&self, child_id: &str, date: &str) -> Result<Vec<Transaction>>;
    
    /// Get the most recent transaction before a specific date
    /// This is useful for finding the starting balance when inserting backdated transactions
    async fn get_latest_transaction_before_date(&self, child_id: &str, date: &str) -> Result<Option<Transaction>>;
    
    /// Update the balance of a specific transaction
    /// Used during balance recalculation after backdated transactions
    async fn update_transaction_balance(&self, transaction_id: &str, new_balance: f64) -> Result<()>;
    
    /// Update multiple transaction balances atomically
    /// Used for bulk balance recalculation after backdated transactions
    async fn update_transaction_balances(&self, updates: &[(String, f64)]) -> Result<()>;
    
    /// Check if transactions exist by their IDs for a specific child
    async fn check_transactions_exist(&self, child_id: &str, transaction_ids: &[String]) -> Result<Vec<String>>;
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