//! Money management domain logic for the allowance tracker.
//!
//! This module contains all business logic related to adding and spending money transactions,
//! form validation, amount parsing, and form state management. The UI should only
//! handle presentation concerns, while all money management business rules are
//! handled here.

use anyhow::Result;
use shared::{
    AddMoneyRequest, AddMoneyResponse, SpendMoneyRequest, SpendMoneyResponse,
    CreateTransactionRequest, MoneyFormState, MoneyFormValidation,
    MoneyManagementConfig, MoneyValidationError,
};
use chrono::{DateTime, Utc, Duration, TimeZone};
use time::OffsetDateTime;
use crate::backend::domain::commands::transactions::CreateTransactionCommand;
use log::{info, error};

// Add imports for the new orchestration methods
use crate::backend::domain::child_service::ChildService;
use crate::backend::domain::transaction_service::TransactionService;

use crate::backend::storage::Connection;

// Create TransactionMapper placeholder
struct TransactionMapper;

impl TransactionMapper {
    pub fn to_dto(transaction: crate::backend::domain::models::transaction::Transaction) -> shared::Transaction {
        shared::Transaction {
            id: transaction.id,
            date: transaction.date,
            amount: transaction.amount,
            description: transaction.description,
            transaction_type: match transaction.transaction_type {
                crate::backend::domain::models::transaction::TransactionType::Income => shared::TransactionType::Income,
                crate::backend::domain::models::transaction::TransactionType::Expense => shared::TransactionType::Expense,
                crate::backend::domain::models::transaction::TransactionType::FutureAllowance => shared::TransactionType::FutureAllowance,
            },
            balance: transaction.balance,
            child_id: transaction.child_id,
        }
    }
}

/// Money management service that handles all money-related business logic
#[derive(Clone)]
pub struct MoneyManagementService {
    config: MoneyManagementConfig,
}

impl MoneyManagementService {
    pub fn new() -> Self {
        Self {
            config: MoneyManagementConfig::default(),
        }
    }

    /// Add money with complete orchestration - handles all business logic for adding money
    /// This method moves the orchestration logic from the REST API layer into the domain layer
    pub fn add_money_complete<C: Connection>(
        &self,
        request: AddMoneyRequest,
        child_service: &ChildService,
        transaction_service: &TransactionService<C>,
    ) -> Result<AddMoneyResponse> {

        info!("💰 MONEY MANAGEMENT: Adding money - description: {}, amount: {}", request.description, request.amount);

        // Step 1: Check for active child first
        info!("🔍 MONEY MANAGEMENT: Checking for active child...");
        let active_child_response = child_service.get_active_child()?;
        // We need to access the connection somehow to debug further
        
        let active_child = match active_child_response.active_child.child {
            Some(child) => {
                info!("✅ MONEY MANAGEMENT: Active child found: {}", child.id);
                child
            },
            None => {
                error!("❌ MONEY MANAGEMENT: No active child found for add money operation");
                return Err(anyhow::anyhow!("No active child found. Please select a child first."));
            }
        };

        // Step 2: Enhanced validation that includes date validation if provided
        println!("🔍 MONEY DEBUG: Starting validation with date: {:?}", request.date);
        println!("🔍 MONEY DEBUG: Validating description: '{}', amount: '{}'", request.description, request.amount.to_string());
        info!("🔍 MONEY MANAGEMENT: Starting validation with date: {:?}", request.date);
        let validation = self.validate_add_money_form_with_date(
            &request.description, 
            &request.amount.to_string(),
            request.date.as_ref(),
            Some(&active_child.created_at.to_rfc3339())
        );

        println!("🔍 MONEY DEBUG: Validation result - is_valid: {}, errors: {:?}", validation.is_valid, validation.errors);
        info!("🔍 MONEY MANAGEMENT: Validation result - is_valid: {}, errors: {:?}", validation.is_valid, validation.errors);

        if !validation.is_valid {
            let error_message = self.get_first_error_message(&validation.errors)
                .unwrap_or_else(|| "Invalid input".to_string());
            println!("❌ MONEY DEBUG: Validation failed: {}", error_message);
            error!("❌ MONEY MANAGEMENT: Validation failed: {}", error_message);
            return Err(anyhow::anyhow!("Validation failed: {}", error_message));
        }

        // Step 3: Convert to CreateTransactionRequest
        info!("🔄 MONEY MANAGEMENT: Converting to CreateTransactionRequest...");
        let create_request = self.to_create_transaction_request(request.clone());
        info!("✅ MONEY MANAGEMENT: CreateTransactionRequest: {:?}", create_request);

        // Step 4: Convert DTO to domain command and create transaction
        let cmd = CreateTransactionCommand {
            description: create_request.description.clone(),
            amount: create_request.amount,
            date: create_request.date.clone(),
        };

        info!("🚀 MONEY MANAGEMENT: Creating transaction via TransactionService...");
        let domain_tx = transaction_service.create_transaction_domain(cmd)?;
        
        let transaction = TransactionMapper::to_dto(domain_tx);
        info!("✅ MONEY MANAGEMENT: Transaction created successfully: {:?}", transaction);
        
        // Step 5: Generate success message with backdated handling
        let success_message = if let Some(date) = &request.date {
            // Check if this was a backdated transaction
            if self.is_backdated_transaction(date) {
                info!("📅 MONEY MANAGEMENT: This was a backdated transaction");
                format!("🎉 {} added successfully (backdated to {})!", 
                              self.format_positive_amount(transaction.amount),
                              date.format("%Y-%m-%d"))
            } else {
                info!("📅 MONEY MANAGEMENT: This was a current-date transaction");
                self.generate_success_message(transaction.amount)
            }
        } else {
            info!("📅 MONEY MANAGEMENT: No date provided, using current date");
            self.generate_success_message(transaction.amount)
        };
        
        let formatted_amount = self.format_positive_amount(transaction.amount);

        let response = AddMoneyResponse {
            transaction_id: transaction.id,
            success_message: success_message.clone(),
            new_balance: transaction.balance,
            formatted_amount,
        };

        info!("✅ MONEY MANAGEMENT: Sending success response: {:?}", response);
        Ok(response)
    }

