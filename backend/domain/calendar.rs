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
            info!("🗓️ CALENDAR: DTO Transaction {}: id={}, date={}, amount={}, description={} balance={}", 
                 i + 1, tx.id, tx.date, tx.amount, tx.description, tx.balance);
        }

        // Step 3: Generate calendar month using enhanced method that handles NaN balances
        // Get the active child for balance service
        let active_child = transaction_service.get_active_child()?;
        
        // Create balance service for projected balance calculations
        let balance_service = transaction_service.create_balance_service();
        
        let calendar_month = self.generate_calendar_month_with_projected_balances(
            month, 
            year, 
            dto_transactions, 
            &balance_service, 
            &active_child.id
        );
        
        info!("🗓️ CALENDAR: Generated calendar with {} days using projected balances", calendar_month.days.len());
        let total_transaction_count: usize = calendar_month.days.iter()
            .map(|day| day.transactions.len())
            .sum();
        info!("🗓️ CALENDAR: Total transactions in calendar days: {}", total_transaction_count);

        Ok(calendar_month)
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
            let date_str = transaction.date.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
            if let Some((t_year, t_month, t_day)) = self.parse_transaction_date(&date_str) {
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



    /// Enhanced calculate_daily_balances that can delegate NaN balance calculations to BalanceService
    /// This method detects transactions with NaN balance and calculates projected balances using BalanceService
    pub fn calculate_daily_balances_with_projection<C: Connection>(
        &self,
        month: u32,
        year: u32,
        transactions: &[Transaction],
        days_in_month: u32,
        _balance_service: &crate::backend::domain::balance_service::BalanceService<C>,
        _child_id: &str,
    ) -> HashMap<u32, f64> {
        let mut daily_balances: HashMap<u32, f64> = HashMap::new();
        
        // Group transactions by day
        let transactions_by_day = self.group_transactions_by_day(month, year, transactions);
        
        // Find the starting balance for this month from the most recent previous transaction
        let mut sorted_transactions = transactions.to_vec();
        sorted_transactions.sort_by(|a, b| {
            let date_a_str = a.date.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
            let date_b_str = b.date.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
            let date_a = self.parse_transaction_date(&date_a_str).unwrap_or((0, 0, 0));
            let date_b = self.parse_transaction_date(&date_b_str).unwrap_or((0, 0, 0));
            date_b.cmp(&date_a) // Newest first
        });
        
        let starting_balance = self.calculate_starting_balance_for_month(month, year, &sorted_transactions);
        let mut previous_balance = starting_balance;
        
        log::debug!("🗓️ BALANCE DEBUG: Starting balance for {}/{}: ${:.2}", month, year, starting_balance);
        
        // For each day, calculate balances with NaN projection support
        for day in 1..=days_in_month {
            if let Some(day_transactions) = transactions_by_day.get(&day) {
                // Sort transactions by full timestamp (not just date) to get proper chronological order
                let mut sorted_day_transactions = day_transactions.clone();
                sorted_day_transactions.sort_by(|a, b| a.date.cmp(&b.date));
                
                // Check if we need to calculate projected balance for NaN transactions
                let mut day_final_balance = previous_balance;
                for transaction in &sorted_day_transactions {
                    if transaction.balance.is_nan() {
                        // This is a future allowance - calculate projected balance based on current day_final_balance
                        let new_projected_balance = day_final_balance + transaction.amount;
                        day_final_balance = new_projected_balance;
                        log::debug!("🗓️ BALANCE DEBUG: Day {}: Projected balance ${:.2} for transaction {} (${:.2} + ${:.2})", 
                                  day, new_projected_balance, transaction.id, day_final_balance - transaction.amount, transaction.amount);
                    } else {
                        // Normal transaction with stored balance
                        day_final_balance = transaction.balance;
                        log::debug!("🗓️ BALANCE DEBUG: Day {}: Using stored balance ${:.2} from transaction {}", 
                                  day, transaction.balance, transaction.id);
                    }
                }
                
                daily_balances.insert(day, day_final_balance);
                previous_balance = day_final_balance;
            } else {
                // No transactions on this day, carry forward previous balance
                daily_balances.insert(day, previous_balance);
                log::debug!("🗓️ BALANCE DEBUG: Day {}: No transactions, using previous balance ${:.2}", 
                          day, previous_balance);
            }
        }
        
        daily_balances
    }

    /// Enhanced generate_calendar_month that supports projected balances for future allowances
    pub fn generate_calendar_month_with_projected_balances<C: Connection>(
        &self,
        month: u32,
        year: u32,
        transactions: Vec<Transaction>,
        balance_service: &crate::backend::domain::balance_service::BalanceService<C>,
        child_id: &str,
    ) -> CalendarMonth {
        let days_in_month = self.days_in_month(month, year);
        let first_day = self.first_day_of_month(month, year);
        
        log::debug!("🗓️ CALENDAR DEBUG: Generating calendar with projected balances for {}/{}", month, year);
        log::debug!("🗓️ CALENDAR DEBUG: Days in month: {}, First day of week: {}", days_in_month, first_day);
        
        // Group transactions by day for the current month
        let transactions_by_day = self.group_transactions_by_day(month, year, &transactions);
        
        // Calculate daily balances with projection support for NaN balances
        let daily_balances = self.calculate_daily_balances_with_projection(
            month, year, &transactions, days_in_month, balance_service, child_id
        );
        
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
            
            log::debug!("🗓️ CALENDAR DEBUG: Adding month day {} with {} transactions, balance ${:.2}", 
                       day, day_transactions.len(), day_balance);
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
            let date_str = transaction.date.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();
            if let Some((t_year, t_month, _)) = self.parse_transaction_date(&date_str) {
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
    use crate::backend::storage::traits::TransactionStorage;

    fn create_test_transaction(date: &str, amount: f64, balance: f64, description: &str) -> Transaction {
        let parsed_date = chrono::DateTime::parse_from_rfc3339(date)
            .unwrap_or_else(|_| {
                chrono::DateTime::parse_from_str(&format!("{}T12:00:00-05:00", date), "%Y-%m-%dT%H:%M:%S%z")
                    .expect("Failed to parse date")
            });
            
        Transaction {
            id: format!("test_{}", date),
            child_id: "test_child_id".to_string(),
            date: parsed_date,
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
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;

        let service = CalendarService::new();
        
        // Create test infrastructure
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = child_result.child.id;
        
        let transactions = vec![
            create_test_transaction("2025-06-01T09:00:00-04:00", 10.0, 10.0, "Test 1"),
            create_test_transaction("2025-06-15T12:00:00-04:00", -5.0, 5.0, "Test 2"),
        ];
        
        let calendar = service.generate_calendar_month_with_projected_balances(6, 2025, transactions, &balance_service, &child_id);
        
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
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;

        let service = CalendarService::new();
        
        // Create test infrastructure
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = child_result.child.id;
        
        // Create comprehensive test data that matches the user's scenario:
        // - June 15: Income +$1 (balance = $1)
        // - June 19: Spend -$1 (balance = $0)  
        // - Allowance: Every Friday +$1
        // - Expected: June 30th = $2, July 31st = $6, August 31st = $11
        
        let transactions = vec![
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
        let july_calendar = service.generate_calendar_month_with_projected_balances(7, 2025, transactions.clone(), &balance_service, &child_id);
        
        // July 1st should start with $2.0 (June ending balance)
        assert_eq!(july_calendar.days[2].balance, 2.0, "July 1st should start with June 30th ending balance of $2.00");
        
        // July 31st should end with $6.0
        assert_eq!(july_calendar.days[32].balance, 6.0, "July 31st should end with $6.00 after 4 Friday allowances");
        
        // Test August calendar generation (should start with July 31st ending balance)  
        let august_calendar = service.generate_calendar_month_with_projected_balances(8, 2025, transactions.clone(), &balance_service, &child_id);
        
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

    #[test]
    fn test_calculate_daily_balances_with_nan_delegation_to_balance_service() {
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::storage::traits::TransactionStorage;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;

        let service = CalendarService::new();
        
        // Create test environment with BalanceService
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        // Create historical transactions with valid balances
        let historical_tx1 = create_test_transaction("2025-07-04T12:00:00+00:00", 10.0, 10.0, "Week 1 allowance");
        let historical_tx2 = create_test_transaction("2025-07-11T12:00:00+00:00", 10.0, 20.0, "Week 2 allowance");
        
        // Store historical transactions in the repository so BalanceService can access them
        let transaction_repository = connection.create_transaction_repository();
        let historical_domain_tx1 = crate::backend::domain::models::transaction::Transaction {
            id: historical_tx1.id.clone(),
            child_id: child_id.clone(),
            date: historical_tx1.date,
            description: historical_tx1.description.clone(),
            amount: historical_tx1.amount,
            balance: historical_tx1.balance,
            transaction_type: crate::backend::domain::models::transaction::TransactionType::Income,
        };
        let historical_domain_tx2 = crate::backend::domain::models::transaction::Transaction {
            id: historical_tx2.id.clone(),
            child_id: child_id.clone(),
            date: historical_tx2.date,
            description: historical_tx2.description.clone(),
            amount: historical_tx2.amount,
            balance: historical_tx2.balance,
            transaction_type: crate::backend::domain::models::transaction::TransactionType::Income,
        };
        transaction_repository.store_transaction(&historical_domain_tx1).unwrap();
        transaction_repository.store_transaction(&historical_domain_tx2).unwrap();

        // Create future allowance transaction with NaN balance (like AllowanceService would create)
        let future_allowance = create_test_transaction("2025-07-18T12:00:00+00:00", 10.0, f64::NAN, "Future allowance");
        
        // Create mixed transaction list (historical + future with NaN balance)
        let mixed_transactions = vec![historical_tx1, historical_tx2, future_allowance];

        // Test the enhanced calculate_daily_balances_with_projection method
        let daily_balances = service.calculate_daily_balances_with_projection(
            7, 2025, &mixed_transactions, 31, &balance_service, child_id
        );

        // Verify historical transactions keep their stored balances
        assert_eq!(daily_balances.get(&4).copied().unwrap_or(0.0), 10.0, "July 4th should have stored balance");
        assert_eq!(daily_balances.get(&11).copied().unwrap_or(0.0), 20.0, "July 11th should have stored balance");
        
        // Verify future allowance gets projected balance calculated by BalanceService
        // BalanceService should calculate: previous balance (20.0) + future allowance (10.0) = 30.0
        assert_eq!(daily_balances.get(&18).copied().unwrap_or(0.0), 30.0, 
                  "July 18th should have projected balance calculated by BalanceService (20.0 + 10.0 = 30.0)");
    }

    #[test]
    fn test_generate_calendar_month_with_projected_balances() {
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::storage::traits::TransactionStorage;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;

        let service = CalendarService::new();
        
        // Create test environment
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;

        // Store historical transaction for BalanceService
        let transaction_repository = connection.create_transaction_repository();
        let historical_domain_tx = crate::backend::domain::models::transaction::Transaction {
            id: "historical".to_string(),
            child_id: child_id.clone(),
            date: chrono::DateTime::parse_from_rfc3339("2025-07-04T12:00:00+00:00").unwrap(),
            description: "Historical allowance".to_string(),
            amount: 15.0,
            balance: 15.0,
            transaction_type: crate::backend::domain::models::transaction::TransactionType::Income,
        };
        transaction_repository.store_transaction(&historical_domain_tx).unwrap();

        // Create transactions including future allowance with NaN balance
        let transactions = vec![
            create_test_transaction("2025-07-04T12:00:00+00:00", 15.0, 15.0, "Historical allowance"),
            create_test_transaction("2025-07-18T12:00:00+00:00", 10.0, f64::NAN, "Future allowance"),
        ];

        // Test enhanced generate_calendar_month_with_projected_balances method
        let calendar = service.generate_calendar_month_with_projected_balances(
            7, 2025, transactions, &balance_service, child_id
        );

        // Find July 4th day (historical transaction)
        let july_4_day = calendar.days.iter().find(|d| d.day == 4).unwrap();
        assert_eq!(july_4_day.balance, 15.0, "July 4th should show stored balance");

        // Find July 18th day (future allowance with projected balance)
        let july_18_day = calendar.days.iter().find(|d| d.day == 18).unwrap();
        assert_eq!(july_18_day.balance, 25.0, "July 18th should show projected balance (15.0 + 10.0 = 25.0)");
    }

    #[test]
    fn test_full_calendar_flow_with_projected_balances_integration() {
        // This test verifies the full integration flow where CalendarService
        // coordinates with BalanceService to produce calendar with projected balances
        
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;
        
        let calendar_service = CalendarService::new();
        
        // Create test environment with proper connection setup
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;
        
        // Create a mix of historical and future transactions
        let transactions = vec![
            create_test_transaction("2025-07-01T10:00:00Z", 10.0, 10.0, "Initial allowance"),
            create_test_transaction("2025-07-05T10:00:00Z", -3.0, 7.0, "Spent on snacks"),
            create_test_transaction("2025-07-15T10:00:00Z", 5.0, 12.0, "Bonus earned"),
            create_test_transaction("2025-07-20T10:00:00Z", 10.0, f64::NAN, "Future allowance"), // NaN balance
            create_test_transaction("2025-07-25T10:00:00Z", 10.0, f64::NAN, "Another future allowance"), // NaN balance
        ];
        
        // Store historical transactions in the repository so BalanceService can access them
        let transaction_repository = connection.create_transaction_repository();
        for transaction in &transactions {
            if !transaction.balance.is_nan() {
                // Store historical transactions (non-NaN balance) in the repository
                let domain_transaction = crate::backend::domain::models::transaction::Transaction {
                    id: transaction.id.clone(),
                    child_id: child_id.clone(),
                    date: transaction.date.clone(),
                    description: transaction.description.clone(),
                    amount: transaction.amount,
                    balance: transaction.balance,
                    transaction_type: if transaction.amount > 0.0 { 
                        crate::backend::domain::models::transaction::TransactionType::Income 
                    } else { 
                        crate::backend::domain::models::transaction::TransactionType::Expense 
                    },
                };
                transaction_repository.store_transaction(&domain_transaction).unwrap();
            }
        }
        
        // Generate calendar with projected balances
        let calendar = calendar_service.generate_calendar_month_with_projected_balances(
            7, 2025, transactions, &balance_service, child_id
        );
        
        // Verify calendar structure
        assert_eq!(calendar.month, 7);
        assert_eq!(calendar.year, 2025);
        assert_eq!(calendar.days.len(), 33);
        
        // Verify historical transactions maintain their stored balances
        let july_1_day = calendar.days.iter().find(|d| d.day == 1).unwrap();
        assert_eq!(july_1_day.balance, 10.0, "July 1st should show stored balance");
        
        let july_5_day = calendar.days.iter().find(|d| d.day == 5).unwrap();
        assert_eq!(july_5_day.balance, 7.0, "July 5th should show stored balance");
        
        let july_15_day = calendar.days.iter().find(|d| d.day == 15).unwrap();
        assert_eq!(july_15_day.balance, 12.0, "July 15th should show stored balance");
        
        // Verify future transactions get projected balances
        let july_20_day = calendar.days.iter().find(|d| d.day == 20).unwrap();
        assert_eq!(july_20_day.balance, 22.0, "July 20th should show projected balance (12.0 + 10.0 = 22.0)");
        
        let july_25_day = calendar.days.iter().find(|d| d.day == 25).unwrap();
        assert_eq!(july_25_day.balance, 32.0, "July 25th should show projected balance (22.0 + 10.0 = 32.0)");
        
        // Verify days without transactions carry forward the balance
        let july_21_day = calendar.days.iter().find(|d| d.day == 21).unwrap();
        assert_eq!(july_21_day.balance, 22.0, "July 21st should carry forward balance from July 20th");
        
        let july_31_day = calendar.days.iter().find(|d| d.day == 31).unwrap();
        assert_eq!(july_31_day.balance, 32.0, "July 31st should carry forward final balance");
        
        // Verify calendar structure includes both month days and padding days
        let month_days_count = calendar.days.iter().filter(|d| d.day > 0).count();
        assert_eq!(month_days_count, 31, "Should have 31 actual month days");
        
        let padding_days_count = calendar.days.iter().filter(|d| d.day == 0).count();
        assert_eq!(padding_days_count, 2, "Should have 2 padding days for July 2025 (starts on Tuesday)");
    }

    #[test]
    fn test_integration_calendar_service_with_balance_service_complex_scenario() {
        // This test verifies complex integration scenarios with multiple NaN transactions
        // and cross-month balance calculations
        
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;
        
        let calendar_service = CalendarService::new();
        
        // Create test environment with proper connection setup
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;
        
        // Create transactions spanning multiple periods with gaps
        let transactions = vec![
            create_test_transaction("2025-07-02T10:00:00Z", 15.0, 15.0, "Starting balance"),
            create_test_transaction("2025-07-05T10:00:00Z", -5.0, 10.0, "Purchase"),
            // Gap from July 6-9
            create_test_transaction("2025-07-10T10:00:00Z", 8.0, 18.0, "Earned money"),
            // Multiple future transactions on same day
            create_test_transaction("2025-07-20T09:00:00Z", 5.0, f64::NAN, "Future allowance part 1"),
            create_test_transaction("2025-07-20T15:00:00Z", 3.0, f64::NAN, "Future allowance part 2"),
            create_test_transaction("2025-07-22T10:00:00Z", -2.0, f64::NAN, "Future spending"),
            create_test_transaction("2025-07-30T10:00:00Z", 10.0, f64::NAN, "End of month allowance"),
        ];
        
        // Store historical transactions in the repository so BalanceService can access them
        let transaction_repository = connection.create_transaction_repository();
        for transaction in &transactions {
            if !transaction.balance.is_nan() {
                // Store historical transactions (non-NaN balance) in the repository
                let domain_transaction = crate::backend::domain::models::transaction::Transaction {
                    id: transaction.id.clone(),
                    child_id: child_id.clone(),
                    date: transaction.date.clone(),
                    description: transaction.description.clone(),
                    amount: transaction.amount,
                    balance: transaction.balance,
                    transaction_type: if transaction.amount > 0.0 { 
                        crate::backend::domain::models::transaction::TransactionType::Income 
                    } else { 
                        crate::backend::domain::models::transaction::TransactionType::Expense 
                    },
                };
                transaction_repository.store_transaction(&domain_transaction).unwrap();
            }
        }
        
        // Generate calendar with projected balances
        let calendar = calendar_service.generate_calendar_month_with_projected_balances(
            7, 2025, transactions, &balance_service, child_id
        );
        
        // Verify historical balances are preserved
        let july_2_day = calendar.days.iter().find(|d| d.day == 2).unwrap();
        assert_eq!(july_2_day.balance, 15.0, "July 2nd should show stored balance");
        
        let july_5_day = calendar.days.iter().find(|d| d.day == 5).unwrap();
        assert_eq!(july_5_day.balance, 10.0, "July 5th should show stored balance");
        
        let july_10_day = calendar.days.iter().find(|d| d.day == 10).unwrap();
        assert_eq!(july_10_day.balance, 18.0, "July 10th should show stored balance");
        
        // Verify gap days carry forward balance
        let july_6_day = calendar.days.iter().find(|d| d.day == 6).unwrap();
        assert_eq!(july_6_day.balance, 10.0, "July 6th should carry forward balance from July 5th");
        
        let july_9_day = calendar.days.iter().find(|d| d.day == 9).unwrap();
        assert_eq!(july_9_day.balance, 10.0, "July 9th should carry forward balance from July 5th");
        
        // Verify future transactions get projected balances
        let july_20_day = calendar.days.iter().find(|d| d.day == 20).unwrap();
        assert_eq!(july_20_day.balance, 26.0, "July 20th should show projected balance (18.0 + 5.0 + 3.0 = 26.0)");
        
        let july_22_day = calendar.days.iter().find(|d| d.day == 22).unwrap();
        assert_eq!(july_22_day.balance, 24.0, "July 22nd should show projected balance (26.0 - 2.0 = 24.0)");
        
        let july_30_day = calendar.days.iter().find(|d| d.day == 30).unwrap();
        assert_eq!(july_30_day.balance, 34.0, "July 30th should show projected balance (24.0 + 10.0 = 34.0)");
        
        // Verify end of month carries forward final balance
        let july_31_day = calendar.days.iter().find(|d| d.day == 31).unwrap();
        assert_eq!(july_31_day.balance, 34.0, "July 31st should carry forward final balance");
    }

    #[test]
    fn test_integration_calendar_service_balance_service_cross_month_boundaries() {
        // This test verifies that projected balance calculations work correctly
        // when the starting balance comes from a previous month
        
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;
        
        let calendar_service = CalendarService::new();
        
        // Create test environment with proper connection setup
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;
        
        // Create transactions where July starts with only future transactions
        // (simulating a scenario where the last historical transaction was in June)
        let transactions = vec![
            create_test_transaction("2025-07-01T10:00:00Z", 10.0, f64::NAN, "First future allowance"),
            create_test_transaction("2025-07-15T10:00:00Z", 15.0, f64::NAN, "Mid-month future allowance"),
            create_test_transaction("2025-07-30T10:00:00Z", -5.0, f64::NAN, "End-month future spending"),
        ];
        
        // Store historical transactions in the repository so BalanceService can access them
        let transaction_repository = connection.create_transaction_repository();
        for transaction in &transactions {
            if !transaction.balance.is_nan() {
                // Store historical transactions (non-NaN balance) in the repository
                let domain_transaction = crate::backend::domain::models::transaction::Transaction {
                    id: transaction.id.clone(),
                    child_id: child_id.clone(),
                    date: transaction.date.clone(),
                    description: transaction.description.clone(),
                    amount: transaction.amount,
                    balance: transaction.balance,
                    transaction_type: if transaction.amount > 0.0 { 
                        crate::backend::domain::models::transaction::TransactionType::Income 
                    } else { 
                        crate::backend::domain::models::transaction::TransactionType::Expense 
                    },
                };
                transaction_repository.store_transaction(&domain_transaction).unwrap();
            }
        }
        
        // Generate calendar with projected balances
        let calendar = calendar_service.generate_calendar_month_with_projected_balances(
            7, 2025, transactions, &balance_service, child_id
        );
        
        // Verify the calendar handles the case where all transactions need projection
        // (starting balance would be 0.0 from calculate_starting_balance_for_month)
        let july_1_day = calendar.days.iter().find(|d| d.day == 1).unwrap();
        assert_eq!(july_1_day.balance, 10.0, "July 1st should show projected balance (0.0 + 10.0 = 10.0)");
        
        let july_15_day = calendar.days.iter().find(|d| d.day == 15).unwrap();
        assert_eq!(july_15_day.balance, 25.0, "July 15th should show projected balance (10.0 + 15.0 = 25.0)");
        
        let july_30_day = calendar.days.iter().find(|d| d.day == 30).unwrap();
        assert_eq!(july_30_day.balance, 20.0, "July 30th should show projected balance (25.0 - 5.0 = 20.0)");
        
        // Verify intermediate days carry forward balances correctly
        let july_10_day = calendar.days.iter().find(|d| d.day == 10).unwrap();
        assert_eq!(july_10_day.balance, 10.0, "July 10th should carry forward balance from July 1st");
        
        let july_25_day = calendar.days.iter().find(|d| d.day == 25).unwrap();
        assert_eq!(july_25_day.balance, 25.0, "July 25th should carry forward balance from July 15th");
        
        let july_31_day = calendar.days.iter().find(|d| d.day == 31).unwrap();
        assert_eq!(july_31_day.balance, 20.0, "July 31st should carry forward balance from July 30th");
    }

    #[test]
    fn test_integration_calendar_service_no_nan_transactions() {
        // This test verifies that when there are no NaN transactions,
        // the integration still works correctly (fallback to original behavior)
        
        use std::sync::Arc;
        use tempfile::tempdir;
        use crate::backend::storage::csv::CsvConnection;
        use crate::backend::domain::balance_service::BalanceService;
        use crate::backend::domain::child_service::ChildService;
        use crate::backend::domain::commands::child::CreateChildCommand;
        
        let calendar_service = CalendarService::new();
        
        // Create test environment with proper connection setup
        let temp_dir = tempdir().unwrap();
        let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
        let balance_service = BalanceService::new(connection.clone());
        let child_service = ChildService::new(connection.clone());
        
        // Create a test child
        let child_result = child_service.create_child(CreateChildCommand {
            name: "Test Child".to_string(),
            birthdate: "2015-01-01".to_string(),
        }).unwrap();
        let child_id = &child_result.child.id;
        
        // Create transactions with all valid balances (no NaN)
        let transactions = vec![
            create_test_transaction("2025-07-01T10:00:00Z", 10.0, 10.0, "Historical allowance"),
            create_test_transaction("2025-07-05T10:00:00Z", -3.0, 7.0, "Historical spending"),
            create_test_transaction("2025-07-15T10:00:00Z", 5.0, 12.0, "Historical bonus"),
        ];
        
        // Store historical transactions in the repository so BalanceService can access them
        let transaction_repository = connection.create_transaction_repository();
        for transaction in &transactions {
            if !transaction.balance.is_nan() {
                // Store historical transactions (non-NaN balance) in the repository
                let domain_transaction = crate::backend::domain::models::transaction::Transaction {
                    id: transaction.id.clone(),
                    child_id: child_id.clone(),
                    date: transaction.date.clone(),
                    description: transaction.description.clone(),
                    amount: transaction.amount,
                    balance: transaction.balance,
                    transaction_type: if transaction.amount > 0.0 { 
                        crate::backend::domain::models::transaction::TransactionType::Income 
                    } else { 
                        crate::backend::domain::models::transaction::TransactionType::Expense 
                    },
                };
                transaction_repository.store_transaction(&domain_transaction).unwrap();
            }
        }
        
        // Generate calendar with projected balances
        let calendar = calendar_service.generate_calendar_month_with_projected_balances(
            7, 2025, transactions, &balance_service, child_id
        );
        
        // Verify all balances are preserved as-is (no projection needed)
        let july_1_day = calendar.days.iter().find(|d| d.day == 1).unwrap();
        assert_eq!(july_1_day.balance, 10.0, "July 1st should show original stored balance");
        
        let july_5_day = calendar.days.iter().find(|d| d.day == 5).unwrap();
        assert_eq!(july_5_day.balance, 7.0, "July 5th should show original stored balance");
        
        let july_15_day = calendar.days.iter().find(|d| d.day == 15).unwrap();
        assert_eq!(july_15_day.balance, 12.0, "July 15th should show original stored balance");
        
        // Verify forward balance propagation still works
        let july_31_day = calendar.days.iter().find(|d| d.day == 31).unwrap();
        assert_eq!(july_31_day.balance, 12.0, "July 31st should carry forward final balance");
    }
} 