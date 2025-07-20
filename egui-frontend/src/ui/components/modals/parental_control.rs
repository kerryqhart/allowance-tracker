//! # Parental Control Modal
//!
//! This module contains the parental control authentication modal functionality.
//!
//! ## Responsibilities:
//! - Multi-stage parental authentication flow
//! - Handle user input and validation
//! - Provide visual feedback for different stages
//! - Manage authentication state transitions
//!
//! ## Purpose:
//! This modal provides a kid-friendly way to restrict access to sensitive features
//! like deleting transactions, requiring parent authentication to proceed.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    /// Render the parental control modal
    pub fn render_parental_control_modal(&mut self, ctx: &egui::Context) {
        if !self.modal.show_parental_control_modal {
            return;
        }
        
        log::info!("🔒 Rendering parental control modal - stage: {:?}", self.modal.parental_control_stage);
        
        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("parental_control_modal_overlay"))
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
                ui.allocate_ui_at_rect(
                    screen_rect,
                    |ui| {
                        ui.centered_and_justified(|ui| {
                            egui::Frame::window(&ui.style())
                                .fill(egui::Color32::WHITE)
                                .stroke(egui::Stroke::new(3.0, egui::Color32::from_rgb(220, 50, 50)))
                                .rounding(egui::Rounding::same(15.0))
                                .inner_margin(egui::Margin::same(20.0))
                                .show(ui, |ui| {
                                    // Set modal size
                                    ui.set_min_size(egui::vec2(450.0, 350.0));
                                    ui.set_max_size(egui::vec2(450.0, 350.0));
                                    
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(15.0);
                                        
                                        // Title
                                        ui.label(egui::RichText::new("🔒 Parental Control")
                                             .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                                             .strong()
                                             .color(egui::Color32::from_rgb(220, 50, 50)));
                                        
                                        ui.add_space(15.0);
                                        
                                        // Content based on current stage
                                        match self.modal.parental_control_stage {
                                            crate::ui::app_state::ParentalControlStage::Question1 => self.render_question1(ui),
                                            crate::ui::app_state::ParentalControlStage::Question2 => self.render_question2(ui),
                                            crate::ui::app_state::ParentalControlStage::Authenticated => self.render_success(ui),
                                        }
                                    });
                                })
                        });
                    })
            });
    }
    
    /// Render the actual modal content
    fn render_parental_control_modal_content(&mut self, ui: &mut egui::Ui) {
        // Modal card background - compact and constrained
        let modal_size = egui::vec2(300.0, 160.0);
        
        egui::Frame::window(&ui.style())
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(126, 120, 229))) // Purple border
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                // Constrain the modal to exact size
                ui.set_min_size(modal_size);
                ui.set_max_size(modal_size);
                
                match self.modal.parental_control_stage {
                    crate::ui::app_state::ParentalControlStage::Question1 => self.render_question1(ui),
                    crate::ui::app_state::ParentalControlStage::Question2 => self.render_question2(ui),
                    crate::ui::app_state::ParentalControlStage::Authenticated => self.render_success(ui),
                }
            });
    }
    
    /// Render first question: "Are you Mom or Dad?"
    fn render_question1(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(5.0);
            
            // Lock icon - smaller
            ui.label(egui::RichText::new("🔒")
                .font(egui::FontId::new(24.0, egui::FontFamily::Proportional)));
            
            ui.add_space(8.0);
            
            // Question - smaller font
            ui.label(egui::RichText::new("Settings Access")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(8.0);
            
            ui.label(egui::RichText::new("Are you Mom or Dad?")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(80, 80, 80)));
            
            ui.add_space(12.0);
            
            // Buttons - properly centered
            ui.horizontal_centered(|ui| {
                // Calculate total width needed: button + space + button
                let total_width = 70.0 + 12.0 + 70.0; // 152px total
                let available_width = ui.available_width();
                let offset = (available_width - total_width) / 2.0;
                
                if offset > 0.0 {
                    ui.add_space(offset);
                }
                
                // Yes button
                let yes_button = egui::Button::new(
                    egui::RichText::new("Yes")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::BLACK)
                )
                .min_size(egui::vec2(70.0, 32.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(147, 51, 234))); // Purple outline
                
                if ui.add(yes_button).clicked() {
                    self.advance_to_question_2();
                }
                
                ui.add_space(12.0);
                
                // No button
                let no_button = egui::Button::new(
                    egui::RichText::new("No")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::BLACK)
                )
                .min_size(egui::vec2(70.0, 32.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(147, 51, 234))); // Purple outline
                
                if ui.add(no_button).clicked() {
                    self.cancel_parental_control_challenge();
                }
            });
            
            ui.add_space(10.0);
        });
    }
    
    /// Render second question: "What's cooler than cool?"
    fn render_question2(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(5.0);
            
            // Question mark icon - smaller
            ui.label(egui::RichText::new("❄️")
                .font(egui::FontId::new(24.0, egui::FontFamily::Proportional)));
            
            ui.add_space(8.0);
            
            // Challenge question - smaller fonts
            ui.label(egui::RichText::new("Oh yeah?? If so...")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(4.0);
            
            ui.label(egui::RichText::new("What's cooler than cool?")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(80, 80, 80)));
            
            ui.add_space(10.0);
            
            // Text input - more compact
            let text_input = egui::TextEdit::singleline(&mut self.modal.parental_control_input)
                .desired_width(300.0)
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional));
            
            let input_response = ui.add(text_input);
            
            // Auto-focus the input field when modal opens or when input is empty
            if input_response.gained_focus() || self.modal.parental_control_input.is_empty() {
                input_response.request_focus();
            }
            
            ui.add_space(10.0);
            
            // Show error message if any
            if let Some(error) = &self.modal.parental_control_error {
                ui.label(egui::RichText::new(error)
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(220, 50, 50)));
                ui.add_space(5.0);
            }
            
            ui.add_space(20.0);
            
            // Buttons
            ui.horizontal(|ui| {
                ui.add_space(80.0);
                
                // Submit button with loading state
                let submit_text = if self.modal.parental_control_loading {
                    "Checking..."
                } else {
                    "Submit"
                };
                
                let submit_button = egui::Button::new(egui::RichText::new(submit_text)
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(if self.modal.parental_control_loading {
                            egui::Color32::from_rgb(120, 120, 120)
                        } else {
                            egui::Color32::WHITE
                        }))
                .fill(egui::Color32::from_rgb(220, 50, 50))
                .stroke(egui::Stroke::new(1.5, if self.modal.parental_control_loading {
                    egui::Color32::from_rgb(120, 120, 120)
                } else {
                    egui::Color32::from_rgb(220, 50, 50)
                }))
                .rounding(egui::Rounding::same(10.0))
                .min_size(egui::vec2(120.0, 40.0));
                
                if ui.add(submit_button).clicked() && !self.modal.parental_control_loading {
                    self.submit_parental_control_answer();
                }
                
                ui.add_space(12.0);
                
                // Cancel button
                let cancel_button = egui::Button::new(
                    egui::RichText::new("Cancel")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::BLACK)
                )
                .min_size(egui::vec2(70.0, 32.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(147, 51, 234))); // Purple outline
                
                if ui.add(cancel_button).clicked() {
                    self.cancel_parental_control_challenge();
                }
            });
            
            ui.add_space(10.0);
        });
    }
    
    /// Render success state (brief display before closing)
    fn render_success(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            
            // Success icon
            ui.label(egui::RichText::new("✅")
                .font(egui::FontId::new(32.0, egui::FontFamily::Proportional)));
            
            ui.add_space(15.0);
            
            ui.label(egui::RichText::new("Access Granted!")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(34, 139, 34))); // Green
            
            ui.add_space(30.0);
        });
    }
} 