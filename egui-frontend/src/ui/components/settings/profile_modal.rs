//! # Profile Modal (Settings)
//!
//! This module contains the child profile modal functionality, moved from modals/profile.rs
//! to the settings submodule for better organization.
//!
//! ## Responsibilities:
//! - Display and edit child profile information
//! - Form validation for name and birthdate
//! - Age calculation and display
//! - Backend integration for profile updates
//!
//! ## Purpose:
//! This modal provides a kid-friendly interface for viewing and editing
//! child profile details with proper validation and parental control protection.
//! It's now part of the centralized settings system.

use eframe::egui;
use chrono::{NaiveDate, Datelike, Local};
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    /// Render the profile modal
    pub fn render_profile_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_profile_modal {
            return;
        }
        
        log::info!("👤 Rendering profile modal");
        
        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("profile_modal_overlay"))
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
                        egui::Frame::window(&ui.style())
                            .fill(egui::Color32::WHITE)
                            .stroke(egui::Stroke::new(3.0, egui::Color32::from_rgb(70, 130, 180))) // Steel blue for profile
                            .rounding(egui::CornerRadius::same(15))
                            .inner_margin(egui::Margin::same(25))
                            .shadow(egui::Shadow {
                                offset: [6, 6],
                                blur: 20,
                                spread: 0,
                                color: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100),
                            })
                            .show(ui, |ui| {
                                // Set modal size
                                ui.set_min_size(egui::vec2(450.0, 450.0));
                                ui.set_max_size(egui::vec2(450.0, 450.0));
                                
                                ui.vertical_centered(|ui| {
                                    ui.add_space(15.0);
                                    
                                    // Title
                                    ui.label(egui::RichText::new("👤 Profile")
                                         .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                                         .strong()
                                         .color(egui::Color32::from_rgb(70, 130, 180)));
                                    
                                    ui.add_space(20.0);
                                    
                                    // Profile form content
                                    self.render_profile_form_content(ui);
                                    
                                    ui.add_space(25.0);
                                    
                                    // Action buttons
                                    self.render_profile_action_buttons(ui);
                                    
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
                            egui::vec2(450.0, 450.0)
                        );
                        
                        if !modal_rect.contains(pointer_pos) {
                            self.close_profile_modal();
                        }
                    }
                }
            });
    }
    
    /// Render the profile form content
    fn render_profile_form_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 40.0); // Add some padding
            
            // Name field
            ui.label(egui::RichText::new("Name")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(5.0);
            
            let name_response = ui.add(
                egui::TextEdit::singleline(&mut self.settings.profile_form.name)
                    .hint_text("Enter child's name")
                    .desired_width(ui.available_width())
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
            );
            
            // Name validation
            if name_response.changed() {
                self.validate_profile_name();
            }
            
            if let Some(error) = &self.settings.profile_form.name_error {
                ui.label(egui::RichText::new(format!("❌ {}", error))
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::RED));
            }
            
            ui.add_space(20.0);
            
            // Birthdate field
            ui.label(egui::RichText::new("Birthdate")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(5.0);
            
            let birthdate_response = ui.add(
                egui::TextEdit::singleline(&mut self.settings.profile_form.birthdate)
                    .hint_text("YYYY-MM-DD (e.g., 2015-03-15)")
                    .desired_width(ui.available_width())
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
            );
            
            // Birthdate validation
            if birthdate_response.changed() {
                self.validate_profile_birthdate();
            }
            
            if let Some(error) = &self.settings.profile_form.birthdate_error {
                ui.label(egui::RichText::new(format!("❌ {}", error))
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::RED));
            }
            
            // Show calculated age if birthdate is valid
            if self.settings.profile_form.birthdate_error.is_none() && !self.settings.profile_form.birthdate.trim().is_empty() {
                if let Ok(birthdate) = NaiveDate::parse_from_str(&self.settings.profile_form.birthdate, "%Y-%m-%d") {
                    let age = self.calculate_age(birthdate);
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(format!("Age: {} years old", age))
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100)));
                }
            }
            
            ui.add_space(20.0);
            
            // Account information (read-only)
            if let Some(child) = self.current_child() {
                ui.separator();
                ui.add_space(15.0);
                
                ui.label(egui::RichText::new("Account Information")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .strong()
                    .color(egui::Color32::from_rgb(80, 80, 80)));
                
                ui.add_space(10.0);
                
                // Account created
                let created_date = child.created_at.format("%B %d, %Y").to_string();
                ui.label(egui::RichText::new(format!("Account created: {}", created_date))
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
                
                // Last updated
                let updated_text = self.format_relative_time(child.updated_at);
                ui.label(egui::RichText::new(format!("Last updated: {}", updated_text))
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
            }
        });
    }
    
    /// Render action buttons for the profile modal
    fn render_profile_action_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Center the buttons
            let button_width = 100.0;
            let spacing = 20.0;
            let total_width = button_width * 2.0 + spacing;
            let available_width = ui.available_width();
            let offset = (available_width - total_width) / 2.0;
            
            if offset > 0.0 {
                ui.add_space(offset);
            }
            
            // Save button
            let save_enabled = self.profile_form_is_valid() && self.profile_form_has_changes() && !self.settings.profile_form.is_saving;
            let save_text = if self.settings.profile_form.is_saving {
                "Saving..."
            } else {
                "Save"
            };
            
            let save_button = egui::Button::new(egui::RichText::new(save_text)
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                    .color(if save_enabled {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(160, 160, 160)
                    }))
                .fill(if save_enabled {
                    egui::Color32::from_rgb(70, 130, 180)
                } else {
                    egui::Color32::from_rgb(200, 200, 200)
                })
                .stroke(egui::Stroke::new(1.5, if save_enabled {
                    egui::Color32::from_rgb(70, 130, 180)
                } else {
                    egui::Color32::from_rgb(180, 180, 180)
                }))
                .rounding(egui::CornerRadius::same(10))
                .min_size(egui::vec2(button_width, 40.0));
            
            if ui.add(save_button).clicked() && save_enabled {
                self.save_profile_changes();
            }
            
            ui.add_space(spacing);
            
            // Cancel button
            let cancel_button = egui::Button::new(egui::RichText::new("Cancel")
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(100, 100, 100)))
                .fill(egui::Color32::from_rgb(245, 245, 245))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)))
                .rounding(egui::CornerRadius::same(10))
                .min_size(egui::vec2(button_width, 40.0));
            
            if ui.add(cancel_button).clicked() {
                self.close_profile_modal();
            }
        });
    }
    
    /// Validate the profile name field
    fn validate_profile_name(&mut self) {
        let name = self.settings.profile_form.name.trim();
        
        if name.is_empty() {
            self.settings.profile_form.name_error = Some("Name cannot be empty".to_string());
        } else if name.len() > 100 {
            self.settings.profile_form.name_error = Some("Name cannot exceed 100 characters".to_string());
        } else {
            self.settings.profile_form.name_error = None;
        }
        
        self.update_profile_form_validity();
    }
    
    /// Validate the profile birthdate field
    fn validate_profile_birthdate(&mut self) {
        let birthdate = self.settings.profile_form.birthdate.trim();
        
        if birthdate.is_empty() {
            self.settings.profile_form.birthdate_error = Some("Birthdate cannot be empty".to_string());
        } else {
            match NaiveDate::parse_from_str(birthdate, "%Y-%m-%d") {
                Ok(date) => {
                    // Check if date is reasonable (between 1900 and current year + 10)
                    let current_year = Local::now().year();
                    if date.year() < 1900 || date.year() > current_year + 10 {
                        self.settings.profile_form.birthdate_error = Some("Please enter a valid birth year".to_string());
                    } else if date > Local::now().date_naive() {
                        self.settings.profile_form.birthdate_error = Some("Birthdate cannot be in the future".to_string());
                    } else {
                        self.settings.profile_form.birthdate_error = None;
                    }
                }
                Err(_) => {
                    self.settings.profile_form.birthdate_error = Some("Please use YYYY-MM-DD format".to_string());
                }
            }
        }
        
        self.update_profile_form_validity();
    }
    
    /// Update the overall form validity status
    fn update_profile_form_validity(&mut self) {
        self.settings.profile_form.is_valid = 
            self.settings.profile_form.name_error.is_none() &&
            self.settings.profile_form.birthdate_error.is_none() &&
            !self.settings.profile_form.name.trim().is_empty() &&
            !self.settings.profile_form.birthdate.trim().is_empty();
    }
    
    /// Check if the profile form is valid
    fn profile_form_is_valid(&self) -> bool {
        self.settings.profile_form.is_valid
    }
    
    /// Check if the profile form has changes compared to current child data
    fn profile_form_has_changes(&self) -> bool {
        if let Some(child) = self.current_child() {
            let name_changed = self.settings.profile_form.name.trim() != child.name;
            
            // Parse form birthdate to compare with child's NaiveDate
            let birthdate_changed = match NaiveDate::parse_from_str(&self.settings.profile_form.birthdate, "%Y-%m-%d") {
                Ok(form_date) => form_date != child.birthdate,
                Err(_) => true, // If form date is invalid, consider it changed
            };
            
            name_changed || birthdate_changed
        } else {
            false
        }
    }
    
    /// Calculate age from birthdate
    fn calculate_age(&self, birthdate: NaiveDate) -> i32 {
        let today = Local::now().date_naive();
        let mut age = today.year() - birthdate.year();
        
        // Adjust age if birthday hasn't occurred this year yet
        if today.month() < birthdate.month() || 
           (today.month() == birthdate.month() && today.day() < birthdate.day()) {
            age -= 1;
        }
        
        age
    }
    
    /// Format relative time (e.g., "2 days ago", "Yesterday", "Just now")
    fn format_relative_time(&self, datetime: chrono::DateTime<chrono::Utc>) -> String {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(datetime);
        
        if duration.num_days() > 7 {
            datetime.format("%B %d, %Y").to_string()
        } else if duration.num_days() > 1 {
            format!("{} days ago", duration.num_days())
        } else if duration.num_days() == 1 {
            "Yesterday".to_string()
        } else if duration.num_hours() > 1 {
            format!("{} hours ago", duration.num_hours())
        } else if duration.num_minutes() > 1 {
            format!("{} minutes ago", duration.num_minutes())
        } else {
            "Just now".to_string()
        }
    }
    
    /// Save profile changes to backend
    fn save_profile_changes(&mut self) {
        // Extract child ID before borrowing self mutably
        let child_id = if let Some(child) = self.current_child() {
            child.id.clone()
        } else {
            log::warn!("🚨 No active child found for profile save");
            self.ui.error_message = Some("No child selected".to_string());
            return;
        };
        
        log::info!("💾 Saving profile changes for child: {}", child_id);
        
        self.settings.profile_form.is_saving = true;
        
        // Create update command
        let command = crate::backend::domain::commands::child::UpdateChildCommand {
            child_id,
            name: Some(self.settings.profile_form.name.trim().to_string()),
            birthdate: Some(self.settings.profile_form.birthdate.clone()),
        };
        
        // Call backend service
        match self.backend().child_service.update_child(command) {
            Ok(result) => {
                log::info!("✅ Profile updated successfully");
                
                // Update current child data
                self.core.current_child = Some(crate::ui::mappers::to_dto(result.child));
                
                // Success feedback
                self.close_profile_modal();
                
                // Profile update success feedback removed
            }
            Err(error) => {
                log::error!("❌ Failed to update profile: {}", error);
                self.settings.profile_form.is_saving = false;
                self.ui.error_message = Some(format!("Failed to update profile: {}", error));
            }
        }
    }
    
    /// Close the profile modal and reset form
    fn close_profile_modal(&mut self) {
        log::info!("❌ Closing profile modal");
        self.settings.show_profile_modal = false;
        self.settings.profile_form.clear();
    }
} 