    /// Spend money with complete orchestration - handles all business logic for spending money
    /// This method moves the orchestration logic from the REST API layer into the domain layer
    pub fn spend_money_complete<C: Connection>(
        &self,
        request: SpendMoneyRequest,
        child_service: &ChildService,
        transaction_service: &TransactionService<C>,
    ) -> Result<SpendMoneyResponse> {
        info!("💸 MONEY MANAGEMENT: Spending money - description: {}, amount: {}", request.description, request.amount);

        // Step 1: Check for active child first
        info!("🔍 MONEY MANAGEMENT: Checking for active child...");
        let active_child_response = child_service.get_active_child()?;
        
        let active_child = match active_child_response.active_child.child {
            Some(child) => {
                info!("✅ MONEY MANAGEMENT: Active child found: {}", child.id);
                child
            },
            None => {
                error!("❌ MONEY MANAGEMENT: No active child found for spend money operation");
                return Err(anyhow::anyhow!("No active child found. Please select a child first."));
            }
        };

        // Step 2: Enhanced validation that includes date validation if provided
        info!("🔍 MONEY MANAGEMENT: Starting validation with date: {:?}", request.date);
        let validation = self.validate_spend_money_form_with_date(
            &request.description, 
            &request.amount.to_string(),
            request.date.as_ref(),
            Some(&active_child.created_at.to_rfc3339())
        );

        info!("🔍 MONEY MANAGEMENT: Validation result - is_valid: {}, errors: {:?}", validation.is_valid, validation.errors);

        if !validation.is_valid {
            let error_message = self.get_first_error_message(&validation.errors)
                .unwrap_or_else(|| "Invalid input".to_string());
            error!("❌ MONEY MANAGEMENT: Validation failed: {}", error_message);
            return Err(anyhow::anyhow!("Validation failed: {}", error_message));
        }

        // Step 3: Convert to CreateTransactionRequest (this will make the amount negative)
        info!("🔄 MONEY MANAGEMENT: Converting to CreateTransactionRequest...");
        let create_request = self.spend_to_create_transaction_request(request.clone());
        info!("✅ MONEY MANAGEMENT: CreateTransactionRequest: {:?}", create_request);

        // Step 4: Convert DTO to domain command and create transaction
        let cmd = CreateTransactionCommand {
            description: create_request.description.clone(),
            amount: create_request.amount,
            date: create_request.date.clone(),
        };

        info!("🚀 MONEY MANAGEMENT: Creating transaction via TransactionService...");
        let domain_tx = transaction_service.create_transaction_domain(cmd)?;
        
        let transaction = TransactionMapper::to_dto(domain_tx);
        info!("✅ MONEY MANAGEMENT: Transaction created successfully: {:?}", transaction);
        
        // Step 5: Generate success message with backdated handling
        let success_message = if let Some(date) = &request.date {
            // Check if this was a backdated transaction
            if self.is_backdated_transaction(date) {
                info!("📅 MONEY MANAGEMENT: This was a backdated transaction");
                format!("💸 {} spent successfully (backdated to {})!", 
                              self.format_amount(request.amount.abs()),
                              date.format("%Y-%m-%d"))
            } else {
                info!("📅 MONEY MANAGEMENT: This was a current-date transaction");
                self.generate_spend_success_message(request.amount)
            }
        } else {
            info!("📅 MONEY MANAGEMENT: No date provided, using current date");
            self.generate_spend_success_message(request.amount)
        };
        
        let formatted_amount = self.format_negative_amount(request.amount);

        let response = SpendMoneyResponse {
            transaction_id: transaction.id,
            success_message: success_message.clone(),
            new_balance: transaction.balance,
            formatted_amount,
        };

        info!("✅ MONEY MANAGEMENT: Sending success response: {:?}", response);
        Ok(response)
    }

