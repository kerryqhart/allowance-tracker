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
use crate::ui::app_state::{AllowanceTrackerApp, MainTab};
use shared::TransactionType;

impl AllowanceTrackerApp {
    /// Render the main content area
    pub fn render_main_content(&mut self, ui: &mut egui::Ui) {
        log::info!("📄 RENDER_MAIN_CONTENT called");
        ui.vertical(|ui| {
            // Render content based on selected tab with toggle header
            match self.current_tab {
                MainTab::Calendar => {
                    // Reserve space for bottom margin before drawing calendar
                    let mut available_rect = ui.available_rect_before_wrap();
                    available_rect.max.y -= 30.0; // Reserve 30px bottom margin
                    self.draw_calendar_section_with_toggle(ui, available_rect, &self.calendar_transactions.clone());
                    
                    // Add bottom spacing to ensure the calendar doesn't touch the edge
                    ui.add_space(30.0);
                }
                MainTab::Table => {
                    // Filter out future allowances - table should only show actual transactions
                    let actual_transactions: Vec<_> = self.calendar_transactions.iter()
                        .filter(|t| t.transaction_type != TransactionType::FutureAllowance)
                        .cloned()
                        .collect();
                    
                    // Reserve space for bottom margin before drawing table
                    let mut available_rect = ui.available_rect_before_wrap();
                    available_rect.max.y -= 30.0; // Reserve 30px bottom margin
                    self.draw_transactions_section_with_toggle(ui, available_rect, &actual_transactions);
                    
                    // Add bottom spacing to ensure the table doesn't touch the edge
                    ui.add_space(30.0);
                }
            }
        });
    }
} 