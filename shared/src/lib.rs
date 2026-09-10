pub mod sync;
pub mod child_id;

pub use child_id::ChildId;

use serde::{Deserialize, Serialize};
use std::fmt;
use chrono::{Datelike, DateTime, FixedOffset, NaiveDate, Utc};

/// A financial transaction representing money in or out.
///
/// Transactions are the core data type for tracking allowance money.
/// Each transaction has a unique ID, amount (positive for income, negative for expenses),
/// description, date, and running balance after the transaction.
///
/// # Fields
/// - `id`: Unique identifier in format "transaction::{income|expense}::{timestamp_ms}"
/// - `amount`: Positive for deposits/allowances, negative for spending
/// - `description`: User-provided description of the transaction
/// - `date`: When the transaction occurred (with timezone)
/// - `balance`: Running balance after this transaction
/// - `transaction_type`: Categorizes as Income or Expense for display
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    /// ID of the child this transaction belongs to
    pub child_id: String,
    /// Timestamp with timezone information
    pub date: DateTime<FixedOffset>,  // FIXED: Now uses proper DateTime object
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
    /// Automatically-issued allowances
    Allowance,
    /// Manually-added positive amounts (was Income)
    OneOffIncome,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CalendarDayType {
    /// Empty padding day before the start of the month
    PaddingBefore,
    /// Actual day within the month
    MonthDay,
    /// Empty padding day after the end of the month (if needed for grid alignment)
    PaddingAfter,
}

/// Represents a calendar month with its associated transaction data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarMonth {
    pub month: u32,
    pub year: u32,
    pub days: Vec<CalendarDay>,
    pub first_day_of_week: u32, // 0 = Sunday, 1 = Monday, etc.
}

/// A single day in the calendar view.
///
/// Represents one cell in the monthly calendar, containing all transactions
/// for that day and summary information for display.
///
/// # Fields
/// - `day`: The calendar date (1-31)
/// - `balance`: Running balance at end of this day
/// - `transactions`: All transactions that occurred on this day
/// - `day_type`: Whether this is a normal day, today, or padding
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarDay {
    pub day: u32,
    pub balance: f64,
    pub transactions: Vec<Transaction>,
    pub day_type: CalendarDayType,
}

/// Request for calendar month data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarMonthRequest {
    pub month: u32,
    pub year: u32,
}

/// Represents the current focus date for calendar navigation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateCalendarFocusRequest {
    pub month: u32,
    pub year: u32,
}

/// Response after updating calendar focus date
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateCalendarFocusResponse {
    pub focus_date: CalendarFocusDate,
    pub success_message: String,
}

/// Represents a formatted transaction for display purposes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AmountType {
    Positive,
    Negative,
    Zero,
}

/// Validation result for transaction form input
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub cleaned_amount: Option<f64>,
    pub suggestions: Vec<String>,
}


/// Request for formatted transaction table data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionTableRequest {
    pub limit: Option<u32>,
    pub after: Option<String>,
}

/// Response containing formatted transaction table data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionTableResponse {
    pub formatted_transactions: Vec<FormattedTransaction>,
    pub pagination: PaginationInfo,
}

/// Represents a parental control validation attempt
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParentalControlAttempt {
    pub id: i64,
    pub attempted_value: String,
    pub timestamp: String,
    pub success: bool,
}

/// Request for parental control validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParentalControlRequest {
    pub answer: String,
}

/// Response from parental control validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParentalControlResponse {
    pub success: bool,
    pub message: String,
}

/// Request for spending money (creating a negative transaction)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpendMoneyRequest {
    pub description: String,
    pub amount: f64,  // User provides positive amount, backend converts to negative
    pub date: Option<DateTime<FixedOffset>>,
}

/// Response after spending money
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpendMoneyResponse {
    pub transaction_id: String,
    pub success_message: String,
    pub new_balance: f64,
    pub formatted_amount: String,
}

/// Request for adding money (creating a positive transaction)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddMoneyRequest {
    pub description: String,
    pub amount: f64,
    pub date: Option<DateTime<FixedOffset>>,
}

/// Response after adding money
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddMoneyResponse {
    pub transaction_id: String,
    pub success_message: String,
    pub new_balance: f64,
    pub formatted_amount: String,
}

/// Request for deleting multiple transactions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteTransactionsRequest {
    pub transaction_ids: Vec<String>,
}

/// Response after deleting transactions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteTransactionsResponse {
    pub deleted_count: usize,
    pub success_message: String,
    pub not_found_ids: Vec<String>,
}

/// Specific validation errors for form input
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationError {
    EmptyDescription,
    DescriptionTooLong(usize),
    EmptyAmount,
    InvalidAmountFormat(String),
    AmountNotPositive,
    AmountTooSmall(f64),
    AmountTooLarge(f64),
    AmountPrecisionTooHigh,
}

/// A child whose allowance is being tracked.
///
/// Each child has their own transaction history, goals, and allowance configuration.
/// The child's data is stored in a dedicated directory named after their sanitized name.
///
/// # Fields
/// - `id`: Unique identifier in format "child::{timestamp_ms}"
/// - `name`: Display name of the child
/// - `birthdate`: Used for age-appropriate features and display
/// - `created_at`/`updated_at`: Audit timestamps
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Child {
    pub id: String,
    pub name: String,
    pub birthdate: NaiveDate, // FIXED: Now uses proper NaiveDate object
    pub created_at: DateTime<Utc>, // FIXED: Now uses proper DateTime object
    pub updated_at: DateTime<Utc>, // FIXED: Now uses proper DateTime object
}

/// Request for creating a new child
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateChildRequest {
    pub name: String,
    pub birthdate: String, // ISO 8601 date format (YYYY-MM-DD)
}