    pub fn with_config(config: MoneyManagementConfig) -> Self {
        Self { config }
    }

    /// Create a new form state for adding money
    pub fn create_form_state() -> MoneyFormState {
        MoneyFormState {
            description: String::new(),
            amount_input: String::new(),
            is_submitting: false,
            error_message: None,
            success_message: None,
            show_success: false,
        }
    }

    /// Validate the add money form input
    pub fn validate_add_money_form(&self, description: &str, amount_input: &str) -> MoneyFormValidation {
        let mut errors = Vec::new();
        let mut suggestions = Vec::new();

        // Validate description
        let description_trimmed = description.trim();
        if description_trimmed.is_empty() {
            errors.push(MoneyValidationError::EmptyDescription);
            suggestions.push("Try: Gift from grandma, gift from aunt...".to_string());
        } else if description_trimmed.len() > self.config.max_description_length {
            errors.push(MoneyValidationError::DescriptionTooLong(description_trimmed.len()));
        }

        // Validate and parse amount
        let cleaned_amount = if amount_input.trim().is_empty() {
            errors.push(MoneyValidationError::EmptyAmount);
            suggestions.push("Enter a positive amount like $5.00 or $10".to_string());
            None
        } else {
            match self.clean_and_parse_amount(amount_input) {
                Ok(amount) => {
                    if amount <= 0.0 {
                        errors.push(MoneyValidationError::AmountNotPositive);
                        suggestions.push("Amount must be greater than 0".to_string());
                        None
                    } else if amount < self.config.min_amount {
                        errors.push(MoneyValidationError::AmountTooSmall(self.config.min_amount));
                        suggestions.push(format!("Minimum amount is {}{:.2}", self.config.currency_symbol, self.config.min_amount));
                        None
                    } else if amount > self.config.max_amount {
                        errors.push(MoneyValidationError::AmountTooLarge(self.config.max_amount));
                        suggestions.push(format!("Maximum amount is {}{:.2}", self.config.currency_symbol, self.config.max_amount));
                        None
                    } else if self.has_too_many_decimal_places(amount) {
                        errors.push(MoneyValidationError::AmountPrecisionTooHigh);
                        suggestions.push("Use at most 2 decimal places (like $5.25)".to_string());
                        None
                    } else {
                        Some(amount)
                    }
                }
                Err(parse_error) => {
                    errors.push(MoneyValidationError::InvalidAmountFormat(parse_error));
                    suggestions.push("Enter a valid number like $5.00 or $10".to_string());
                    None
                }
            }
        };

        MoneyFormValidation {
            is_valid: errors.is_empty(),
            errors,
            cleaned_amount,
            suggestions,
        }
    }

    /// Clean and parse amount input string
    pub fn clean_and_parse_amount(&self, amount_input: &str) -> Result<f64, String> {
        // Clean the input - remove dollar signs, spaces, commas
        let cleaned = amount_input
            .trim()
            .replace(&self.config.currency_symbol, "")
            .replace(",", "")
            .replace(" ", "");

        // Handle empty input after cleaning
        if cleaned.is_empty() {
            return Err("Empty amount after cleaning".to_string());
        }

        // Try to parse as float
        cleaned.parse::<f64>()
            .map_err(|e| format!("Invalid number format: {}", e))
    }

    /// Check if amount has too many decimal places
    fn has_too_many_decimal_places(&self, amount: f64) -> bool {
        // Convert to string and check decimal places
        let amount_str = format!("{:.3}", amount);
        if let Some(decimal_pos) = amount_str.find('.') {
            let decimal_part = &amount_str[decimal_pos + 1..];
            // Check if there are more than 2 significant decimal places
            if decimal_part.len() > 2 && !decimal_part.ends_with("0") {
                return true;
            }
        }
        false
    }

    /// Format amount for display
    pub fn format_amount(&self, amount: f64) -> String {
        format!("{}{:.2}", self.config.currency_symbol, amount)
    }

    /// Format amount for positive display (with + sign)
    pub fn format_positive_amount(&self, amount: f64) -> String {
        format!("+{}{:.2}", self.config.currency_symbol, amount)
    }

    /// Create a transaction request from validated form data
    pub fn create_add_money_request(&self, description: String, amount: f64, date: Option<chrono::DateTime<chrono::FixedOffset>>) -> AddMoneyRequest {
        AddMoneyRequest {
            description: description.trim().to_string(),
            amount,
            date,
        }
    }

    /// Convert AddMoneyRequest to CreateTransactionRequest
    pub fn to_create_transaction_request(&self, add_money_request: AddMoneyRequest) -> CreateTransactionRequest {
        CreateTransactionRequest {
            description: add_money_request.description,
            amount: add_money_request.amount,
            date: add_money_request.date,
        }
    }

