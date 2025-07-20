//! # Child Selector Modal
//!
//! This module contains the child selection modal functionality.
//!
//! ## Responsibilities:
//! - Display list of available children
//! - Handle child selection and activation
//! - Provide visual feedback for active child
//! - Handle child loading and refresh
//!
//! ## Purpose:
//! This modal allows users to switch between different children in the system,
//! making it easy to manage multiple children's allowances from one interface.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::mappers::to_dto;
use crate::backend::domain::commands::child::SetActiveChildCommand;

impl AllowanceTrackerApp {
    /// Render the child selector modal
    pub fn render_child_selector_modal(&mut self, ctx: &egui::Context) {
        if !self.modal.show_child_selector {
            return;
        }

        log::info!("🚀 RENDERING CHILD SELECTOR MODAL");

        egui::Window::new("Select Child")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("👤 Available Children:")
                    .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                    .strong());
                
                // List all children
                match self.backend().child_service.list_children() {
                    Ok(children_result) => {
                        if children_result.children.is_empty() {
                            ui.label("No children found!");
                            ui.label("Debug: Check if test_data directory exists");
                        } else {
                            for child in children_result.children {
                                ui.horizontal(|ui| {
                                    // Show if this is the current active child
                                    let is_active = self.current_child().as_ref()
                                        .map(|c| c.id == child.id)
                                        .unwrap_or(false);
                                    
                                    if is_active {
                                        ui.label(egui::RichText::new("•")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(0, 120, 215))); // Bullet for active child
                                    } else {
                                        ui.label("   "); // Spacing
                                    }
                                    
                                    // Create hover button using the working chip approach
                                    let button_size = egui::vec2(200.0, 24.0); // Fixed size for consistency
                                    let (button_rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());
                                    
                                    // DEBUG: Log hover state
                                    if response.hovered() {
                                        log::info!("🔍 HOVER DETECTED for child: {}", child.name);
                                    }
                                    
                                    // Determine background color based on hover state (like working chips)
                                    let background_color = if response.hovered() {
                                        log::info!("🎨 PAINTING HOVER BACKGROUND for: {}", child.name);
                                        egui::Color32::from_rgba_unmultiplied(220, 220, 220, 255) // Light gray on hover
                                    } else {
                                        egui::Color32::TRANSPARENT // Transparent when not hovered
                                    };
                                    
                                    // Draw background FIRST (like chips do)
                                    ui.painter().rect_filled(
                                        button_rect,
                                        egui::Rounding::same(4.0),
                                        background_color
                                    );
                                    
                                    // DEBUG: Always paint a small debug marker to verify painting works
                                    if response.hovered() {
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(button_rect.min, egui::vec2(10.0, 10.0)),
                                            egui::Rounding::ZERO,
                                            egui::Color32::RED // Red debug square
                                        );
                                    }
                                    
                                    // Draw text on top
                                    let text_color = if is_active { 
                                        egui::Color32::from_rgb(0, 120, 215) // Active child in blue
                                    } else { 
                                        egui::Color32::from_rgb(60, 60, 60) // Default dark gray
                                    };
                                    
                                    ui.painter().text(
                                        button_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        &child.name,
                                        egui::FontId::new(16.0, egui::FontFamily::Proportional),
                                        text_color,
                                    );
                                    
                                    // Change cursor on hover
                                    if response.hovered() {
                                        ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                                    }
                                    
                                    let child_button_response = response;
                                    
                                    if child_button_response.clicked() {
                                        // Set this child as active
                                        let command = SetActiveChildCommand {
                                            child_id: child.id.clone(),
                                        };
                                        match self.backend().child_service.set_active_child(command) {
                                            Ok(_) => {
                                                self.core.current_child = Some(to_dto(child.clone()));
                                                self.load_balance();
                                                self.load_calendar_data();
                                                self.modal.show_child_selector = false;
                                                self.ui.success_message = Some("Child selected successfully!".to_string());
                                            }
                                            Err(e) => {
                                                self.ui.error_message = Some(format!("Failed to select child: {}", e));
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
                        self.modal.show_child_selector = false;
                    }
                    
                    if ui.button(egui::RichText::new("🔄 Refresh")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
                        // Try to reload the active child
                        self.load_initial_data();
                    }
                });
            });
    }
} 