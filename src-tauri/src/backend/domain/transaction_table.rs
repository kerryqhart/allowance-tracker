//! Transaction table domain logic for the allowance tracker.
//!
//! This module contains all business logic related to transaction table display,
//! formatting, validation, and presentation. It handles the transformation of
//! raw transaction data into formatted, user-friendly table representations.
//!
//! ## Key Responsibilities
//!
//! - **Table Formatting**: Converting raw transactions into formatted display data
//! - **Amount Formatting**: Configurable currency and sign display options
//! - **Date Formatting**: Multiple date format options (ISO, short, long)
//! - **Input Validation**: Validating transaction form inputs before submission
//! - **CSS Classification**: Providing styling hints for positive/negative amounts
//! - **Configuration Management**: Flexible display configuration options
//!
//! ## Core Components
//!
//! - **TransactionTableService**: Main service for table operations
//! - **TransactionTableConfig**: Configuration for display preferences
//! - **FormattedTransaction**: Structured data for table display
//! - **ValidationResult**: Input validation results with error details
//!
//! ## Design Principles
//!
//! - **Presentation Logic**: Focused specifically on table display concerns
//! - **Configuration Driven**: Flexible formatting options for different use cases
//! - **Type Safety**: Strong typing for all formatting operations
//! - **Validation First**: Comprehensive input validation with detailed error messages
//! - **UI Agnostic**: Pure formatting logic independent of specific UI frameworks

use shared::{Transaction, FormattedTransaction, AmountType, ValidationResult, ValidationError};
use anyhow::Result;
use serde::{Serialize, Deserialize};

/// Configuration for transaction table display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransactionTableConfig {
    pub show_currency_symbol: bool,
    pub decimal_places: u8,
    pub date_format: DateFormat,
    pub amount_format: AmountFormat,
}

/// Date formatting options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DateFormat {
    MonthDayYear,  // "June 13, 2025"
    ShortDate,     // "06/13/2025"
    ISO,           // "2025-06-13"
}

/// Amount formatting options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AmountFormat {
    PlusMinusSign,    // "+$10.00" / "-$5.00"
    ParenthesesNeg,   // "$10.00" / "($5.00)"
    ColorOnly,        // "$10.00" (styled with color)
}

/// Transaction table service that handles all table-related business logic
#[derive(Clone)]
pub struct TransactionTableService {
    config: TransactionTableConfig,
}

impl TransactionTableService {
    /// Create a new TransactionTableService with default configuration
    pub fn new() -> Self {
        Self {
            config: TransactionTableConfig::default(),
        }
    }

    /// Create a new TransactionTableService with custom configuration
    pub fn with_config(config: TransactionTableConfig) -> Self {
        Self { config }
    }

    /// Format a list of transactions for table display
    pub fn format_transactions_for_table(&self, transactions: &[Transaction]) -> Vec<FormattedTransaction> {
        transactions
            .iter()
            .map(|tx| self.format_single_transaction(tx))
            .collect()
    }

    /// Format a single transaction for display
    pub fn format_single_transaction(&self, transaction: &Transaction) -> FormattedTransaction {
        FormattedTransaction {
            id: transaction.id.clone(),
            formatted_date: self.format_date(&transaction.date),
            description: transaction.description.clone(),
            formatted_amount: self.format_amount(transaction.amount),
            amount_type: self.classify_amount(transaction.amount),
            formatted_balance: self.format_balance(transaction.balance),
            raw_amount: transaction.amount,
            raw_balance: transaction.balance,
            raw_date: transaction.date.clone(),
        }
    }

    /// Format a date for display based on configuration
    pub fn format_date(&self, rfc3339_date: &str) -> String {
        if let Some((year, month, day)) = self.parse_date(rfc3339_date) {
            match self.config.date_format {
                DateFormat::MonthDayYear => {
                    format!("{} {}, {}", self.month_name(month), day, year)
                }
                DateFormat::ShortDate => {
                    format!("{:02}/{:02}/{}", month, day, year)
                }
                DateFormat::ISO => {
                    format!("{}-{:02}-{:02}", year, month, day)
                }
            }
        } else {
            // Fallback to original string
            rfc3339_date.to_string()
        }
    }

