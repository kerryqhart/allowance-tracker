use serde::{Deserialize, Serialize};
use std::fmt;
use chrono::{Datelike, DateTime, FixedOffset, NaiveDate, Utc};

/// Transaction ID in format: "transaction::<income|expense>::epoch_millis"
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    /// ID of the child this transaction belongs to
    pub child_id: String,
    /// Timestamp with timezone information
    pub date: DateTime<FixedOffset>,  // ✅ FIXED: Now uses proper DateTime object
    /// Description of the transaction (max 256 characters)
    pub description: String,
    /// Transaction amount (positive for income, negative for expense)
    pub amount: f64,
    /// Account balance after this transaction
    pub balance: f64,
    /// Type of transaction for rendering purposes
    pub transaction_type: TransactionType,
}

/// Type of transaction for rendering and business logic
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    /// Regular income transaction (money added)
    Income,
    /// Regular expense transaction (money spent)
    Expense,
    /// Future allowance transaction (not yet received)
    FutureAllowance,
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
    /// Optional date override - uses current time if not provided
    pub date: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// Type of calendar day for explicit rendering logic
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CalendarDayType {
    /// Empty padding day before the start of the month
    PaddingBefore,
    /// Actual day within the month
    MonthDay,
    /// Empty padding day after the end of the month (if needed for grid alignment)
    PaddingAfter,
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
    pub day_type: CalendarDayType,
    #[deprecated(note = "Use day_type instead of is_empty")]
    pub is_empty: bool, // For padding days before/after month
}

/// Request for calendar month data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalendarMonthRequest {
    pub month: u32,
    pub year: u32,
}

/// Represents the current focus date for calendar navigation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalendarFocusDate {
    pub month: u32,
    pub year: u32,
}

impl Default for CalendarFocusDate {
    fn default() -> Self {
        let now = chrono::Local::now();
        Self {
            month: now.month(),
            year: now.year() as u32,
        }
    }
}

/// Request to update the calendar focus date
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateCalendarFocusRequest {
    pub month: u32,
    pub year: u32,
}

/// Response after updating calendar focus date
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateCalendarFocusResponse {
    pub focus_date: CalendarFocusDate,
    pub success_message: String,
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
    pub raw_date: String, // Original RFC 3339 date for chart parsing
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

/// Represents a parental control validation attempt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentalControlAttempt {
    pub id: i64,
    pub attempted_value: String,
    pub timestamp: String,
    pub success: bool,
}

/// Request for parental control validation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentalControlRequest {
    pub answer: String,
}

/// Response from parental control validation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentalControlResponse {
    pub success: bool,
    pub message: String,
}

/// Request for spending money (creating a negative transaction)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpendMoneyRequest {
    pub description: String,
    pub amount: f64,  // User provides positive amount, backend converts to negative
    pub date: Option<DateTime<FixedOffset>>,
}

/// Response after spending money
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpendMoneyResponse {
    pub transaction_id: String,
    pub success_message: String,
    pub new_balance: f64,
    pub formatted_amount: String,
}

/// Request for adding money (creating a positive transaction)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddMoneyRequest {
    pub description: String,
    pub amount: f64,
    pub date: Option<DateTime<FixedOffset>>,
}

/// Response after adding money
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddMoneyResponse {
    pub transaction_id: String,
    pub success_message: String,
    pub new_balance: f64,
    pub formatted_amount: String,
}

/// Request for deleting multiple transactions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeleteTransactionsRequest {
    pub transaction_ids: Vec<String>,
}

/// Response after deleting transactions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeleteTransactionsResponse {
    pub deleted_count: usize,
    pub success_message: String,
    pub not_found_ids: Vec<String>,
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

/// Represents a child in the allowance tracking system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Child {
    pub id: String,
    pub name: String,
    pub birthdate: NaiveDate, // ✅ FIXED: Now uses proper NaiveDate object
    pub created_at: DateTime<Utc>, // ✅ FIXED: Now uses proper DateTime object
    pub updated_at: DateTime<Utc>, // ✅ FIXED: Now uses proper DateTime object
}

