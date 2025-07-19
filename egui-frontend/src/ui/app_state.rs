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
//! containing:
//! - Backend connection and data access
//! - Current user context (selected child, balance)
//! - UI state (loading, messages, current tab)
//! - Calendar state (selected month/year, transactions)
//! - Modal visibility states
//! - Form input states
//!
//! ## State Management:
//! The AllowanceTrackerApp struct holds all application state in a single location,
//! making it easy to manage and pass between different UI components. This follows
//! the single source of truth principle for state management.

use log::info;
use chrono::{Datelike, TimeZone};
use std::collections::HashSet;
use shared::*;
use crate::backend::Backend;
use crate::ui::components::dropdown_menu::DropdownMenu;

/// Tabs available in the main interface
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Calendar,
    Table,
}

/// Types of overlays that can be shown for calendar day interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayType {
    AddMoney,
    SpendMoney,
    CreateGoal,
}

/// Stages of the parental control challenge flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentalControlStage {
    Question1,      // "Are you Mom or Dad?"
    Question2,      // "What's cooler than cool?"
    Authenticated,  // Success state
}

/// Types of protected actions that require parental control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedAction {
    DeleteTransactions,
    // Future extensions: ConfigureAllowance, ExportData, etc.
}

/// Transaction type for money modal configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Income,
    Expense,
}

/// Generic configuration for money transaction modals (income/expense)
pub struct MoneyTransactionModalConfig {
    // Visual elements
    pub title: &'static str,
    pub icon: &'static str,
    pub button_text: &'static str,
    pub hint_text: &'static str,
    pub color: egui::Color32,
    
    // Field configurations
    pub description_placeholder: &'static str,
    pub amount_placeholder: &'static str,
    pub max_description_length: usize,
    
    // Transaction type for backend integration
    pub transaction_type: TransactionType,
}

/// Generic form state for money transaction modals
#[derive(Debug, Clone)]
pub struct MoneyTransactionFormState {
    pub description: String,
    pub amount: String,
    pub description_error: Option<String>,
    pub amount_error: Option<String>,
    pub is_valid: bool,
}

impl MoneyTransactionFormState {
    pub fn new() -> Self {
        Self {
            description: String::new(),
            amount: String::new(),
            description_error: None,
            amount_error: None,
            is_valid: true,
        }
    }
    
    pub fn clear(&mut self) {
        self.description.clear();
        self.amount.clear();
        self.description_error = None;
        self.amount_error = None;
        self.is_valid = true;
    }
}

impl MoneyTransactionModalConfig {
    /// Configuration for income (Add Money) modal
    pub fn income_config() -> Self {
        Self {
            title: "Add Extra Money",
            icon: "💰",
            button_text: "Add Extra Money",
            hint_text: "Enter the amount of money you received and what it was for",
            color: egui::Color32::from_rgb(34, 139, 34), // Green
            description_placeholder: "What is this money for?",
            amount_placeholder: "0.00",
            max_description_length: 70,
            transaction_type: TransactionType::Income,
        }
    }
    
    /// Configuration for expense (Spend Money) modal  
    pub fn expense_config() -> Self {
        Self {
            title: "Spend Money",
            icon: "💸", 
            button_text: "Spend Money",
            hint_text: "Enter the amount you want to spend from your allowance",
            color: egui::Color32::from_rgb(128, 128, 128), // Gray (as requested)
            description_placeholder: "What did you buy?",
            amount_placeholder: "0.00",
            max_description_length: 70,
            transaction_type: TransactionType::Expense,
        }
    }
}

/// Main application struct for the egui allowance tracker
pub struct AllowanceTrackerApp {
    pub backend: Backend,
    
    // Application state
    pub current_child: Option<Child>,
    pub current_balance: f64,
    
    // UI state
    pub loading: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub current_tab: MainTab,
    
    // Calendar state
    #[allow(dead_code)]
    pub calendar_loading: bool,
    pub calendar_transactions: Vec<Transaction>,
    pub calendar_month: Option<shared::CalendarMonth>,
    pub selected_month: u32,
    pub selected_year: i32,
    
    // Calendar interaction state
    pub selected_day: Option<chrono::NaiveDate>,
    pub expanded_day: Option<chrono::NaiveDate>, // Track which day is expanded to show all chips
    pub active_overlay: Option<OverlayType>,
    pub modal_just_opened: bool, // Prevents backdrop click detection on same frame modal opens
    
    // Modal states
    pub show_add_money_modal: bool,
    pub show_child_selector: bool,
    #[allow(dead_code)]
    pub show_allowance_config_modal: bool,
    
    // Parental control state
    pub show_parental_control_modal: bool,
    pub parental_control_stage: ParentalControlStage,
    pub pending_protected_action: Option<ProtectedAction>,
    pub parental_control_input: String,
    pub parental_control_error: Option<String>,
    pub parental_control_loading: bool,
    