    /// Convert AddMoneyRequest to CreateTransactionCommand (domain)
    pub fn to_create_transaction_command(&self, add_money_request: AddMoneyRequest) -> CreateTransactionCommand {
        CreateTransactionCommand {
            description: add_money_request.description,
            amount: add_money_request.amount,
            date: add_money_request.date,
        }
    }

    /// Generate success message for successful money addition
    pub fn generate_success_message(&self, amount: f64) -> String {
        format!("🎉 {} added successfully!", self.format_positive_amount(amount))
    }

    /// Get user-friendly error message for validation error
    pub fn get_error_message(&self, error: &MoneyValidationError) -> String {
        match error {
            MoneyValidationError::EmptyDescription => "Please enter a description".to_string(),
            MoneyValidationError::DescriptionTooLong(len) => {
                format!("Description is too long ({} characters). Maximum is {}.", len, self.config.max_description_length)
            }
            MoneyValidationError::EmptyAmount => "Please enter an amount".to_string(),
            MoneyValidationError::InvalidAmountFormat(msg) => {
                format!("Please enter a valid amount (like 5 or 5.00): {}", msg)
            }
            MoneyValidationError::AmountNotPositive => "Amount must be greater than 0".to_string(),
            MoneyValidationError::AmountTooSmall(min) => {
                format!("Amount is too small. Minimum is {}{:.2}", self.config.currency_symbol, min)
            }
            MoneyValidationError::AmountTooLarge(max) => {
                format!("Amount is too large. Maximum is {}{:.2}", self.config.currency_symbol, max)
            }
            MoneyValidationError::AmountPrecisionTooHigh => "Amount has too many decimal places. Use at most 2 decimal places.".to_string(),
        }
    }

    /// Get all validation error messages as a list
    pub fn get_error_messages(&self, errors: &[MoneyValidationError]) -> Vec<String> {
        errors.iter().map(|e| self.get_error_message(e)).collect()
    }

    /// Get the first error message (for displaying single error)
    pub fn get_first_error_message(&self, errors: &[MoneyValidationError]) -> Option<String> {
        errors.first().map(|e| self.get_error_message(e))
    }

    /// Update form state with validation results
    pub fn update_form_state_with_validation(&self, mut state: MoneyFormState, validation: MoneyFormValidation) -> MoneyFormState {
        if validation.is_valid {
            state.error_message = None;
        } else {
            state.error_message = self.get_first_error_message(&validation.errors);
        }
        state
    }

    /// Clear form state after successful submission
    pub fn clear_form_after_success(&self, mut state: MoneyFormState, success_message: String) -> MoneyFormState {
        state.description = String::new();
        state.amount_input = String::new();
        state.is_submitting = false;
        state.error_message = None;
        state.success_message = Some(success_message);
        state.show_success = true;
        state
    }

    /// Set form state to submitting
    pub fn set_form_submitting(&self, mut state: MoneyFormState) -> MoneyFormState {
        state.is_submitting = true;
        state.error_message = None;
        state.show_success = false;
        state
    }

    /// Set form state with error
    pub fn set_form_error(&self, mut state: MoneyFormState, error_message: String) -> MoneyFormState {
        state.is_submitting = false;
        state.error_message = Some(error_message);
        state.show_success = false;
        state
    }

    /// Generate common money descriptions as suggestions
    pub fn get_description_suggestions(&self) -> Vec<String> {
        vec![
            "Gift from grandma".to_string(),
            "Gift from aunt".to_string(),
            "Gift from uncle".to_string(),
            "Birthday gift".to_string(),
            "Chores completed".to_string(),
            "Found money".to_string(),
            "Allowance bonus".to_string(),
            "Good grades reward".to_string(),
        ]
    }

    /// Validate amount in real-time (for form feedback)
    pub fn validate_amount_realtime(&self, amount_input: &str) -> Result<f64, String> {
        if amount_input.trim().is_empty() {
            return Err("Enter an amount".to_string());
        }

        match self.clean_and_parse_amount(amount_input) {
            Ok(amount) => {
                if amount <= 0.0 {
                    Err("Must be greater than 0".to_string())
                } else if amount > self.config.max_amount {
                    Err(format!("Maximum is {}", self.format_amount(self.config.max_amount)))
                } else {
                    Ok(amount)
                }
            }
            Err(_) => Err("Invalid number".to_string())
        }
    }

    /// Get configuration
    pub fn get_config(&self) -> &MoneyManagementConfig {
        &self.config
    }

