//! # Form State Module
//!
//! This module contains all state related to form inputs and validation.
//!
//! ## Responsibilities:
//! - Form input values
//! - Form validation states and error messages
//! - Transaction type configurations
//!
//! ## Purpose:
//! This centralizes all form-related state management, making it easier to
//! maintain consistent form behavior and validation across the application.

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

/// All form-related state for the application
#[derive(Debug)]
pub struct FormState {
    /// Legacy add money form fields (kept for compatibility)
    pub add_money_amount: String,
    pub add_money_description: String,
    
    /// Legacy add money form validation state
    pub add_money_description_error: Option<String>,
    pub add_money_amount_error: Option<String>,
    pub add_money_is_valid: bool,
    
    /// Generic money transaction form states
    pub income_form_state: MoneyTransactionFormState,
    pub expense_form_state: MoneyTransactionFormState,
}

impl FormState {
    /// Create new form state with empty forms
    pub fn new() -> Self {
        Self {
            add_money_amount: String::new(),
            add_money_description: String::new(),
            add_money_description_error: None,
            add_money_amount_error: None,
            add_money_is_valid: true,
            income_form_state: MoneyTransactionFormState::new(),
            expense_form_state: MoneyTransactionFormState::new(),
        }
    }
    
    /// Clear all form data
    pub fn clear_all_forms(&mut self) {
        self.add_money_amount.clear();
        self.add_money_description.clear();
        self.add_money_description_error = None;
        self.add_money_amount_error = None;
        self.add_money_is_valid = true;
        self.income_form_state.clear();
        self.expense_form_state.clear();
    }
} 