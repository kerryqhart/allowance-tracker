//! # App State Module
//!
//! This module defines the central application state structure and initialization logic
//! for the allowance tracker app.
//!
//! ## Key Types:
//! - `MainTab` - Enum defining available tabs (Calendar, Table)
//! - `AllowanceTrackerApp` - Main application state struct
//!
//! ## Key Functions:
//! - `new()` - Initialize new app instance with backend connection
//! - `clear_messages()` - Clear success/error messages
//!
//! ## Purpose:
//! This module serves as the central state management for the entire application,
//! containing:
//! - Backend connection and data access
//! - Current user context (selected child, balance)
//! - UI state (loading, messages, current tab)
//! - Calendar state (selected month/year, transactions)
//! - Modal visibility states
//! - Form input states
//!
//! ## State Management:
//! The AllowanceTrackerApp struct holds all application state in a single location,
//! making it easy to manage and pass between different UI components. This follows
//! the single source of truth principle for state management.

use log::info;
use chrono::Datelike;
use shared::*;
use crate::backend::Backend;

/// Tabs available in the main interface
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Calendar,
    Table,
}

/// Main application struct for the egui allowance tracker
pub struct AllowanceTrackerApp {
    pub backend: Backend,
    
    // Application state
    pub current_child: Option<Child>,
    pub current_balance: f64,
    
    // UI state
    pub loading: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub current_tab: MainTab,
    
    // Calendar state
    #[allow(dead_code)]
    pub calendar_loading: bool,
    pub calendar_transactions: Vec<Transaction>,
    pub selected_month: u32,
    pub selected_year: i32,
    
    // Modal states
    pub show_add_money_modal: bool,
    pub show_spend_money_modal: bool,
    pub show_child_selector: bool,
    pub show_child_dropdown: bool,
    pub child_dropdown_just_opened: bool,
    #[allow(dead_code)]
    pub show_settings_menu: bool,
    #[allow(dead_code)]
    pub show_allowance_config_modal: bool,
    
    // Form states
    pub add_money_amount: String,
    pub add_money_description: String,
    pub spend_money_amount: String,
    pub spend_money_description: String,
}

impl AllowanceTrackerApp {
    /// Create a new AllowanceTrackerApp with default values
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, anyhow::Error> {
        info!("🚀 Initializing AllowanceTrackerApp with refactored UI");
        
        // Setup custom fonts including Chalkboard
        crate::ui::setup_custom_fonts(&cc.egui_ctx);
        
        // Install image loaders for background support
        egui_extras::install_image_loaders(&cc.egui_ctx);
        
        let backend = crate::backend::Backend::new()?;
        
        let now = chrono::Local::now();
        let current_month = now.month();
        let current_year = now.year();
        
        Ok(Self {
            backend,
            
            // Application state
            current_child: None,
            current_balance: 0.0,
            
            // UI state
            loading: true,
            error_message: None,
            success_message: None,
            current_tab: MainTab::Calendar, // Default to calendar view
            
            // Calendar state
            calendar_loading: false,
            calendar_transactions: Vec::new(),
            selected_month: current_month,
            selected_year: current_year,
            
            // Modal states
            show_add_money_modal: false,
            show_spend_money_modal: false,
            show_child_selector: false,
            show_child_dropdown: false,
            child_dropdown_just_opened: false,
            show_settings_menu: false,
            show_allowance_config_modal: false,
            
            // Form states
            add_money_amount: String::new(),
            add_money_description: String::new(),
            spend_money_amount: String::new(),
            spend_money_description: String::new(),
        })
    }

    /// Navigate to the previous month
    pub fn navigate_to_previous_month(&mut self) {
        if self.selected_month == 1 {
            self.selected_month = 12;
            self.selected_year -= 1;
        } else {
            self.selected_month -= 1;
        }
        
        // Reload calendar data for the new month
        self.calendar_loading = true;
        self.load_calendar_data();
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
        
        // Reload calendar data for the new month
        self.calendar_loading = true;
        self.load_calendar_data();
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

    /// Clear any error or success messages
    pub fn clear_messages(&mut self) {
        self.error_message = None;
        self.success_message = None;
    }
} 