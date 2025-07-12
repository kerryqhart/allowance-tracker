//! Calendar domain logic for the allowance tracker.
//!
//! This module contains all business logic related to calendar operations,
//! date calculations, and transaction organization by date. The UI should
//! only handle presentation concerns, while all calendar computations
//! and business rules are handled here.

use shared::{Transaction, TransactionType, CalendarMonth, CalendarDay, CalendarDayType, CurrentDateResponse, CalendarFocusDate};
use std::collections::HashMap;
use chrono::{Local, Datelike};
use std::sync::{Arc, Mutex};
use log::{self, info};

// Add imports for the new orchestration method
use crate::backend::domain::transaction_service::TransactionService;
use crate::backend::domain::commands::transactions::CalendarTransactionsQuery;
use crate::backend::storage::Connection;
use anyhow::Result;

// We need to create a TransactionMapper module - for now let's create a simple placeholder
struct TransactionMapper;

impl TransactionMapper {
    pub fn to_dto(transaction: crate::backend::domain::models::transaction::Transaction) -> Transaction {
        Transaction {
            id: transaction.id,
            date: transaction.date,
            amount: transaction.amount,
            description: transaction.description,
            transaction_type: match transaction.transaction_type {
                crate::backend::domain::models::transaction::TransactionType::Income => TransactionType::Income,
                crate::backend::domain::models::transaction::TransactionType::Expense => TransactionType::Expense,
                crate::backend::domain::models::transaction::TransactionType::FutureAllowance => TransactionType::FutureAllowance,
            },
            balance: transaction.balance,
            child_id: transaction.child_id,
        }
    }
}

/// Calendar service that handles all calendar-related business logic
#[derive(Clone)]
pub struct CalendarService {
    /// Current focus date for calendar navigation (month/year only)
    /// This is kept in memory and not persisted to database
    current_focus_date: Arc<Mutex<CalendarFocusDate>>,
}

impl CalendarService {
    /// Create a new CalendarService instance
    pub fn new() -> Self {
        Self {
            current_focus_date: Arc::new(Mutex::new(CalendarFocusDate::default())),
        }
    }

    /// Get calendar month with transactions - orchestrates transaction retrieval and calendar generation
    /// This method moves the orchestration logic from the REST API layer into the domain layer
    pub fn get_calendar_month_with_transactions<C: Connection>(
        &self,
        month: u32,
        year: u32,
        transaction_service: &TransactionService<C>,
    ) -> Result<CalendarMonth> {
        info!("🗓️ CALENDAR: Getting calendar month with transactions for {}/{}", month, year);

        // Step 1: Get transactions for calendar (including future allowances)
        let query = CalendarTransactionsQuery { month, year };
        
        let result = transaction_service.list_transactions_for_calendar(query)?;
        
        info!("🗓️ CALENDAR: Domain service returned {} transactions for calendar", result.transactions.len());

        // Step 2: Convert domain transactions to DTOs for calendar service
        let dto_transactions: Vec<Transaction> = result
            .transactions
            .into_iter()
            .map(TransactionMapper::to_dto)
            .collect();
        
        info!("🗓️ CALENDAR: Total transactions for calendar: {} transactions", dto_transactions.len());
        for (i, tx) in dto_transactions.iter().enumerate().take(5) {
            info!("🗓️ CALENDAR: DTO Transaction {}: id={}, date={}, amount={}, description={}", 
                 i + 1, tx.id, tx.date, tx.amount, tx.description);
        }

        // Step 3: Generate calendar month using existing method
        let calendar_month = self.generate_calendar_month(month, year, dto_transactions);
        
        info!("🗓️ CALENDAR: Generated calendar with {} days", calendar_month.days.len());
        let total_transaction_count: usize = calendar_month.days.iter()
            .map(|day| day.transactions.len())
            .sum();
        info!("🗓️ CALENDAR: Total transactions in calendar days: {}", total_transaction_count);

        Ok(calendar_month)
    }