/// Request for updating an existing child
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateChildRequest {
    pub name: Option<String>,
    pub birthdate: Option<String>, // ISO 8601 date format (YYYY-MM-DD)
}

/// Response after creating or updating a child
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildResponse {
    pub child: Child,
    pub success_message: String,
}

/// Response containing a list of children
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildListResponse {
    pub children: Vec<Child>,
}

/// Request for setting the active child
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetActiveChildRequest {
    pub child_id: String,
}

/// Response after setting active child
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetActiveChildResponse {
    pub success_message: String,
    pub active_child: Child,
}

/// Response containing the active child information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveChildResponse {
    pub active_child: Option<Child>,
}

/// Configuration for money management forms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllowanceConfig {
    pub child_id: String,
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
    #[serde(default)]
    pub use_age_based_amount: bool, // If true, amount = child's age in years
    pub created_at: DateTime<Utc>, // FIXED: Now uses proper DateTime object
    pub updated_at: DateTime<Utc>, // FIXED: Now uses proper DateTime object
}

/// Request for getting allowance configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetAllowanceConfigRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response containing allowance configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetAllowanceConfigResponse {
    pub allowance_config: Option<AllowanceConfig>,
}

/// Request for updating allowance configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateAllowanceConfigRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
    #[serde(default)]
    pub use_age_based_amount: bool,
}

/// Response after updating allowance configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateAllowanceConfigResponse {
    pub allowance_config: AllowanceConfig,
    pub success_message: String,
}

/// Current date information from the backend
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentDateResponse {
    pub month: u32,
    pub year: u32,
    pub day: u32,
    pub formatted_date: String, // e.g., "June 19, 2025"
    pub iso_date: String, // e.g., "2025-06-19"
}

// Goal-related types

/// Goal state enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// A savings goal the child is working toward.
///
/// Goals track progress toward a target amount and can calculate
/// estimated completion dates based on allowance frequency.
///
/// # Fields
/// - `id`: Unique identifier in format "goal::{child_id}::{timestamp_ms}"
/// - `child_id`: The child this goal belongs to
/// - `description`: What the child is saving for
/// - `target_amount`: How much money is needed
/// - `state`: Active, Completed, or Cancelled
/// - `created_at`/`updated_at`: Lifecycle timestamps
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub child_id: String,
    pub description: String,
    pub target_amount: f64,
    pub state: GoalState,
    pub created_at: DateTime<Utc>, // FIXED: Now uses proper DateTime object
    pub updated_at: DateTime<Utc>, // FIXED: Now uses proper DateTime object
}

impl Goal {
    /// Generate a unique goal ID
    pub fn generate_id(child_id: &str, timestamp_millis: u64) -> String {
        format!("goal::{}::{}::{:04x}", child_id, timestamp_millis, rand::random::<u16>())
    }
}

/// Goal completion projection calculations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalCalculation {
    pub current_balance: f64,
    pub amount_needed: f64,
    pub projected_completion_date: Option<String>, // RFC 3339 timestamp, None if not achievable
    pub allowances_needed: u32,
    pub is_achievable: bool,
    pub exceeds_time_limit: bool, // true if takes > 1 year
}

/// Request to create a new goal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub description: String,
    pub target_amount: f64,
}

/// Response after creating a goal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateGoalResponse {
    pub goal: Goal,
    pub calculation: GoalCalculation,
    pub success_message: String,
}

/// Request to update an existing goal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub description: Option<String>,
    pub target_amount: Option<f64>,
}

/// Response after updating a goal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateGoalResponse {
    pub goal: Goal,
    pub calculation: GoalCalculation,
    pub success_message: String,
}

/// Request to get current goal information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCurrentGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response containing current goal with calculations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetCurrentGoalResponse {
    pub goal: Option<Goal>,
    pub calculation: Option<GoalCalculation>,
}

/// Request to get goal history
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetGoalHistoryRequest {
    pub child_id: Option<String>, // If None, uses active child
    pub limit: Option<u32>,
}

/// Response containing goal history
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetGoalHistoryResponse {
    pub goals: Vec<Goal>,
}

/// Request to cancel current goal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelGoalRequest {
    pub child_id: Option<String>, // If None, uses active child
}

/// Response after cancelling a goal
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelGoalResponse {
    pub goal: Goal,
    pub success_message: String,
}


/// Request to export transaction data as CSV
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportDataRequest {
    /// Optional child ID - if None, uses active child
    pub child_id: Option<String>,
}

/// Response containing CSV data for export
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportToPathRequest {
    /// Optional child ID - if None, uses active child
    pub child_id: Option<String>,
    /// Optional custom directory path - if None, uses Documents folder
    pub custom_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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


#[cfg(test)]
mod tests {
    use super::*;


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
                use_age_based_amount: false,
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

    #[test]
    fn goal_suffix_is_random_not_clock_derived() {
        // The old implementation used timestamp-only, which produced identical ids.
        // The new implementation uses a random suffix. This test discriminates by checking
        // ordering: clock-derived suffixes give ~0-2 descending steps (only at wraps),
        // random gives ~50%. Counting distinctness alone cannot tell them apart.
        let ids: Vec<String> = (0..200)
            .map(|_| Goal::generate_id("child123", 1_702_516_125_000))
            .collect();

        let suffixes: Vec<u16> = ids.iter()
            .map(|id| {
                let parts: Vec<&str> = id.split("::").collect();
                u16::from_str_radix(parts[3], 16).unwrap()
            })
            .collect();

        let descending_steps = suffixes.windows(2)
            .filter(|w| w[1] < w[0])
            .count();

        // Clock-derived: ~0-2. Random: ~100. Assert > 30 to be well above clock but below random.
        assert!(descending_steps > 30, "only {} descending steps in 200 draws (expected ~100 for random)", descending_steps);
    }
}
