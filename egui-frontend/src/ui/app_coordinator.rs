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
        log::info!("🔄 APP UPDATE called - main render loop");
        // Set up kid-friendly styling
        setup_kid_friendly_style(ctx);
        
        // Handle ESC key to close dropdown
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.child_dropdown.is_open = false;
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
            
            // STEP 2: Three-layer layout with subheader for toggle buttons
            // Calculate layout areas
            let header_height = 80.0;
            let subheader_height = 50.0;
            
            let header_rect = egui::Rect::from_min_size(
                full_rect.min,
                egui::vec2(full_rect.width(), header_height)
            );
            
            let subheader_y = full_rect.min.y + header_height;
            let subheader_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.min.x, subheader_y),
                egui::vec2(full_rect.width(), subheader_height)
            );
            
            let content_y = full_rect.min.y + header_height + subheader_height;
            let content_height = full_rect.height() - header_height - subheader_height;
            let content_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.min.x, content_y),
                egui::vec2(full_rect.width(), content_height)
            );
            
            // Layer 1: Header (existing function, positioned in header area)
            ui.allocate_ui_at_rect(header_rect, |ui| {
                self.render_header(ui);
            });
            
            // Layer 2: Subheader (Calendar/Table toggle buttons)
            ui.allocate_ui_at_rect(subheader_rect, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(20.0); // Left padding
                    
                    // Tab-specific controls on the left
                    self.draw_tab_specific_controls(ui);
                    
                    // Tab toggle buttons on the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(20.0); // Right padding
                        self.draw_tab_toggle_buttons(ui);
                    });
                });
            });
            
            // Layer 3: Content (main content area)
            ui.allocate_ui_at_rect(content_rect, |ui| {
                // Error and success messages
                self.render_messages(ui);
                
                // Main content area
                self.render_main_content(ui);
            });
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

    /// Draw tab-specific controls for the subheader
    fn draw_tab_specific_controls(&mut self, ui: &mut egui::Ui) {
        use crate::ui::app_state::MainTab;
        
        match self.current_tab {
            MainTab::Calendar => {
                self.draw_calendar_navigation_controls(ui);
            }
            MainTab::Table => {
                // Future: Table-specific controls (filters, sorting, etc.)
                // For now, leave empty
            }
        }
    }

    /// Draw calendar month navigation controls
    fn draw_calendar_navigation_controls(&mut self, ui: &mut egui::Ui) {
        use crate::ui::components::theme::colors;
        
        ui.horizontal(|ui| {
            // Previous month button with consistent hover styling
            let prev_button = egui::Button::new("<")
                .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100))
                .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple outline
                .rounding(egui::Rounding::same(6.0))
                .min_size(egui::vec2(35.0, 35.0));
            
            if ui.add(prev_button).clicked() {
                self.navigate_to_previous_month();
            }
            
            ui.add_space(15.0);
            
            // Current month and year display - disable selection to prevent dropdown interference
            let month_year_text = format!("{} {}", self.get_current_month_name(), self.selected_year);
            ui.add(egui::Label::new(egui::RichText::new(month_year_text)
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::WHITE)
                .strong())
                .selectable(false)); // Disable text selection
            
            ui.add_space(15.0);
            
            // Next month button with consistent hover styling
            let next_button = egui::Button::new(">")
                .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100))
                .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple outline
                .rounding(egui::Rounding::same(6.0))
                .min_size(egui::vec2(35.0, 35.0));
            
            if ui.add(next_button).clicked() {
                self.navigate_to_next_month();
            }
        });
    }
} 