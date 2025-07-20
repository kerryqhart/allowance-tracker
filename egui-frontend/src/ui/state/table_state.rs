//! # Table State Module
//!
//! This module contains all state related to the transaction table view and pagination.
//!
//! ## Responsibilities:
//! - Transaction table pagination state
//! - Infinite scroll loading state
//! - Table filtering and sorting (future)
//! - Table interaction state
//!
//! ## Purpose:
//! This isolates all table-specific state management, making it easier to
//! maintain and test table functionality independently.

use shared::*;

/// Transaction table-specific state for pagination and display
#[derive(Debug)]
pub struct TableState {
    /// All currently loaded and displayed transactions
    pub displayed_transactions: Vec<Transaction>,
    
    /// Whether we are currently loading more transactions
    pub is_loading_more: bool,
    
    /// Whether there are more transactions available to load
    pub has_more_transactions: bool,
    
    /// Cursor for pagination (ID of last loaded transaction)
    pub next_cursor: Option<String>,
    
    /// Total number of transactions loaded so far
    pub total_loaded: usize,
    
    /// Whether the initial load has completed
    pub initial_load_complete: bool,
    
    /// Error message for pagination failures
    pub pagination_error: Option<String>,
    
    /// Page size for pagination requests
    pub page_size: u32,
}

impl TableState {
    /// Create a new TableState with default values
    pub fn new() -> Self {
        Self {
            displayed_transactions: Vec::new(),
            is_loading_more: false,
            has_more_transactions: true,
            next_cursor: None,
            total_loaded: 0,
            initial_load_complete: false,
            pagination_error: None,
            page_size: 50, // Load 50 transactions at a time
        }
    }
    
    /// Reset the table state for a fresh load
    pub fn reset(&mut self) {
        self.displayed_transactions.clear();
        self.is_loading_more = false;
        self.has_more_transactions = true;
        self.next_cursor = None;
        self.total_loaded = 0;
        self.initial_load_complete = false;
        self.pagination_error = None;
    }
    
    /// Add new transactions from a pagination response
    pub fn append_transactions(&mut self, transactions: Vec<Transaction>, has_more: bool, next_cursor: Option<String>) {
        // Filter out duplicates (in case of cursor issues)
        let existing_ids: std::collections::HashSet<String> = 
            self.displayed_transactions.iter().map(|t| t.id.clone()).collect();
        
        let new_transactions: Vec<Transaction> = transactions
            .into_iter()
            .filter(|t| !existing_ids.contains(&t.id))
            .collect();
        
        self.total_loaded += new_transactions.len();
        self.displayed_transactions.extend(new_transactions);
        self.has_more_transactions = has_more;
        self.next_cursor = next_cursor;
        self.is_loading_more = false;
        self.initial_load_complete = true;
        self.pagination_error = None;
    }
    
    /// Mark loading state and clear any previous errors
    pub fn start_loading(&mut self) {
        self.is_loading_more = true;
        self.pagination_error = None;
    }
    
    /// Handle pagination error
    pub fn handle_error(&mut self, error: String) {
        self.is_loading_more = false;
        self.pagination_error = Some(error);
    }
    
    /// Check if we can load more transactions
    pub fn can_load_more(&self) -> bool {
        self.has_more_transactions && !self.is_loading_more
    }
    
    /// Get the number of currently displayed transactions
    pub fn transaction_count(&self) -> usize {
        self.displayed_transactions.len()
    }
} 