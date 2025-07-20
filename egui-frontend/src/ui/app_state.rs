//! # App State Module
//!
//! This module defines the central application state structure and initialization logic
//! for the allowance tracker app.
//!
//! ## Key Types:
//! - `MainTab` - Enum defining available tabs (Calendar, Table)
//! - `AllowanceTrackerApp` - Main application state struct
//!
//! ## Key Functions:
//! - `new()` - Initialize new app instance with backend connection
//! - `clear_messages()` - Clear success/error messages
//!
//! ## Purpose:
//! This module serves as the central state management for the entire application,
//! using a modular architecture with separate state modules for different concerns.
//!
//! ## State Management:
//! The AllowanceTrackerApp struct composes multiple focused state modules:
//! - CoreAppState: Backend, child, balance, tab
//! - UIState: Loading, messages
//! - CalendarState: Calendar navigation, transactions
//! - ModalState: Modal visibility and flow
//! - FormState: Form inputs and validation
//! - InteractionState: User selections, dropdowns

use log::info;
use chrono::{Datelike, TimeZone};
use shared::*;
use crate::backend::Backend;

// Import all state modules
use crate::ui::state::*;

// Re-export types from state modules to avoid duplication
pub use crate::ui::state::{MainTab, OverlayType, ParentalControlStage, ProtectedAction, TransactionType};
pub use crate::ui::state::form_state::{MoneyTransactionModalConfig, MoneyTransactionFormState};

/// Main application struct for the egui allowance tracker
/// 
/// This uses a modular architecture with focused state modules for maintainability.
pub struct AllowanceTrackerApp {
    // Modular state architecture
    pub core: CoreAppState,           // Backend, child, balance, tab
    pub ui: UIState,                  // Loading, messages
    pub calendar: CalendarState,      // Calendar navigation, overlays  
    pub modal: ModalState,            // All modal states
    pub form: FormState,              // Form validation, inputs
    pub interaction: InteractionState, // User selections, dropdowns
    pub table: TableState,            // Transaction table pagination
}

impl AllowanceTrackerApp {
    /// Create a new AllowanceTrackerApp with modular architecture
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, anyhow::Error> {
        info!("🚀 Initializing AllowanceTrackerApp with modular architecture");
        
        // Setup custom fonts including Chalkboard
        crate::ui::setup_custom_fonts(&cc.egui_ctx);
        
        // Install image loaders for background support
        egui_extras::install_image_loaders(&cc.egui_ctx);
        
        let backend = crate::backend::Backend::new()?;
        
        let now = chrono::Local::now();
        let _current_month = now.month();
        let _current_year = now.year();
        
        // Initialize modular state components
        let core = CoreAppState::new(backend);
        let ui = UIState::new();
        let calendar = CalendarState::new(); // Uses current date
        let modal = ModalState::new();
        let form = FormState::new();
        let interaction = InteractionState::new();
        let table = TableState::new();
        
        Ok(Self {
            // Modular state
            core,
            ui,
            calendar,
            modal,
            form,
            interaction,
            table,
        })
    }

    // TEMPORARY: Getter methods for backward compatibility
    pub fn backend(&self) -> &Backend {
        &self.core.backend
    }
    
    pub fn current_child(&self) -> &Option<Child> {
        &self.core.current_child
    }
    
    pub fn current_balance(&self) -> f64 {
        self.core.current_balance
    }
    
    pub fn current_tab(&self) -> MainTab {
        self.core.current_tab
    }
    
    // TEMPORARY: Setter methods for state synchronization
    pub fn set_current_tab(&mut self, tab: MainTab) {
        self.core.current_tab = tab;
    }
    
    pub fn set_loading(&mut self, loading: bool) {
        self.ui.loading = loading;
    }

    /// Start parental control challenge for a specific action
    pub fn start_parental_control_challenge(&mut self, action: ProtectedAction) {
        log::info!("🔒 Starting parental control challenge for: {:?}", action);
        self.modal.pending_protected_action = Some(action);
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
        self.modal.show_parental_control_modal = true;
    }