/// Request for creating a new child
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateChildRequest {
    pub name: String,
    pub birthdate: String, // ISO 8601 date format (YYYY-MM-DD)
}

/// Request for updating an existing child
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateChildRequest {
    pub name: Option<String>,
    pub birthdate: Option<String>, // ISO 8601 date format (YYYY-MM-DD)
}

/// Response after creating or updating a child
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildResponse {
    pub child: Child,
    pub success_message: String,
}

/// Response containing a list of children
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildListResponse {
    pub children: Vec<Child>,
}

/// Request for setting the active child
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetActiveChildRequest {
    pub child_id: String,
}

/// Response after setting active child
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetActiveChildResponse {
    pub success_message: String,
    pub active_child: Child,
}

/// Response containing the active child information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveChildResponse {
    pub active_child: Option<Child>,
}

/// Response containing current data directory information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetDataDirectoryResponse {
    pub current_path: String,
    pub is_redirected: bool, // True if current location is via redirect file
}

/// Request to relocate data directory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelocateDataDirectoryRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub new_path: String,
}

/// Response after relocating data directory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelocateDataDirectoryResponse {
    pub success: bool,
    pub message: String,
    pub new_path: String,
}

/// Request to revert data directory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevertDataDirectoryRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response after reverting data directory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevertDataDirectoryResponse {
    pub success: bool,
    pub message: String,
    pub was_redirected: bool, // Whether there was actually a redirect to revert
}

/// Request to check if a directory has conflicts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckDataDirectoryConflictRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub new_path: String,
}

/// Response with conflict detection results
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckDataDirectoryConflictResponse {
    pub has_conflict: bool,
    pub conflict_details: Option<String>, // Description of what conflicts were found
    pub can_proceed_safely: bool, // Whether relocation can proceed without user decision
}

/// User decision for resolving data directory conflicts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictResolution {
    /// Overwrite target location with current child data
    OverwriteTarget,
    /// Use target location's data and archive current data
    UseTargetData,
    /// Cancel the operation
    Cancel,
}

/// Request to relocate with conflict resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelocateWithConflictResolutionRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub new_path: String,
    pub resolution: ConflictResolution,
}

/// Response after relocating with conflict resolution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelocateWithConflictResolutionResponse {
    pub success: bool,
    pub message: String,
    pub new_path: String,
    pub archived_to: Option<String>, // Path where original data was archived, if applicable
}

/// Request to return data to default location
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReturnToDefaultLocationRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response after returning to default location
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReturnToDefaultLocationResponse {
    pub success: bool,
    pub message: String,
    pub default_path: String,
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

/// Represents an allowance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllowanceConfig {
    pub child_id: String,
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
    pub created_at: DateTime<Utc>, // ✅ FIXED: Now uses proper DateTime object
    pub updated_at: DateTime<Utc>, // ✅ FIXED: Now uses proper DateTime object
}

/// Request for getting allowance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetAllowanceConfigRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response containing allowance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetAllowanceConfigResponse {
    pub allowance_config: Option<AllowanceConfig>,
}

/// Request for updating allowance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateAllowanceConfigRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
}

/// Response after updating allowance configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateAllowanceConfigResponse {
    pub allowance_config: AllowanceConfig,
    pub success_message: String,
}

/// Current date information from the backend
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CurrentDateResponse {
    pub month: u32,
    pub year: u32,
    pub day: u32,
    pub formatted_date: String, // e.g., "June 19, 2025"
    pub iso_date: String, // e.g., "2025-06-19"
}

// Goal-related types

/// Goal state enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalState {
    Active,
    Cancelled,
    Completed,
}

