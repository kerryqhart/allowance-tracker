//! # Allowance Configuration Modal
//!
//! This module contains the allowance configuration modal functionality.
//!
//! ## Responsibilities:
//! - Display allowance configuration form with amount and day of week fields
//! - Handle form validation and change detection
//! - Integrate with backend AllowanceService API
//! - Provide success feedback after configuration
//!
//! ## Purpose:
//! This modal provides an interface for configuring allowance settings
//! with proper validation, change detection, and backend integration.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::settings::shared::{
    SettingsModalStyle, render_form_field_with_error
};
use crate::backend::domain::commands::allowance::{GetAllowanceConfigCommand, UpdateAllowanceConfigCommand};

impl AllowanceTrackerApp {
    /// Render the allowance configuration modal
    pub fn render_allowance_config_modal(&mut self, ctx: &egui::Context) {
        // Only log when modal state changes, not every frame
        // log::info!("🔍 MODAL_CHECK: allowance config modal called, show_modal={}", self.settings.show_allowance_config_modal);
        if !self.settings.show_allowance_config_modal {
            return;
        }

        let _current_has_changes = self.settings.allowance_config_form.has_changes();
        // Only log modal render when has_changes state changes to reduce noise
        // log::info!("⚙️ MODAL_RENDER: amount='{}', day={} ({}), has_changes={}", 
        //     self.settings.allowance_config_form.amount,
        //     self.settings.allowance_config_form.day_of_week,
        //     self.settings.allowance_config_form.day_name(),
        //     current_has_changes);

        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("allowance_config_modal_overlay"))
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
                                ui.set_min_size(style.modal_size);
                                ui.set_max_size(style.modal_size);

