//! Calendar domain logic for the allowance tracker.
//!
//! This module contains all business logic related to calendar operations,
//! date calculations, and transaction organization by date. The UI should
//! only handle presentation concerns, while all calendar computations
//! and business rules are handled here.

use shared::{Transaction, CalendarMonth, CalendarDay, CurrentDateResponse};
use std::collections::HashMap;
use chrono::{Local, Datelike};

/// Calendar service that handles all calendar-related business logic
#[derive(Clone)]
pub struct CalendarService;

impl CalendarService {
    /// Create a new CalendarService instance
    pub fn new() -> Self {
        Self
    }

    /// Generate a calendar month view with transaction data
    pub fn generate_calendar_month(
        &self,
        month: u32,
        year: u32,
        transactions: Vec<Transaction>,
    ) -> CalendarMonth {
        let days_in_month = self.days_in_month(month, year);
        let first_day = self.first_day_of_month(month, year);
        
        // Group transactions by day for the current month
        let transactions_by_day = self.group_transactions_by_day(month, year, &transactions);
        
        // Calculate daily balances
        let daily_balances = self.calculate_daily_balances(month, year, &transactions, days_in_month);
        
        let mut calendar_days = Vec::new();
        
        // Add empty cells for days before the first day of month
        for _ in 0..first_day {
            calendar_days.push(CalendarDay {
                day: 0,
                balance: 0.0,
                transactions: Vec::new(),
                is_empty: true,
            });
        }
        
        // Add days of the month
        for day in 1..=days_in_month {
            let day_transactions = transactions_by_day.get(&day).cloned().unwrap_or_default();
            let day_balance = daily_balances.get(&day).copied().unwrap_or(0.0);
            
            calendar_days.push(CalendarDay {
                day,
                balance: day_balance,
                transactions: day_transactions,
                is_empty: false,
            });
        }
        
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
        // Calculate using Zeller's congruence or similar algorithm
        // This is a simplified calculation - in production, consider using a proper date library
        let days_since_epoch = (year - 1970) * 365 + (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
        let days_in_months = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let mut total_days = days_since_epoch + days_in_months[(month - 1) as usize];
        
        // Add leap day if current year is leap and month > February
        if month > 2 && self.is_leap_year(year) {
            total_days += 1;
        }
        
        (total_days + 4) % 7 // January 1, 1970 was a Thursday (4)
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

    /// Calculate daily running balances for a month
    fn calculate_daily_balances(
        &self,
        month: u32,
        year: u32,
        transactions: &[Transaction],
        days_in_month: u32,
    ) -> HashMap<u32, f64> {
        let mut daily_balances: HashMap<u32, f64> = HashMap::new();
        
        // Sort all transactions by date to get proper chronological order
        let mut sorted_transactions = transactions.to_vec();
        sorted_transactions.sort_by(|a, b| {
            let date_a = self.parse_transaction_date(&a.date).unwrap_or((0, 0, 0));
            let date_b = self.parse_transaction_date(&b.date).unwrap_or((0, 0, 0));
            // Reverse chronological order (newest first)
            date_b.cmp(&date_a)
        });
        
        // Find the balance at the start of this month
        let mut current_balance = self.calculate_starting_balance_for_month(
            month,
            year,
            &sorted_transactions,
        );
        
        // Group transactions by day for this month
        let transactions_by_day = self.group_transactions_by_day(month, year, transactions);
        
        // Calculate balance for each day
        for day in 1..=days_in_month {
            if let Some(day_transactions) = transactions_by_day.get(&day) {
                let daily_change: f64 = day_transactions.iter().map(|t| t.amount).sum();
                current_balance += daily_change;
            }
            daily_balances.insert(day, current_balance);
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
        // Find first transaction of current month to calculate starting balance
        for transaction in sorted_transactions {
            if let Some((t_year, t_month, _)) = self.parse_transaction_date(&transaction.date) {
                if t_year == year && t_month == month {
                    // This is a transaction in our target month
                    // Work backwards to get starting balance
                    return transaction.balance - transaction.amount;
                }
            }
        }
        
        // No transactions found in this month, return 0 as default
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
        let day_1 = calendar.days.iter().find(|d| d.day == 1 && !d.is_empty);
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
} 