    /// Generate a calendar month view with transaction data and future allowances
    pub fn generate_calendar_month(
        &self,
        month: u32,
        year: u32,
        transactions: Vec<Transaction>,
    ) -> CalendarMonth {
        let days_in_month = self.days_in_month(month, year);
        let first_day = self.first_day_of_month(month, year);
        
        log::debug!("🗓️ CALENDAR DEBUG: Generating calendar for {}/{}", month, year);
        log::debug!("🗓️ CALENDAR DEBUG: Days in month: {}, First day of week: {}", days_in_month, first_day);
        
        // Transactions already include future allowances from the transaction service
        let all_transactions = transactions;
        
        // Group all transactions by day for the current month
        let transactions_by_day = self.group_transactions_by_day(month, year, &all_transactions);
        
        // Calculate daily balances using stored balances from ALL transactions (including future allowances)
        // Future allowances now have proper projected balances calculated in the transaction service
        let daily_balances = self.calculate_daily_balances(month, year, &all_transactions, days_in_month);
        
        let mut calendar_days = Vec::new();
        
        // Add empty cells for days before the first day of month
        log::debug!("🗓️ CALENDAR DEBUG: Adding {} padding days before month", first_day);
        for i in 0..first_day {
            log::debug!("🗓️ CALENDAR DEBUG: Adding padding day {} with PaddingBefore", i);
            calendar_days.push(CalendarDay {
                day: 0,
                balance: 0.0,
                transactions: Vec::new(),
                day_type: CalendarDayType::PaddingBefore,
                #[allow(deprecated)]
                is_empty: true,
            });
        }
        
        // Add days of the month
        log::debug!("🗓️ CALENDAR DEBUG: Adding {} actual month days", days_in_month);
        for day in 1..=days_in_month {
            let day_transactions = transactions_by_day.get(&day).cloned().unwrap_or_default();
            let day_balance = daily_balances.get(&day).copied().unwrap_or(0.0);
            
            log::debug!("🗓️ CALENDAR DEBUG: Adding month day {} with {} transactions", day, day_transactions.len());
            calendar_days.push(CalendarDay {
                day,
                balance: day_balance,
                transactions: day_transactions,
                day_type: CalendarDayType::MonthDay,
                #[allow(deprecated)]
                is_empty: false,
            });
        }
        
        log::debug!("🗓️ CALENDAR DEBUG: Total calendar days created: {}", calendar_days.len());
        
        CalendarMonth {
            month,
            year,
            days: calendar_days,
            first_day_of_week: first_day,
        }
    }

    /// Get the number of days in a given month and year
    pub fn days_in_month(&self, month: u32, year: u32) -> u32 {
        match month {
            2 => if self.is_leap_year(year) { 29 } else { 28 },
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }

    /// Check if a year is a leap year
    pub fn is_leap_year(&self, year: u32) -> bool {
        year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
    }

    /// Get the first day of month (0 = Sunday, 1 = Monday, etc.)
    pub fn first_day_of_month(&self, month: u32, year: u32) -> u32 {
        // Use chrono to get the correct first day of month
        use chrono::{NaiveDate, Datelike};
        
        if let Some(date) = NaiveDate::from_ymd_opt(year as i32, month, 1) {
            // chrono's weekday(): Monday = 1, ..., Sunday = 7
            // Our format: Sunday = 0, Monday = 1, ..., Saturday = 6
            date.weekday().num_days_from_sunday()
        } else {
            // Invalid date, fallback to 0 (Sunday)
            0
        }
    }

    /// Get the human-readable name for a month number
    pub fn month_name(&self, month: u32) -> &'static str {
        match month {
            1 => "January", 2 => "February", 3 => "March", 4 => "April",
            5 => "May", 6 => "June", 7 => "July", 8 => "August",
            9 => "September", 10 => "October", 11 => "November", 12 => "December",
            _ => "Invalid Month",
        }
    }

    /// Group transactions by day for a specific month and year
    fn group_transactions_by_day(
        &self,
        month: u32,
        year: u32,
        transactions: &[Transaction],
    ) -> HashMap<u32, Vec<Transaction>> {
        let mut transactions_by_day: HashMap<u32, Vec<Transaction>> = HashMap::new();
        
        for transaction in transactions {
            if let Some((t_year, t_month, t_day)) = self.parse_transaction_date(&transaction.date) {
                if t_month == month && t_year == year {
                    transactions_by_day
                        .entry(t_day)
                        .or_insert_with(Vec::new)
                        .push(transaction.clone());
                }
            }
        }
        
        transactions_by_day
    }