    /// Cancel parental control challenge
    pub fn cancel_parental_control_challenge(&mut self) {
        log::info!("🔒 Cancelling parental control challenge");
        self.modal.show_parental_control_modal = false;
        self.modal.pending_protected_action = None;
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
    }

    /// Reset parental control state to question 1
    pub fn reset_parental_control(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
    }

    /// Advance to question 2
    pub fn advance_to_question_2(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question2;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
    }

    /// Mark authentication as successful
    pub fn authenticate_parental_control(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
        
        // Sync compatibility fields
        // self.selected_month = self.calendar.selected_month;
        // self.selected_year = self.calendar.selected_year;
        // self.calendar_loading = self.calendar.calendar_loading;
        
        // Reload calendar data for the new month
        self.load_calendar_data();
        log::info!("📅 Navigated to previous month: {}/{}", self.calendar.selected_month, self.calendar.selected_year);
    }

    /// Navigate to the next month
    pub fn navigate_to_next_month(&mut self) {
        self.calendar.navigate_to_next_month();
        
        // Sync compatibility fields
        // self.selected_month = self.calendar.selected_month;
        // self.selected_year = self.calendar.selected_year;
        // self.calendar_loading = self.calendar.calendar_loading;
        
        // Reload calendar data for the new month
        self.load_calendar_data();
        log::info!("📅 Navigated to next month: {}/{}", self.calendar.selected_month, self.calendar.selected_year);
    }

    /// Get the current month name as a string
    pub fn get_current_month_name(&self) -> String {
        self.calendar.get_current_month_name()
    }

    /// Clear any error or success messages
    pub fn clear_messages(&mut self) {
        self.ui.clear_messages();
        
        // Sync compatibility fields
        // self.error_message = self.ui.error_message.clone(); // Removed
        // self.success_message = self.ui.success_message.clone(); // Removed
    }

    /// Submit answer for parental control validation
    pub fn submit_parental_control_answer(&mut self) {
        // Validate input
        if self.modal.parental_control_input.trim().is_empty() {
            self.modal.parental_control_error = Some("Please enter an answer".to_string());
            return;
        }
        
        // Set loading state and clear errors
        self.modal.parental_control_loading = true;
        self.modal.parental_control_error = None;
        
        // Create command for backend validation
        let command = crate::backend::domain::commands::parental_control::ValidateParentalControlCommand {
            answer: self.modal.parental_control_input.clone(),
        };
        
        // Call backend service
        match self.backend().parental_control_service.validate_answer(command) {
            Ok(result) => {
                self.modal.parental_control_loading = false;
                
                if result.success {
                    log::info!("✅ Parental control authentication successful");
                    self.modal.parental_control_stage = ParentalControlStage::Authenticated;
                    
                    // Execute the pending action
                    if let Some(action) = self.modal.pending_protected_action {
                        self.execute_protected_action(action);
                    }
                    
                    // Close modal after brief success display
                    self.modal.show_parental_control_modal = false;
                    self.ui.success_message = Some("Access granted!".to_string());
                } else {
                    log::info!("❌ Parental control validation failed");
                    self.modal.parental_control_error = Some(result.message);
                    self.modal.parental_control_input.clear();
                }
            }
            Err(e) => {
                self.modal.parental_control_loading = false;
                log::error!("🚨 Parental control validation error: {}", e);
                self.modal.parental_control_error = Some("Validation failed. Please try again.".to_string());
            }
        }
    }
    
    /// Execute the action after successful authentication
    fn execute_protected_action(&mut self, action: ProtectedAction) {
        match action {
            ProtectedAction::DeleteTransactions => {
                log::info!("🗑️ Executing delete transactions action");
                self.enter_transaction_selection_mode();
            }
        }
        
        self.modal.pending_protected_action = None;
    }
    
    // ====================
    // TRANSACTION SELECTION METHODS
    // ====================
    