    // Transaction selection state (for deletion)
    pub transaction_selection_mode: bool,
    pub selected_transaction_ids: HashSet<String>,
    
    // Dropdown states using generalized component
    pub child_dropdown: DropdownMenu,
    pub settings_dropdown: DropdownMenu,
    
    // Form states
    pub add_money_amount: String,
    pub add_money_description: String,
    
    // Add money form validation state
    pub add_money_description_error: Option<String>,
    pub add_money_amount_error: Option<String>,
    pub add_money_is_valid: bool,
    
    // Generic money transaction form states
    pub income_form_state: MoneyTransactionFormState,
    pub expense_form_state: MoneyTransactionFormState,
}

impl AllowanceTrackerApp {
    /// Create a new AllowanceTrackerApp with default values
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, anyhow::Error> {
        info!("🚀 Initializing AllowanceTrackerApp with refactored UI");
        
        // Setup custom fonts including Chalkboard
        crate::ui::setup_custom_fonts(&cc.egui_ctx);
        
        // Install image loaders for background support
        egui_extras::install_image_loaders(&cc.egui_ctx);
        
        let backend = crate::backend::Backend::new()?;
        
        let now = chrono::Local::now();
        let current_month = now.month();
        let current_year = now.year();
        
        Ok(Self {
            backend,
            
            // Application state
            current_child: None,
            current_balance: 0.0,
            
            // UI state
            loading: true,
            error_message: None,
            success_message: None,
            current_tab: MainTab::Calendar, // Default to calendar view
            
            // Calendar state
            calendar_loading: false,
            calendar_transactions: Vec::new(),
            calendar_month: None,
            selected_month: current_month,
            selected_year: current_year,
            
            // Calendar interaction state
            selected_day: None,
            expanded_day: None,
            active_overlay: None,
            modal_just_opened: false,
            
            // Modal states
            show_add_money_modal: false,
            show_child_selector: false,
            show_allowance_config_modal: false,
            
            // Parental control state
            show_parental_control_modal: false,
            parental_control_stage: ParentalControlStage::Question1,
            pending_protected_action: None,
            parental_control_input: String::new(),
            parental_control_error: None,
            parental_control_loading: false,
            
            // Transaction selection state
            transaction_selection_mode: false,
            selected_transaction_ids: HashSet::new(),
            
            // Dropdown states using generalized component
            child_dropdown: DropdownMenu::new("child_dropdown".to_string()),
            settings_dropdown: DropdownMenu::new("settings_dropdown".to_string()),
            
            // Form states
            add_money_amount: String::new(),
            add_money_description: String::new(),
            
            // Add money form validation state
            add_money_description_error: None,
            add_money_amount_error: None,
            add_money_is_valid: true,
            
            // Generic money transaction form states
            income_form_state: MoneyTransactionFormState::new(),
            expense_form_state: MoneyTransactionFormState::new(),
        })
    }

    /// Navigate to the previous month
    pub fn navigate_to_previous_month(&mut self) {
        if self.selected_month == 1 {
            self.selected_month = 12;
            self.selected_year -= 1;
        } else {
            self.selected_month -= 1;
        }
        
        // Reload calendar data for the new month
        self.calendar_loading = true;
        self.load_calendar_data();
        log::info!("📅 Navigated to previous month: {}/{}", self.selected_month, self.selected_year);
    }

    /// Navigate to the next month
    pub fn navigate_to_next_month(&mut self) {
        if self.selected_month == 12 {
            self.selected_month = 1;
            self.selected_year += 1;
        } else {
            self.selected_month += 1;
        }
        
        // Reload calendar data for the new month
        self.calendar_loading = true;
        self.load_calendar_data();
        log::info!("📅 Navigated to next month: {}/{}", self.selected_month, self.selected_year);
    }

    /// Get the current month name as a string
    pub fn get_current_month_name(&self) -> String {
        match self.selected_month {
            1 => "January",
            2 => "February", 
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "Unknown"
        }.to_string()
    }

    /// Clear any error or success messages
    pub fn clear_messages(&mut self) {
        self.error_message = None;
        self.success_message = None;
    }

    // ====================
    // PARENTAL CONTROL METHODS
    // ====================

    /// Start parental control challenge for a specific action
    pub fn start_parental_control_challenge(&mut self, action: ProtectedAction) {
        log::info!("🔒 Starting parental control challenge for: {:?}", action);
        self.pending_protected_action = Some(action);
        self.parental_control_stage = ParentalControlStage::Question1;
        self.parental_control_input.clear();
        self.parental_control_error = None;
        self.parental_control_loading = false;
        self.show_parental_control_modal = true;
    }
    
