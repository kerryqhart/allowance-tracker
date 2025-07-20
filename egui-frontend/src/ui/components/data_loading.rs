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

impl AllowanceTrackerApp {
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
                }
                self.ui.loading = false;
            }
            Err(e) => {
                self.ui.error_message = Some(format!("Failed to load active child: {}", e));
                self.ui.loading = false;
            }
        }
    }
    
    /// Load current balance
    pub fn load_balance(&mut self) {
        let current_child = self.current_child().clone();
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
            
            match self.backend().transaction_service.list_transactions_domain(query) {
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
} 