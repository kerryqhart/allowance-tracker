use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::mappers::to_dto;
use crate::backend::domain::commands::child::SetActiveChildCommand;

impl AllowanceTrackerApp {
    /// Render the add money modal
    pub fn render_add_money_modal(&mut self, ctx: &egui::Context) {
        if !self.show_add_money_modal {
            return;
        }

        egui::Window::new("Add Money")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.text_edit_singleline(&mut self.add_money_amount);
                ui.text_edit_singleline(&mut self.add_money_description);
                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        // TODO: Implement add money logic
                        self.show_add_money_modal = false;
                        self.success_message = Some("Money added!".to_string());
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_add_money_modal = false;
                    }
                });
            });
    }

    /// Render the spend money modal
    pub fn render_spend_money_modal(&mut self, ctx: &egui::Context) {
        if !self.show_spend_money_modal {
            return;
        }

        egui::Window::new("Spend Money")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.text_edit_singleline(&mut self.spend_money_amount);
                ui.text_edit_singleline(&mut self.spend_money_description);
                ui.horizontal(|ui| {
                    if ui.button("Spend").clicked() {
                        // TODO: Implement spend money logic
                        self.show_spend_money_modal = false;
                        self.success_message = Some("Money spent!".to_string());
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_spend_money_modal = false;
                    }
                });
            });
    }

    /// Render the child selector modal
    pub fn render_child_selector_modal(&mut self, ctx: &egui::Context) {
        if !self.show_child_selector {
            return;
        }

        egui::Window::new("Select Child")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("👤 Available Children:")
                    .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                    .strong());
                
                // List all children
                match self.backend.child_service.list_children() {
                    Ok(children_result) => {
                        if children_result.children.is_empty() {
                            ui.label("No children found!");
                            ui.label("Debug: Check if test_data directory exists");
                        } else {
                            for child in children_result.children {
                                ui.horizontal(|ui| {
                                    // Show if this is the current active child
                                    let is_active = self.current_child.as_ref()
                                        .map(|c| c.id == child.id)
                                        .unwrap_or(false);
                                    
                                    if is_active {
                                        ui.label(egui::RichText::new("•")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(0, 120, 215))); // Bullet for active child
                                    } else {
                                        ui.label("   "); // Spacing
                                    }
                                    
                                    if ui.button(&child.name).clicked() {
                                        // Set this child as active
                                        let command = SetActiveChildCommand {
                                            child_id: child.id.clone(),
                                        };
                                        match self.backend.child_service.set_active_child(command) {
                                            Ok(_) => {
                                                self.current_child = Some(to_dto(child.clone()));
                                                self.load_balance();
                                                self.load_calendar_data();
                                                self.show_child_selector = false;
                                                self.success_message = Some("Child selected successfully!".to_string());
                                            }
                                            Err(e) => {
                                                self.error_message = Some(format!("Failed to select child: {}", e));
                                            }
                                        }
                                    }
                                    
                                    ui.label(child.birthdate.to_string());
                                });
                            }
                        }
                    }
                    Err(e) => {
                        ui.label(format!("Error loading children: {}", e));
                        ui.label("Debug: Check backend initialization");
                    }
                }
                
                ui.separator();
                
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.show_child_selector = false;
                    }
                    
                    if ui.button(egui::RichText::new("🔄 Refresh")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
                        // Try to reload the active child
                        self.load_initial_data();
                    }
                });
            });
    }

    /// Render day action overlay based on active overlay type
    pub fn render_day_action_overlay(&mut self, ctx: &egui::Context) {
        let overlay_type = match self.active_overlay {
            Some(overlay) => overlay,
            None => return,
        };
        
        // Define colors for each overlay type to match transaction chip themes
        let (overlay_color, title_text, content_text) = match overlay_type {
            crate::ui::app_state::OverlayType::AddMoney => (
                egui::Color32::from_rgb(34, 139, 34), // Green to match income chips
                "Add Money",
                "Enter the amount you want to add to your allowance"
            ),
            crate::ui::app_state::OverlayType::SpendMoney => (
                egui::Color32::from_rgb(128, 128, 128), // Gray to match expense chips
                "Spend Money", 
                "Enter the amount you want to spend from your allowance"
            ),
            crate::ui::app_state::OverlayType::CreateGoal => (
                egui::Color32::from_rgb(199, 112, 221), // Pink for goals
                "Create Goal",
                "Set a savings goal for something special"
            ),
        };
        
        // Create a more focused modal dialog positioned above the glyphs
        egui::Window::new(title_text)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 100.0)) // Position in upper portion of screen
            .fixed_size(egui::vec2(450.0, 280.0))
            .frame(egui::Frame::window(&ctx.style())
                .fill(egui::Color32::WHITE) // Solid white background
                .stroke(egui::Stroke::new(3.0, overlay_color))
                .rounding(egui::Rounding::same(16.0))
                .shadow(egui::Shadow {
                    offset: egui::vec2(6.0, 6.0),
                    blur: 20.0,
                    spread: 0.0,
                    color: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100),
                }))
            .show(ctx, |ui| {
                // Add a subtle backdrop click detector
                let backdrop_response = ui.ctx().input(|i| i.pointer.any_click());
                if backdrop_response {
                    // Check if click was outside the window bounds
                    let window_rect = ui.available_rect_before_wrap();
                    let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
                    if let Some(pos) = pointer_pos {
                        if !window_rect.contains(pos) {
                            self.active_overlay = None;
                            self.selected_day = None;
                        }
                    }
                }
                
                ui.vertical_centered(|ui| {
                    ui.add_space(25.0);
                    
                    // Title with icon
                    let title_icon = match overlay_type {
                        crate::ui::app_state::OverlayType::AddMoney => "💰",
                        crate::ui::app_state::OverlayType::SpendMoney => "💸",
                        crate::ui::app_state::OverlayType::CreateGoal => "🎯",
                    };
                    
                                         ui.label(egui::RichText::new(format!("{} {}", title_icon, title_text))
                         .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                         .strong()
                         .color(overlay_color)); // Use overlay color directly on white background
                    
                    ui.add_space(15.0);
                    
                    // Content
                    ui.label(egui::RichText::new(content_text)
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(80, 80, 80)));
                    
                    ui.add_space(40.0);
                    
                    // Buttons
                    ui.horizontal(|ui| {
                        ui.add_space(80.0);
                        
                        // OK button
                        let ok_button = egui::Button::new(egui::RichText::new("OK")
                            .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::WHITE))
                            .fill(overlay_color)
                            .rounding(egui::Rounding::same(10.0))
                            .min_size(egui::vec2(90.0, 40.0));
                        
                        if ui.add(ok_button).clicked() {
                            self.active_overlay = None;
                            self.selected_day = None;
                        }
                        
                        ui.add_space(30.0);
                        
                        // Cancel button
                        let cancel_button = egui::Button::new(egui::RichText::new("Cancel")
                            .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(100, 100, 100)))
                            .fill(egui::Color32::from_rgb(245, 245, 245))
                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)))
                            .rounding(egui::Rounding::same(10.0))
                            .min_size(egui::vec2(90.0, 40.0));
                        
                        if ui.add(cancel_button).clicked() {
                            self.active_overlay = None;
                            self.selected_day = None;
                        }
                    });
                    
                    ui.add_space(25.0);
                });
            });
    }

    /// Render all modals
    pub fn render_modals(&mut self, ctx: &egui::Context) {
        self.render_add_money_modal(ctx);
        self.render_spend_money_modal(ctx);
        self.render_child_selector_modal(ctx);
        self.render_day_action_overlay(ctx);
    }
} 