    /// Enter transaction selection mode for deletion
    pub fn enter_transaction_selection_mode(&mut self) {
        log::info!("🎯 Entering transaction selection mode");
        self.interaction.transaction_selection_mode = true;
        self.interaction.selected_transaction_ids.clear();
        
        // TEMPORARY: Sync compatibility fields
        // self.transaction_selection_mode = true;
        // self.selected_transaction_ids.clear();
        
        self.ui.success_message = Some("Select transactions to delete by clicking checkboxes. Click trash button when ready.".to_string());
    }
    
    /// Exit transaction selection mode without deleting
    pub fn exit_transaction_selection_mode(&mut self) {
        log::info!("🚫 Exiting transaction selection mode");
        self.interaction.transaction_selection_mode = false;
        self.interaction.selected_transaction_ids.clear();
        
        // TEMPORARY: Sync compatibility fields
        // self.transaction_selection_mode = false;
        // self.selected_transaction_ids.clear();
        
        self.clear_messages();
    }
    
    /// Toggle selection of a transaction
    pub fn toggle_transaction_selection(&mut self, transaction_id: &str) {
        if self.interaction.selected_transaction_ids.contains(transaction_id) {
            log::info!("➖ Deselecting transaction: {}", transaction_id);
            self.interaction.selected_transaction_ids.remove(transaction_id);
            // self.selected_transaction_ids.remove(transaction_id); // Sync compatibility field
        } else {
            log::info!("✅ Selecting transaction: {}", transaction_id);
            self.interaction.selected_transaction_ids.insert(transaction_id.to_string());
            // self.selected_transaction_ids.insert(transaction_id.to_string()); // Sync compatibility field
        }
    }
    
    /// Check if a transaction is selected
    pub fn is_transaction_selected(&self, transaction_id: &str) -> bool {
        self.interaction.selected_transaction_ids.contains(transaction_id)
    }
    
    /// Get count of selected transactions
    pub fn selected_transaction_count(&self) -> usize {
        self.interaction.selected_transaction_ids.len()
    }
    
    /// Clear all selected transactions
    pub fn clear_transaction_selection(&mut self) {
        log::info!("🧹 Clearing all transaction selections");
        self.interaction.selected_transaction_ids.clear();
        // self.selected_transaction_ids.clear(); // Sync compatibility field
    }
    
    /// Check if any transactions are selected
    pub fn has_selected_transactions(&self) -> bool {
        !self.interaction.selected_transaction_ids.is_empty()
    }
    
    // ====================
    // ADD MONEY FORM VALIDATION METHODS
    // ====================
    
    /// Validate the add money form and update validation state
    pub fn validate_add_money_form(&mut self) {
        self.form.add_money_description_error = None;
        self.form.add_money_amount_error = None;
        
        // Validate description
        let description = self.form.add_money_description.trim();
        if description.is_empty() {
            self.form.add_money_description_error = Some("Description is required".to_string());
        } else if description.len() > 70 {
            self.form.add_money_description_error = Some(format!("Description too long ({}/70 characters)", description.len()));
        }
        
        // Validate amount
        let amount_input = self.form.add_money_amount.trim();
        if amount_input.is_empty() {
            // Don't show "Amount is required" error immediately - let the grayed button be sufficient
            self.form.add_money_amount_error = None;
        } else {
            // Clean and parse amount
            match self.clean_and_parse_amount(amount_input) {
                Ok(amount) => {
                    if amount <= 0.0 {
                        self.form.add_money_amount_error = Some("Amount must be positive".to_string());
                    } else if amount > 1_000_000.0 {
                        self.form.add_money_amount_error = Some("Amount too large (max $1,000,000)".to_string());
                    } else if amount < 0.01 {
                        self.form.add_money_amount_error = Some("Amount too small (min $0.01)".to_string());
                    } else if self.has_too_many_decimal_places(amount) {
                        self.form.add_money_amount_error = Some("Maximum 2 decimal places allowed".to_string());
                    }
                }
                Err(error) => {
                    self.form.add_money_amount_error = Some(error);
                }
            }
        }
        
        // Update overall validation state
        self.form.add_money_is_valid = self.form.add_money_description_error.is_none() && self.form.add_money_amount_error.is_none();
    }
    
