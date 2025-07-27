//! # Export Data Modal
//!
//! This module contains the export data modal functionality.
//!
//! ## Responsibilities:
//! - Display export data form with default/custom location options
//! - Handle form validation and user input
//! - Integrate with backend ExportService API
//! - Provide visual feedback and error handling
//! - Show export progress and success confirmation
//!
//! ## Purpose:
//! This modal provides an intuitive interface for exporting transaction data
//! as CSV files with proper path validation and backend integration.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::settings::shared::{
    SettingsModalStyle, render_form_field_with_error
};
use crate::ui::components::settings::ExportType;
use shared::{ExportToPathRequest};

impl AllowanceTrackerApp {
    /// Render the export data modal
    pub fn render_export_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_export_modal {
            return;
        }

        log::info!("📄 Rendering export data modal");

        // Update preview on render to ensure it's current
        let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
        let child_name_ref = child_name.as_deref();
        self.settings.export_form.update_preview(child_name_ref);

        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("export_modal_overlay"))
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
                                // Set modal size - slightly larger for export content
                                ui.set_min_size(egui::vec2(500.0, 450.0));
                                ui.set_max_size(egui::vec2(500.0, 450.0));

                                ui.vertical_centered(|ui| {
                                    ui.add_space(15.0);

                                    // Title
                                    ui.label(egui::RichText::new("📄 Export Data")
                                        .font(egui::FontId::new(style.title_font_size, egui::FontFamily::Proportional))
                                        .strong()
                                        .color(style.title_color));

                                    ui.add_space(20.0);

                                    // Subtitle/instructions
                                    ui.label(egui::RichText::new("Export all transaction data as a CSV file")
                                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                        .color(egui::Color32::from_rgb(100, 100, 100)));

                                    ui.add_space(25.0);

                                    // Form content
                                    self.render_export_form_content(ui);

                                    ui.add_space(25.0);

                                    // Action buttons
                                    self.render_export_action_buttons(ui);

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
                            egui::vec2(500.0, 450.0)
                        );
                        
                        if !modal_rect.contains(pointer_pos) {
                            log::info!("📄 Export modal closed via backdrop click");
                            self.close_export_modal();
                        }
                    }
                }
            });
    }

    /// Render the form content for export modal
    fn render_export_form_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Export location options
            ui.label(egui::RichText::new("Export Location")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong());
            
            ui.add_space(10.0);

            // Default location option
            let default_selected = self.settings.export_form.export_type == ExportType::Default;
            if ui.radio(default_selected, "Default location (Documents folder)").clicked() {
                self.settings.export_form.export_type = ExportType::Default;
                self.settings.export_form.clear_messages();
                
                // Update preview
                let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
                let child_name_ref = child_name.as_deref();
                self.settings.export_form.update_preview(child_name_ref);
            }

            ui.add_space(8.0);

            // Custom location option
            let custom_selected = self.settings.export_form.export_type == ExportType::Custom;
            if ui.radio(custom_selected, "Custom location").clicked() {
                self.settings.export_form.export_type = ExportType::Custom;
                self.settings.export_form.clear_messages();
                
                // Update preview
                let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
                let child_name_ref = child_name.as_deref();
                self.settings.export_form.update_preview(child_name_ref);
            }

            ui.add_space(10.0);

            // Custom path input (only show if custom is selected)
            if self.settings.export_form.export_type == ExportType::Custom {
                // File selection section
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Select File Location:")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .strong());
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("📁 Browse...")
                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional)))
                            .clicked() 
                        {
                            self.open_file_browser();
                        }
                    });
                });

                ui.add_space(8.0);

                // Show selected file or fallback to manual entry
                if self.settings.export_form.selected_file_path.is_some() {
                    ui.label(egui::RichText::new("Selected file:")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100)));
                    
                    if let Some(ref path) = self.settings.export_form.selected_file_path {
                        ui.label(egui::RichText::new(path)
                            .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                            .color(egui::Color32::from_rgb(70, 130, 180)));
                    }

                    ui.add_space(5.0);

                    // Option to clear selection and use manual entry
                    if ui.small_button("Clear selection").clicked() {
                        self.settings.export_form.selected_file_path = None;
                        let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
                        let child_name_ref = child_name.as_deref();
                        self.settings.export_form.update_preview(child_name_ref);
                    }
                } else {
                    // Manual path input when no file is selected
                    ui.label(egui::RichText::new("Or enter a directory path manually:")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100)));
                    
                    ui.add_space(5.0);
                    
                    let path_response = render_form_field_with_error(
                        ui,
                        "Directory Path",
                        &mut self.settings.export_form.custom_path,
                        "Enter directory path (e.g., /Users/user/Desktop/exports)",
                        &None, // No error message for now - could add path validation later
                        None, // No character limit
                    );

                    // Update preview when path changes
                    if path_response.changed() {
                        let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
                        let child_name_ref = child_name.as_deref();
                        self.settings.export_form.update_preview(child_name_ref);
                    }

                    ui.add_space(5.0);

                    // Help text for custom path
                    ui.label(egui::RichText::new("💡 Tip: Use the Browse button for easier file selection")
                        .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(120, 120, 120)));
                }

                ui.add_space(15.0);
            }

            // Preview section
            ui.separator();
            ui.add_space(15.0);

            ui.label(egui::RichText::new("Preview")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong());

            ui.add_space(8.0);

            // Filename preview
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Filename:")
                    .color(egui::Color32::from_rgb(100, 100, 100)));
                ui.label(egui::RichText::new(&self.settings.export_form.preview_filename)
                    .font(egui::FontId::new(14.0, egui::FontFamily::Monospace))
                    .color(egui::Color32::from_rgb(70, 130, 180)));
            });

            ui.add_space(5.0);

            // Location preview
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Will be saved to:")
                    .color(egui::Color32::from_rgb(100, 100, 100)));
            });
            ui.label(egui::RichText::new(&self.settings.export_form.preview_location)
                .font(egui::FontId::new(13.0, egui::FontFamily::Monospace))
                .color(egui::Color32::from_rgb(70, 130, 180)));

            // Show messages if any
            if let Some(ref success_msg) = self.settings.export_form.success_message {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(format!("✅ {}", success_msg))
                    .color(egui::Color32::from_rgb(0, 150, 0)));
            }

            if let Some(ref error_msg) = self.settings.export_form.error_message {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(format!("❌ {}", error_msg))
                    .color(egui::Color32::from_rgb(200, 0, 0)));
            }
        });
    }

    /// Render action buttons for export modal
    fn render_export_action_buttons(&mut self, ui: &mut egui::Ui) {
        let form_ready = self.settings.export_form.is_ready_for_export();
        let is_exporting = self.settings.export_form.is_exporting;

        // Use mutable borrow flags to handle closure conflicts
        let mut should_export = false;
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
                let button_text = if is_exporting {
                    "⏳ Exporting..."
                } else {
                    "📄 Export Data"
                };

                let button = egui::Button::new(egui::RichText::new(button_text)
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                    .strong())
                    .fill(if form_ready && !is_exporting {
                        egui::Color32::from_rgb(70, 130, 180) // Steel blue
                    } else {
                        egui::Color32::LIGHT_GRAY
                    });

                if ui.add_enabled(form_ready && !is_exporting, button).clicked() {
                    should_export = true;
                }
            });
        });

        // Handle actions outside the UI closure to avoid borrowing conflicts
        if should_export {
            self.submit_export_form();
        }
        if should_cancel {
            log::info!("📄 Export modal cancelled");
            self.close_export_modal();
        }
    }

    /// Open native file browser to select export location
    fn open_file_browser(&mut self) {
        log::info!("📁 Opening native file browser for export location");

        // Generate default filename based on current child
        let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
        let child_name_formatted = child_name
            .as_deref()
            .unwrap_or("child")
            .replace(" ", "_")
            .to_lowercase();
        
        let now = chrono::Utc::now();
        let default_filename = format!(
            "{}_transactions_{}.csv",
            child_name_formatted,
            now.format("%Y%m%d")
        );

        // Open save file dialog
        let file_dialog = rfd::FileDialog::new()
            .set_title("Export Data As...")
            .set_file_name(&default_filename)
            .add_filter("CSV Files", &["csv"])
            .add_filter("All Files", &["*"]);

        // Set initial directory to Documents if available
        let dialog_with_dir = if let Some(docs_dir) = dirs::document_dir() {
            file_dialog.set_directory(docs_dir)
        } else {
            file_dialog
        };

        // Execute the dialog
        if let Some(path) = dialog_with_dir.save_file() {
            log::info!("📁 User selected export path: {:?}", path);
            self.settings.export_form.selected_file_path = Some(path.to_string_lossy().to_string());
            
            // Update preview with selected path
            let child_name_ref = child_name.as_deref();
            self.settings.export_form.update_preview(child_name_ref);
            
            // Clear any existing error messages
            self.settings.export_form.clear_messages();
        } else {
            log::info!("📁 User cancelled file selection");
        }
    }

    /// Submit the export form
    fn submit_export_form(&mut self) {
        log::info!("📄 Submitting export form");

        if !self.settings.export_form.is_ready_for_export() {
            log::warn!("📄 Export form not ready for submission");
            return;
        }

        // Set loading state
        self.settings.export_form.is_exporting = true;
        self.settings.export_form.clear_messages();

        // Prepare request - use the effective custom path which prioritizes file dialog selection
        let custom_path = self.settings.export_form.get_effective_custom_path();

        let request = ExportToPathRequest {
            child_id: self.get_current_child_from_backend().as_ref().map(|c| c.id.clone()),
            custom_path,
        };

        // Execute export command
        match self.backend().export_service.export_to_path(
            request,
            &self.backend().child_service,
            &self.backend().transaction_service,
        ) {
            Ok(response) => {
                self.settings.export_form.is_exporting = false;
                
                if response.success {
                    log::info!("✅ Export completed successfully: {}", response.file_path);
                    
                    let success_message = format!(
                        "Successfully exported {} transactions to:\n{}",
                        response.transaction_count,
                        response.file_path
                    );
                    
                    self.settings.export_form.set_success(success_message);
                    
                    // Close modal after a brief delay to show success message
                    // For now, just log - could add a timer-based close later
                    log::info!("📄 Export modal will remain open to show file location");
                } else {
                    log::error!("🚨 Export failed: {}", response.message);
                    self.settings.export_form.set_error(response.message);
                }
            }
            Err(e) => {
                log::error!("🚨 Export service error: {}", e);
                self.settings.export_form.is_exporting = false;
                self.settings.export_form.set_error(format!("Export failed: {}", e));
            }
        }
    }

    /// Close export modal and reset form
    fn close_export_modal(&mut self) {
        self.settings.show_export_modal = false;
        self.settings.export_form.clear();
        log::info!("📄 Export modal closed and form reset");
    }
} 