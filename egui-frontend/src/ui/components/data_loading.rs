//! # Data Loading Module
//!
//! This module handles all data loading operations for the allowance tracker app,
//! interfacing with the backend to fetch and update application state.
//!
//! ## Key Functions:
//! - `load_initial_data()` - Load all required data on app startup
//! - `load_balance()` - Fetch current balance for selected child
//! - `load_calendar_data()` - Load transaction data for calendar view
//!
//! ## Purpose:
//! This module centralizes all data loading logic, ensuring consistent error handling
//! and state management. It serves as the bridge between the UI and the backend,
//! handling:
//! - Initial app data loading
//! - Balance queries and updates
//! - Transaction data for calendar display
//! - Error handling and user feedback
//!
//! ## Data Flow:
//! 1. UI triggers data loading request
//! 2. Module calls appropriate backend service
//! 3. Maps backend responses to UI state
//! 4. Updates application state with loaded data
//! 5. Handles any errors and provides user feedback
//!
//! This module ensures the UI always has the most current data available.

use log::{info, warn};
use chrono::Datelike;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::mappers::to_dto;
use crate::backend::domain::commands::transactions::TransactionListQuery;
use shared::{Transaction, TransactionType};

impl AllowanceTrackerApp {
    /// Refresh all data for current child - common method used when switching children
    /// This ensures all views (calendar, table, chart, goals) are updated consistently
    pub fn refresh_all_data_for_current_child(&mut self) {
        info!("🔄 Refreshing all data for current child");
        
        self.load_balance();
        self.load_calendar_data();
        self.reset_table_for_new_child();
        self.load_chart_data();
        self.load_goal_data();
        
        info!("✅ All data refreshed for current child");
    }

    /// Load initial data
    pub fn load_initial_data(&mut self) {
        info!("📊 Loading initial data");
        
        // Load active child
        match self.backend().child_service.get_active_child() {
            Ok(response) => {
                if let Some(child) = response.active_child.child {
                    self.core.current_child = Some(to_dto(child));
                    self.load_balance();
                    self.load_calendar_data();
                    self.reset_table_for_new_child(); // Reset table state for initial load
                    self.load_chart_data(); // Refresh chart for initial load
                    self.load_goal_data(); // Load goal data for initial load
                }
                self.ui.loading = false;
            }
            Err(e) => {
                self.ui.error_message = Some(format!("Failed to load active child: {}", e));
                self.ui.loading = false;
            }
        }
    }
    
    /// Reset table state when switching to a new child
    pub fn reset_table_for_new_child(&mut self) {
        log::info!("📋 Resetting table state for new child");
        
        // Clear all table state
        self.table.reset();
        
        // Start loading transactions for the new child
        self.load_initial_table_transactions();
    }
    