    /// Calculate daily balances using stored transaction balances (much simpler and more reliable)
    fn calculate_daily_balances(
        &self,
        month: u32,
        year: u32,
        transactions: &[Transaction],
        days_in_month: u32,
    ) -> HashMap<u32, f64> {
        let mut daily_balances: HashMap<u32, f64> = HashMap::new();
        
        // Group transactions by day
        let transactions_by_day = self.group_transactions_by_day(month, year, transactions);
        
        // Find the starting balance for this month from the most recent previous transaction
        let mut sorted_transactions = transactions.to_vec();
        sorted_transactions.sort_by(|a, b| {
            let date_a = self.parse_transaction_date(&a.date).unwrap_or((0, 0, 0));
            let date_b = self.parse_transaction_date(&b.date).unwrap_or((0, 0, 0));
            date_b.cmp(&date_a) // Newest first
        });
        
        let starting_balance = self.calculate_starting_balance_for_month(month, year, &sorted_transactions);
        let mut previous_balance = starting_balance;
        
        log::debug!("🗓️ BALANCE DEBUG: Starting balance for {}/{}: ${:.2}", month, year, starting_balance);
        
        // For each day, find the latest transaction (chronologically) and use its balance
        for day in 1..=days_in_month {
            if let Some(day_transactions) = transactions_by_day.get(&day) {
                // Sort transactions by full timestamp (not just date) to get proper chronological order
                let mut sorted_day_transactions = day_transactions.clone();
                sorted_day_transactions.sort_by(|a, b| a.date.cmp(&b.date));
                
                // Use the balance from the chronologically last transaction of the day
                if let Some(latest) = sorted_day_transactions.last() {
                    daily_balances.insert(day, latest.balance);
                    previous_balance = latest.balance;
                    log::debug!("🗓️ BALANCE DEBUG: Day {}: Using stored balance ${:.2} from transaction {}", 
                              day, latest.balance, latest.id);
                } else {
                    // No transactions on this day, carry forward previous balance
                    daily_balances.insert(day, previous_balance);
                }
            } else {
                // No transactions on this day, carry forward previous balance
                daily_balances.insert(day, previous_balance);
                log::debug!("🗓️ BALANCE DEBUG: Day {}: No transactions, using previous balance ${:.2}", 
                          day, previous_balance);
            }
        }
        
        daily_balances
    }

    /// Calculate the starting balance for a month (end of previous month)
    fn calculate_starting_balance_for_month(
        &self,
        month: u32,
        year: u32,
        sorted_transactions: &[Transaction],
    ) -> f64 {
        // Find the most recent transaction BEFORE the start of the target month
        // This gives us the balance at the end of the previous month
        for transaction in sorted_transactions {
            if let Some((t_year, t_month, _)) = self.parse_transaction_date(&transaction.date) {
                // Check if this transaction is before the target month
                let transaction_is_before_target = if t_year < year {
                    true // Transaction is from a previous year
                } else if t_year == year && t_month < month {
                    true // Transaction is from earlier month in same year
                } else {
                    false // Transaction is from target month or later
                };
                
                if transaction_is_before_target {
                    // This is the most recent transaction before our target month
                    // Return its balance (which represents the account balance after this transaction)
                    log::debug!("🗓️ BALANCE DEBUG: Found starting balance for {}/{}: ${:.2} from transaction on {}/{}/{}", 
                              month, year, transaction.balance, t_month, t_year, transaction.id);
                    return transaction.balance;
                }
            }
        }
        
        // No transactions found before this month, starting balance is 0
        log::debug!("🗓️ BALANCE DEBUG: No transactions found before {}/{}, starting balance: $0.00", month, year);
        0.0
    }

