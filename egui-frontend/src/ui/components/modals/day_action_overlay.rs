//! # Day Action Overlay Modal
//!
//! This module contains the day action overlay functionality for calendar interactions.
//!
//! ## Responsibilities:
//! - Handle different overlay types (AddMoney, SpendMoney, CreateGoal)
//! - Coordinate with money transaction modal for income/expense
//! - Provide fallback overlay UI for goal creation
//! - Handle backdrop clicks and modal lifecycle
//!
//! ## Purpose:
//! This overlay provides contextual actions when users click on calendar days,
//! allowing them to add transactions or create goals for specific dates.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    /// Render day action overlay based on active overlay type
    pub fn render_day_action_overlay(&mut self, ctx: &egui::Context) {
        let overlay_type = match self.calendar.active_overlay {
            Some(overlay) => overlay,
            None => return, // No overlay to show
        };
        
        // Handle AddMoney with generic modal
        if overlay_type == crate::ui::app_state::OverlayType::AddMoney {
            let config = crate::ui::app_state::MoneyTransactionModalConfig::income_config();
            let mut form_state = self.form.income_form_state.clone();
            let form_submitted = self.render_money_transaction_modal(
                ctx,
                &config,
                &mut form_state,
                true,
                overlay_type,
            );
            
            // Update the form state back to the struct
            self.form.income_form_state = form_state;
            
            if form_submitted {
                // Submit to backend and handle response
                let success = self.submit_income_transaction();
                if success {
                    self.form.income_form_state.clear();
                    self.calendar.active_overlay = None;
                    self.calendar.selected_day = None;
                    
                    // TEMPORARY: Sync compatibility fields
                    // self.active_overlay = None; // Removed
                    // self.selected_day = None; // Removed
                }
                // Note: Error messages are handled in submit_income_transaction()
            }
            return;
        }
        
        // Handle SpendMoney with generic modal
        if overlay_type == crate::ui::app_state::OverlayType::SpendMoney {
            let config = crate::ui::app_state::MoneyTransactionModalConfig::expense_config();
            let mut form_state = self.form.expense_form_state.clone();
            let form_submitted = self.render_money_transaction_modal(
                ctx,
                &config,
                &mut form_state,
                true,
                overlay_type,
            );
            
            // Update the form state back to the struct
            self.form.expense_form_state = form_state;
            
            if form_submitted {
                // Submit to backend and handle response
                let success = self.submit_expense_transaction();
                if success {
                    self.form.expense_form_state.clear();
                    self.calendar.active_overlay = None;
                    self.calendar.selected_day = None;
                    
                    // TEMPORARY: Sync compatibility fields
                    // self.active_overlay = None; // Removed
                    // self.selected_day = None; // Removed
                }
                // Note: Error messages are handled in submit_expense_transaction()
            }
            return;
        }
        
        // Handle other overlay types with existing implementation
        let (overlay_color, title_text, _content_text) = match overlay_type {
            crate::ui::app_state::OverlayType::CreateGoal => (
                egui::Color32::from_rgb(199, 112, 221), // Pink for goals
                "Create Goal",
                "Set a savings goal for something special"
            ),
            crate::ui::app_state::OverlayType::AddMoney => unreachable!("AddMoney handled above"),
            crate::ui::app_state::OverlayType::SpendMoney => unreachable!("SpendMoney handled above"),
        };
        
        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("day_action_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Full screen semi-transparent background
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    egui::Rounding::ZERO,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 80) // Subtle dark background
                );
                
                // Center the modal content
                ui.allocate_ui_at_rect(screen_rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        // Modal card with proper styling
                        egui::Frame::window(&ui.style())
                            .fill(egui::Color32::WHITE)
                            .stroke(egui::Stroke::new(3.0, overlay_color))
                            .rounding(egui::Rounding::same(16.0))
                            .inner_margin(egui::Margin::same(25.0))
                            .shadow(egui::Shadow {
                                offset: egui::vec2(6.0, 6.0),
                                blur: 20.0,
                                spread: 0.0,
                                color: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100),
                            })
                            .show(ui, |ui| {
                                // Set modal size
                                ui.set_min_size(egui::vec2(450.0, 350.0));
                                ui.set_max_size(egui::vec2(450.0, 350.0));
                                
                                ui.vertical_centered(|ui| {
                                    ui.add_space(15.0);
                                    
                                    // Title with icon
                                    let title_icon = match overlay_type {
                                        crate::ui::app_state::OverlayType::AddMoney => "💰",
                                        crate::ui::app_state::OverlayType::SpendMoney => "💸",
                                        crate::ui::app_state::OverlayType::CreateGoal => "🎯",
                                    };
                                    
                                    ui.label(egui::RichText::new(format!("{} {}", title_icon, title_text))
                                         .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                                         .strong()
                                         .color(overlay_color));
                                    
                                    ui.add_space(15.0);
                                    
                                    // Content - different for each overlay type
                                    match overlay_type {
                                        crate::ui::app_state::OverlayType::AddMoney => {
                                            // Form fields for Add Money
                                            ui.label(egui::RichText::new("Enter the amount of money you received and what it was for")
                                                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                                .color(egui::Color32::from_rgb(80, 80, 80)));
                                            
                                            ui.add_space(20.0);
                                            
                                            // Character counter - shows how many characters are left
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("Description:")
                                                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::from_rgb(60, 60, 60)));
                                                
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    let char_count = self.form.add_money_description.len();
                                                    let max_chars = 70;
                                                    let color = if char_count > max_chars {
                                                        egui::Color32::from_rgb(220, 50, 50) // Red if over limit
                                                    } else if char_count > max_chars - 10 {
                                                        egui::Color32::from_rgb(255, 165, 0) // Orange if close to limit
                                                    } else {
                                                        egui::Color32::from_rgb(120, 120, 120) // Gray normal
                                                    };
                                                    
                                                    ui.label(egui::RichText::new(format!("{}/{}", char_count, max_chars))
                                                        .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                                        .color(color));
                                                });
                                            });
                                            ui.add_space(5.0);
                                            
                                            let description_response = ui.add(
                                                egui::TextEdit::singleline(&mut self.form.add_money_description)
                                                    .hint_text("What is this money for?")
                                                    .desired_width(400.0)
                                                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                            );
                                            
                                            // Show description error message
                                            if let Some(error) = &self.form.add_money_description_error {
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
                                            
                                            // Amount input with static dollar sign and permanent visible frame
                                            ui.horizontal(|ui| {
                                                // Static dollar sign
                                                ui.label(egui::RichText::new("$")
                                                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::from_rgb(60, 60, 60)));
                                                
                                                ui.add_space(2.0);
                                                
                                                // Amount field with default egui styling
                                                let _amount_response = ui.add(
                                                    egui::TextEdit::singleline(&mut self.form.add_money_amount)
                                                        .hint_text("0.00")
                                                        .desired_width(120.0)
                                                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                                );
                                                
                                                // Validate form whenever fields change
                                                if description_response.changed() || _amount_response.changed() {
                                                    self.validate_add_money_form();
                                                }
                                            });
                                            
                                            // Show amount error message
                                            if let Some(error) = &self.form.add_money_amount_error {
                                                ui.add_space(3.0);
                                                ui.label(egui::RichText::new(error)
                                                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::from_rgb(220, 50, 50)));
                                            }
                                            
                                            ui.add_space(30.0);
                                            
                                            // Buttons
                                            ui.horizontal(|ui| {
                                                ui.add_space(50.0);
                                                
                                                // Submit button - only enabled if form is valid
                                                let button_enabled = self.form.add_money_is_valid && !self.form.add_money_description.trim().is_empty() && !self.form.add_money_amount.trim().is_empty();
                                                
                                                let button_color = if button_enabled {
                                                    egui::Color32::from_rgb(34, 139, 34) // Green
                                                } else {
                                                    egui::Color32::from_rgb(180, 180, 180) // Gray when disabled
                                                };
                                                
                                                let add_button = egui::Button::new(egui::RichText::new("Add Extra Money")
                                                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::WHITE))
                                                    .fill(button_color)
                                                    .stroke(egui::Stroke::new(2.0, button_color))
                                                    .rounding(egui::Rounding::same(10.0))
                                                    .min_size(egui::vec2(150.0, 40.0));
                                                
                                                let add_button_response = ui.add(add_button);
                                                if add_button_response.clicked() && button_enabled {
                                                    // Submit form using existing validation
                                                    log::info!("💰 Add money form submitted: '{}', ${}", 
                                                              self.form.add_money_description, self.form.add_money_amount);
                                                    
                                                    self.ui.success_message = Some("Add Extra Money functionality coming in next phase!".to_string());
                                                    self.calendar.active_overlay = None;
                                                    self.calendar.selected_day = None;
                                                    
                                                    // TEMPORARY: Sync compatibility fields
                                                    // self.active_overlay = None; // Removed
                                                    // self.selected_day = None; // Removed
                                                }
                                                
                                                // Show tooltip for disabled button
                                                if !button_enabled {
                                                    add_button_response.on_hover_text("Please fix the errors above to continue");
                                                }
                                                
                                                ui.add_space(30.0);
                                                
                                                // Cancel button
                                                let cancel_button = egui::Button::new(egui::RichText::new("Cancel")
                                                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::from_rgb(100, 100, 100)))
                                                    .fill(egui::Color32::from_rgb(245, 245, 245))
                                                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)))
                                                    .rounding(egui::Rounding::same(10.0))
                                                    .min_size(egui::vec2(90.0, 40.0));
                                                
                                                if ui.add(cancel_button).clicked() {
                                                    // Clear form fields when canceling Add Money
                                                    self.clear_add_money_form();
                                                    self.calendar.active_overlay = None;
                                                    self.calendar.selected_day = None;
                                                    
                                                    // TEMPORARY: Sync compatibility fields
                                                    // self.active_overlay = None; // Removed
                                                    // self.selected_day = None; // Removed
                                                }
                                            });
                                            
                                            ui.add_space(15.0);
                                        },
                                        _ => {
                                            // Default content for other overlay types
                                            ui.label("Feature coming soon!");
                                            
                                            ui.add_space(20.0);
                                            
                                            // Cancel button for other types
                                            let cancel_button = egui::Button::new("Cancel")
                                                .min_size(egui::vec2(80.0, 35.0));
                                            
                                            if ui.add(cancel_button).clicked() {
                                                self.calendar.active_overlay = None;
                                                self.calendar.selected_day = None;
                                                
                                                // TEMPORARY: Sync compatibility fields
                                                // self.active_overlay = None; // Removed
                                                // self.selected_day = None; // Removed
                                            }
                                        }
                                    }
                                    
                                    ui.add_space(30.0);
                                    
                                    // Buttons
                                    ui.horizontal(|ui| {
                                        ui.add_space(50.0);
                                        
                                        // OK button (text changes based on overlay type)
                                        let ok_text = match overlay_type {
                                            crate::ui::app_state::OverlayType::AddMoney => "Add Extra Money",
                                            crate::ui::app_state::OverlayType::SpendMoney => "Spend Money", 
                                            crate::ui::app_state::OverlayType::CreateGoal => "Create Goal",
                                        };
                                        
                                        // Check if button should be enabled (for Add Money, check validation)
                                        let button_enabled = match overlay_type {
                                            crate::ui::app_state::OverlayType::AddMoney => {
                                                self.form.add_money_is_valid && !self.form.add_money_description.trim().is_empty() && !self.form.add_money_amount.trim().is_empty()
                                            },
                                            _ => true, // Other overlay types always enabled
                                        };
                                        
                                        let button_color = if button_enabled {
                                            overlay_color
                                        } else {
                                            egui::Color32::from_rgb(180, 180, 180) // Gray when disabled
                                        };
                                        
                                        let text_color = if button_enabled {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::from_rgb(120, 120, 120) // Darker gray text when disabled
                                        };
                                        
                                        let ok_button = egui::Button::new(egui::RichText::new(ok_text)
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(text_color))
                                            .fill(button_color)
                                            .rounding(egui::Rounding::same(10.0))
                                            .min_size(egui::vec2(120.0, 40.0));
                                        
                                        let ok_response = ui.add(ok_button);
                                        
                                        // Only handle click if button is enabled
                                        if ok_response.clicked() && button_enabled {
                                            match overlay_type {
                                                crate::ui::app_state::OverlayType::AddMoney => {
                                                    // TODO: Implement add money logic in next phase
                                                    log::info!("💰 Add Extra Money clicked - Description: '{}', Amount: '{}'", 
                                                              self.form.add_money_description, self.form.add_money_amount);
                                                    self.ui.success_message = Some("Add Extra Money functionality coming in next phase!".to_string());
                                                },
                                                _ => {
                                                    // Default behavior for other overlays
                                                }
                                            }
                                            self.calendar.active_overlay = None;
                                            self.calendar.selected_day = None;
                                        }
                                        
                                        // Show tooltip for disabled button
                                        if !button_enabled && ok_response.hovered() {
                                            ok_response.on_hover_text("Please fix the errors above to continue");
                                        }
                                        
                                        ui.add_space(30.0);
                                        
                                        // Cancel button
                                        let cancel_button = egui::Button::new(egui::RichText::new("Cancel")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(100, 100, 100)))
                                            .fill(egui::Color32::from_rgb(245, 245, 245))
                                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)))
                                            .rounding(egui::Rounding::same(10.0))
                                            .min_size(egui::vec2(90.0, 40.0));
                                        
                                        if ui.add(cancel_button).clicked() {
                                            // Clear form fields when canceling Add Money
                                            if overlay_type == crate::ui::app_state::OverlayType::AddMoney {
                                                self.clear_add_money_form();
                                            }
                                            self.calendar.active_overlay = None;
                                            self.calendar.selected_day = None;
                                        }
                                    });
                                    
                                    ui.add_space(15.0);
                                });
                            });
                    });
                });
                
                // Handle backdrop clicks to close overlay (skip if modal was just opened this frame)
                if !self.calendar.modal_just_opened && ui.ctx().input(|i| i.pointer.any_click()) {
                    let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
                    if let Some(pos) = pointer_pos {
                        // Check if the click was outside the overlay area
                        let modal_center = ui.ctx().screen_rect().center();
                        let modal_rect = egui::Rect::from_center_size(
                            modal_center,
                            egui::vec2(500.0, 400.0)
                        );
                        
                        if !modal_rect.contains(pos) {
                            self.calendar.active_overlay = None;
                            self.calendar.selected_day = None;
                            
                            // TEMPORARY: Sync compatibility fields
                            // self.active_overlay = None; // Removed
                            // self.selected_day = None; // Removed
                        }
                    }
                }
                
                // Reset the modal_just_opened flag at the end of the frame
                self.calendar.modal_just_opened = false;
            });
    }
} 