    /// Validate the spend money form input
    pub fn validate_spend_money_form(&self, description: &str, amount_input: &str) -> MoneyFormValidation {
        let mut errors = Vec::new();
        let mut suggestions = Vec::new();

        // Validate description
        let description_trimmed = description.trim();
        if description_trimmed.is_empty() {
            errors.push(MoneyValidationError::EmptyDescription);
            suggestions.push("Try: Toy, book, game...".to_string());
        } else if description_trimmed.len() > self.config.max_description_length {
            errors.push(MoneyValidationError::DescriptionTooLong(description_trimmed.len()));
        }

        // Validate and parse amount (user enters positive, we'll convert to negative later)
        let cleaned_amount = if amount_input.trim().is_empty() {
            errors.push(MoneyValidationError::EmptyAmount);
            suggestions.push("Enter how much you spent, like $2.50 or $5".to_string());
            None
        } else {
            match self.clean_and_parse_amount(amount_input) {
                Ok(amount) => {
                    if amount <= 0.0 {
                        errors.push(MoneyValidationError::AmountNotPositive);
                        suggestions.push("Amount must be greater than 0".to_string());
                        None
                    } else if amount < self.config.min_amount {
                        errors.push(MoneyValidationError::AmountTooSmall(self.config.min_amount));
                        suggestions.push(format!("Minimum amount is {}{:.2}", self.config.currency_symbol, self.config.min_amount));
                        None
                    } else if amount > self.config.max_amount {
                        errors.push(MoneyValidationError::AmountTooLarge(self.config.max_amount));
                        suggestions.push(format!("Maximum amount is {}{:.2}", self.config.currency_symbol, self.config.max_amount));
                        None
                    } else if self.has_too_many_decimal_places(amount) {
                        errors.push(MoneyValidationError::AmountPrecisionTooHigh);
                        suggestions.push("Use at most 2 decimal places (like $5.25)".to_string());
                        None
                    } else {
                        Some(amount)
                    }
                }
                Err(parse_error) => {
                    errors.push(MoneyValidationError::InvalidAmountFormat(parse_error));
                    suggestions.push("Enter a valid number like $2.50 or $5".to_string());
                    None
                }
            }
        };

        MoneyFormValidation {
            is_valid: errors.is_empty(),
            errors,
            cleaned_amount,
            suggestions,
        }
    }

    /// Format amount for negative display (with - sign)
    pub fn format_negative_amount(&self, amount: f64) -> String {
        format!("-{}{:.2}", self.config.currency_symbol, amount.abs())
    }

    /// Create a spend money request from validated form data
    pub fn create_spend_money_request(&self, description: String, amount: f64, date: Option<chrono::DateTime<chrono::FixedOffset>>) -> SpendMoneyRequest {
        SpendMoneyRequest {
            description: description.trim().to_string(),
            amount,  // Keep positive, backend will convert to negative
            date,
        }
    }

    /// Convert SpendMoneyRequest to CreateTransactionRequest (converting amount to negative)
    pub fn spend_to_create_transaction_request(&self, spend_money_request: SpendMoneyRequest) -> CreateTransactionRequest {
        CreateTransactionRequest {
            description: spend_money_request.description,
            amount: -spend_money_request.amount.abs(),  // Ensure negative amount
            date: spend_money_request.date,
        }
    }

    /// Convert SpendMoneyRequest to CreateTransactionCommand (domain, converting amount to negative)
    pub fn spend_to_create_transaction_command(&self, spend_money_request: SpendMoneyRequest) -> CreateTransactionCommand {
        CreateTransactionCommand {
            description: spend_money_request.description,
            amount: -spend_money_request.amount.abs(),  // Ensure negative amount
            date: spend_money_request.date,
        }
    }

    /// Generate success message for successful money spending
    pub fn generate_spend_success_message(&self, amount: f64) -> String {
        format!("💸 {} spent successfully!", self.format_amount(amount.abs()))
    }

    /// Generate common spending descriptions as suggestions
    pub fn get_spending_suggestions(&self) -> Vec<String> {
        vec![
            "Toy".to_string(),
            "Candy".to_string(),
            "Book".to_string(),
            "Game".to_string(),
            "Snack".to_string(),
            "Art supplies".to_string(),
            "Small gift".to_string(),
            "Trading cards".to_string(),
            "App purchase".to_string(),
            "Movie ticket".to_string(),
        ]
    }