    /// Clean and parse amount input string (similar to MoneyManagementService)
    fn clean_and_parse_amount(&self, amount_input: &str) -> Result<f64, String> {
        // Clean the input - remove dollar signs, spaces, commas
        let cleaned = amount_input
            .trim()
            .replace("$", "")
            .replace(",", "")
            .replace(" ", "");

        // Handle empty input after cleaning
        if cleaned.is_empty() {
            return Err("Amount cannot be empty".to_string());
        }

        // Try to parse as float
        cleaned.parse::<f64>()
            .map_err(|_| "Invalid number format".to_string())
    }
    
    /// Check if amount has too many decimal places
    fn has_too_many_decimal_places(&self, _amount: f64) -> bool {
        // Check the original input string instead of the parsed float
        let input = self.form.add_money_amount.trim();
        if let Some(decimal_pos) = input.find('.') {
            let decimal_part = &input[decimal_pos + 1..];
            // Reject if more than 2 decimal places
            if decimal_part.len() > 2 {
                return true;
            }
        }
        false
    }
    
    /// Format amount for currency display ($XX.XX)
    pub fn format_currency_amount(&self, amount: f64) -> String {
        format!("${:.2}", amount)
    }
    
    /// Clear add money form and validation state
    pub fn clear_add_money_form(&mut self) {
        self.form.add_money_description.clear();
        self.form.add_money_amount.clear();
        self.form.add_money_description_error = None;
        self.form.add_money_amount_error = None;
        self.form.add_money_is_valid = true;
    }
    
    /// Auto-format amount field as user types (adds $ and proper decimal formatting)
    pub fn auto_format_amount_field(&mut self) {
        let input = self.form.add_money_amount.clone();
        
        // Only auto-format if the input looks like a valid number
        if let Ok(amount) = self.clean_and_parse_amount(&input) {
            // Only format if the amount is reasonable and has <= 2 decimal places
            if amount > 0.0 && amount < 1_000_000.0 && !self.has_too_many_decimal_places(amount) {
                // Format as $XX.XX but only if user isn't currently typing
                if !input.ends_with('.') && !input.ends_with('0') {
                    self.form.add_money_amount = format!("{:.2}", amount);
                }
            }
        }
    }
    
    // ====================
    // GENERIC MONEY TRANSACTION FORM VALIDATION METHODS
    // ====================
    
    /// Validate a generic money transaction form and update its validation state
    pub fn validate_money_transaction_form(&self, form_state: &mut MoneyTransactionFormState, config: &MoneyTransactionModalConfig) {
        form_state.description_error = None;
        form_state.amount_error = None;
        
        // Validate description
        let description = form_state.description.trim();
        if description.is_empty() {
            form_state.description_error = Some("Description is required".to_string());
        } else if description.len() > config.max_description_length {
            form_state.description_error = Some(format!("Description too long ({}/{} characters)", description.len(), config.max_description_length));
        }
        
        // Validate amount
        let amount_input = form_state.amount.trim();
        if amount_input.is_empty() {
            // Don't show "Amount is required" error immediately - let the grayed button be sufficient
            form_state.amount_error = None;
        } else {
            // Clean and parse amount
            match self.clean_and_parse_amount(amount_input) {
                Ok(amount) => {
                    if amount <= 0.0 {
                        form_state.amount_error = Some("Amount must be positive".to_string());
                    } else if amount > 1_000_000.0 {
                        form_state.amount_error = Some("Amount too large (max $1,000,000)".to_string());
                    } else if amount < 0.01 {
                        form_state.amount_error = Some("Amount too small (min $0.01)".to_string());
                    } else if self.has_too_many_decimal_places_generic(amount, &form_state.amount) {
                        form_state.amount_error = Some("Maximum 2 decimal places allowed".to_string());
                    }
                }
                Err(error) => {
                    form_state.amount_error = Some(error);
                }
            }
        }
        
        // Update overall validation state
        form_state.is_valid = form_state.description_error.is_none() && form_state.amount_error.is_none();
    }
    
