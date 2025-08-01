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
use crate::ui::components::styling::{setup_kid_friendly_style, draw_image_background};

impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // log::info!("🔄 APP UPDATE called - main render loop"); // Commented out - too verbose
        // Set up kid-friendly styling
        setup_kid_friendly_style(ctx);
        
        // Handle ESC key to close dropdown
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.interaction.child_dropdown.is_open = false;
        }
        
        // Load initial data on first run
        // Note: Use cached current_child here to avoid infinite backend calls during loading
        if self.ui.loading && self.core.current_child.is_none() {
            self.load_initial_data();
        }
        
        // Check for pending allowances periodically (throttled to avoid excessive calls)
        // This allows the app to issue allowances without requiring a restart
        // The refresh is throttled using Instant/Duration timing to prevent checking every frame
        self.refresh_allowances();
        
        // Clear messages after a delay
        if self.ui.error_message.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        
        // Main UI with image background
        egui::CentralPanel::default().show(ctx, |ui| {
            // Draw image background with blue overlay first
            let full_rect = ui.available_rect_before_wrap();
            draw_image_background(ui, full_rect);
            
            if self.ui.loading {
                self.render_loading_screen(ui);
                return;
            }
            
            // STEP 2: Four-layer layout with selection controls bar and subheader for toggle buttons
            // Calculate layout areas - optimized reservations for better space utilization
            let header_height = 70.0; // Reduced from 80px
            let selection_bar_height = if self.interaction.transaction_selection_mode { 50.0 } else { 0.0 };
            let subheader_height = 50.0; // Toggle buttons area
            
            // Content area dimensions (remaining space after header, selection bar, and subheader)
            let content_height = full_rect.height() - header_height - selection_bar_height - subheader_height;
            
            // Define rectangles for each layer
            let header_rect = egui::Rect::from_min_size(
                full_rect.min,
                egui::vec2(full_rect.width(), header_height)
            );
            
            let selection_bar_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.left(), full_rect.top() + header_height),
                egui::vec2(full_rect.width(), selection_bar_height)
            );
            
            let subheader_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.left(), full_rect.top() + header_height + selection_bar_height),
                egui::vec2(full_rect.width(), subheader_height)
            );
            
            let content_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.left(), full_rect.top() + header_height + selection_bar_height + subheader_height),
                egui::vec2(full_rect.width(), content_height)
            );
            
            // DEBUG: Log parent space allocation (commented out - too verbose)
            // log::info!("🏢 WINDOW SPACE: full_rect.height={:.0}, content_height={:.0}, reserved={:.0}px", 
            //           full_rect.height(), content_height, 
            //           header_height + selection_bar_height + subheader_height);
            
            // Layer 1: Header (existing function, positioned in header area)
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                self.render_header(ui);
            });
            
            // Layer 2: Selection controls bar (only when in selection mode)
            if self.interaction.transaction_selection_mode {
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(selection_bar_rect), |ui| {
                    self.render_selection_controls_bar(ui);
                });
            }
            
            // Layer 3: Subheader (Calendar/Table toggle buttons)
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(subheader_rect), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(20.0); // Left padding
                    
                    // Tab-specific controls on the left with vertical centering
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        self.draw_tab_specific_controls(ui);
                    });
                    
                    // Tab toggle buttons on the right with vertical centering
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(20.0); // Right padding
                        self.draw_tab_toggle_buttons(ui);
                    });
                });
            });
            
            // Layer 4: Content (main content area)
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
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
    /// Check if the current goal is complete (helper function)
    pub fn is_goal_complete(&self) -> bool {
        if let Some(ref calculation) = self.goal.goal_calculation {
            calculation.amount_needed <= 0.0
        } else {
            false
        }
    }

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
        use crate::ui::components::chart_renderer::ChartPeriod;
        
        match self.current_tab() {
            MainTab::Calendar => {
                self.draw_calendar_navigation_controls(ui);
            }
            MainTab::Table => {
                // Show table title in subheader
                ui.label(egui::RichText::new("📋 Recent Transactions")
                    .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::WHITE)
                    .strong());
            }
            MainTab::Chart => {
                ui.horizontal(|ui| {
                    // Chart title on the left
                    ui.label(egui::RichText::new("📊 Balance Chart")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::WHITE)
                        .strong());
                    
                    ui.add_space(20.0); // Space between title and buttons
                    
                    // Time period buttons
                    // 30 Days button
                    let days_30_button = egui::Button::new(
                        egui::RichText::new("30 Days")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(if self.chart.selected_period == ChartPeriod::Days30 { 
                                egui::Color32::WHITE 
                            } else { 
                                egui::Color32::from_gray(200) 
                            })
                    )
                    .min_size(egui::vec2(60.0, 28.0))
                    .corner_radius(egui::CornerRadius::same(6))
                    .fill(if self.chart.selected_period == ChartPeriod::Days30 {
                        egui::Color32::from_rgb(100, 150, 255) // Active blue
                    } else {
                        egui::Color32::from_rgb(240, 240, 240) // Light gray background for inactive
                    })
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)));
                    
                    if ui.add(days_30_button).clicked() {
                        self.chart.selected_period = ChartPeriod::Days30;
                        self.chart.chart_data.clear(); // Clear data to force reload
                        self.load_chart_data();
                    }
                    
                    ui.add_space(8.0);
                    
                    // 90 Days button
                    let days_90_button = egui::Button::new(
                        egui::RichText::new("90 Days")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(if self.chart.selected_period == ChartPeriod::Days90 { 
                                egui::Color32::WHITE 
                            } else { 
                                egui::Color32::from_gray(200) 
                            })
                    )
                    .min_size(egui::vec2(60.0, 28.0))
                    .corner_radius(egui::CornerRadius::same(6))
                    .fill(if self.chart.selected_period == ChartPeriod::Days90 {
                        egui::Color32::from_rgb(100, 150, 255) // Active blue
                    } else {
                        egui::Color32::from_rgb(240, 240, 240) // Light gray background for inactive
                    })
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)));
                    
                    if ui.add(days_90_button).clicked() {
                        self.chart.selected_period = ChartPeriod::Days90;
                        self.chart.chart_data.clear(); // Clear data to force reload
                        self.load_chart_data();
                    }
                    
                    ui.add_space(8.0);
                    
                    // All Time button
                    let all_time_button = egui::Button::new(
                        egui::RichText::new("All Time")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(if self.chart.selected_period == ChartPeriod::AllTime { 
                                egui::Color32::WHITE 
                            } else { 
                                egui::Color32::from_rgb(100, 100, 100) 
                            })
                    )
                    .min_size(egui::vec2(70.0, 28.0))
                    .corner_radius(egui::CornerRadius::same(6))
                    .fill(if self.chart.selected_period == ChartPeriod::AllTime {
                        egui::Color32::from_rgb(100, 150, 255) // Active blue
                    } else {
                        egui::Color32::from_rgb(240, 240, 240) // Light gray background for inactive
                    })
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)));
                    
                    if ui.add(all_time_button).clicked() {
                        self.chart.selected_period = ChartPeriod::AllTime;
                        self.chart.chart_data.clear(); // Clear data to force reload
                        self.load_chart_data();
                    }
                });
            }
            MainTab::Goal => {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // Show goal title in subheader with proper vertical centering
                    ui.label(egui::RichText::new("🎯 My Goal")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::WHITE)
                        .strong());
                    
                    // Add cancel button if there's an active goal
                    if self.goal.has_active_goal() {
                        ui.add_space(20.0);
                        
                        // Change button text based on goal completion status
                        let button_text = if self.is_goal_complete() {
                            "Start new goal"
                        } else {
                            "Cancel Goal"
                        };
                        
                        // Match the styling of the inactive toggle buttons
                        let cancel_button = egui::Button::new(egui::RichText::new(button_text)
                                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                .strong()
                                .color(egui::Color32::from_rgb(100, 100, 100))) // Same gray text as inactive buttons
                            .fill(egui::Color32::from_rgb(240, 240, 240)) // Same light gray background as inactive buttons
                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200))) // Same light gray border as inactive buttons
                            .corner_radius(egui::CornerRadius::same(8)) // Same rounding as toggle buttons
                            .min_size(egui::vec2(110.0, 35.0)); // Same height as toggle buttons
                        
                        if ui.add(cancel_button).clicked() {
                            self.cancel_current_goal();
                        }
                    }
                });
            }
        }
    }

    /// Draw calendar month navigation controls
    fn draw_calendar_navigation_controls(&mut self, ui: &mut egui::Ui) {
        use crate::ui::components::styling::colors;
        
        ui.horizontal(|ui| {
            // Previous month button with consistent hover styling
            let prev_button = egui::Button::new("<")
                .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100))
                .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple outline
                .corner_radius(egui::CornerRadius::same(6))
                .min_size(egui::vec2(35.0, 35.0));
            
            if ui.add(prev_button).clicked() {
                self.navigate_month(-1);
            }
            
            ui.add_space(15.0);
            
            // Calculate the maximum width needed for any month name + year
            let font_id = egui::FontId::new(16.0, egui::FontFamily::Proportional);
            let current_year = self.calendar.selected_year;
            
            // Test all month names with the current year to find the maximum width
            let month_names = [
                "January", "February", "March", "April", "May", "June",
                "July", "August", "September", "October", "November", "December"
            ];
            
            let max_width = month_names.iter()
                .map(|month| {
                    let text = format!("{} {}", month, current_year);
                    ui.fonts(|f| f.layout_no_wrap(
                        text, 
                        font_id.clone(), 
                        egui::Color32::WHITE
                    )).size().x
                })
                .fold(0.0, f32::max);
            
            // Add padding for safety
            let fixed_width = max_width + 20.0;
            
            // Current month and year display in fixed-width area
            let month_year_text = format!("{} {}", self.get_current_month_name(), self.calendar.selected_year);
            ui.allocate_ui_with_layout(
                egui::vec2(fixed_width, 35.0),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(month_year_text)
                        .font(font_id)
                        .color(egui::Color32::WHITE)
                        .strong())
                        .selectable(false)); // Disable text selection
                }
            );
            
            ui.add_space(15.0);
            
            // Next month button with consistent hover styling
            let next_button = egui::Button::new(">")
                .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100))
                .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple outline
                .corner_radius(egui::CornerRadius::same(6))
                .min_size(egui::vec2(35.0, 35.0));
            
            if ui.add(next_button).clicked() {
                self.navigate_to_next_month();
            }
        });
    }
    
    /// Refresh pending allowances if enough time has passed since last check
    /// 
    /// This method implements periodic allowance checking without overwhelming the system.
    /// Since egui's update() loop runs 60+ times per second, we need to throttle
    /// allowance checks to avoid excessive CPU usage and database calls.
    /// 
    /// Timing Strategy:
    /// - Use Instant::now() to track when we last checked allowances
    /// - Use Duration to define the interval (default: 5 minutes)
    /// - Only check allowances when enough time has passed
    /// - This prevents checking allowances every frame while keeping the app responsive
    /// 
    /// Why not frame counting? Frame rates vary, so timing would be inconsistent.
    /// Why not external timers? Overkill for this simple use case.
    /// Why Instant/Duration? Designed for this exact purpose - measuring time intervals.
    pub fn refresh_allowances(&mut self) {
        // Check if it's time to refresh allowances (throttled to avoid excessive calls)
        if self.ui.should_refresh_allowances() {
            log::info!("🔄 Performing periodic allowance refresh check");
            
            // Use the existing backend method to check and issue pending allowances
            match self.core.backend.transaction_service.as_ref().check_and_issue_pending_allowances() {
                Ok(count) => {
                    if count > 0 {
                        log::info!("🎯 Periodic refresh: Issued {} pending allowances", count);
                        
                        // Reload calendar data to show the new allowance transactions immediately
                        // This ensures the calendar view updates without requiring manual navigation
                        log::info!("🔄 Reloading calendar data to show new allowances");
                        self.load_calendar_data();
                        
                        // Optionally show a success message to the user
                        // self.ui.set_success_message(format!("Issued {} allowances!", count));
                    } else {
                        log::debug!("🎯 Periodic refresh: No pending allowances found");
                    }
                }
                Err(e) => {
                    log::warn!("🎯 Periodic refresh failed: {}", e);
                    // Don't show error to user for background refresh - just log it
                }
            }
            
            // Mark that we just performed a refresh (updates the timestamp)
            self.ui.mark_allowance_refresh();
        }
    }
} 