    /// Validate a transaction date string
    /// Accepts both YYYY-MM-DD format (from date picker) and RFC 3339 format
    /// Rules:
    /// - Date cannot be more than 45 days in the past
    /// - Date cannot be in the future
    pub fn validate_transaction_date(&self, date: &str, _child_created_at: Option<&str>) -> Result<(), String> {
        // Try to parse as RFC 3339 first, then fall back to YYYY-MM-DD
        let transaction_date = if let Ok(dt) = DateTime::parse_from_rfc3339(date) {
            dt
        } else {
            // Try parsing as YYYY-MM-DD and convert to RFC 3339 with current time
            match chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                Ok(naive_date) => {
                    // Convert to datetime at noon Eastern Time for consistency
                    let naive_datetime = naive_date.and_hms_opt(12, 0, 0)
                        .ok_or_else(|| "Failed to create datetime from date".to_string())?;
                    
                    // Create Eastern Time offset (EST/EDT)
                    let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600)
                        .ok_or_else(|| "Failed to create Eastern timezone offset".to_string())?;
                    
                    eastern_offset.from_local_datetime(&naive_datetime)
                        .single()
                        .ok_or_else(|| "Failed to create timezone-aware datetime".to_string())?
                }
                Err(_) => {
                    return Err(format!("Invalid date format. Expected YYYY-MM-DD (e.g., '2025-06-19') or RFC 3339 format (e.g., '2025-01-15T14:30:00-05:00'): {}", date));
                }
            }
        };

        let now = Utc::now();
        let now_with_tz = now.with_timezone(&transaction_date.timezone());

        // Check if date is in the future (allow same day)
        let start_of_tomorrow = now_with_tz.date_naive().succ_opt()
            .ok_or_else(|| "Failed to calculate tomorrow's date".to_string())?
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| "Failed to create start of tomorrow".to_string())?;
        let tomorrow_with_tz = transaction_date.timezone().from_local_datetime(&start_of_tomorrow)
            .single()
            .ok_or_else(|| "Failed to create timezone-aware tomorrow".to_string())?;

        if transaction_date >= tomorrow_with_tz {
            return Err("Transaction date cannot be in the future".to_string());
        }

        // Check 45-day limit
        let forty_five_days_ago = now_with_tz - Duration::days(45);
        if transaction_date < forty_five_days_ago {
            return Err("Transaction date cannot be more than 45 days in the past".to_string());
        }

        Ok(())
    }

    /// Enhanced validation for add money form that includes date validation
    pub fn validate_add_money_form_with_date(&self, description: &str, amount_input: &str, date: Option<&chrono::DateTime<chrono::FixedOffset>>, child_created_at: Option<&str>) -> MoneyFormValidation {
        let validation = self.validate_add_money_form(description, amount_input);

        // DateTime objects are already validated by construction, so no additional date validation needed
        // The frontend is responsible for creating valid DateTime objects
        
        validation
    }

    /// Enhanced validation for spend money form that includes date validation
    pub fn validate_spend_money_form_with_date(&self, description: &str, amount_input: &str, date: Option<&chrono::DateTime<chrono::FixedOffset>>, child_created_at: Option<&str>) -> MoneyFormValidation {
        let validation = self.validate_spend_money_form(description, amount_input);

        // DateTime objects are already validated by construction, so no additional date validation needed
        // The frontend is responsible for creating valid DateTime objects
        
        validation
    }

    /// Check if a transaction date would require balance recalculation
    /// (i.e., it's being backdated)
    pub fn is_backdated_transaction(&self, transaction_date: &chrono::DateTime<chrono::FixedOffset>) -> bool {
        let now = chrono::Utc::now();
        let now_with_tz = now.with_timezone(&transaction_date.timezone());
        
        // Consider backdated if more than 1 hour in the past
        // This gives some leeway for timezone differences and normal delays
        let one_hour_ago = now_with_tz - chrono::Duration::hours(1);
        
        *transaction_date < one_hour_ago
    }

    /// Generate current RFC 3339 timestamp in Eastern Time
    /// This is used as the default date when none is provided
    pub fn generate_current_timestamp(&self) -> Result<String, String> {
        let now = std::time::SystemTime::now();
        let utc_datetime = OffsetDateTime::from(now);
        
        // Convert to Eastern Time (assuming EST/EDT, UTC-5/-4)
        // In a production app, you'd want to detect the actual timezone
        let eastern_offset = time::UtcOffset::from_hms(-5, 0, 0)
            .map_err(|e| format!("Failed to create timezone offset: {}", e))?;
        let eastern_datetime = utc_datetime.to_offset(eastern_offset);
        
        eastern_datetime.format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| format!("Failed to format timestamp: {}", e))
    }
}

