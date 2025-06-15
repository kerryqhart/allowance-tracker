use serde::{Deserialize, Serialize};
use std::fmt;

/// Transaction ID in format: "transaction::<income|expense>::epoch_millis"
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    /// Human-readable timestamp with timezone (RFC 3339)
    pub date: String,
    /// Description of the transaction (max 256 characters)
    pub description: String,
    /// Transaction amount (positive for income, negative for expense)
    pub amount: f64,
    /// Account balance after this transaction
    pub balance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionListRequest {
    /// Cursor for pagination - transaction ID to start after
    pub after: Option<String>,
    /// Maximum number of transactions to return
    pub limit: Option<u32>,
    /// Start date for filtering (RFC 3339)
    pub start_date: Option<String>,
    /// End date for filtering (RFC 3339)
    pub end_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionListResponse {
    pub transactions: Vec<Transaction>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateTransactionRequest {
    /// Description of the transaction (max 256 characters)
    pub description: String,
    /// Transaction amount (positive for income, negative for expense)
    pub amount: f64,
    /// Optional date override (RFC 3339) - uses current time if not provided
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// Represents a calendar month with its associated transaction data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalendarMonth {
    pub month: u32,
    pub year: u32,
    pub days: Vec<CalendarDay>,
    pub first_day_of_week: u32, // 0 = Sunday, 1 = Monday, etc.
}

/// Represents a single day in the calendar
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalendarDay {
    pub day: u32,
    pub balance: f64,
    pub transactions: Vec<Transaction>,
    pub is_empty: bool, // For padding days before/after month
}

/// Request for calendar month data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalendarMonthRequest {
    pub month: u32,
    pub year: u32,
}

/// Represents a formatted transaction for display purposes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormattedTransaction {
    pub id: String,
    pub formatted_date: String,
    pub description: String,
    pub formatted_amount: String,
    pub amount_type: AmountType,
    pub formatted_balance: String,
    pub raw_amount: f64,
    pub raw_balance: f64,
}

/// Type of transaction amount for styling and display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AmountType {
    Positive,
    Negative,
    Zero,
}

/// Validation result for transaction form input
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub cleaned_amount: Option<f64>,
}

/// Specific validation errors
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ValidationError {
    EmptyDescription,
    DescriptionTooLong(usize),
    InvalidAmount(String),
    AmountNotPositive,
    AmountTooLarge,
    AmountTooSmall,
}

/// Request for formatted transaction table data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransactionTableRequest {
    pub limit: Option<u32>,
    pub after: Option<String>,
}

/// Response containing formatted transaction table data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransactionTableResponse {
    pub formatted_transactions: Vec<FormattedTransaction>,
    pub pagination: PaginationInfo,
}

/// Request for adding money (creating a positive transaction)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddMoneyRequest {
    pub description: String,
    pub amount: f64,
    pub date: Option<String>,
}

/// Response after adding money
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddMoneyResponse {
    pub transaction_id: String,
    pub success_message: String,
    pub new_balance: f64,
    pub formatted_amount: String,
}

/// Form validation result specific to money management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoneyFormValidation {
    pub is_valid: bool,
    pub errors: Vec<MoneyValidationError>,
    pub cleaned_amount: Option<f64>,
    pub suggestions: Vec<String>,
}

/// Specific validation errors for money forms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MoneyValidationError {
    EmptyDescription,
    DescriptionTooLong(usize),
    EmptyAmount,
    InvalidAmountFormat(String),
    AmountNotPositive,
    AmountTooSmall(f64),
    AmountTooLarge(f64),
    AmountPrecisionTooHigh,
}

/// State for managing money input forms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoneyFormState {
    pub description: String,
    pub amount_input: String,
    pub is_submitting: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub show_success: bool,
}

/// Configuration for money management forms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoneyManagementConfig {
    pub max_description_length: usize,
    pub min_amount: f64,
    pub max_amount: f64,
    pub success_message_duration_ms: u64,
    pub currency_symbol: String,
    pub enable_debug_logging: bool,
}

