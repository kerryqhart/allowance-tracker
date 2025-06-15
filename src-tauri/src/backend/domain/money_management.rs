//! Money management domain logic for the allowance tracker.
//!
//! This module contains all business logic related to adding money transactions,
//! form validation, amount parsing, and form state management. The UI should only
//! handle presentation concerns, while all money management business rules are
//! handled here.

use shared::{CreateTransactionRequest, AddMoneyRequest, MoneyFormValidation, MoneyValidationError, MoneyFormState, MoneyManagementConfig};



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
            suggestions.push("Try: Birthday gift, Chores completed, Found money, etc.".to_string());
        } else if description_trimmed.len() > self.config.max_description_length {
            errors.push(MoneyValidationError::DescriptionTooLong(description_trimmed.len()));
        }

        // Validate and parse amount
        let cleaned_amount = if amount_input.trim().is_empty() {
            errors.push(MoneyValidationError::EmptyAmount);
            suggestions.push("Enter a positive amount like 5.00 or 10".to_string());
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
                        suggestions.push("Use at most 2 decimal places (like 5.25)".to_string());
                        None
                    } else {
                        Some(amount)
                    }
                }
                Err(parse_error) => {
                    errors.push(MoneyValidationError::InvalidAmountFormat(parse_error));
                    suggestions.push("Enter a valid number like 5.00 or 10".to_string());
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
    pub fn create_add_money_request(&self, description: String, amount: f64, date: Option<String>) -> AddMoneyRequest {
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
            "Birthday gift".to_string(),
            "Chores completed".to_string(),
            "Found money".to_string(),
            "Allowance bonus".to_string(),
            "Good grades reward".to_string(),
            "Helping neighbors".to_string(),
            "Selling items".to_string(),
            "Gift from family".to_string(),
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
        
        assert!(service.validate_amount_realtime("10.50").is_ok());
        assert!(service.validate_amount_realtime("").is_err());
        assert!(service.validate_amount_realtime("abc").is_err());
        assert!(service.validate_amount_realtime("-5").is_err());
    }
} 