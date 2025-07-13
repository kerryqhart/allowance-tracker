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
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::mappers::{to_dto, TransactionMapper};
use crate::backend::domain::commands::transactions::TransactionListQuery;

impl AllowanceTrackerApp {
    /// Load initial data
    pub fn load_initial_data(&mut self) {
        info!("📊 Loading initial data");
        
        // Load active child
        match self.backend.child_service.get_active_child() {
            Ok(response) => {
                if let Some(child) = response.active_child.child {
                    self.current_child = Some(to_dto(child));
                    self.load_balance();
                    self.load_calendar_data();
                }
                self.loading = false;
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to load active child: {}", e));
                self.loading = false;
            }
        }
    }
    
    /// Load current balance
    pub fn load_balance(&mut self) {
        if let Some(child) = &self.current_child {
            info!("💰 Loading balance for child: {} (ID: {})", child.name, child.id);
            
            // Get the most recent transaction to get the current balance
            let query = TransactionListQuery {
                after: None,
                limit: Some(1), // Just get the most recent transaction
                start_date: None,
                end_date: None,
            };
            
            info!("💰 DEBUG: About to call list_transactions_domain with query: {:?}", query);
            
            match self.backend.transaction_service.list_transactions_domain(query) {
                Ok(result) => {
                    info!("💰 DEBUG: list_transactions_domain returned {} transactions", result.transactions.len());
                    
                    if let Some(latest_transaction) = result.transactions.first() {
                        self.current_balance = latest_transaction.balance;
                        info!("💰 SUCCESS: Found latest transaction ID={}, balance=${:.2}", 
                             latest_transaction.id, self.current_balance);
                        info!("💰 DEBUG: Transaction details - date={}, description={}, amount={}", 
                             latest_transaction.date, latest_transaction.description, latest_transaction.amount);
                    } else {
                        // No transactions, balance is 0
                        self.current_balance = 0.0;
                        info!("💰 WARNING: No transactions found for child {}, balance is $0.00", child.name);
                        
                        // Additional debug: Let's try to load ALL transactions to see what's happening
                        let debug_query = TransactionListQuery {
                            after: None,
                            limit: Some(100), // Get more transactions for debugging
                            start_date: None,
                            end_date: None,
                        };
                        
                        match self.backend.transaction_service.list_transactions_domain(debug_query) {
                            Ok(debug_result) => {
                                info!("💰 DEBUG: Extended query returned {} transactions", debug_result.transactions.len());
                                for (i, tx) in debug_result.transactions.iter().take(5).enumerate() {
                                    info!("💰 DEBUG: Transaction {}: id={}, date={}, amount={}, balance={}", 
                                         i+1, tx.id, tx.date, tx.amount, tx.balance);
                                }
                            }
                            Err(e) => {
                                warn!("💰 DEBUG: Extended query also failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to load balance for child {}: {}", child.name, e);
                    self.error_message = Some(format!("Failed to load balance: {}", e));
                    self.current_balance = 0.0;
                }
            }
        } else {
            info!("💰 DEBUG: No current child set, balance = $0.00");
            self.current_balance = 0.0;
        }
    }
    
    /// Load calendar data
    pub fn load_calendar_data(&mut self) {
        info!("📅 Loading calendar data for {}/{}", self.selected_month, self.selected_year);
        
        // Load recent transactions for the current month
        let query = TransactionListQuery {
            after: None,
            limit: Some(20), // Load last 20 transactions
            start_date: None,
            end_date: None,
        };
        
        match self.backend.transaction_service.list_transactions_domain(query) {
            Ok(result) => {
                info!("📊 Successfully loaded {} transactions", result.transactions.len());
                
                // Convert domain transactions to DTOs
                self.calendar_transactions = result.transactions
                    .into_iter()
                    .map(TransactionMapper::to_dto)
                    .collect();
            }
            Err(e) => {
                warn!("❌ Failed to load transactions: {}", e);
                self.error_message = Some(format!("Failed to load transactions: {}", e));
                self.calendar_transactions = Vec::new();
            }
        }
    }
} 