impl Default for MoneyManagementConfig {
    fn default() -> Self {
        Self {
            max_description_length: 256,
            min_amount: 0.01,
            max_amount: 1_000_000.0,
            success_message_duration_ms: 3000,
            currency_symbol: "$".to_string(),
            enable_debug_logging: false,
        }
    }
}

impl Transaction {
    /// Generate transaction ID from amount and timestamp
    pub fn generate_id(amount: f64, epoch_millis: u64) -> String {
        let transaction_type = if amount < 0.0 { "expense" } else { "income" };
        format!("transaction::{}::{}", transaction_type, epoch_millis)
    }

    /// Parse transaction ID to extract components
    pub fn parse_id(id: &str) -> Result<(String, u64), TransactionIdError> {
        let parts: Vec<&str> = id.split("::").collect();
        if parts.len() != 3 || parts[0] != "transaction" {
            return Err(TransactionIdError::InvalidFormat);
        }

        let transaction_type = parts[1];
        if transaction_type != "income" && transaction_type != "expense" {
            return Err(TransactionIdError::InvalidType);
        }

        let epoch_millis = parts[2]
            .parse::<u64>()
            .map_err(|_| TransactionIdError::InvalidTimestamp)?;

        Ok((transaction_type.to_string(), epoch_millis))
    }

    /// Extract epoch timestamp from transaction ID for sorting
    pub fn extract_timestamp(&self) -> Result<u64, TransactionIdError> {
        Self::parse_id(&self.id).map(|(_, timestamp)| timestamp)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransactionIdError {
    InvalidFormat,
    InvalidType,
    InvalidTimestamp,
}

impl fmt::Display for TransactionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionIdError::InvalidFormat => write!(f, "Invalid transaction ID format"),
            TransactionIdError::InvalidType => write!(f, "Invalid transaction type"),
            TransactionIdError::InvalidTimestamp => write!(f, "Invalid timestamp in transaction ID"),
        }
    }
}

impl std::error::Error for TransactionIdError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_transaction_id() {
        // Test income transaction
        let income_id = Transaction::generate_id(10.0, 1702516122000);
        assert_eq!(income_id, "transaction::income::1702516122000");

        // Test expense transaction
        let expense_id = Transaction::generate_id(-5.0, 1702516125000);
        assert_eq!(expense_id, "transaction::expense::1702516125000");

        // Test zero amount (should be income)
        let zero_id = Transaction::generate_id(0.0, 1702516130000);
        assert_eq!(zero_id, "transaction::income::1702516130000");
    }

    #[test]
    fn test_parse_transaction_id() {
        // Test valid income ID
        let (tx_type, timestamp) = Transaction::parse_id("transaction::income::1702516122000").unwrap();
        assert_eq!(tx_type, "income");
        assert_eq!(timestamp, 1702516122000);

        // Test valid expense ID
        let (tx_type, timestamp) = Transaction::parse_id("transaction::expense::1702516125000").unwrap();
        assert_eq!(tx_type, "expense");
        assert_eq!(timestamp, 1702516125000);

        // Test invalid format
        assert!(Transaction::parse_id("invalid::format").is_err());
        assert!(Transaction::parse_id("transaction::income").is_err());
        assert!(Transaction::parse_id("not_transaction::income::123").is_err());

        // Test invalid type
        assert!(Transaction::parse_id("transaction::invalid::123").is_err());

        // Test invalid timestamp
        assert!(Transaction::parse_id("transaction::income::not_a_number").is_err());
    }

    #[test]
    fn test_extract_timestamp() {
        let transaction = Transaction {
            id: "transaction::income::1702516122000".to_string(),
            date: "2023-12-14T01:02:02.000Z".to_string(),
            description: "Test transaction".to_string(),
            amount: 10.0,
            balance: 100.0,
        };

        assert_eq!(transaction.extract_timestamp().unwrap(), 1702516122000);
    }
}
