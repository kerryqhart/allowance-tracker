//! # App Coordinator Module
//!
//! This module contains the main application coordination logic, handling the primary
//! update loop and overall application lifecycle.
//!
//! ## Key Functions:
//! - `eframe::App::update()` - Main application update loop (implements eframe::App trait)
//! - `render_loading_screen()` - Displays loading screen while data is being fetched
//!
//! ## Purpose:
//! This module serves as the central coordinator for the entire application, orchestrating:
//! - UI styling setup
//! - Input handling (ESC key, etc.)
//! - Data loading coordination
//! - Main content rendering
//! - Modal management
//! - Header rendering
//!
//! ## Application Flow:
//! 1. Set up kid-friendly styling
//! 2. Handle global input (ESC key)
//! 3. Load data if needed
//! 4. Render loading screen OR main content
//! 5. Render header and any active modals
//!
//! This is the main entry point that ties together all other UI modules.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::*;

impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set up kid-friendly styling
        setup_kid_friendly_style(ctx);
        
        // Handle ESC key to close dropdown
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_child_dropdown = false;
        }
        
        // Load initial data on first run
        if self.loading && self.current_child.is_none() {
            self.load_initial_data();
        }
        
        // Clear messages after a delay
        if self.error_message.is_some() || self.success_message.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        
        // Main UI with image background
        egui::CentralPanel::default().show(ctx, |ui| {
            // Draw image background with blue overlay first
            let full_rect = ui.available_rect_before_wrap();
            crate::ui::draw_image_background(ui, full_rect);
            
            if self.loading {
                self.render_loading_screen(ui);
                return;
            }
            
            // Header
            self.render_header(ui);
            
            // Error and success messages
            self.render_messages(ui);
            
            // Main content area
            self.render_main_content(ui);
        });
        
        // Render modals
        self.render_modals(ctx);
    }
}

impl AllowanceTrackerApp {
    /// Render the loading screen
    pub fn render_loading_screen(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.label("Loading...");
        });
    }
} 