    /// Parse an RFC 3339 date string to extract year, month, day
    pub fn parse_transaction_date(&self, date_str: &str) -> Option<(u32, u32, u32)> {
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

    /// Format a date for human-readable display
    pub fn format_date_for_display(&self, date_str: &str) -> String {
        if let Some((year, month, day)) = self.parse_transaction_date(date_str) {
            format!("{} {}, {}", self.month_name(month), day, year)
        } else {
            // Fallback to original string
            date_str.to_string()
        }
    }

    /// Navigate to the previous month
    pub fn previous_month(&self, current_month: u32, current_year: u32) -> (u32, u32) {
        if current_month == 1 {
            (12, current_year - 1)
        } else {
            (current_month - 1, current_year)
        }
    }

    /// Navigate to the next month
    pub fn next_month(&self, current_month: u32, current_year: u32) -> (u32, u32) {
        if current_month == 12 {
            (1, current_year + 1)
        } else {
            (current_month + 1, current_year)
        }
    }

    /// Get current date information
    pub fn get_current_date(&self) -> CurrentDateResponse {
        let now = Local::now();
        let month = now.month();
        let year = now.year() as u32;
        let day = now.day();
        
        // Format the date
        let month_name = self.month_name(month);
        let formatted_date = format!("{} {}, {}", month_name, day, year);
        let iso_date = format!("{:04}-{:02}-{:02}", year, month, day);
        
        CurrentDateResponse {
            month,
            year,
            day,
            formatted_date,
            iso_date,
        }
    }

    /// Get the current focus date for calendar navigation
    pub fn get_focus_date(&self) -> CalendarFocusDate {
        self.current_focus_date.lock().unwrap().clone()
    }

    /// Set the focus date for calendar navigation
    pub fn set_focus_date(&self, month: u32, year: u32) -> Result<CalendarFocusDate, String> {
        if month < 1 || month > 12 {
            return Err(format!("Invalid month: {}. Must be between 1 and 12", month));
        }
        
        let new_focus_date = CalendarFocusDate { month, year };
        
        {
            let mut focus_date = self.current_focus_date.lock().unwrap();
            *focus_date = new_focus_date.clone();
        }
        
        Ok(new_focus_date)
    }

    /// Navigate to the previous month
    pub fn navigate_previous_month(&self) -> CalendarFocusDate {
        let current_focus = self.get_focus_date();
        let (prev_month, prev_year) = self.previous_month(current_focus.month, current_focus.year);
        
        // This should never fail since previous_month returns valid values
        self.set_focus_date(prev_month, prev_year).unwrap()
    }

    /// Navigate to the next month  
    pub fn navigate_next_month(&self) -> CalendarFocusDate {
        let current_focus = self.get_focus_date();
        let (next_month, next_year) = self.next_month(current_focus.month, current_focus.year);
        
        // This should never fail since next_month returns valid values
        self.set_focus_date(next_month, next_year).unwrap()
    }
}

impl Default for CalendarService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_transaction(date: &str, amount: f64, balance: f64, description: &str) -> Transaction {
        Transaction {
            id: format!("test_{}", date),
            child_id: "test_child_id".to_string(),
            date: date.to_string(),
            description: description.to_string(),
            amount,
            balance,
            transaction_type: if amount >= 0.0 { TransactionType::Income } else { TransactionType::Expense },
        }
    }

    #[test]
    fn test_days_in_month() {
        let service = CalendarService::new();
        
        // Test regular months
        assert_eq!(service.days_in_month(1, 2025), 31); // January
        assert_eq!(service.days_in_month(4, 2025), 30); // April
        assert_eq!(service.days_in_month(2, 2025), 28); // February (non-leap)
        assert_eq!(service.days_in_month(2, 2024), 29); // February (leap year)
    }

    #[test]
    fn test_is_leap_year() {
        let service = CalendarService::new();
        
        assert!(!service.is_leap_year(2025)); // Regular year
        assert!(service.is_leap_year(2024));  // Divisible by 4
        assert!(!service.is_leap_year(1900)); // Divisible by 100 but not 400
        assert!(service.is_leap_year(2000));  // Divisible by 400
    }

    #[test]
    fn test_month_name() {
        let service = CalendarService::new();
        
        assert_eq!(service.month_name(1), "January");
        assert_eq!(service.month_name(6), "June");
        assert_eq!(service.month_name(12), "December");
        assert_eq!(service.month_name(13), "Invalid Month");
    }

    #[test]
    fn test_parse_transaction_date() {
        let service = CalendarService::new();
        
        assert_eq!(
            service.parse_transaction_date("2025-06-13T09:00:00-04:00"),
            Some((2025, 6, 13))
        );
        
        assert_eq!(
            service.parse_transaction_date("invalid-date"),
            None
        );
    }

    #[test]
    fn test_format_date_for_display() {
        let service = CalendarService::new();
        
        assert_eq!(
            service.format_date_for_display("2025-06-13T09:00:00-04:00"),
            "June 13, 2025"
        );
        
        assert_eq!(
            service.format_date_for_display("invalid-date"),
            "invalid-date"
        );
    }