    /// Handle "Yes" button click on first question
    pub fn parental_control_advance_to_question2(&mut self) {
        log::info!("🔒 Advancing to parental control question 2");
        self.parental_control_stage = ParentalControlStage::Question2;
        self.parental_control_input.clear();
        self.parental_control_error = None;
    }
    
    /// Cancel parental control challenge
    pub fn cancel_parental_control_challenge(&mut self) {
        log::info!("🔒 Cancelling parental control challenge");
        self.show_parental_control_modal = false;
        self.pending_protected_action = None;
        self.parental_control_stage = ParentalControlStage::Question1;
        self.parental_control_input.clear();
        self.parental_control_error = None;
        self.parental_control_loading = false;
    }
    
    /// Submit answer for validation
    pub fn submit_parental_control_answer(&mut self) {
        if self.parental_control_input.trim().is_empty() {
            self.parental_control_error = Some("Please enter an answer".to_string());
            return;
        }
        
        log::info!("🔒 Submitting parental control answer for validation");
        self.parental_control_loading = true;
        self.parental_control_error = None;
        
        // Create command for backend
        let command = crate::backend::domain::commands::parental_control::ValidateParentalControlCommand {
            answer: self.parental_control_input.clone(),
        };
        
        // Call backend service
        match self.backend.parental_control_service.validate_answer(command) {
            Ok(result) => {
                self.parental_control_loading = false;
                
                if result.success {
                    log::info!("✅ Parental control validation successful");
                    self.parental_control_stage = ParentalControlStage::Authenticated;
                    
                    // Execute the pending action
                    if let Some(action) = self.pending_protected_action {
                        self.execute_protected_action(action);
                    }
                    
                    // Close modal after brief success display
                    self.show_parental_control_modal = false;
                    self.success_message = Some("Access granted!".to_string());
                } else {
                    log::info!("❌ Parental control validation failed");
                    self.parental_control_error = Some(result.message);
                    self.parental_control_input.clear();
                }
            }
            Err(e) => {
                self.parental_control_loading = false;
                log::error!("🚨 Parental control validation error: {}", e);
                self.parental_control_error = Some("Validation failed. Please try again.".to_string());
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
        
        self.pending_protected_action = None;
    }
    
    // ====================
    // TRANSACTION SELECTION METHODS
    // ====================
    
    /// Enter transaction selection mode for deletion
    pub fn enter_transaction_selection_mode(&mut self) {
        log::info!("🎯 Entering transaction selection mode");
        self.transaction_selection_mode = true;
        self.selected_transaction_ids.clear();
        self.success_message = Some("Select transactions to delete by clicking checkboxes. Click trash button when ready.".to_string());
    }
    
    /// Exit transaction selection mode without deleting
    pub fn exit_transaction_selection_mode(&mut self) {
        log::info!("🚫 Exiting transaction selection mode");
        self.transaction_selection_mode = false;
        self.selected_transaction_ids.clear();
        self.clear_messages();
    }
    
    /// Toggle selection of a transaction
    pub fn toggle_transaction_selection(&mut self, transaction_id: &str) {
        if self.selected_transaction_ids.contains(transaction_id) {
            log::info!("➖ Deselecting transaction: {}", transaction_id);
            self.selected_transaction_ids.remove(transaction_id);
        } else {
            log::info!("✅ Selecting transaction: {}", transaction_id);
            self.selected_transaction_ids.insert(transaction_id.to_string());
        }
    }
    
    /// Check if a transaction is selected
    pub fn is_transaction_selected(&self, transaction_id: &str) -> bool {
        self.selected_transaction_ids.contains(transaction_id)
    }
    
    /// Get count of selected transactions
    pub fn selected_transaction_count(&self) -> usize {
        self.selected_transaction_ids.len()
    }
    
    /// Clear all selected transactions
    pub fn clear_transaction_selection(&mut self) {
        log::info!("🧹 Clearing all transaction selections");
        self.selected_transaction_ids.clear();
    }
    
    /// Check if any transactions are selected
    pub fn has_selected_transactions(&self) -> bool {
        !self.selected_transaction_ids.is_empty()
    }
    
    // ====================
    // ADD MONEY FORM VALIDATION METHODS
    // ====================
    
    /// Validate the add money form and update validation state
    pub fn validate_add_money_form(&mut self) {
        self.add_money_description_error = None;
        self.add_money_amount_error = None;
        
        // Validate description
        let description = self.add_money_description.trim();
        if description.is_empty() {
            self.add_money_description_error = Some("Description is required".to_string());
        } else if description.len() > 70 {
            self.add_money_description_error = Some(format!("Description too long ({}/70 characters)", description.len()));
        }
        
        // Validate amount
        let amount_input = self.add_money_amount.trim();
        if amount_input.is_empty() {
            // Don't show "Amount is required" error immediately - let the grayed button be sufficient
            self.add_money_amount_error = None;
        } else {
            // Clean and parse amount
            match self.clean_and_parse_amount(amount_input) {
                Ok(amount) => {
                    if amount <= 0.0 {
                        self.add_money_amount_error = Some("Amount must be positive".to_string());
                    } else if amount > 1_000_000.0 {
                        self.add_money_amount_error = Some("Amount too large (max $1,000,000)".to_string());
                    } else if amount < 0.01 {
                        self.add_money_amount_error = Some("Amount too small (min $0.01)".to_string());
                    } else if self.has_too_many_decimal_places(amount) {
                        self.add_money_amount_error = Some("Maximum 2 decimal places allowed".to_string());
                    }
                }
                Err(error) => {
                    self.add_money_amount_error = Some(error);
                }
            }
        }
        
        // Update overall validation state
        self.add_money_is_valid = self.add_money_description_error.is_none() && self.add_money_amount_error.is_none();
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
        let input = self.add_money_amount.trim();
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
        self.add_money_description.clear();
        self.add_money_amount.clear();
        self.add_money_description_error = None;
        self.add_money_amount_error = None;
        self.add_money_is_valid = true;
    }
    
    /// Auto-format amount field as user types (adds $ and proper decimal formatting)
    pub fn auto_format_amount_field(&mut self) {
        let input = self.add_money_amount.clone();
        
        // Only auto-format if the input looks like a valid number
        if let Ok(amount) = self.clean_and_parse_amount(&input) {
            // Only format if the amount is reasonable and has <= 2 decimal places
            if amount > 0.0 && amount < 1_000_000.0 && !self.has_too_many_decimal_places(amount) {
                // Format as $XX.XX but only if user isn't currently typing
                if !input.ends_with('.') && !input.ends_with('0') {
                    self.add_money_amount = format!("{:.2}", amount);
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
                  self.income_form_state.description, self.income_form_state.amount);
        
        // Parse amount from form
        let amount = match self.clean_and_parse_amount(&self.income_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("❌ Failed to parse amount: {}", error);
                self.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        
        // Create AddMoneyRequest with selected date from calendar as proper DateTime object
        let date_time = self.selected_day.map(|date| {
            // Convert NaiveDate to DateTime at noon Eastern Time
            let naive_datetime = date.and_hms_opt(12, 0, 0).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::AddMoneyRequest {
            description: self.income_form_state.description.trim().to_string(),
            amount,
            date: date_time,
        };
        
        // Create MoneyManagementService instance
        let money_service = MoneyManagementService::new();
        
        // Call backend with references to other services
        match money_service.add_money_complete(
            request,
            &self.backend.child_service,
            &self.backend.transaction_service,
        ) {
            Ok(response) => {
                log::info!("✅ Income transaction successful: {}", response.success_message);
                self.success_message = Some(response.success_message);
                self.current_balance = response.new_balance;
                
                // Refresh calendar data to show the new transaction
                self.load_calendar_data();
                
                true
            }
            Err(error) => {
                log::error!("❌ Income transaction failed: {}", error);
                self.error_message = Some(format!("Failed to add money: {}", error));
                false
            }
        }
    }

    /// Submit expense transaction to backend using spend_money_complete
    pub fn submit_expense_transaction(&mut self) -> bool {
        use crate::backend::domain::money_management::MoneyManagementService;
        
        log::info!("💸 Submitting expense transaction - Description: '{}', Amount: '{}'", 
                  self.expense_form_state.description, self.expense_form_state.amount);
        
        // Parse amount from form
        let amount = match self.clean_and_parse_amount(&self.expense_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("❌ Failed to parse amount: {}", error);
                self.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        
        // Create SpendMoneyRequest with selected date from calendar as proper DateTime object
        let date_time = self.selected_day.map(|date| {
            // Convert NaiveDate to DateTime at noon Eastern Time
            let naive_datetime = date.and_hms_opt(12, 0, 0).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap(); // EST (UTC-5)
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::SpendMoneyRequest {
            description: self.expense_form_state.description.trim().to_string(),
            amount, // User enters positive amount, backend converts to negative
            date: date_time,
        };
        
        // Create MoneyManagementService instance
        let money_service = MoneyManagementService::new();
        
        // Call backend with references to other services
        match money_service.spend_money_complete(
            request,
            &self.backend.child_service,
            &self.backend.transaction_service,
        ) {
            Ok(response) => {
                log::info!("✅ Expense transaction successful: {}", response.success_message);
                self.success_message = Some(response.success_message);
                self.current_balance = response.new_balance;
                
                // Refresh calendar data to show the new transaction
                self.load_calendar_data();
                
                true
            }
            Err(error) => {
                log::error!("❌ Expense transaction failed: {}", error);
                self.error_message = Some(format!("Failed to spend money: {}", error));
                false
            }
        }
    }
} 