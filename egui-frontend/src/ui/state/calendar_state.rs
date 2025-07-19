//! # Calendar State Module
//!
//! This module contains all state related to the calendar view and navigation.
//!
//! ## Responsibilities:
//! - Calendar month/year navigation
//! - Transaction data for calendar display
//! - Calendar interaction state (selected days, overlays)
//! - Calendar loading states
//!
//! ## Purpose:
//! This isolates all calendar-specific state management, making it easier to
//! maintain and test calendar functionality independently.

use chrono::Datelike;
use shared::*;

/// Types of overlays that can be shown for calendar day interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayType {
    AddMoney,
    SpendMoney,
    CreateGoal,
}

/// Calendar-specific state for month navigation and display
#[derive(Debug)]
pub struct CalendarState {
    /// Whether calendar is currently loading
    #[allow(dead_code)]
    pub calendar_loading: bool,
    
    /// Transactions to display on the calendar
    pub calendar_transactions: Vec<Transaction>,
    
    /// Calendar month data from backend
    pub calendar_month: Option<shared::CalendarMonth>,
    
    /// Currently selected month (1-12)
    pub selected_month: u32,
    
    /// Currently selected year
    pub selected_year: i32,
    
    /// Currently selected day on the calendar
    pub selected_day: Option<chrono::NaiveDate>,
    
    /// Day that is expanded to show all transaction chips
    pub expanded_day: Option<chrono::NaiveDate>,
    
    /// Active overlay for day interaction
    pub active_overlay: Option<OverlayType>,
    
    /// Prevents backdrop click detection on same frame modal opens
    pub modal_just_opened: bool,
}

impl CalendarState {
    /// Create new calendar state with current month/year
    pub fn new() -> Self {
        let now = chrono::Local::now();
        let current_month = now.month();
        let current_year = now.year();
        
        Self {
            calendar_loading: false,
            calendar_transactions: Vec::new(),
            calendar_month: None,
            selected_month: current_month,
            selected_year: current_year,
            selected_day: None,
            expanded_day: None,
            active_overlay: None,
            modal_just_opened: false,
        }
    }
    
    /// Navigate to the previous month
    pub fn navigate_to_previous_month(&mut self) {
        if self.selected_month == 1 {
            self.selected_month = 12;
            self.selected_year -= 1;
        } else {
            self.selected_month -= 1;
        }
        
        // Mark calendar as loading for new month
        self.calendar_loading = true;
        log::info!("📅 Navigated to previous month: {}/{}", self.selected_month, self.selected_year);
    }

    /// Navigate to the next month
    pub fn navigate_to_next_month(&mut self) {
        if self.selected_month == 12 {
            self.selected_month = 1;
            self.selected_year += 1;
        } else {
            self.selected_month += 1;
        }
        
        // Mark calendar as loading for new month
        self.calendar_loading = true;
        log::info!("📅 Navigated to next month: {}/{}", self.selected_month, self.selected_year);
    }

    /// Get the current month name as a string
    pub fn get_current_month_name(&self) -> String {
        match self.selected_month {
            1 => "January",
            2 => "February", 
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "Unknown"
        }.to_string()
    }
} 