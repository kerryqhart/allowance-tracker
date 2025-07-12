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
                                        ui.label(egui::RichText::new("👑")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))); // Crown for active child
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

    /// Render all modals
    pub fn render_modals(&mut self, ctx: &egui::Context) {
        self.render_add_money_modal(ctx);
        self.render_spend_money_modal(ctx);
        self.render_child_selector_modal(ctx);
    }
} 