    /// Format an amount for display based on configuration
    pub fn format_amount(&self, amount: f64) -> String {
        let abs_amount = amount.abs();
        let currency = if self.config.show_currency_symbol { "$" } else { "" };
        let formatted_value = format!("{}{:.2}", currency, abs_amount);

        match self.config.amount_format {
            AmountFormat::PlusMinusSign => {
                if amount >= 0.0 {
                    format!("+{}", formatted_value)
                } else {
                    format!("-{}", formatted_value)
                }
            }
            AmountFormat::ParenthesesNeg => {
                if amount >= 0.0 {
                    formatted_value
                } else {
                    format!("({})", formatted_value)
                }
            }
            AmountFormat::ColorOnly => formatted_value,
        }
    }

    /// Format a balance for display
    pub fn format_balance(&self, balance: f64) -> String {
        let currency = if self.config.show_currency_symbol { "$" } else { "" };
        format!("{}{:.2}", currency, balance)
    }

    /// Classify amount type for styling purposes
    pub fn classify_amount(&self, amount: f64) -> AmountType {
        if amount > 0.0 {
            AmountType::Positive
        } else if amount < 0.0 {
            AmountType::Negative
        } else {
            AmountType::Zero
        }
    }

    /// Get CSS class name for amount styling
    pub fn amount_css_class(&self, amount: f64) -> &'static str {
        match self.classify_amount(amount) {
            AmountType::Positive => "amount positive",
            AmountType::Negative => "amount negative",
            AmountType::Zero => "amount zero",
        }
    }

    /// Validate transaction form input
    pub fn validate_transaction_input(&self, description: &str, amount_input: &str) -> ValidationResult {
        let mut errors = Vec::new();

        // Validate description
        if description.trim().is_empty() {
            errors.push(ValidationError::EmptyDescription);
        } else if description.len() > 256 {
            errors.push(ValidationError::DescriptionTooLong(description.len()));
        }

        // Validate and parse amount
        let cleaned_amount = match self.clean_and_parse_amount(amount_input) {
            Ok(amount) => {
                if amount <= 0.0 {
                    errors.push(ValidationError::AmountNotPositive);
                    None
                } else if amount > 1_000_000.0 {
                    errors.push(ValidationError::AmountTooLarge);
                    None
                } else if amount < 0.01 {
                    errors.push(ValidationError::AmountTooSmall);
                    None
                } else {
                    Some(amount)
                }
            }
            Err(parse_error) => {
                errors.push(ValidationError::InvalidAmount(parse_error.to_string()));
                None
            }
        };

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            cleaned_amount,
        }
    }

    /// Clean and parse amount input string
    pub fn clean_and_parse_amount(&self, amount_input: &str) -> Result<f64> {
        // Clean the input - remove dollar signs, spaces, commas
        let cleaned = amount_input
            .trim()
            .replace("$", "")
            .replace(",", "")
            .replace(" ", "");

        // Try to parse as float
        cleaned.parse::<f64>()
            .map_err(|e| anyhow::anyhow!("Invalid number format: {}", e))
    }

    /// Parse RFC 3339 date string to extract year, month, day
    fn parse_date(&self, date_str: &str) -> Option<(u32, u32, u32)> {
        // Parse RFC 3339 date (e.g., "2025-06-13T09:00:00-04:00")
        if let Some(date_part) = date_str.split('T').next() {
            let parts: Vec<&str> = date_part.split('-').collect();
            if parts.len() == 3 {
                if let (Ok(year), Ok(month), Ok(day)) = (
                    parts[0].parse::<u32>(),
                    parts[1].parse::<u32>(),
                    parts[2].parse::<u32>(),
                ) {
                    return Some((year, month, day));
                }
            }
        }
        None
    }

    /// Get human-readable month name
    fn month_name(&self, month: u32) -> &'static str {
        match month {
            1 => "January", 2 => "February", 3 => "March", 4 => "April",
            5 => "May", 6 => "June", 7 => "July", 8 => "August",
            9 => "September", 10 => "October", 11 => "November", 12 => "December",
            _ => "Invalid Month",
        }
    }

    /// Get error message for validation error
    pub fn validation_error_message(&self, error: &ValidationError) -> String {
        match error {
            ValidationError::EmptyDescription => "Please enter a description".to_string(),
            ValidationError::DescriptionTooLong(len) => {
                format!("Description is too long ({} characters). Maximum is 256.", len)
            }
            ValidationError::InvalidAmount(msg) => {
                format!("Please enter a valid amount (like 5 or 5.00): {}", msg)
            }
            ValidationError::AmountNotPositive => "Amount must be greater than 0".to_string(),
            ValidationError::AmountTooLarge => "Amount is too large. Maximum is $1,000,000".to_string(),
            ValidationError::AmountTooSmall => "Amount is too small. Minimum is $0.01".to_string(),
        }
    }

    /// Get all validation error messages as a single string
    pub fn validation_error_messages(&self, errors: &[ValidationError]) -> Vec<String> {
        errors.iter().map(|e| self.validation_error_message(e)).collect()
    }
}