    /// Check if amount has too many decimal places for generic form (takes amount input string)
    fn has_too_many_decimal_places_generic(&self, _amount: f64, amount_input: &str) -> bool {
        // Check the original input string instead of the parsed float
        let input = amount_input.trim();
        if let Some(decimal_pos) = input.find('.') {
            let decimal_part = &input[decimal_pos + 1..];
            // Reject if more than 2 decimal places
            if decimal_part.len() > 2 {
                return true;
            }
        }
        false
    }
    
    // ====================
    // BACKEND INTEGRATION METHODS
    // ====================
    
    /// Submit income transaction to backend
    pub fn submit_income_transaction(&mut self) -> bool {
        use crate::backend::domain::money_management::MoneyManagementService;
        
        log::info!("💰 Submitting income transaction - Description: '{}', Amount: '{}'", 
                  self.form.income_form_state.description, self.form.income_form_state.amount);
        
        // Parse amount from form
        let amount = match self.clean_and_parse_amount(&self.form.income_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("❌ Failed to parse amount: {}", error);
                self.ui.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        
        // Create AddMoneyRequest with selected date from calendar as proper DateTime object
        let date_time = self.calendar.selected_day.map(|date| {
            // Convert NaiveDate to DateTime at noon Eastern Time
            let naive_datetime = date.and_hms_opt(12, 0, 0).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::AddMoneyRequest {
            description: self.form.income_form_state.description.trim().to_string(),
            amount,
            date: date_time,
        };
        
        // Create MoneyManagementService instance
        let money_service = MoneyManagementService::new();
        
        // Call backend with references to other services
        match money_service.add_money_complete(
            request,
            &self.backend().child_service,
            &self.backend().transaction_service,
        ) {
            Ok(response) => {
                log::info!("✅ Income transaction successful: {}", response.success_message);
                self.ui.success_message = Some(response.success_message);
                self.core.current_balance = response.new_balance;
                
                // TEMPORARY: Sync compatibility field  
                // self.current_balance = response.new_balance;
                
                // Refresh calendar data to show the new transaction
                self.load_calendar_data();
                
                true
            }
            Err(error) => {
                log::error!("❌ Income transaction failed: {}", error);
                self.ui.error_message = Some(format!("Failed to add money: {}", error));
                false
            }
        }
    }

    /// Submit expense transaction to backend using spend_money_complete
    pub fn submit_expense_transaction(&mut self) -> bool {
        use crate::backend::domain::money_management::MoneyManagementService;
        
        log::info!("💸 Submitting expense transaction - Description: '{}', Amount: '{}'", 
                  self.form.expense_form_state.description, self.form.expense_form_state.amount);
        
        // Parse amount from form
        let amount = match self.clean_and_parse_amount(&self.form.expense_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("❌ Failed to parse amount: {}", error);
                self.ui.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        
        // Create SpendMoneyRequest with selected date from calendar as proper DateTime object
        let date_time = self.calendar.selected_day.map(|date| {
            // Convert NaiveDate to DateTime at noon Eastern Time
            let naive_datetime = date.and_hms_opt(12, 0, 0).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::SpendMoneyRequest {
            description: self.form.expense_form_state.description.trim().to_string(),
            amount, // User enters positive amount, backend converts to negative
            date: date_time,
        };
        
        // Create MoneyManagementService instance
        let money_service = MoneyManagementService::new();
        
        // Call backend with references to other services
        match money_service.spend_money_complete(
            request,
            &self.backend().child_service,
            &self.backend().transaction_service,
        ) {
            Ok(response) => {
                log::info!("✅ Expense transaction successful: {}", response.success_message);
                self.ui.success_message = Some(response.success_message);
                self.core.current_balance = response.new_balance;
                
                // TEMPORARY: Sync compatibility field  
                // self.current_balance = response.new_balance;
                
                // Refresh calendar data to show the new transaction
                self.load_calendar_data();
                
                true
            }
            Err(error) => {
                log::error!("❌ Expense transaction failed: {}", error);
                self.ui.error_message = Some(format!("Failed to spend money: {}", error));
                false
            }
        }
    }
} 