    #[test]
    fn test_navigation() {
        let service = CalendarService::new();
        
        // Test previous month
        assert_eq!(service.previous_month(6, 2025), (5, 2025));
        assert_eq!(service.previous_month(1, 2025), (12, 2024));
        
        // Test next month
        assert_eq!(service.next_month(6, 2025), (7, 2025));
        assert_eq!(service.next_month(12, 2025), (1, 2026));
    }

    #[test]
    fn test_generate_calendar_month() {
        let service = CalendarService::new();
        
        let transactions = vec![
            create_test_transaction("2025-06-01T09:00:00-04:00", 10.0, 10.0, "Test 1"),
            create_test_transaction("2025-06-15T12:00:00-04:00", -5.0, 5.0, "Test 2"),
        ];
        
        let calendar = service.generate_calendar_month(6, 2025, transactions);
        
        assert_eq!(calendar.month, 6);
        assert_eq!(calendar.year, 2025);
        assert!(!calendar.days.is_empty());
        
        // Find day 1 and verify it has a transaction
        let day_1 = calendar.days.iter().find(|d| d.day == 1 && d.day_type == CalendarDayType::MonthDay);
        assert!(day_1.is_some());
        assert_eq!(day_1.unwrap().transactions.len(), 1);
    }

    #[test]
    fn test_group_transactions_by_day() {
        let service = CalendarService::new();
        
        let transactions = vec![
            create_test_transaction("2025-06-01T09:00:00-04:00", 10.0, 10.0, "Day 1 Transaction 1"),
            create_test_transaction("2025-06-01T15:00:00-04:00", 5.0, 15.0, "Day 1 Transaction 2"),
            create_test_transaction("2025-06-15T12:00:00-04:00", -5.0, 10.0, "Day 15 Transaction"),
            create_test_transaction("2025-05-30T12:00:00-04:00", 20.0, 30.0, "Different month"),
        ];
        
        let grouped = service.group_transactions_by_day(6, 2025, &transactions);
        
        assert_eq!(grouped.get(&1).unwrap().len(), 2); // Day 1 has 2 transactions
        assert_eq!(grouped.get(&15).unwrap().len(), 1); // Day 15 has 1 transaction
        assert!(grouped.get(&30).is_none()); // Different month transaction not included
    }

    #[test]
    fn test_get_focus_date() {
        let service = CalendarService::new();
        
        // Should return current month/year by default
        let focus_date = service.get_focus_date();
        assert!(focus_date.month >= 1 && focus_date.month <= 12);
        assert!(focus_date.year >= 2025); // Assuming we're in 2025 or later
    }

    #[test]
    fn test_set_focus_date() {
        let service = CalendarService::new();
        
        // Test valid date
        let result = service.set_focus_date(6, 2025);
        assert!(result.is_ok());
        let focus_date = result.unwrap();
        assert_eq!(focus_date.month, 6);
        assert_eq!(focus_date.year, 2025);
        
        // Verify it's actually set
        let retrieved = service.get_focus_date();
        assert_eq!(retrieved.month, 6);
        assert_eq!(retrieved.year, 2025);
        
        // Test invalid month
        let result = service.set_focus_date(13, 2025);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid month"));
        
        let result = service.set_focus_date(0, 2025);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid month"));
    }

    #[test]
    fn test_navigate_previous_month() {
        let service = CalendarService::new();
        
        // Set to June 2025
        service.set_focus_date(6, 2025).unwrap();
        
        // Navigate to previous month
        let focus_date = service.navigate_previous_month();
        assert_eq!(focus_date.month, 5);
        assert_eq!(focus_date.year, 2025);
        
        // Test year rollover
        service.set_focus_date(1, 2025).unwrap();
        let focus_date = service.navigate_previous_month();
        assert_eq!(focus_date.month, 12);
        assert_eq!(focus_date.year, 2024);
    }

    #[test]
    fn test_navigate_next_month() {
        let service = CalendarService::new();
        
        // Set to June 2025
        service.set_focus_date(6, 2025).unwrap();
        
        // Navigate to next month
        let focus_date = service.navigate_next_month();
        assert_eq!(focus_date.month, 7);
        assert_eq!(focus_date.year, 2025);
        
        // Test year rollover
        service.set_focus_date(12, 2025).unwrap();
        let focus_date = service.navigate_next_month();
        assert_eq!(focus_date.month, 1);
        assert_eq!(focus_date.year, 2026);
    }