                                ui.vertical_centered(|ui| {
                                    ui.add_space(15.0);

                                    // Title
                                    ui.label(egui::RichText::new("⚙️ Configure Allowance")
                                        .font(egui::FontId::new(style.title_font_size, egui::FontFamily::Proportional))
                                        .strong()
                                        .color(style.title_color));

                                    ui.add_space(20.0);

                                    // Success message if present
                                    if let Some(success_msg) = &self.settings.allowance_config_form.success_message {
                                        ui.label(egui::RichText::new(format!("✅ {}", success_msg))
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(0, 150, 0)));
                                        
                                        ui.add_space(20.0);

                                        // Close button for success state
                                        if ui.add_sized([120.0, 32.0], 
                                            egui::Button::new(egui::RichText::new("Close")
                                                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                                .strong())
                                                .fill(egui::Color32::from_rgb(70, 130, 180))
                                        ).clicked() {
                                            self.close_allowance_config_modal();
                                        }
                                    } else {
                                        // Normal form state - subtitle/instructions
                                        ui.label(egui::RichText::new("Set up weekly allowance for your child")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(100, 100, 100)));

                                        ui.add_space(25.0);

                                        // Form content
                                        self.render_allowance_config_form_content(ui);

                                        ui.add_space(25.0);

                                        // Action buttons
                                        self.render_allowance_config_action_buttons(ui);
                                    }

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
                            egui::vec2(450.0, 400.0)
                        );
                        
                        if !modal_rect.contains(pointer_pos) {
                            log::info!("⚙️ Allowance config modal closed via backdrop click");
                            self.close_allowance_config_modal();
                        }
                    }
                }
            });
    }

    /// Render the form content for allowance configuration modal
    fn render_allowance_config_form_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Amount field
            let amount_response = render_form_field_with_error(
                ui,
                "Weekly Allowance Amount",
                &mut self.settings.allowance_config_form.amount,
                "Enter dollar amount (e.g., 5.00)",
                &self.settings.allowance_config_form.amount_error,
                Some(20), // Reasonable limit for allowance amounts
            );

            // Always validate amount (not just on change) to ensure change detection works
            // Only log amount input when it actually changes to reduce noise
            if amount_response.changed() {
                log::info!("⚙️ AMOUNT_INPUT_STATE: value = '{}', changed = {}", 
                    self.settings.allowance_config_form.amount, amount_response.changed());
                log::info!("🔄 EGUI_DETECTED_CHANGE: Input field actually changed to '{}'", 
                    self.settings.allowance_config_form.amount);
            }
            
            self.validate_allowance_config_form_field("amount");

            ui.add_space(15.0);

            // Day of week field
            ui.label(egui::RichText::new("Day of Week")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));

            ui.add_space(5.0);

            // Day dropdown with proper styling
            let combo_response = egui::ComboBox::from_label("")
                .width(200.0)
                .selected_text(self.settings.allowance_config_form.day_name())
                .show_ui(ui, |ui| {
                    // Style the dropdown content with solid background
                    ui.style_mut().visuals.extreme_bg_color = egui::Color32::WHITE;
                    ui.style_mut().visuals.faint_bg_color = egui::Color32::from_rgb(248, 248, 248);
                    ui.style_mut().visuals.widgets.noninteractive.bg_fill = egui::Color32::WHITE;
                    ui.style_mut().visuals.widgets.inactive.bg_fill = egui::Color32::WHITE;
                    
                    let mut changed = false;
                    let original_day = self.settings.allowance_config_form.day_of_week;
                    
                    changed |= ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 0, "Sunday").changed();
                    changed |= ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 1, "Monday").changed();
                    changed |= ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 2, "Tuesday").changed();
                    changed |= ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 3, "Wednesday").changed();
                    changed |= ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 4, "Thursday").changed();
                    changed |= ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 5, "Friday").changed();
                    changed |= ui.selectable_value(&mut self.settings.allowance_config_form.day_of_week, 6, "Saturday").changed();
                    
                    if changed {
                        log::info!("⚙️ Day of week changed from {} to {} ({})", 
                            original_day, 
                            self.settings.allowance_config_form.day_of_week,
                            self.settings.allowance_config_form.day_name());
                    }
                    
                    changed
                });
            
            // Log if the combo box interaction caused any issues
            if let Some(inner_response) = combo_response.inner {
                if inner_response {
                    log::info!("⚙️ ComboBox day selection changed successfully");
                }
            }

            ui.add_space(10.0);

            // Help text
            ui.label(egui::RichText::new("💡 Allowance will be automatically added every week on this day")
                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));
        });
    }

    /// Render action buttons for allowance configuration modal
    fn render_allowance_config_action_buttons(&mut self, ui: &mut egui::Ui) {
        let form_valid = self.settings.allowance_config_form.is_valid && 
                        !self.settings.allowance_config_form.amount.trim().is_empty();
        let has_changes = self.settings.allowance_config_form.has_changes();
        let is_saving = self.settings.allowance_config_form.is_saving;
        let button_enabled = form_valid && has_changes && !is_saving;
        
        // Only log button state when it changes from previous state (reduced noise)
        // log::info!("⚙️ BUTTON_STATE: form_valid={}, has_changes={}, is_saving={}, button_enabled={}", 
        //     form_valid, has_changes, is_saving, button_enabled);

        // Use mutable borrow flags to handle closure conflicts
        let mut should_submit = false;
        let mut should_cancel = false;

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Cancel button
                if ui.button(egui::RichText::new("Cancel")
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional)))
                    .clicked() 
                {
                    should_cancel = true;
                }

                ui.add_space(10.0);

                // Primary action button
                let button_text = if is_saving {
                    "⏳ Updating..."
                } else {
                    "Update Allowance"
                };

                let button_color = if button_enabled {
                    egui::Color32::from_rgb(70, 130, 180) // Steel blue
                } else {
                    egui::Color32::LIGHT_GRAY
                };

                let button = egui::Button::new(egui::RichText::new(button_text)
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                    .strong())
                    .fill(button_color);

                let button_response = ui.add_enabled(button_enabled, button);
                
                // Only log button interactions when something actually happens
                if button_response.clicked() {
                    log::info!("🔥 UPDATE BUTTON CLICKED! enabled={}, hovered={}, clicked={}", 
                        button_enabled, button_response.hovered(), button_response.clicked());
                    should_submit = true;
                }
            });
        });

        // Show help text if no changes
        if form_valid && !has_changes {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("ℹ️ No changes to save")
                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));
        }

        // Handle actions outside the UI closure to avoid borrowing conflicts
        // Only log action flags when there are actual actions (reduced noise)
        if should_submit || should_cancel {
            log::info!("🔍 ACTION_FLAGS: should_submit={}, should_cancel={}", should_submit, should_cancel);
        }
        
        if should_submit {
            log::info!("🚀 CALLING submit_allowance_config_form()");
            self.submit_allowance_config_form();
        }
        if should_cancel {
            log::info!("⚙️ Allowance config modal cancelled");
            self.close_allowance_config_modal();
        }
    }

    /// Load allowance configuration when modal opens
    pub fn load_allowance_config_for_modal(&mut self) {
        let child_from_backend = self.get_current_child_from_backend();
        let child_from_cache = self.get_current_child_from_backend();
        
        // 🔍 SURGICAL DEBUG: Compare UI cache vs backend for current child
        log::info!("🔍 MODAL_LOAD_DEBUG: Backend child: {:?}", 
            child_from_backend.as_ref().map(|c| (&c.id, &c.name)));
        log::info!("🔍 MODAL_LOAD_DEBUG: UI Cache child: {:?}", 
            child_from_cache.as_ref().map(|c| (&c.id, &c.name)));
        
        let child_id = child_from_backend.as_ref().map(|c| c.id.clone());
        log::info!("🔍 MODAL_LOAD_DEBUG: Using child_id for GetAllowanceConfigCommand: {:?}", child_id);
        
        let command = GetAllowanceConfigCommand { child_id };
        
        match self.backend().allowance_service.get_allowance_config(command) {
            Ok(result) => {
                log::info!("🔍 MODAL_LOAD_DEBUG: Backend returned config: amount={:?}, day={:?}", 
                    result.allowance_config.as_ref().map(|c| c.amount),
                    result.allowance_config.as_ref().map(|c| c.day_of_week));
                
                if let Some(config) = result.allowance_config {
                    log::info!("✅ Loaded existing allowance config: ${:.2} on {}", config.amount, config.day_name());
                    self.settings.allowance_config_form.load_from_config(&config);
                } else {
                    log::info!("ℹ️ No existing allowance config found, using defaults");
                    self.settings.allowance_config_form.clear(); // Reset to defaults
                }
            }
            Err(e) => {
                log::error!("❌ Failed to load allowance config: {}", e);
                self.settings.allowance_config_form.error_message = Some(format!("Failed to load config: {}", e));
            }
        }
    }

    /// Validate specific allowance config form field
    pub fn validate_allowance_config_form_field(&mut self, field_name: &str) {
        match field_name {
            "amount" => {
                let amount_text = self.settings.allowance_config_form.amount.trim();
                if amount_text.is_empty() {
                    self.settings.allowance_config_form.amount_error = None; // Let grayed button be sufficient
                } else {
                    match amount_text.parse::<f64>() {
                        Ok(amount) => {
                            if amount <= 0.0 {
                                self.settings.allowance_config_form.amount_error = Some("Amount must be positive".to_string());
                            } else if amount > 1000.0 {
                                self.settings.allowance_config_form.amount_error = Some("Amount too large (max $1,000)".to_string());
                            } else if amount < 0.01 {
                                self.settings.allowance_config_form.amount_error = Some("Amount too small (min $0.01)".to_string());
                            } else {
                                self.settings.allowance_config_form.amount_error = None;
                            }
                        }
                        Err(_) => {
                            self.settings.allowance_config_form.amount_error = Some("Please enter a valid number".to_string());
                        }
                    }
                }
                
                // Update overall validation state
                self.settings.allowance_config_form.is_valid = self.settings.allowance_config_form.amount_error.is_none();
            }
            _ => {
                log::warn!("⚠️ Unknown field for allowance config validation: {}", field_name);
            }
        }
    }

    /// Submit allowance configuration form
    pub fn submit_allowance_config_form(&mut self) {
        log::info!("⚙️ Submitting allowance config form");
        
        // Validate form first
        self.validate_allowance_config_form_field("amount");
        
        if !self.settings.allowance_config_form.is_valid {
            log::warn!("⚠️ Allowance config form validation failed");
            return;
        }

        // Parse amount
        let amount = match self.settings.allowance_config_form.amount.trim().parse::<f64>() {
            Ok(amt) => amt,
            Err(e) => {
                log::error!("❌ Failed to parse amount: {}", e);
                self.settings.allowance_config_form.error_message = Some("Invalid amount format".to_string());
                return;
            }
        };

        self.settings.allowance_config_form.is_saving = true;
        self.settings.allowance_config_form.error_message = None;

        let child_from_backend = self.get_current_child_from_backend();
        let child_from_cache = self.get_current_child_from_backend();
        
        // 🔍 SURGICAL DEBUG: Compare child IDs at submit time
        log::info!("🔍 SUBMIT_DEBUG: Backend child: {:?}", 
            child_from_backend.as_ref().map(|c| (&c.id, &c.name)));
        log::info!("🔍 SUBMIT_DEBUG: UI Cache child: {:?}", 
            child_from_cache.as_ref().map(|c| (&c.id, &c.name)));
        
        let child_id = child_from_backend.as_ref().map(|c| c.id.clone());
        log::info!("🔍 SUBMIT_DEBUG: Using child_id for UpdateAllowanceConfigCommand: {:?}", child_id);
        
        let command = UpdateAllowanceConfigCommand {
            child_id,
            amount,
            day_of_week: self.settings.allowance_config_form.day_of_week,
            is_active: true, // Always set to active when updating
        };

        match self.backend().allowance_service.update_allowance_config(command) {
            Ok(result) => {
                log::info!("✅ Allowance config updated successfully: {}", result.success_message);
                self.settings.allowance_config_form.is_saving = false;
                self.settings.allowance_config_form.success_message = Some(self.settings.allowance_config_form.get_success_message());
                self.settings.allowance_config_form.error_message = None;
                
                // Update original values for future change detection
                self.settings.allowance_config_form.original_amount = Some(amount);
                self.settings.allowance_config_form.original_day_of_week = Some(self.settings.allowance_config_form.day_of_week);
                self.settings.allowance_config_form.has_existing_config = true;
            }
            Err(e) => {
                log::error!("❌ Failed to update allowance config: {}", e);
                self.settings.allowance_config_form.is_saving = false;
                self.settings.allowance_config_form.error_message = Some(format!("Failed to update: {}", e));
            }
        }
    }

    /// Close allowance configuration modal
    pub fn close_allowance_config_modal(&mut self) {
        log::info!("⚙️ Closing allowance config modal");
        self.settings.show_allowance_config_modal = false;
        self.settings.allowance_config_form.clear();
    }
} 