//! # Goal Creation Modal
//!
//! This module provides the goal creation modal interface, allowing users to set up
//! new savings goals with target amounts and descriptions.
//!
//! ## Key Functions:
//! - `render_goal_creation_modal()` - Main modal rendering
//! - Form validation and error handling
//! - Goal creation submission
//!
//! ## Purpose:
//! This modal provides a clean, kid-friendly interface for creating new goals
//! with proper validation and error handling.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::styling::colors;

impl AllowanceTrackerApp {
    /// Render the goal creation modal with simple opacity effects
    pub fn render_goal_creation_modal(&mut self, ctx: &egui::Context) {
        if !self.goal.show_creation_modal {
            return;
        }
        
        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("goal_creation_modal_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // EFFECT 1: Simple frosted glass background
                let screen_rect = ctx.screen_rect();
                
                // Darker overlay with slight blue tint for better frosted glass feel
                ui.painter().rect_filled(
                    screen_rect,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgba_unmultiplied(10, 20, 40, 150)
                );
                
                // Center the modal content
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(screen_rect), |ui| {
                    ui.centered_and_justified(|ui| {
                        // Simple frosted glass effect - just set opacity for the modal frame
                        ui.set_opacity(0.95); // Slight transparency for frosted effect
                        
                        egui::Frame::window(&ui.style())
                            .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 230)) // Slightly transparent white
                            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgba_unmultiplied(100, 150, 255, 180)))
                            .corner_radius(egui::CornerRadius::same(15))
                            .inner_margin(egui::Margin::same(20))
                            .show(ui, |ui| {
                                // Reset opacity for content
                                ui.set_opacity(1.0);
                                
                                // Set modal size
                                ui.set_min_size(egui::vec2(450.0, 350.0));
                                ui.set_max_size(egui::vec2(450.0, 350.0));
                                
                                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    
                    // Modal header
                    ui.label(egui::RichText::new("🎯 Create New Goal")
                        .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_PRIMARY)
                        .strong());
                    
                    ui.add_space(20.0);
                    
                    // Description input
                    ui.label(egui::RichText::new("What do you plan to buy?")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_PRIMARY));
                    
                    ui.add_space(5.0);
                    
                    let description_response = ui.add(
                        egui::TextEdit::singleline(&mut self.goal.creation_form.description)
                            .hint_text("e.g., New bike, Video game, Toy...")
                            .desired_width(ui.available_width() - 20.0)
                    );
                    
                    // Description error
                    if let Some(error) = &self.goal.creation_form.description_error {
                        ui.label(egui::RichText::new(format!("❌ {}", error))
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::RED));
                    }
                    
                    ui.add_space(15.0);
                    
                    // Amount input
                    ui.label(egui::RichText::new("How much do you plan to save for?")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_PRIMARY));
                    
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("$");
                        let amount_response = ui.add(
                            egui::TextEdit::singleline(&mut self.goal.creation_form.target_amount_text)
                                .hint_text("25.00")
                                .desired_width(ui.available_width() - 40.0)
                        );
                        
                        // Validate amount on change
                        if amount_response.changed() {
                            self.goal.creation_form.validate();
                        }
                    });
                    
                    // Amount error
                    if let Some(error) = &self.goal.creation_form.amount_error {
                        ui.label(egui::RichText::new(format!("❌ {}", error))
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::RED));
                    }
                    
                    ui.add_space(15.0);
                    
                    // Submission error
                    if let Some(error) = &self.goal.creation_form.submission_error {
                        ui.label(egui::RichText::new(format!("❌ {}", error))
                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::RED));
                        ui.add_space(10.0);
                    }
                    
                    // Validate on description change
                    if description_response.changed() {
                        self.goal.creation_form.validate();
                    }
                    
                    ui.add_space(20.0);
                    
                    // EFFECT 2: Simple opacity effect on buttons
                    ui.horizontal(|ui| {
                        // Cancel button with subtle opacity
                        ui.set_opacity(0.85); // Subtle transparency to de-emphasize cancel
                        let cancel_button = egui::Button::new("Cancel")
                            .fill(egui::Color32::from_rgb(240, 240, 240))
                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)))
                            .corner_radius(egui::CornerRadius::same(6))
                            .min_size(egui::vec2(80.0, 35.0));
                        
                        if ui.add(cancel_button).clicked() {
                            self.goal.hide_creation_modal();
                        }
                        
                        ui.add_space(10.0);
                        
                        // Create button with full opacity for emphasis
                        ui.set_opacity(1.0);
                        let create_enabled = self.goal.creation_form.can_submit();
                        let create_button = egui::Button::new(
                                if self.goal.creation_form.submitting { 
                                    "Creating..." 
                                } else { 
                                    "Create Goal" 
                                }
                            )
                            .fill(if create_enabled { 
                                egui::Color32::from_rgb(100, 150, 255) 
                            } else { 
                                egui::Color32::from_rgb(200, 200, 200) 
                            })
                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 130, 235)))
                            .corner_radius(egui::CornerRadius::same(6))
                            .min_size(egui::vec2(100.0, 35.0));
                        
                        let create_response = ui.add_enabled(create_enabled && !self.goal.creation_form.submitting, create_button);
                        
                        if create_response.clicked() && self.goal.creation_form.validate() {
                            self.goal.creation_form.start_submission();
                            
                            // Get validated values
                            let description = self.goal.creation_form.description.trim().to_string();
                            let target_amount = self.goal.creation_form.target_amount.unwrap_or(0.0);
                            
                            // Create the goal
                            self.create_goal(description, target_amount);
                        }
                    });
                    
                    ui.add_space(10.0);
                                });
                            });
                    });
                });
            });
    }
} 