impl GoalState {
    /// Convert to string for CSV storage
    pub fn to_string(&self) -> String {
        match self {
            GoalState::Active => "active".to_string(),
            GoalState::Cancelled => "cancelled".to_string(),
            GoalState::Completed => "completed".to_string(),
        }
    }

    /// Parse from string for CSV loading
    pub fn from_string(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "active" => Ok(GoalState::Active),
            "cancelled" => Ok(GoalState::Cancelled),
            "completed" => Ok(GoalState::Completed),
            _ => Err(format!("Invalid goal state: {}", s)),
        }
    }
}

/// A savings goal with target amount and description
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Goal {
    pub id: String,
    pub child_id: String,
    pub description: String,
    pub target_amount: f64,
    pub state: GoalState,
    pub created_at: DateTime<Utc>, // ✅ FIXED: Now uses proper DateTime object
    pub updated_at: DateTime<Utc>, // ✅ FIXED: Now uses proper DateTime object
}

impl Goal {
    /// Generate a unique goal ID
    pub fn generate_id(child_id: &str, timestamp_millis: u64) -> String {
        format!("goal::{}::{}", child_id, timestamp_millis)
    }
}

/// Goal completion projection calculations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalCalculation {
    pub current_balance: f64,
    pub amount_needed: f64,
    pub projected_completion_date: Option<String>, // RFC 3339 timestamp, None if not achievable
    pub allowances_needed: u32,
    pub is_achievable: bool,
    pub exceeds_time_limit: bool, // true if takes > 1 year
}

/// Request to create a new goal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub description: String,
    pub target_amount: f64,
}

/// Response after creating a goal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateGoalResponse {
    pub goal: Goal,
    pub calculation: GoalCalculation,
    pub success_message: String,
}

/// Request to update an existing goal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub description: Option<String>,
    pub target_amount: Option<f64>,
}

/// Response after updating a goal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateGoalResponse {
    pub goal: Goal,
    pub calculation: GoalCalculation,
    pub success_message: String,
}

/// Request to get current goal information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetCurrentGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response containing current goal with calculations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetCurrentGoalResponse {
    pub goal: Option<Goal>,
    pub calculation: Option<GoalCalculation>,
}

/// Request to get goal history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetGoalHistoryRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub limit: Option<u32>,
}

/// Response containing goal history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GetGoalHistoryResponse {
    pub goals: Vec<Goal>,
}

/// Request to cancel current goal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CancelGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response after cancelling a goal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CancelGoalResponse {
    pub goal: Goal,
    pub success_message: String,
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

/// Request to export transaction data as CSV
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportDataRequest {
    /// Optional child ID - if None, uses active child
    pub child_id: Option<String>,
}

