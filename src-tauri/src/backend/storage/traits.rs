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
} 