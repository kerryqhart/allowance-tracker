//! # Money Transaction Modal
//!
//! This module contains the generic money transaction modal functionality.
//!
//! ## Responsibilities:
//! - Generic modal for income and expense transactions
//! - Form validation and user input handling
//! - Visual feedback and error display
//! - Configuration-based modal rendering
//!
//! ## Purpose:
//! This modal provides a reusable interface for both adding money (income)
//! and spending money (expense) transactions with consistent validation and UX.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    /// Generic money transaction modal renderer (income/expense)
    /// Returns true if the form was submitted successfully
    pub fn render_money_transaction_modal(
        &mut self,
        ctx: &egui::Context,
        config: &crate::ui::app_state::MoneyTransactionModalConfig,
        form_state: &mut crate::ui::app_state::MoneyTransactionFormState,
        is_visible: bool,
        _overlay_type: crate::ui::app_state::OverlayType,
    ) -> bool {
        if !is_visible {
            return false;
        }

        let mut form_submitted = false;

        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("money_transaction_modal_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Dark semi-transparent background
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    egui::Rounding::ZERO,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128)
                );
                
                // Center the modal content
                ui.allocate_ui_at_rect(screen_rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        egui::Frame::window(&ui.style())
                            .fill(egui::Color32::WHITE)
                            .stroke(egui::Stroke::new(3.0, config.color))
                            .rounding(egui::Rounding::same(15.0))
                            .inner_margin(egui::Margin::same(20.0))
                            .show(ui, |ui| {
                                // Set modal size
                                ui.set_min_size(egui::vec2(450.0, 350.0));
                                ui.set_max_size(egui::vec2(450.0, 350.0));
                                
                                ui.vertical_centered(|ui| {
                                    ui.add_space(15.0);
                                    
                                    // Title with icon
                                    ui.label(egui::RichText::new(format!("{} {}", config.icon, config.title))
                                         .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                                         .strong()
                                         .color(config.color));
                                    
                                    ui.add_space(15.0);
                                    
                                    // Hint text
                                    ui.label(egui::RichText::new(config.hint_text)
                                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                        .color(egui::Color32::from_rgb(80, 80, 80)));
                                    
                                    ui.add_space(20.0);
                                    
                                    // Description field with validation
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("Description:")
                                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(60, 60, 60)));
                                        
                                        // Character count
                                        let char_count = form_state.description.len();
                                        let count_color = if char_count > config.max_description_length { 
                                            egui::Color32::from_rgb(220, 50, 50) // Red if over limit
                                        } else if char_count > (config.max_description_length * 4 / 5) { 
                                            egui::Color32::from_rgb(255, 140, 0) // Orange if approaching limit (80%)
                                        } else { 
                                            egui::Color32::from_rgb(120, 120, 120) // Gray for normal
                                        };
                                        
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(egui::RichText::new(format!("{}/{}", char_count, config.max_description_length))
                                                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                                .color(count_color));
                                        });
                                    });
                                    ui.add_space(5.0);
                                    
                                    // Description field
                                    let description_response = ui.add(
                                        egui::TextEdit::singleline(&mut form_state.description)
                                            .hint_text(config.description_placeholder)
                                            .desired_width(400.0)
                                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                    );
                                    
                                    // Show description error message
                                    if let Some(error) = &form_state.description_error {
                                        ui.add_space(3.0);
                                        ui.label(egui::RichText::new(error)
                                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(220, 50, 50)));
                                    }
                                    
                                    ui.add_space(15.0);
                                    
                                    // Amount field with validation
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("Amount:")
                                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(60, 60, 60)));
                                    });
                                    ui.add_space(5.0);
                                    
                                    // Amount input with static dollar sign
                                    ui.horizontal(|ui| {
                                        // Static dollar sign
                                        ui.label(egui::RichText::new("$")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(60, 60, 60)));
                                        
                                        ui.add_space(2.0);
                                        
                                        // Amount field
                                        let amount_response = ui.add(
                                            egui::TextEdit::singleline(&mut form_state.amount)
                                                .hint_text(config.amount_placeholder)
                                                .desired_width(120.0)
                                                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                        );
                                        
                                        // Validate form whenever fields change
                                        if description_response.changed() || amount_response.changed() {
                                            self.validate_money_transaction_form(form_state, config);
                                        }
                                    });
                                    
                                    // Show amount error message
                                    if let Some(error) = &form_state.amount_error {
                                        ui.add_space(3.0);
                                        ui.label(egui::RichText::new(error)
                                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(220, 50, 50)));
                                    }
                                    
                                    ui.add_space(30.0);
                                    
                                    // Buttons
                                    ui.horizontal(|ui| {
                                        ui.add_space(50.0);
                                        
                                        // Submit button
                                        let button_enabled = form_state.is_valid && 
                                            !form_state.description.trim().is_empty() && 
                                            !form_state.amount.trim().is_empty();
                                        
                                        let button_color = if button_enabled {
                                            config.color
                                        } else {
                                            egui::Color32::from_rgb(180, 180, 180) // Gray when disabled
                                        };
                                        
                                        let submit_button = egui::Button::new(egui::RichText::new(config.button_text)
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::WHITE))
                                            .fill(button_color)
                                            .stroke(egui::Stroke::new(2.0, button_color))
                                            .rounding(egui::Rounding::same(10.0))
                                            .min_size(egui::vec2(150.0, 40.0));
                                        
                                        let submit_response = ui.add(submit_button);
                                        
                                        if submit_response.clicked() && button_enabled {
                                            form_submitted = true;
                                        }
                                        
                                        // Show tooltip for disabled button
                                        if !button_enabled && submit_response.hovered() {
                                            submit_response.on_hover_text("Please fix the errors above to continue");
                                        }
                                        
                                        ui.add_space(30.0);
                                        
                                        // Cancel button
                                        let cancel_button = egui::Button::new(egui::RichText::new("Cancel")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::WHITE))
                                            .fill(egui::Color32::from_rgb(120, 120, 120))
                                            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(120, 120, 120)))
                                            .rounding(egui::Rounding::same(10.0))
                                            .min_size(egui::vec2(100.0, 40.0));
                                        
                                        if ui.add(cancel_button).clicked() {
                                            // Clear form and close modal
                                            form_state.clear();
                                            self.calendar.selected_day = None;
                                            
                                            // TEMPORARY: Sync compatibility field
                                            // self.selected_day = None; // Removed
                                        }
                                    });
                                    
                                    ui.add_space(15.0);
                                });
                            });
                    });
                });
                
                // Handle modal backdrop click to close
                if is_visible {
                    // Only detect backdrop clicks after the modal has been open for at least one frame
                    // This prevents the modal from immediately closing when it's opened by a button click
                    if !self.calendar.modal_just_opened && ui.ctx().input(|i| i.pointer.any_click()) {
                        // Check if the click was outside the modal area
                        if let Some(pointer_pos) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                            let modal_rect = egui::Rect::from_center_size(
                                ui.ctx().screen_rect().center(),
                                egui::vec2(450.0, 350.0)
                            );
                            
                            if !modal_rect.contains(pointer_pos) {
                                // Click was outside modal - close it
                                form_state.clear();
                                self.calendar.active_overlay = None;
                                self.calendar.selected_day = None;
                                
                                // TEMPORARY: Sync compatibility fields
                                // self.active_overlay = None; // Removed
                                // self.selected_day = None; // Removed
                            }
                        }
                    }
                    
                    // Reset the modal_just_opened flag after the first frame
                    if self.calendar.modal_just_opened {
                        self.calendar.modal_just_opened = false;
                    }
                }
            });
            
        form_submitted
    }
} 