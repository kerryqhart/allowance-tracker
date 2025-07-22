//! # Create Child Modal
//!
//! This module contains the create child modal functionality.
//!
//! ## Responsibilities:
//! - Display create child form with name and birthdate fields
//! - Handle form validation and user input
//! - Integrate with backend CreateChildCommand API
//! - Provide visual feedback and error handling
//!
//! ## Purpose:
//! This modal provides a kid-friendly interface for creating new children
//! with proper validation and backend integration.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::settings::shared::{
    SettingsModalStyle, render_form_field_with_error
};
use crate::backend::domain::commands::child::CreateChildCommand;
use crate::backend::domain::commands::child::SetActiveChildCommand;

impl AllowanceTrackerApp {
    /// Render the create child modal
    pub fn render_create_child_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_create_child_modal {
            return;
        }

        log::info!("👶 Rendering create child modal");

        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("create_child_modal_overlay"))
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
                ui.allocate_ui_at_rect(screen_rect, |ui| {
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
                                    ui.label(egui::RichText::new("👶 Create Child")
                                        .font(egui::FontId::new(style.title_font_size, egui::FontFamily::Proportional))
                                        .strong()
                                        .color(style.title_color));

                                    ui.add_space(20.0);

                                    // Subtitle/instructions
                                    ui.label(egui::RichText::new("Add a new child to start tracking their allowance")
                                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                        .color(egui::Color32::from_rgb(100, 100, 100)));

                                    ui.add_space(25.0);

                                    // Form content
                                    self.render_create_child_form_content(ui);

                                    ui.add_space(25.0);

                                    // Action buttons
                                    self.render_create_child_action_buttons(ui);

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
                            log::info!("👶 Create child modal closed via backdrop click");
                            self.close_create_child_modal();
                        }
                    }
                }
            });
    }

    /// Render the form content for create child modal
    fn render_create_child_form_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Name field
            let name_response = render_form_field_with_error(
                ui,
                "Child's Name",
                &mut self.settings.create_child_form.name,
                "Enter the child's full name",
                &self.settings.create_child_form.name_error,
                Some(100), // Character limit
            );

            // Validate name on change
            if name_response.changed() {
                self.validate_create_child_form_field("name");
            }

            ui.add_space(15.0);

            // Birthdate field
            let birthdate_response = render_form_field_with_error(
                ui,
                "Birthdate",
                &mut self.settings.create_child_form.birthdate,
                "YYYY-MM-DD (e.g., 2015-03-20)",
                &self.settings.create_child_form.birthdate_error,
                Some(10), // YYYY-MM-DD is exactly 10 characters
            );

            // Validate birthdate on change
            if birthdate_response.changed() {
                self.validate_create_child_form_field("birthdate");
            }

            ui.add_space(10.0);

            // Help text for birthdate
            ui.label(egui::RichText::new("💡 Tip: Use format YYYY-MM-DD, like 2015-03-20 for March 20, 2015")
                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));
        });
    }

    /// Render action buttons for create child modal
    fn render_create_child_action_buttons(&mut self, ui: &mut egui::Ui) {
        let form_valid = self.settings.create_child_form.is_valid && 
                        !self.settings.create_child_form.name.trim().is_empty() &&
                        !self.settings.create_child_form.birthdate.trim().is_empty();
        let is_saving = self.settings.create_child_form.is_saving;

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
                    "⏳ Processing..."
                } else {
                    "Create Child"
                };

                let button = egui::Button::new(egui::RichText::new(button_text)
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                    .strong())
                    .fill(if form_valid && !is_saving {
                        egui::Color32::from_rgb(70, 130, 180) // Steel blue
                    } else {
                        egui::Color32::LIGHT_GRAY
                    });

                if ui.add_enabled(form_valid && !is_saving, button).clicked() {
                    should_submit = true;
                }
            });
        });

        // Handle actions outside the UI closure to avoid borrowing conflicts
        if should_submit {
            self.submit_create_child_form();
        }
        if should_cancel {
            log::info!("👶 Create child modal cancelled");
            self.close_create_child_modal();
        }
    }

    /// Validate specific field in create child form
    fn validate_create_child_form_field(&mut self, field: &str) {
        match field {
            "name" => {
                let trimmed_name = self.settings.create_child_form.name.trim();
                if trimmed_name.is_empty() {
                    self.settings.create_child_form.name_error = Some("Child name is required".to_string());
                } else if trimmed_name.len() > 100 {
                    self.settings.create_child_form.name_error = Some("Name cannot exceed 100 characters".to_string());
                } else {
                    self.settings.create_child_form.name_error = None;
                }
            }
            "birthdate" => {
                if self.settings.create_child_form.birthdate.trim().is_empty() {
                    self.settings.create_child_form.birthdate_error = Some("Birthdate is required".to_string());
                } else if !self.settings.create_child_form.is_valid_date_format(&self.settings.create_child_form.birthdate) {
                    self.settings.create_child_form.birthdate_error = Some("Use format YYYY-MM-DD (e.g., 2015-03-20)".to_string());
                } else {
                    self.settings.create_child_form.birthdate_error = None;
                }
            }
            _ => {}
        }

        // Update overall validity
        self.settings.create_child_form.is_valid = 
            self.settings.create_child_form.name_error.is_none() && 
            self.settings.create_child_form.birthdate_error.is_none();
    }

    /// Submit the create child form
    fn submit_create_child_form(&mut self) {
        log::info!("👶 Submitting create child form");

        // Full validation before submission
        self.settings.create_child_form.validate();

        if !self.settings.create_child_form.is_valid {
            log::warn!("👶 Create child form validation failed");
            return;
        }

        // Set loading state
        self.settings.create_child_form.is_saving = true;

        // Prepare command
        let command = CreateChildCommand {
            name: self.settings.create_child_form.name.trim().to_string(),
            birthdate: self.settings.create_child_form.birthdate.trim().to_string(),
        };

        // Execute create child command
        match self.backend().child_service.create_child(command) {
            Ok(result) => {
                log::info!("✅ Child created successfully: {}", result.child.name);
                
                // Clone child name and ID before moving
                let child_name = result.child.name.clone();
                let child_id = result.child.id.clone();
                
                // Set the new child as active
                let set_active_command = SetActiveChildCommand {
                    child_id: child_id.clone(),
                };

                match self.backend().child_service.set_active_child(set_active_command) {
                    Ok(_) => {
                        log::info!("✅ New child set as active: {}", child_name);
                        
                        // Update current child in core state 
                        let child_dto = crate::ui::mappers::to_dto(result.child);
                        self.core.current_child = Some(child_dto);
                        
                        // Refresh all data for new child (calendar, table, chart, goals, balance)
                        self.refresh_all_data_for_current_child();
                        
                        // Close modal and reset form
                        self.close_create_child_modal();
                    }
                    Err(e) => {
                        log::error!("🚨 Failed to set new child as active: {}", e);
                        self.settings.create_child_form.is_saving = false;
                        self.ui.error_message = Some("Child created but failed to set as active. Please select manually.".to_string());
                    }
                }
            }
            Err(e) => {
                log::error!("🚨 Failed to create child: {}", e);
                self.settings.create_child_form.is_saving = false;
                self.ui.error_message = Some(format!("Failed to create child: {}", e));
            }
        }
    }

    /// Close create child modal and reset form
    fn close_create_child_modal(&mut self) {
        self.settings.show_create_child_modal = false;
        self.settings.create_child_form.clear();
        log::info!("👶 Create child modal closed and form reset");
    }


} 