/// Response containing CSV data for export
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportDataResponse {
    /// CSV content as a string
    pub csv_content: String,
    /// Suggested filename for the export
    pub filename: String,
    /// Number of transactions exported
    pub transaction_count: usize,
    /// Child name for the exported data
    pub child_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportToPathRequest {
    /// Optional child ID - if None, uses active child
    pub child_id: Option<String>,
    /// Optional custom directory path - if None, uses Documents folder
    pub custom_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportToPathResponse {
    /// Whether the export was successful
    pub success: bool,
    /// Success or error message
    pub message: String,
    /// Full path where the file was written
    pub file_path: String,
    /// Number of transactions exported
    pub transaction_count: usize,
    /// Child name for the exported data
    pub child_name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub component: Option<String>,
}

impl AllowanceConfig {
    /// Get the day name for the configured day of week
    pub fn day_name(&self) -> &'static str {
        match self.day_of_week {
            0 => "Sunday",
            1 => "Monday",
            2 => "Tuesday",
            3 => "Wednesday",
            4 => "Thursday",
            5 => "Friday",
            6 => "Saturday",
            _ => "Invalid",
        }
    }

    /// Validate day of week value
    pub fn is_valid_day_of_week(day: u8) -> bool {
        day <= 6
    }
}

impl Child {
    /// Generate a child ID based on timestamp
    pub fn generate_id(epoch_millis: u64) -> String {
        format!("child::{}", epoch_millis)
    }

    /// Parse a child ID to extract the timestamp
    pub fn parse_id(id: &str) -> Result<u64, ChildIdError> {
        let parts: Vec<&str> = id.split("::").collect();
        if parts.len() != 2 || parts[0] != "child" {
            return Err(ChildIdError::InvalidFormat);
        }

        parts[1].parse::<u64>().map_err(|_| ChildIdError::InvalidTimestamp)
    }

    /// Extract timestamp from child ID
    pub fn extract_timestamp(&self) -> Result<u64, ChildIdError> {
        Self::parse_id(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChildIdError {
    InvalidFormat,
    InvalidTimestamp,
}

impl fmt::Display for ChildIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChildIdError::InvalidFormat => write!(f, "Invalid child ID format"),
            ChildIdError::InvalidTimestamp => write!(f, "Invalid timestamp in child ID"),
        }
    }
}

impl std::error::Error for ChildIdError {}

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
            child_id: "test_child_id".to_string(),
            date: Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()), // ✅ FIXED: Use east_opt instead
            description: "Test transaction".to_string(),
            amount: 10.0,
            balance: 100.0,
            transaction_type: TransactionType::Income,
        };

        assert_eq!(transaction.extract_timestamp().unwrap(), 1702516122000);
    }

    #[test]
    fn test_generate_child_id() {
        let child_id = Child::generate_id(1702516122000);
        assert_eq!(child_id, "child::1702516122000");
    }

    #[test]
    fn test_parse_child_id() {
        // Test valid child ID
        let timestamp = Child::parse_id("child::1702516122000").unwrap();
        assert_eq!(timestamp, 1702516122000);

        // Test invalid format
        assert!(Child::parse_id("invalid::format").is_err());
        assert!(Child::parse_id("child").is_err());
        assert!(Child::parse_id("not_child::123").is_err());

        // Test invalid timestamp
        assert!(Child::parse_id("child::not_a_number").is_err());
    }

    #[test]
    fn test_child_extract_timestamp() {
        let child = Child {
            id: "child::1702516122000".to_string(),
            name: "Test Child".to_string(),
            birthdate: NaiveDate::from_ymd_opt(2015, 6, 15).unwrap(),  // ✅ FIXED: Use proper NaiveDate
            created_at: DateTime::parse_from_rfc3339("2023-12-14T01:02:02.000Z").unwrap().with_timezone(&Utc),  // ✅ FIXED: Use proper DateTime
            updated_at: DateTime::parse_from_rfc3339("2023-12-14T01:02:02.000Z").unwrap().with_timezone(&Utc),  // ✅ FIXED: Use proper DateTime
        };

        assert_eq!(child.extract_timestamp().unwrap(), 1702516122000);
    }

    #[test]
    fn test_allowance_config_day_names() {
        let days = [
            (0, "Sunday"),
            (1, "Monday"),
            (2, "Tuesday"),
            (3, "Wednesday"),
            (4, "Thursday"),
            (5, "Friday"),
            (6, "Saturday"),
            (7, "Invalid"),
        ];

        for (day_num, expected_name) in days {
            let config = AllowanceConfig {
                child_id: "test".to_string(),
                amount: 10.0,
                day_of_week: day_num,
                is_active: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            assert_eq!(config.day_name(), expected_name);
        }
    }

    #[test]
    fn test_allowance_config_is_valid_day_of_week() {
        assert!(AllowanceConfig::is_valid_day_of_week(0));
        assert!(AllowanceConfig::is_valid_day_of_week(1));
        assert!(AllowanceConfig::is_valid_day_of_week(6));
        assert!(!AllowanceConfig::is_valid_day_of_week(7));
        assert!(!AllowanceConfig::is_valid_day_of_week(255));
    }
}