impl Default for MoneyManagementService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_service() -> MoneyManagementService {
        MoneyManagementService::new()
    }

    #[test]
    fn test_validate_add_money_form_success() {
        let service = create_test_service();
        
        let validation = service.validate_add_money_form("Birthday gift", "10.50");
        
        assert!(validation.is_valid);
        assert!(validation.errors.is_empty());
        assert_eq!(validation.cleaned_amount, Some(10.50));
        assert!(validation.suggestions.is_empty());
    }

    #[test]
    fn test_validate_add_money_form_empty_description() {
        let service = create_test_service();
        
        let validation = service.validate_add_money_form("", "10.50");
        
        assert!(!validation.is_valid);
        assert!(matches!(validation.errors[0], MoneyValidationError::EmptyDescription));
        assert!(!validation.suggestions.is_empty());
    }

    #[test]
    fn test_validate_add_money_form_invalid_amount() {
        let service = create_test_service();
        
        let validation = service.validate_add_money_form("Valid description", "abc");
        
        assert!(!validation.is_valid);
        assert!(matches!(validation.errors[0], MoneyValidationError::InvalidAmountFormat(_)));
        assert!(!validation.suggestions.is_empty());
    }

    #[test]
    fn test_validate_add_money_form_negative_amount() {
        let service = create_test_service();
        
        let validation = service.validate_add_money_form("Valid description", "-5.00");
        
        assert!(!validation.is_valid);
        assert!(matches!(validation.errors[0], MoneyValidationError::AmountNotPositive));
    }

    #[test]
    fn test_clean_and_parse_amount() {
        let service = create_test_service();
        
        assert_eq!(service.clean_and_parse_amount("10.50").unwrap(), 10.50);
        assert_eq!(service.clean_and_parse_amount("$10.50").unwrap(), 10.50);
        assert_eq!(service.clean_and_parse_amount(" $1,234.56 ").unwrap(), 1234.56);
        assert_eq!(service.clean_and_parse_amount("5").unwrap(), 5.0);
        
        assert!(service.clean_and_parse_amount("abc").is_err());
        assert!(service.clean_and_parse_amount("").is_err());
    }

    #[test]
    fn test_format_amount() {
        let service = create_test_service();
        
        assert_eq!(service.format_amount(10.50), "$10.50");
        assert_eq!(service.format_positive_amount(10.50), "+$10.50");
    }

    #[test]
    fn test_create_add_money_request() {
        let service = create_test_service();
        
        let request = service.create_add_money_request("Test description".to_string(), 10.50, None);
        
        assert_eq!(request.description, "Test description");
        assert_eq!(request.amount, 10.50);
        assert_eq!(request.date, None);
    }

    #[test]
    fn test_to_create_transaction_request() {
        let service = create_test_service();
        
        let add_money_request = AddMoneyRequest {
            description: "Test".to_string(),
            amount: 10.50,
            date: None,
        };
        
        let create_request = service.to_create_transaction_request(add_money_request);
        
        assert_eq!(create_request.description, "Test");
        assert_eq!(create_request.amount, 10.50);
        assert_eq!(create_request.date, None);
    }

    #[test]
    fn test_generate_success_message() {
        let service = create_test_service();
        
        let message = service.generate_success_message(10.50);
        
        assert_eq!(message, "🎉 +$10.50 added successfully!");
    }

    #[test]
    fn test_form_state_management() {
        let service = create_test_service();
        
        let initial_state = MoneyManagementService::create_form_state();
        assert_eq!(initial_state.description, "");
        assert_eq!(initial_state.amount_input, "");
        assert!(!initial_state.is_submitting);
        assert!(initial_state.error_message.is_none());
        
        let submitting_state = service.set_form_submitting(initial_state);
        assert!(submitting_state.is_submitting);
        assert!(submitting_state.error_message.is_none());
        
        let error_state = service.set_form_error(submitting_state, "Test error".to_string());
        assert!(!error_state.is_submitting);
        assert_eq!(error_state.error_message, Some("Test error".to_string()));
        
        let success_state = service.clear_form_after_success(error_state, "Success!".to_string());
        assert_eq!(success_state.description, "");
        assert_eq!(success_state.amount_input, "");
        assert!(!success_state.is_submitting);
        assert!(success_state.error_message.is_none());
        assert_eq!(success_state.success_message, Some("Success!".to_string()));
        assert!(success_state.show_success);
    }

    #[test]
    fn test_error_messages() {
        let service = create_test_service();
        
        let error = MoneyValidationError::EmptyDescription;
        assert_eq!(service.get_error_message(&error), "Please enter a description");
        
        let error = MoneyValidationError::AmountNotPositive;
        assert_eq!(service.get_error_message(&error), "Amount must be greater than 0");
        
        let error = MoneyValidationError::DescriptionTooLong(300);
        assert!(service.get_error_message(&error).contains("too long"));
    }

    #[test]
    fn test_description_suggestions() {
        let service = create_test_service();
        
        let suggestions = service.get_description_suggestions();
        
        assert!(!suggestions.is_empty());
        assert!(suggestions.contains(&"Birthday gift".to_string()));
        assert!(suggestions.contains(&"Chores completed".to_string()));
    }

    #[test]
    fn test_realtime_validation() {
        let service = create_test_service();
        
        // Valid amount
        assert!(service.validate_amount_realtime("10.50").is_ok());
        
        // Empty amount
        assert!(service.validate_amount_realtime("").is_err());
        
        // Invalid format
        assert!(service.validate_amount_realtime("abc").is_err());
        
        // Zero amount
        assert!(service.validate_amount_realtime("0").is_err());
        
        // Negative amount
        assert!(service.validate_amount_realtime("-5").is_err());
    }

    #[test]
    fn test_validate_spend_money_form_success() {
        let service = create_test_service();
        
        let validation = service.validate_spend_money_form("Toy", "3.25");
        
        assert!(validation.is_valid);
        assert!(validation.errors.is_empty());
        assert_eq!(validation.cleaned_amount, Some(3.25));
        assert!(validation.suggestions.is_empty());
    }

    #[test]
    fn test_validate_spend_money_form_empty_description() {
        let service = create_test_service();
        
        let validation = service.validate_spend_money_form("", "5.00");
        
        assert!(!validation.is_valid);
        assert!(matches!(validation.errors[0], MoneyValidationError::EmptyDescription));
        assert!(!validation.suggestions.is_empty());
        assert!(validation.suggestions[0].contains("Toy"));
    }

    #[test]
    fn test_validate_spend_money_form_invalid_amount() {
        let service = create_test_service();
        
        let validation = service.validate_spend_money_form("Valid description", "invalid");
        
        assert!(!validation.is_valid);
        assert!(matches!(validation.errors[0], MoneyValidationError::InvalidAmountFormat(_)));
        assert!(!validation.suggestions.is_empty());
    }

    #[test]
    fn test_create_spend_money_request() {
        let service = create_test_service();
        
        let request = service.create_spend_money_request("Candy".to_string(), 2.50, None);
        
        assert_eq!(request.description, "Candy");
        assert_eq!(request.amount, 2.50);
        assert!(request.date.is_none());
    }

    #[test]
    fn test_spend_to_create_transaction_request() {
        let service = create_test_service();
        
        let spend_request = SpendMoneyRequest {
            description: "Game".to_string(),
            amount: 15.00,
            date: None,
        };
        
        let transaction_request = service.spend_to_create_transaction_request(spend_request);
        
        assert_eq!(transaction_request.description, "Game");
        assert_eq!(transaction_request.amount, -15.00); // Should be negative
        assert!(transaction_request.date.is_none());
    }

    #[test]
    fn test_generate_spend_success_message() {
        let service = create_test_service();
        
        let message = service.generate_spend_success_message(7.50);
        
        assert!(message.contains("💸"));
        assert!(message.contains("$7.50"));
        assert!(message.contains("spent successfully"));
    }

    #[test]
    fn test_format_negative_amount() {
        let service = create_test_service();
        
        let formatted = service.format_negative_amount(5.25);
        
        assert_eq!(formatted, "-$5.25");
    }

    #[test]
    fn test_get_spending_suggestions() {
        let service = create_test_service();
        
        let suggestions = service.get_spending_suggestions();
        
        assert!(!suggestions.is_empty());
        assert!(suggestions.contains(&"Toy".to_string()));
        assert!(suggestions.contains(&"Candy".to_string()));
    }

    #[test]
    fn test_validate_transaction_date_valid() {
        let service = create_test_service();
        
        // Valid date in the past (within 45 days)
        let valid_date = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(10))
            .unwrap()
            .to_rfc3339();
        
        let result = service.validate_transaction_date(&valid_date, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_transaction_date_invalid_format() {
        let service = create_test_service();
        
        let result = service.validate_transaction_date("invalid-date-format", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid date format"));
    }

    #[test]
    fn test_validate_transaction_date_future() {
        let service = create_test_service();
        
        // Date in the future
        let future_date = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(1))
            .unwrap()
            .to_rfc3339();
        
        let result = service.validate_transaction_date(&future_date, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be in the future"));
    }

    #[test]
    fn test_validate_transaction_date_too_old() {
        let service = create_test_service();
        
        // Date more than 45 days in the past
        let old_date = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(50))
            .unwrap()
            .to_rfc3339();
        
        let result = service.validate_transaction_date(&old_date, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("more than 45 days in the past"));
    }

    #[test]
    fn test_validate_add_money_form_with_date_valid() {
        let service = create_test_service();
        
        let valid_date = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(5))
            .unwrap()
            .to_rfc3339();
        
        let validation = service.validate_add_money_form_with_date(
            "Birthday gift", 
            "25.00", 
            Some(&valid_date), 
            None
        );
        
        assert!(validation.is_valid);
    }

    #[test]
    fn test_validate_add_money_form_with_date_invalid_date() {
        let service = create_test_service();
        
        let future_date = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(1))
            .unwrap()
            .to_rfc3339();
        
        let validation = service.validate_add_money_form_with_date(
            "Birthday gift", 
            "25.00", 
            Some(&future_date), 
            None
        );
        
        assert!(!validation.is_valid);
        assert!(!validation.errors.is_empty());
    }

    #[test]
    fn test_is_backdated_transaction() {
        let service = create_test_service();
        
        // Current time (should not be backdated)
        let now = chrono::Utc::now().to_rfc3339();
        let result = service.is_backdated_transaction(&now);
        assert!(result.is_ok());
        assert!(!result.unwrap());
        
        // 2 hours ago (should be backdated)
        let backdated = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::hours(2))
            .unwrap()
            .to_rfc3339();
        let result = service.is_backdated_transaction(&backdated);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_generate_current_timestamp() {
        let service = create_test_service();
        
        let result = service.generate_current_timestamp();
        assert!(result.is_ok());
        
        let timestamp = result.unwrap();
        assert!(!timestamp.is_empty());
        
        // Should be parseable as RFC 3339
        let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp);
        assert!(parsed.is_ok());
    }
} 