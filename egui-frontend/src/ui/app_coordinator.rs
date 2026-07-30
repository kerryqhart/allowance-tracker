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
use crate::backend::domain::{SyncCommand, SyncMessage, SyncStatus};
use shared::sync::EntityType;

impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // log::info!("APP UPDATE called - main render loop"); // Commented out - too verbose
        // Set up kid-friendly styling
        setup_kid_friendly_style(ctx);

        // Handle sync messages from the background sync thread (needs backend access)
        self.handle_sync_messages();

        // Detect app focus changes and trigger an immediate sync poll on focus-gain
        let is_focused = ctx.input(|i| i.focused);
        if is_focused && !self.was_focused {
            log::info!("SYNC: app gained focus — sending PollNow");
            if let Some(ref tx) = self.sync_command_tx {
                let _ = tx.send(SyncCommand::PollNow);
            }
        }
        self.was_focused = is_focused;

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
                ui.label(egui::RichText::new("Recent Transactions")
                    .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::WHITE)
                    .strong());
            }
            MainTab::Chart => {
                ui.horizontal(|ui| {
                    // Chart title on the left
                    ui.label(egui::RichText::new("Balance Chart")
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
                    ui.label(egui::RichText::new("My Goal")
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
    
    // ====================
    // SYNC MESSAGE HANDLING
    // ====================

    /// Drain all pending sync messages from the background thread. Called each frame.
    ///
    /// This must live in app_coordinator (not SyncUiState) because several message
    /// variants require backend access to read/write local repositories.
    fn handle_sync_messages(&mut self) {
        let mut local_state_dirty = false;
        while let Some(msg) = self.sync.try_recv_message() {
            match msg {
                SyncMessage::ReadEntityRequest { child_id, entity_type, entity_id, response_tx } => {
                    let json = self.read_entity_for_sync(&child_id, &entity_type, &entity_id);
                    let _ = response_tx.send(json);
                }
                SyncMessage::GetChildIdsRequest { response_tx } => {
                    let ids = match self.backend().child_service.list_children() {
                        Ok(result) => result.children.into_iter().map(|c| c.id).collect(),
                        Err(e) => {
                            log::warn!("SYNC: list_children failed: {e}");
                            Vec::new()
                        }
                    };
                    let _ = response_tx.send(ids);
                }
                SyncMessage::ApplyRemoteEntity { child_id, entity_type, entity_id, entity_json, event_id } => {
                    self.apply_remote_entity(&child_id, &entity_type, &entity_id, &entity_json, &event_id);
                    local_state_dirty = true;
                }
                SyncMessage::DeleteLocalEntity { child_id, entity_type, entity_id, event_id } => {
                    self.delete_local_entity(&child_id, &entity_type, &entity_id, &event_id);
                    local_state_dirty = true;
                }
                SyncMessage::StatusChanged(status) => {
                    self.sync.status = status;
                }
                SyncMessage::Error(error) => {
                    log::error!("Sync error: {}", error);
                    self.sync.status = SyncStatus::Error(error);
                }
                SyncMessage::PushFailed { event_id, error } => {
                    log::warn!("Sync push failed for event {}: {}", event_id, error);
                }
                SyncMessage::EntitiesUpdated { .. } => {
                    // Entity updates are applied inline via ApplyRemoteEntity; this is
                    // just a count notification — no additional action required.
                }
                SyncMessage::ConflictDetected(conflict) => {
                    self.sync.conflicts.push(conflict);
                    self.sync.status = SyncStatus::HasConflicts(self.sync.pending_conflict_count());
                }
            }
        }
        // Refresh UI once after draining, rather than per-entity during bulk sync.
        if local_state_dirty {
            self.load_initial_data();
        }
    }

    /// Read a local entity by ID and serialize it to JSON for the sync thread.
    ///
    /// Returns `Some(json)` if the entity exists, `None` if not found or on error.
    fn read_entity_for_sync(&self, child_id: &str, entity_type: &EntityType, entity_id: &str) -> Option<String> {
        match entity_type {
            EntityType::Transaction => {
                match self.core.backend.transaction_service
                    .get_transaction_by_id(child_id, entity_id)
                {
                    Ok(Some(tx)) => {
                        match serde_json::to_string(&tx) {
                            Ok(json) => Some(json),
                            Err(e) => {
                                log::warn!("Failed to serialize transaction {}: {}", entity_id, e);
                                None
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("Transaction {} not found for child {}", entity_id, child_id);
                        None
                    }
                    Err(e) => {
                        log::warn!("Error reading transaction {} for child {}: {}", entity_id, child_id, e);
                        None
                    }
                }
            }
            EntityType::Goal => {
                match self.core.backend.goal_service.get_goal_by_id(child_id, entity_id) {
                    Ok(Some(goal)) => {
                        match serde_json::to_string(&goal) {
                            Ok(json) => Some(json),
                            Err(e) => {
                                log::warn!("Failed to serialize goal {}: {}", entity_id, e);
                                None
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("Goal {} not found for child {}", entity_id, child_id);
                        None
                    }
                    Err(e) => {
                        log::warn!("Error reading goal {} for child {}: {}", entity_id, child_id, e);
                        None
                    }
                }
            }
            EntityType::Child => {
                use crate::backend::domain::commands::child::GetChildCommand;
                match self.core.backend.child_service.get_child(GetChildCommand { child_id: child_id.to_string() }) {
                    Ok(result) => {
                        match result.child {
                            Some(child) => {
                                match serde_json::to_string(&child) {
                                    Ok(json) => Some(json),
                                    Err(e) => {
                                        log::warn!("Failed to serialize child {}: {}", child_id, e);
                                        None
                                    }
                                }
                            }
                            None => {
                                log::warn!("Child {} not found", child_id);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Error reading child {}: {}", child_id, e);
                        None
                    }
                }
            }
        }
    }

    /// Apply a remote entity to local storage. Called when the sync thread pulls a
    /// remote change. Does NOT fire SyncNotifier (Option A — prevents sync loops).
    fn apply_remote_entity(
        &mut self,
        child_id: &str,
        entity_type: &EntityType,
        entity_id: &str,
        entity_json: &str,
        _event_id: &str,
    ) {
        use crate::backend::domain::models::transaction::Transaction as DomainTransaction;
        use crate::backend::domain::models::goal::DomainGoal;
        use crate::backend::domain::models::child::Child as DomainChild;

        match entity_type {
            EntityType::Transaction => {
                match serde_json::from_str::<DomainTransaction>(entity_json) {
                    Ok(transaction) => {
                        if let Err(e) = self.core.backend.transaction_service.upsert_transaction_from_sync(&transaction) {
                            log::error!("Failed to apply remote transaction {}: {}", entity_id, e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to deserialize remote transaction {}: {}", entity_id, e);
                    }
                }
            }
            EntityType::Goal => {
                match serde_json::from_str::<DomainGoal>(entity_json) {
                    Ok(goal) => {
                        if let Err(e) = self.core.backend.goal_service.upsert_goal_from_sync(&goal) {
                            log::error!("Failed to apply remote goal {}: {}", entity_id, e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to deserialize remote goal {}: {}", entity_id, e);
                    }
                }
            }
            EntityType::Child => {
                match serde_json::from_str::<DomainChild>(entity_json) {
                    Ok(child) => {
                        if let Err(e) = self.core.backend.child_service.upsert_child_from_sync(&child) {
                            log::error!("Failed to apply remote child {}: {}", child_id, e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to deserialize remote child {}: {}", child_id, e);
                    }
                }
            }
        }
    }

    /// Delete a local entity that was deleted on the remote. Does NOT fire SyncNotifier.
    fn delete_local_entity(
        &mut self,
        child_id: &str,
        entity_type: &EntityType,
        entity_id: &str,
        _event_id: &str,
    ) {
        match entity_type {
            EntityType::Transaction => {
                if let Err(e) = self.core.backend.transaction_service
                    .delete_transaction_by_id(child_id, entity_id)
                {
                    log::error!("Failed to delete local transaction {}: {}", entity_id, e);
                }
            }
            EntityType::Goal => {
                if let Err(e) = self.core.backend.goal_service.delete_goal_by_id(child_id, entity_id) {
                    log::error!("Failed to delete local goal {}: {}", entity_id, e);
                }
            }
            EntityType::Child => {
                if let Err(e) = self.core.backend.child_service.delete_child_by_id(child_id) {
                    log::error!("Failed to delete local child {}: {}", child_id, e);
                }
            }
        }
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
            log::debug!("Performing periodic allowance refresh check");
            
            // Use the existing backend method to check and issue pending allowances
            match self.core.backend.transaction_service.as_ref().check_and_issue_pending_allowances() {
                Ok(count) => {
                    if count > 0 {
                        log::info!("Periodic refresh: Issued {} pending allowances", count);

                        // Reload every transaction-derived view so the new
                        // allowance transactions show up immediately without a
                        // restart: the header balance, the calendar, the goal
                        // progress, and the chart. (Deliberately not the table —
                        // reloading it would reset the user's scroll position and
                        // pagination mid-session.)
                        log::info!("Reloading balance, calendar, goal, and chart to show new allowances");
                        self.load_balance();
                        self.load_calendar_data();
                        self.load_goal_data();
                        self.load_chart_data();

                        // Optionally show a success message to the user
                        // self.ui.set_success_message(format!("Issued {} allowances!", count));
                    } else {
                        log::debug!("Periodic refresh: No pending allowances found");
                    }
                }
                Err(e) => {
                    log::warn!("Periodic refresh failed: {}", e);
                    // Don't show error to user for background refresh - just log it
                }
            }
            
            // Mark that we just performed a refresh (updates the timestamp)
            self.ui.mark_allowance_refresh();
        }
    }
}

#[cfg(test)]
mod refresh_allowance_tests {
    use crate::ui::app_state::AllowanceTrackerApp;
    use crate::backend::Backend;
    use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
    use crate::backend::domain::commands::allowance::UpdateAllowanceConfigCommand;
    use chrono::Datelike;

    /// Regression test: when the periodic allowance refresh issues new
    /// allowance transactions, the header balance (`current_balance`) must be
    /// reloaded — not left stale until the next app restart.
    ///
    /// Reproduces the bug where `refresh_allowances` reloaded the calendar but
    /// not the balance, so the top-of-screen balance stayed stale after a
    /// background allowance was issued.
    #[test]
    fn refresh_allowances_reloads_stale_header_balance() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None)
            .expect("backend on temp dir");

        // Seed a child and make it active.
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");

        // Configure an active allowance due today, so the periodic check has
        // at least one pending allowance to issue.
        let today = chrono::Local::now().date_naive();
        backend
            .allowance_service
            .update_allowance_config(UpdateAllowanceConfigCommand {
                child_id: Some(child.id.clone()),
                amount: 10.0,
                day_of_week: today.weekday().num_days_from_sunday() as u8,
                is_active: true,
                use_age_based_amount: false,
            })
            .expect("configure allowance");

        let mut app = AllowanceTrackerApp::new_for_test(backend);

        // Simulate a stale header balance left over from before the background
        // allowance was issued.
        app.core.current_balance = 999.0;

        // Run the periodic refresh. A fresh app has never refreshed, so
        // `should_refresh_allowances()` returns true and the issuance path runs.
        app.refresh_allowances();

        // The store now reflects the issued allowance(s)...
        let store_balance = app
            .backend()
            .balance_service
            .get_current_balance(&child.id)
            .expect("store balance");
        assert!(
            store_balance > 0.0,
            "precondition: allowance issuance should have changed the store balance"
        );

        // ...and the in-memory header balance must match it, not the stale value.
        assert_eq!(
            app.current_balance(),
            store_balance,
            "header balance was not reloaded after background allowance issuance"
        );
    }
} 