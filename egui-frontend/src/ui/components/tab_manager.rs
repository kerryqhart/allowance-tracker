//! # Tab Manager Module
//!
//! This module handles the main content routing and tab management for the allowance tracker app.
//!
//! ## Key Functions:
//! - `render_main_content()` - Routes to appropriate content based on selected tab
//!
//! ## Purpose:
//! This module acts as the central content router, determining which UI components to render
//! based on the user's current tab selection (Calendar vs Table view). It integrates with
//! the toggle header system to provide consistent navigation throughout the app.
//!
//! ## Tab Flow:
//! - MainTab::Calendar -> Renders calendar view with transactions
//! - MainTab::Table -> Renders transaction table view
//! - Future tabs can be easily added by extending the MainTab enum

use eframe::egui;
use log::info;
use crate::ui::app_state::{AllowanceTrackerApp, MainTab};

impl AllowanceTrackerApp {
    /// Render the main content area
    pub fn render_main_content(&mut self, ui: &mut egui::Ui) {
        info!("📄 RENDER_MAIN_CONTENT called");
        ui.vertical(|ui| {
            // Render content based on selected tab with toggle header
            match self.current_tab() {
                MainTab::Calendar => {
                    // Use full available space - let calendar manage its own margins
                    let available_rect = ui.available_rect_before_wrap();
                    
                    // DEBUG: Log tab manager space allocation
                    info!("📋 TAB_MANAGER: available_rect.height={:.0}, passing to calendar", available_rect.height());
                    
                    self.draw_calendar_section_with_toggle(ui, available_rect, &self.calendar.calendar_transactions.clone());
                    
                    // No bottom spacing - test for other padding sources
                    // ui.add_space(0.0); // Removed entirely
                }
                MainTab::Table => {
                    // Use table state for infinite scroll instead of calendar transactions
                    let table_transactions = self.table.displayed_transactions.clone();
                    
                    // Use full available space - let table manage its own margins
                    let available_rect = ui.available_rect_before_wrap();
                    self.draw_transactions_section_with_toggle(ui, available_rect, &table_transactions);
                    
                    // Small bottom spacing to prevent edge contact
                    ui.add_space(10.0);
                }
            }
        });
    }
} 