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
        // For now, set a placeholder balance
        // TODO: Implement actual balance calculation
        self.current_balance = 42.50;
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
                
                // Update balance from the most recent transaction
                if let Some(latest_transaction) = self.calendar_transactions.first() {
                    self.current_balance = latest_transaction.balance;
                }
            }
            Err(e) => {
                warn!("❌ Failed to load transactions: {}", e);
                self.error_message = Some(format!("Failed to load transactions: {}", e));
                self.calendar_transactions = Vec::new();
            }
        }
    }
} 