    #[test]
    fn test_cross_month_balance_forwarding_comprehensive() {
        let service = CalendarService::new();
        
        // Create comprehensive test data that matches the user's scenario:
        // - June 15: Income +$1 (balance = $1)
        // - June 19: Spend -$1 (balance = $0)  
        // - Allowance: Every Friday +$1
        // - Expected: June 30th = $2, July 31st = $6, August 31st = $11
        
        let mut transactions = vec![
            // Historical real transactions
            create_test_transaction("2025-06-15T12:00:00Z", 1.0, 1.0, "Income"),
            create_test_transaction("2025-06-19T12:00:00Z", -1.0, 0.0, "Spend"),
            
            // June future allowances (after June 19th)
            create_test_transaction("2025-06-20T12:00:00Z", 1.0, 1.0, "Friday Allowance"), // June 20 (Friday)
            create_test_transaction("2025-06-27T12:00:00Z", 1.0, 2.0, "Friday Allowance"), // June 27 (Friday)
            
            // July future allowances  
            create_test_transaction("2025-07-04T12:00:00Z", 1.0, 3.0, "Friday Allowance"), // July 4 (Friday)
            create_test_transaction("2025-07-11T12:00:00Z", 1.0, 4.0, "Friday Allowance"), // July 11 (Friday)
            create_test_transaction("2025-07-18T12:00:00Z", 1.0, 5.0, "Friday Allowance"), // July 18 (Friday)
            create_test_transaction("2025-07-25T12:00:00Z", 1.0, 6.0, "Friday Allowance"), // July 25 (Friday)
            
            // August future allowances
            create_test_transaction("2025-08-01T12:00:00Z", 1.0, 7.0, "Friday Allowance"), // August 1 (Friday)
            create_test_transaction("2025-08-08T12:00:00Z", 1.0, 8.0, "Friday Allowance"), // August 8 (Friday)
            create_test_transaction("2025-08-15T12:00:00Z", 1.0, 9.0, "Friday Allowance"), // August 15 (Friday)
            create_test_transaction("2025-08-22T12:00:00Z", 1.0, 10.0, "Friday Allowance"), // August 22 (Friday)
            create_test_transaction("2025-08-29T12:00:00Z", 1.0, 11.0, "Friday Allowance"), // August 29 (Friday)
        ];
        
        // Test July calendar generation (should start with June 30th ending balance)
        let july_calendar = service.generate_calendar_month(7, 2025, transactions.clone());
        
        // July 1st should start with $2.0 (June ending balance)
        assert_eq!(july_calendar.days[2].balance, 2.0, "July 1st should start with June 30th ending balance of $2.00");
        
        // July 31st should end with $6.0
        assert_eq!(july_calendar.days[32].balance, 6.0, "July 31st should end with $6.00 after 4 Friday allowances");
        
        // Test August calendar generation (should start with July 31st ending balance)  
        let august_calendar = service.generate_calendar_month(8, 2025, transactions.clone());
        
        // August 1st should start with $7.0 (July ending balance + August 1st allowance)
        assert_eq!(august_calendar.days[5].balance, 7.0, "August 1st should show $7.00 (July end $6.00 + August 1st allowance $1.00)");
        
        // August 31st should end with $11.0
        assert_eq!(august_calendar.days[35].balance, 11.0, "August 31st should end with $11.00 after 5 Friday allowances");
        
        // Verify proper balance progression within July
        let july_4_day = july_calendar.days.iter().find(|d| d.day == 4).unwrap();
        assert_eq!(july_4_day.balance, 3.0, "July 4th should show $3.00");
        
        let july_25_day = july_calendar.days.iter().find(|d| d.day == 25).unwrap();
        assert_eq!(july_25_day.balance, 6.0, "July 25th should show $6.00");
        
        // Verify proper balance progression within August
        let august_15_day = august_calendar.days.iter().find(|d| d.day == 15).unwrap();
        assert_eq!(august_15_day.balance, 9.0, "August 15th should show $9.00");
        
        let august_29_day = august_calendar.days.iter().find(|d| d.day == 29).unwrap();
        assert_eq!(august_29_day.balance, 11.0, "August 29th should show $11.00");
    }
} 