    /// Load current balance
    pub fn load_balance(&mut self) {
        let current_child = self.get_current_child_from_backend().clone();
        if let Some(child) = &current_child {
            info!("💰 Loading balance for child: {} (ID: {})", child.name, child.id);
            
            // Get the most recent transaction to get the current balance
            let query = TransactionListQuery {
                after: None,
                limit: Some(1), // Just get the most recent transaction
                start_date: None,
                end_date: None,
            };
            
            info!("💰 DEBUG: About to call list_transactions_domain with query: {:?}", query);
            
            match self.backend().transaction_service.as_ref().list_transactions_domain(query) {
                Ok(result) => {
                    info!("💰 DEBUG: list_transactions_domain returned {} transactions", result.transactions.len());
                    
                    if let Some(latest_transaction) = result.transactions.first() {
                        self.core.current_balance = latest_transaction.balance;
                        log::info!("📊 Updated balance from latest transaction {}: ${:.2}", 
                                  latest_transaction.id, self.core.current_balance);
                    } else {
                        // No transactions found - set balance to 0
                        self.core.current_balance = 0.0;
                        log::info!("📊 No transactions found, setting balance to $0.00");
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to load balance for child {}: {}", child.name, e);
                    self.ui.error_message = Some(format!("Failed to load balance: {}", e));
                    self.core.current_balance = 0.0;
                }
            }
        } else {
            log::warn!("⚠️ No current child - unable to update balance");
            // Clear balance if no child is selected
            self.core.current_balance = 0.0;
        }
        
        log::info!("📊 Balance update complete - Final balance: ${:.2}", self.core.current_balance);
    }
    
    /// Load calendar data for the selected month/year
    pub fn load_calendar_data(&mut self) {
        log::info!("📅 Loading calendar data for {}/{}", self.calendar.selected_month, self.calendar.selected_year);
        
        // Calculate the start and end dates for the selected month
        let start_date = match chrono::NaiveDate::from_ymd_opt(self.calendar.selected_year, self.calendar.selected_month, 1) {
            Some(date) => date,
            None => {
                log::error!("❌ Failed to create start date for {}/{}", self.calendar.selected_month, self.calendar.selected_year);
                self.ui.error_message = Some("Invalid date".to_string());
                return;
            }
        };
        
        let end_date = if self.calendar.selected_month == 12 {
            chrono::NaiveDate::from_ymd_opt(self.calendar.selected_year + 1, 1, 1).unwrap()
        } else {
            chrono::NaiveDate::from_ymd_opt(self.calendar.selected_year, self.calendar.selected_month + 1, 1).unwrap()
        } - chrono::Duration::days(1);
        
        log::info!("🗓️  Querying transactions from {} to {}", start_date, end_date);
        
        // Use calendar service instead of transaction service directly
        // This ensures proper cross-month balance forwarding
        match self.backend().calendar_service.get_calendar_month_with_transactions(
            self.calendar.selected_month,
            self.calendar.selected_year as u32,
            &self.backend().transaction_service,
        ) {
            Ok(calendar_month) => {
                log::info!("📊 Successfully loaded calendar month with {} days for {}/{}", 
                          calendar_month.days.len(), self.calendar.selected_month, self.calendar.selected_year);
                
                // DEBUG: Log July 21st specifically
                if self.calendar.selected_month == 7 && self.calendar.selected_year == 2025 {
                    if let Some(july_21) = calendar_month.days.iter().find(|d| d.day == 21) {
                        log::info!("🔍 FRONTEND DEBUG: July 21st from backend - balance: ${:.2}, transactions: {}", 
                                  july_21.balance, july_21.transactions.len());
                        for (i, tx) in july_21.transactions.iter().enumerate() {
                            log::info!("🔍 FRONTEND DEBUG: July 21st transaction {}: {} at {} = balance ${:.2}", 
                                      i + 1, tx.description, tx.date.format("%H:%M:%S"), tx.balance);
                        }
                    }
                }
                
                // Extract transactions from all calendar days (for backward compatibility)
                let mut all_transactions = Vec::new();
                for day in &calendar_month.days {
                    all_transactions.extend(day.transactions.clone());
                }
                
                // Store converted transactions in modular calendar state
                self.calendar.calendar_transactions = all_transactions.clone();
                
                // Store the calendar month data in modular state
                self.calendar.calendar_month = Some(calendar_month.clone());
                
                // TEMPORARY: Sync compatibility field
                // self.calendar_transactions = all_transactions; // Removed
                
                log::info!("🔄 Converted to {} DTO transactions", self.calendar.calendar_transactions.len());
                
                // Debug first few transactions to verify conversion
                for (i, transaction) in self.calendar.calendar_transactions.iter().enumerate() {
                    log::debug!("📝 Transaction {}: {} on {} (amount: ${})", 
                               i + 1, transaction.description, transaction.date, transaction.amount);
                }
                
                // Specifically check for June transactions
                let june_transactions: Vec<_> = self.calendar.calendar_transactions.iter()
                    .filter(|t| {
                        let transaction_date = t.date.naive_local().date();
                        transaction_date.month() == 6 && transaction_date.year() == self.calendar.selected_year
                    })
                    .collect();
                
                log::info!("🗓️  Found {} June {} transactions", june_transactions.len(), self.calendar.selected_year);
                for transaction in june_transactions {
                    log::info!("  - June transaction: {} on {} (amount: ${})", 
                              transaction.description, transaction.date, transaction.amount);
                }
            }
            Err(e) => {
                log::error!("❌ Failed to load transactions: {}", e);
                self.ui.error_message = Some(format!("Failed to load transactions: {}", e));
                
                // Clear modular fields
                self.calendar.calendar_transactions = Vec::new();
                self.calendar.calendar_month = None;
                
                // TEMPORARY: Also clear compatibility fields
                // self.calendar_transactions = Vec::new(); // Removed
                self.calendar.calendar_month = None;
            }
        }
    }
    
    /// Load initial transactions for the table view
    pub fn load_initial_table_transactions(&mut self) {
        log::info!("📋 Loading initial transactions for table view");
        
        // Reset table state for fresh load
        self.table.reset();
        
        // Start loading first batch
        self.load_more_table_transactions();
    }
    
    /// Load more transactions for infinite scroll
    pub fn load_more_table_transactions(&mut self) {
        let current_child = self.get_current_child_from_backend().clone();
        if let Some(child) = &current_child {
            // Check if we can load more
            if !self.table.can_load_more() {
                log::info!("📋 Cannot load more transactions: already loading or no more available");
                return;
            }
            
            log::info!("📋 Loading more transactions for child: {} (cursor: {:?})", 
                      child.name, self.table.next_cursor);
            
            // Mark as loading
            self.table.start_loading();
            
            let query = TransactionListQuery {
                after: self.table.next_cursor.clone(),
                limit: Some(self.table.page_size),
                start_date: None,
                end_date: None,
            };
            
            log::info!("📋 Making pagination request with query: {:?}", query);
            
            match self.backend().transaction_service.as_ref().list_transactions_domain(query) {
                Ok(result) => {
                    log::info!("📋 Successfully loaded {} more transactions (has_more: {})", 
                              result.transactions.len(), result.pagination.has_more);
                    
                    // Convert domain transactions to DTOs
                    let dto_transactions: Vec<Transaction> = result
                        .transactions
                        .into_iter()
                        .map(|domain_tx| crate::ui::mappers::TransactionMapper::to_dto(domain_tx))
                        .filter(|t| t.transaction_type != TransactionType::FutureAllowance) // Filter out future allowances
                        .collect();
                    
                    // Add to table state
                    self.table.append_transactions(
                        dto_transactions,
                        result.pagination.has_more,
                        result.pagination.next_cursor,
                    );
                    
                    log::info!("📋 Table now has {} total transactions", self.table.transaction_count());
                }
                Err(e) => {
                    log::error!("❌ Failed to load more transactions: {}", e);
                    self.table.handle_error(format!("Failed to load transactions: {}", e));
                    self.ui.error_message = Some(format!("Failed to load transactions: {}", e));
                }
            }
        } else {
            log::warn!("📋 No active child selected for table transactions");
            self.table.handle_error("No child selected".to_string());
        }
    }
} 