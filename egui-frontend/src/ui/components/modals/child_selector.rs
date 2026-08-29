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
use crate::backend::domain::ChildStatus;
use crate::ui::state::roster::status_label;

/// One row of the selector, snapshotted out of the roster so the roster borrow
/// ends before the closure needs `&mut self`.
struct SelectorRow {
    id: shared::ChildId,
    name: String,
    /// `Some` for a child we can actually switch to; `None` while it is
    /// downloading or unresolvable, in which case `reason` says why.
    child: Option<crate::backend::domain::models::child::Child>,
    reason: Option<&'static str>,
}

impl AllowanceTrackerApp {
    /// Render the child selector modal
    pub fn render_child_selector_modal(&mut self, ctx: &egui::Context) {
        if !self.modal.show_child_selector {
            return;
        }

        log::info!("RENDERING CHILD SELECTOR MODAL");

        egui::Window::new("Select Child")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("👤 Available Children:")
                    .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                    .strong());
                
                // List all children from the roster — never a filesystem scan.
                // This is a render path, and `list_children()` here would walk
                // every child folder, blocking the frame on any that iCloud has
                // not materialized.
                let rows: Vec<SelectorRow> = self
                    .roster
                    .entries()
                    .iter()
                    .map(|e| SelectorRow {
                        id: e.entry.id.clone(),
                        name: e.display_name().to_string(),
                        child: match &e.status {
                            ChildStatus::Available(c) => Some(c.clone()),
                            _ => None,
                        },
                        reason: status_label(&e.status),
                    })
                    .collect();

                if rows.is_empty() {
                    ui.label("No children registered yet.");
                    ui.label("Settings → Children → Add existing child…");
                } else {
                    for row in rows {
                        ui.horizontal(|ui| {
                            let selectable = row.child.is_some();

                            // Show if this is the current active child
                            let is_active = selectable
                                && self.get_current_child_from_backend().as_ref()
                                    .map(|c| c.id.as_str() == row.id.as_str())
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
                            let sense = if selectable { egui::Sense::click() } else { egui::Sense::hover() };
                            let (button_rect, response) = ui.allocate_exact_size(button_size, sense);

                            // Hover highlight only for a child that can be picked.
                            let background_color = if selectable && response.hovered() {
                                egui::Color32::from_rgba_unmultiplied(220, 220, 220, 255) // Light gray on hover
                            } else {
                                egui::Color32::TRANSPARENT // Transparent when not hovered
                            };

                            // Draw background FIRST (like chips do)
                            ui.painter().rect_filled(
                                button_rect,
                                egui::CornerRadius::same(4),
                                background_color
                            );

                            // Draw text on top. A child that cannot be picked
                            // is greyed rather than hidden — a silently missing
                            // child is indistinguishable from the bug this
                            // design exists to fix.
                            let text_color = if !selectable {
                                egui::Color32::from_rgb(150, 150, 150)
                            } else if is_active {
                                egui::Color32::from_rgb(0, 120, 215) // Active child in blue
                            } else {
                                egui::Color32::from_rgb(60, 60, 60) // Default dark gray
                            };

                            ui.painter().text(
                                button_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &row.name,
                                egui::FontId::new(16.0, egui::FontFamily::Proportional),
                                text_color,
                            );

                            // Change cursor on hover
                            if selectable && response.hovered() {
                                ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                            }

                            if let Some(child) = row.child {
                                if response.clicked() {
                                    // Set this child as active
                                    let command = SetActiveChildCommand {
                                        child_id: row.id.as_str().to_string(),
                                    };
                                    match self.backend().child_service.set_active_child(command) {
                                        Ok(_) => {
                                            self.core.current_child = Some(to_dto(child.clone()));
                                            self.refresh_all_data_for_current_child();
                                            self.modal.show_child_selector = false;
                                            // Child selection feedback removed
                                        }
                                        Err(e) => {
                                            self.ui.error_message = Some(format!("Failed to select child: {}", e));
                                        }
                                    }
                                }
                                ui.label(child.birthdate.to_string());
                            } else if let Some(reason) = row.reason {
                                ui.label(egui::RichText::new(reason)
                                    .color(egui::Color32::from_rgb(150, 150, 150)));
                            }
                        });
                    }
                }


                ui.separator();
                
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.modal.show_child_selector = false;
                    }
                    
                    if ui.button(egui::RichText::new("Refresh")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
                        // Re-walk the registry on the worker thread — this is
                        // the user's manual retry for a child that was still
                        // downloading — then reload the active child once the
                        // walk says its folder is readable. The rebuild leaves
                        // every entry `Downloading`, so this must be a request,
                        // not a direct call.
                        self.rebuild_roster();
                        self.pending_initial_load = true;
                    }
                });
            });
    }
} 