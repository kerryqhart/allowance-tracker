//! # Data Directory Modal
//!
//! This module contains the data directory management modal functionality.
//!
//! ## Responsibilities:
//! - Display current data directory location
//! - Allow user to browse for new data directory location
//! - Handle conflict detection and resolution when target location has existing data
//! - Integrate with backend DataDirectoryService API
//! - Provide visual feedback and progress indication
//!
//! ## Purpose:
//! This modal provides an intuitive interface for managing child data directory
//! locations with proper conflict resolution and backend integration.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::settings::shared::{
    SettingsModalStyle, render_form_field_with_error
};
use shared::{
    CheckDataDirectoryConflictRequest, 
    RelocateWithConflictResolutionRequest, ConflictResolution,
    ReturnToDefaultLocationRequest
};

impl AllowanceTrackerApp {
    /// Render the data directory management modal
    pub fn render_data_directory_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_data_directory_modal {
            return;
        }

        log::info!("📁 Rendering data directory modal");

        // Load current directory if not already loaded
        if self.settings.data_directory_form.current_path.is_empty() && !self.settings.data_directory_form.is_loading {
            self.load_current_data_directory();
        }

        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("data_directory_modal_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Dark semi-transparent background
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128)
                );

                // Center the modal content
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(screen_rect), |ui| {
                    ui.centered_and_justified(|ui| {
                        let style = SettingsModalStyle::default_style();
                        style.apply_frame_styling()
                            .show(ui, |ui| {
                                // Set modal size
                                ui.set_min_size(egui::vec2(600.0, 500.0));
                                ui.set_max_size(egui::vec2(600.0, 500.0));

                                ui.vertical_centered(|ui| {
                                    ui.add_space(15.0);

                                    // Title
                                    ui.label(egui::RichText::new("📁 Data Directory")
                                        .font(egui::FontId::new(style.title_font_size, egui::FontFamily::Proportional))
                                        .strong()
                                        .color(style.title_color));

                                    ui.add_space(20.0);

                                    // Subtitle/instructions
                                    ui.label(egui::RichText::new("Manage where your child's data is stored")
                                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                        .color(egui::Color32::from_rgb(100, 100, 100)));

                                    ui.add_space(25.0);

                                    // Form content
                                    if self.settings.data_directory_form.has_conflict {
                                        self.render_conflict_resolution_content(ui);
                                    } else {
                                        self.render_data_directory_form_content(ui);
                                    }

                                    ui.add_space(25.0);

                                    // Action buttons
                                    self.render_data_directory_action_buttons(ui);

                                    ui.add_space(15.0);
                                });
                            });
                    });
                });

                // Handle backdrop clicks to close modal
                if ui.ctx().input(|i| i.pointer.any_click()) {
                    if let Some(pointer_pos) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        let modal_rect = egui::Rect::from_center_size(
                            ui.ctx().screen_rect().center(),
                            egui::vec2(600.0, 500.0)
                        );
                        
                        if !modal_rect.contains(pointer_pos) {
                            log::info!("📁 Data directory modal closed via backdrop click");
                            self.close_data_directory_modal();
                        }
                    }
                }
            });
    }

    /// Render the main form content for data directory modal
    fn render_data_directory_form_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Current location section
            ui.label(egui::RichText::new("Current Data Directory")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong());
            
            ui.add_space(10.0);

            if self.settings.data_directory_form.is_loading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Loading current directory...");
                });
            } else {
                ui.label(egui::RichText::new(&self.settings.data_directory_form.current_path)
                    .font(egui::FontId::new(13.0, egui::FontFamily::Monospace))
                    .color(egui::Color32::from_rgb(70, 130, 180)));
                
                // Show "Return to Default Location" button if data is redirected
                if self.settings.data_directory_form.is_redirected {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("(Data is redirected)")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(150, 100, 50)));
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(egui::RichText::new("Return to Default Location")
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional)))
                                .clicked() 
                            {
                                self.return_to_default_location();
                            }
                        });
                    });
                }
            }

            ui.add_space(20.0);

            // New location section
            ui.label(egui::RichText::new("New Data Directory")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong());
            
            ui.add_space(10.0);

            // File selection section
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Select New Location:")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional)));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(egui::RichText::new("📁 Browse...")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional)))
                        .clicked() 
                    {
                        self.open_data_directory_browser();
                    }
                });
            });

            ui.add_space(8.0);

            // Manual path input
            let path_response = render_form_field_with_error(
                ui,
                "Directory Path",
                &mut self.settings.data_directory_form.new_path,
                "Enter directory path or use Browse button",
                &None, // No error message for now
                None, // No character limit
            );

            // Update state when path changes
            if path_response.changed() {
                self.settings.data_directory_form.clear_messages();
            }

            ui.add_space(15.0);

            // Help text
            ui.label(egui::RichText::new("💡 Tip: Use the Browse button to easily select a directory")
                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));

            ui.add_space(20.0);

            // Show messages if any
            if let Some(ref success_msg) = self.settings.data_directory_form.success_message {
                ui.label(egui::RichText::new(success_msg)
                    .color(egui::Color32::from_rgb(0, 150, 0)));
            }

            if let Some(ref error_msg) = self.settings.data_directory_form.error_message {
                ui.label(egui::RichText::new(error_msg)
                    .color(egui::Color32::from_rgb(200, 0, 0)));
            }
        });
    }

    /// Render conflict resolution content
    fn render_conflict_resolution_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Conflict warning
            ui.label(egui::RichText::new("⚠️ Conflict Detected")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(200, 100, 0)));

            ui.add_space(10.0);

            if let Some(ref details) = self.settings.data_directory_form.conflict_details {
                ui.label(egui::RichText::new(details)
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(100, 100, 100)));
            }

            ui.add_space(20.0);

            // Resolution options
            ui.label(egui::RichText::new("How would you like to resolve this?")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong());

            ui.add_space(15.0);

            // Option 1: Overwrite target
            let overwrite_selected = matches!(self.settings.data_directory_form.user_decision, Some(ConflictResolution::OverwriteTarget));
            if ui.radio(overwrite_selected, "Overwrite target location with current data").clicked() {
                self.settings.data_directory_form.user_decision = Some(ConflictResolution::OverwriteTarget);
            }
            ui.label(egui::RichText::new("   Replace the target directory contents with your current child data")
                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));

            ui.add_space(10.0);

            // Option 2: Use target data
            let use_target_selected = matches!(self.settings.data_directory_form.user_decision, Some(ConflictResolution::UseTargetData));
            if ui.radio(use_target_selected, "Use existing data at target location").clicked() {
                self.settings.data_directory_form.user_decision = Some(ConflictResolution::UseTargetData);
            }
            ui.label(egui::RichText::new("   Switch to using the data found at the target location (current data will be archived)")
                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));

            ui.add_space(10.0);

            // Option 3: Cancel
            let cancel_selected = matches!(self.settings.data_directory_form.user_decision, Some(ConflictResolution::Cancel));
            if ui.radio(cancel_selected, "Cancel the operation").clicked() {
                self.settings.data_directory_form.user_decision = Some(ConflictResolution::Cancel);
            }
            ui.label(egui::RichText::new("   Keep using the current data directory")
                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));
        });
    }

    /// Render action buttons for data directory modal
    fn render_data_directory_action_buttons(&mut self, ui: &mut egui::Ui) {
        let is_loading = self.settings.data_directory_form.is_loading;
        let has_conflict = self.settings.data_directory_form.has_conflict;
        let has_success = self.settings.data_directory_form.success_message.is_some();

        // Use mutable borrow flags to handle closure conflicts
        let mut should_proceed = false;
        let mut should_move = false;
        let mut should_close = false;

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if has_success {
                    // After successful move, only show Close button
                    if ui.button(egui::RichText::new("Close")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional)))
                        .clicked() 
                    {
                        should_close = true;
                    }
                } else {
                    // Before move, show Cancel and primary action button
                    if ui.button(egui::RichText::new("Cancel")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional)))
                        .clicked() 
                    {
                        should_close = true;
                    }

                    ui.add_space(10.0);

                    // Primary action button
                    let (button_text, form_ready) = if has_conflict {
                        ("Proceed with Resolution", self.settings.data_directory_form.is_ready_for_resolution())
                    } else if is_loading {
                        ("⏳ Processing...", false)
                    } else {
                        ("Move Directory", self.settings.data_directory_form.is_ready_for_change())
                    };

                    let button = egui::Button::new(egui::RichText::new(button_text)
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .strong())
                        .fill(if form_ready && !is_loading {
                            egui::Color32::from_rgb(70, 130, 180) // Steel blue
                        } else {
                            egui::Color32::LIGHT_GRAY
                        });

                    if ui.add_enabled(form_ready && !is_loading, button).clicked() {
                        if has_conflict {
                            should_proceed = true;
                        } else {
                            should_move = true;
                        }
                    }
                }
            });
        });

        // Handle actions outside the UI closure to avoid borrowing conflicts
        if should_proceed {
            self.proceed_with_data_directory_resolution();
        }
        if should_move {
            self.move_data_directory();
        }
        if should_close {
            log::info!("📁 Data directory modal closed");
            self.close_data_directory_modal();
        }
    }

    /// Load current data directory from backend
    fn load_current_data_directory(&mut self) {
        log::info!("📁 Loading current data directory");
        self.settings.data_directory_form.set_loading(true);

        let child_id = self.get_current_child_from_backend().as_ref().map(|c| c.id.clone());
        
        match self.backend().data_directory_service.get_current_directory(child_id) {
            Ok(response) => {
                self.settings.data_directory_form.current_path = response.current_path;
                self.settings.data_directory_form.is_redirected = response.is_redirected;
                self.settings.data_directory_form.set_loading(false);
                log::info!("✅ Loaded current directory: {} (redirected: {})", self.settings.data_directory_form.current_path, response.is_redirected);
            }
            Err(e) => {
                log::error!("🚨 Failed to load current directory: {}", e);
                self.settings.data_directory_form.set_loading(false);
                self.settings.data_directory_form.set_error(format!("Failed to load current directory: {}", e));
            }
        }
    }

    /// Open native file browser to select directory
    fn open_data_directory_browser(&mut self) {
        log::info!("📁 Opening native directory browser");

        // Open directory picker dialog
        let directory_dialog = rfd::FileDialog::new()
            .set_title("Select Data Directory");

        // Set initial directory to current location if available
        let dialog_with_dir = if !self.settings.data_directory_form.current_path.is_empty() {
            let current_path = std::path::Path::new(&self.settings.data_directory_form.current_path);
            if let Some(parent) = current_path.parent() {
                directory_dialog.set_directory(parent)
            } else {
                directory_dialog
            }
        } else {
            directory_dialog
        };

        // Execute the dialog
        if let Some(path) = dialog_with_dir.pick_folder() {
            log::info!("📁 User selected directory: {:?}", path);
            self.settings.data_directory_form.new_path = path.to_string_lossy().to_string();
            self.settings.data_directory_form.clear_messages();
        } else {
            log::info!("📁 User cancelled directory selection");
        }
    }

    /// Move data directory (with automatic conflict checking)
    fn move_data_directory(&mut self) {
        log::info!("📁 Moving data directory");
        self.settings.data_directory_form.set_loading(true);

        let child_id = self.get_current_child_from_backend().as_ref().map(|c| c.id.clone());
        let request = CheckDataDirectoryConflictRequest {
            child_id,
            new_path: self.settings.data_directory_form.new_path.clone(),
        };

        match self.backend().data_directory_service.check_relocation_conflicts(request) {
            Ok(response) => {
                self.settings.data_directory_form.set_loading(false);
                
                if response.has_conflict {
                    log::info!("⚠️ Conflict detected: {:?}", response.conflict_details);
                    self.settings.data_directory_form.set_conflict(true, response.conflict_details);
                } else {
                    log::info!("✅ No conflicts - proceeding with relocation");
                    // No conflicts, proceed directly with relocation
                    self.proceed_with_simple_relocation();
                }
            }
            Err(e) => {
                log::error!("🚨 Failed to move directory: {}", e);
                self.settings.data_directory_form.set_loading(false);
                self.settings.data_directory_form.set_error(format!("Failed to move directory: {}", e));
            }
        }
    }

    /// Proceed with simple relocation (no conflicts)
    fn proceed_with_simple_relocation(&mut self) {
        log::info!("📁 Proceeding with simple relocation");
        self.settings.data_directory_form.set_loading(true);

        let child_id = self.get_current_child_from_backend().as_ref().map(|c| c.id.clone());
        let request = RelocateWithConflictResolutionRequest {
            child_id,
            new_path: self.settings.data_directory_form.new_path.clone(),
            resolution: ConflictResolution::OverwriteTarget, // Safe since no conflicts
        };

        match self.backend().data_directory_service.relocate_with_conflict_resolution(request) {
            Ok(response) => {
                self.settings.data_directory_form.set_loading(false);
                
                if response.success {
                    log::info!("✅ Data directory relocated successfully");
                    self.settings.data_directory_form.set_success(response.message);
                    // Update current path
                    self.settings.data_directory_form.current_path = response.new_path;
                } else {
                    log::error!("🚨 Relocation failed: {}", response.message);
                    self.settings.data_directory_form.set_error(response.message);
                }
            }
            Err(e) => {
                log::error!("🚨 Relocation service error: {}", e);
                self.settings.data_directory_form.set_loading(false);
                self.settings.data_directory_form.set_error(format!("Relocation failed: {}", e));
            }
        }
    }

    /// Proceed with conflict resolution
    fn proceed_with_data_directory_resolution(&mut self) {
        log::info!("📁 Proceeding with conflict resolution");
        
        let resolution = match self.settings.data_directory_form.user_decision.clone() {
            Some(resolution) => resolution,
            None => {
                log::warn!("No resolution selected");
                return;
            }
        };

        if matches!(resolution, ConflictResolution::Cancel) {
            log::info!("📁 User chose to cancel");
            self.close_data_directory_modal();
            return;
        }

        self.settings.data_directory_form.set_loading(true);

        let child_id = self.get_current_child_from_backend().as_ref().map(|c| c.id.clone());
        let request = RelocateWithConflictResolutionRequest {
            child_id,
            new_path: self.settings.data_directory_form.new_path.clone(),
            resolution,
        };

        match self.backend().data_directory_service.relocate_with_conflict_resolution(request) {
            Ok(response) => {
                self.settings.data_directory_form.set_loading(false);
                
                if response.success {
                    log::info!("✅ Data directory conflict resolved successfully");
                    let message = if let Some(archived_to) = response.archived_to {
                        format!("{}\n\nArchived to: {}", response.message, archived_to)
                    } else {
                        response.message
                    };
                    self.settings.data_directory_form.set_success(message);
                    // Update current path
                    self.settings.data_directory_form.current_path = response.new_path;
                    // Clear conflict state
                    self.settings.data_directory_form.set_conflict(false, None);
                } else {
                    log::error!("🚨 Conflict resolution failed: {}", response.message);
                    self.settings.data_directory_form.set_error(response.message);
                }
            }
            Err(e) => {
                log::error!("🚨 Conflict resolution service error: {}", e);
                self.settings.data_directory_form.set_loading(false);
                self.settings.data_directory_form.set_error(format!("Operation failed: {}", e));
            }
        }
    }

    /// Return data to default location
    fn return_to_default_location(&mut self) {
        log::info!("📁 Returning data to default location");
        self.settings.data_directory_form.set_loading(true);

        let child_id = self.get_current_child_from_backend().as_ref().map(|c| c.id.clone());
        let request = ReturnToDefaultLocationRequest { child_id };

        match self.backend().data_directory_service.return_to_default_location(request) {
            Ok(response) => {
                self.settings.data_directory_form.set_loading(false);
                
                if response.success {
                    log::info!("✅ Successfully returned to default location");
                    self.settings.data_directory_form.set_success(response.message);
                    // Update current path and redirect status
                    self.settings.data_directory_form.current_path = response.default_path;
                    self.settings.data_directory_form.is_redirected = false;
                } else {
                    log::error!("🚨 Return to default failed: {}", response.message);
                    self.settings.data_directory_form.set_error(response.message);
                }
            }
            Err(e) => {
                log::error!("🚨 Return to default service error: {}", e);
                self.settings.data_directory_form.set_loading(false);
                self.settings.data_directory_form.set_error(format!("Failed to return to default location: {}", e));
            }
        }
    }

    /// Close data directory modal and reset form
    fn close_data_directory_modal(&mut self) {
        self.settings.show_data_directory_modal = false;
        self.settings.data_directory_form.clear();
        log::info!("📁 Data directory modal closed and form reset");
    }
} 