impl Default for TransactionTableService {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TransactionTableConfig {
    fn default() -> Self {
        Self {
            show_currency_symbol: true,
            decimal_places: 2,
            date_format: DateFormat::MonthDayYear,
            amount_format: AmountFormat::PlusMinusSign,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_transaction(id: &str, date: &str, description: &str, amount: f64, balance: f64) -> Transaction {
        Transaction {
            id: id.to_string(),
            child_id: "test_child_id".to_string(),
            date: date.to_string(),
            description: description.to_string(),
            amount,
            balance,
            transaction_type: if amount >= 0.0 { shared::TransactionType::Income } else { shared::TransactionType::Expense },
        }
    }

    #[test]
    fn test_format_single_transaction() {
        let service = TransactionTableService::new();
        let transaction = create_test_transaction(
            "test_1",
            "2025-06-13T09:00:00-04:00",
            "Test transaction",
            10.50,
            100.50
        );

        let formatted = service.format_single_transaction(&transaction);

        assert_eq!(formatted.id, "test_1");
        assert_eq!(formatted.formatted_date, "June 13, 2025");
        assert_eq!(formatted.description, "Test transaction");
        assert_eq!(formatted.formatted_amount, "+$10.50");
        assert_eq!(formatted.amount_type, AmountType::Positive);
        assert_eq!(formatted.formatted_balance, "$100.50");
        assert_eq!(formatted.raw_amount, 10.50);
        assert_eq!(formatted.raw_balance, 100.50);
    }

    #[test]
    fn test_format_negative_amount() {
        let service = TransactionTableService::new();
        let transaction = create_test_transaction(
            "test_2",
            "2025-06-13T09:00:00-04:00",
            "Expense",
            -5.25,
            95.25
        );

        let formatted = service.format_single_transaction(&transaction);

        assert_eq!(formatted.formatted_amount, "-$5.25");
        assert_eq!(formatted.amount_type, AmountType::Negative);
    }

    #[test]
    fn test_different_date_formats() {
        let mut config = TransactionTableConfig::default();
        
        // Test short date format
        config.date_format = DateFormat::ShortDate;
        let service = TransactionTableService::with_config(config.clone());
        assert_eq!(service.format_date("2025-06-13T09:00:00-04:00"), "06/13/2025");

        // Test ISO format
        config.date_format = DateFormat::ISO;
        let service = TransactionTableService::with_config(config);
        assert_eq!(service.format_date("2025-06-13T09:00:00-04:00"), "2025-06-13");
    }

    #[test]
    fn test_different_amount_formats() {
        let mut config = TransactionTableConfig::default();
        
        // Test parentheses format
        config.amount_format = AmountFormat::ParenthesesNeg;
        let service = TransactionTableService::with_config(config.clone());
        assert_eq!(service.format_amount(10.0), "$10.00");
        assert_eq!(service.format_amount(-10.0), "($10.00)");

        // Test color only format
        config.amount_format = AmountFormat::ColorOnly;
        let service = TransactionTableService::with_config(config);
        assert_eq!(service.format_amount(10.0), "$10.00");
        assert_eq!(service.format_amount(-10.0), "$10.00");
    }

    #[test]
    fn test_amount_classification() {
        let service = TransactionTableService::new();
        
        assert_eq!(service.classify_amount(10.0), AmountType::Positive);
        assert_eq!(service.classify_amount(-5.0), AmountType::Negative);
        assert_eq!(service.classify_amount(0.0), AmountType::Zero);
    }

    #[test]
    fn test_css_class_generation() {
        let service = TransactionTableService::new();
        
        assert_eq!(service.amount_css_class(10.0), "amount positive");
        assert_eq!(service.amount_css_class(-5.0), "amount negative");
        assert_eq!(service.amount_css_class(0.0), "amount zero");
    }

    #[test]
    fn test_clean_and_parse_amount() {
        let service = TransactionTableService::new();
        
        assert_eq!(service.clean_and_parse_amount("10.50").unwrap(), 10.50);
        assert_eq!(service.clean_and_parse_amount("$10.50").unwrap(), 10.50);
        assert_eq!(service.clean_and_parse_amount(" $1,234.56 ").unwrap(), 1234.56);
        assert_eq!(service.clean_and_parse_amount("5").unwrap(), 5.0);
        
        assert!(service.clean_and_parse_amount("abc").is_err());
        assert!(service.clean_and_parse_amount("").is_err());
    }

    #[test]
    fn test_validation_success() {
        let service = TransactionTableService::new();
        
        let result = service.validate_transaction_input("Valid description", "10.50");
        
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.cleaned_amount, Some(10.50));
    }

    #[test]
    fn test_validation_errors() {
        let service = TransactionTableService::new();
        
        // Empty description
        let result = service.validate_transaction_input("", "10.50");
        assert!(!result.is_valid);
        assert!(matches!(result.errors[0], ValidationError::EmptyDescription));
        
        // Invalid amount
        let result = service.validate_transaction_input("Valid", "abc");
        assert!(!result.is_valid);
        assert!(matches!(result.errors[0], ValidationError::InvalidAmount(_)));
        
        // Negative amount
        let result = service.validate_transaction_input("Valid", "-5.00");
        assert!(!result.is_valid);
        assert!(matches!(result.errors[0], ValidationError::AmountNotPositive));
        
        // Zero amount
        let result = service.validate_transaction_input("Valid", "0");
        assert!(!result.is_valid);
        assert!(matches!(result.errors[0], ValidationError::AmountNotPositive));
    }

    #[test]
    fn test_validation_error_messages() {
        let service = TransactionTableService::new();
        
        let error = ValidationError::EmptyDescription;
        assert_eq!(service.validation_error_message(&error), "Please enter a description");
        
        let error = ValidationError::AmountNotPositive;
        assert_eq!(service.validation_error_message(&error), "Amount must be greater than 0");
    }

    #[test]
    fn test_format_transactions_for_table() {
        let service = TransactionTableService::new();
        let transactions = vec![
            create_test_transaction("1", "2025-06-13T09:00:00-04:00", "Income", 10.0, 10.0),
            create_test_transaction("2", "2025-06-12T15:30:00-04:00", "Expense", -5.0, 5.0),
        ];

        let formatted = service.format_transactions_for_table(&transactions);

        assert_eq!(formatted.len(), 2);
        assert_eq!(formatted[0].formatted_amount, "+$10.00");
        assert_eq!(formatted[1].formatted_amount, "-$5.00");
    }
} 