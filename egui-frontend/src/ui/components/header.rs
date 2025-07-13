//! # Header Module
//!
//! This module handles rendering the application header, including child selection,
//! balance display, and action buttons.
//!
//! ## Key Functions:
//! - `render_header()` - Main header rendering with child selector and balance
//! - `render_child_dropdown()` - Child selection dropdown menu
//! - `render_messages()` - Success/error message display
//!
//! ## Purpose:
//! The header provides essential navigation and information display:
//! - Current child selection with dropdown
//! - Current balance display
//! - Quick action buttons (Add Money, Spend Money)
//! - Message display for user feedback
//!
//! ## Features:
//! - Translucent background for modern look
//! - Responsive design
//! - Interactive child selection
//! - Visual feedback for user actions

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    /// Render the header
    pub fn render_header(&mut self, ui: &mut egui::Ui) {
        // Use Frame with translucent fill for proper transparency
        let header_height = 60.0;
        
        // Create a frame with translucent background
        let frame = egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30)) // Truly translucent white
            .inner_margin(egui::Margin::symmetric(10.0, 10.0));
        
        frame.show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), header_height - 20.0), // Account for margin
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.horizontal(|ui| {
                        // Clean title without emoji
                        ui.label(egui::RichText::new("Allowance Tracker")
                            .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                            .strong()
                            .color(egui::Color32::from_rgb(60, 60, 60))); // Dark gray for readability
                        
                        // Flexible space to push right content to the right
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(child) = &self.current_child {
                                // Balance with clean styling (no color coding)
                                ui.label(egui::RichText::new(format!("${:.2}", self.current_balance))
                                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                                    .strong()
                                    .color(egui::Color32::from_rgb(60, 60, 60))); // Same dark gray as title
                                
                                // Add spacing between balance and name
                                ui.add_space(15.0);
                                
                                // Child name as clickable text with hover effects
                                let child_name_text = &child.name;
                                
                                // Create the clickable button (no text selection)
                                let child_button_response = ui.add(egui::Button::new(
                                    egui::RichText::new(child_name_text)
                                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                                        .strong()
                                        .color(egui::Color32::from_rgb(80, 80, 80))
                                )
                                .fill(egui::Color32::TRANSPARENT)  // Transparent background
                                .stroke(egui::Stroke::NONE)        // No border
                                .rounding(egui::Rounding::ZERO));   // No rounding
                                
                                // Add hover, click, and dropdown-open effects
                                if child_button_response.hovered() || child_button_response.is_pointer_button_down_on() || self.show_child_dropdown {
                                    // Keep hand cursor for both hover and click
                                    ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                                    
                                    // Draw the overlay with different alpha for hover vs click/dropdown
                                    let expanded_rect = child_button_response.rect.expand(4.0);
                                    
                                    // Use higher alpha when clicked or dropdown is open, lower when just hovered
                                    let alpha = if child_button_response.is_pointer_button_down_on() || self.show_child_dropdown {
                                        60 // Clicked state or dropdown open - more opaque
                                    } else {
                                        20 // Hovered state - subtle
                                    };
                                    
                                    // Draw semi-transparent white background
                                    ui.painter().rect_filled(
                                        expanded_rect,
                                        egui::Rounding::same(6.0),
                                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
                                    );
                                    
                                    // Draw Wednesday color border
                                    ui.painter().rect_stroke(
                                        expanded_rect,
                                        egui::Rounding::same(6.0),
                                        egui::Stroke::new(1.5, egui::Color32::from_rgb(232, 150, 199))
                                    );
                                }
                                
                                if child_button_response.clicked() {
                                    if !self.show_child_dropdown {
                                        self.show_child_dropdown = true;
                                        self.child_dropdown_just_opened = true;
                                    } else {
                                        self.show_child_dropdown = false;
                                    }
                                }
                                
                                // Show dropdown if opened
                                if self.show_child_dropdown {
                                    self.render_child_dropdown(ui, child_button_response.rect);
                                }
                            } else {
                                // No child selected - show select child text
                                let select_text = "Select Child";
                                
                                // Create the clickable button (no text selection)
                                let select_button_response = ui.add(egui::Button::new(
                                    egui::RichText::new(select_text)
                                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                        .color(egui::Color32::GRAY)
                                )
                                .fill(egui::Color32::TRANSPARENT)  // Transparent background
                                .stroke(egui::Stroke::NONE)        // No border
                                .rounding(egui::Rounding::ZERO));   // No rounding
                                
                                // Always show pressed state when dropdown is open
                                if self.show_child_dropdown {
                                    // Keep hand cursor when dropdown is open
                                    ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                                    
                                    // Draw pressed state overlay
                                    let expanded_rect = select_button_response.rect.expand(4.0);
                                    
                                    // Draw semi-transparent white background with pressed alpha
                                    ui.painter().rect_filled(
                                        expanded_rect,
                                        egui::Rounding::same(6.0),
                                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 60)
                                    );
                                    
                                    // Draw Wednesday color border
                                    ui.painter().rect_stroke(
                                        expanded_rect,
                                        egui::Rounding::same(6.0),
                                        egui::Stroke::new(1.0, egui::Color32::from_rgb(126, 120, 229))
                                    );
                                } else {
                                    // Add hover effect only when not pressed
                                    if select_button_response.hovered() || select_button_response.is_pointer_button_down_on() {
                                        // Keep hand cursor for both hover and click
                                        ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                                        
                                        // Draw the overlay with different alpha for hover vs click
                                        let expanded_rect = select_button_response.rect.expand(4.0);
                                        
                                        // Use higher alpha when clicked, lower when just hovered
                                        let alpha = if select_button_response.is_pointer_button_down_on() {
                                            60 // Clicked state - more opaque
                                        } else {
                                            20 // Hovered state - subtle
                                        };
                                        
                                        // Draw semi-transparent white background
                                        ui.painter().rect_filled(
                                            expanded_rect,
                                            egui::Rounding::same(6.0),
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
                                        );
                                        
                                        // Draw Wednesday color border
                                        ui.painter().rect_stroke(
                                            expanded_rect,
                                            egui::Rounding::same(6.0),
                                            egui::Stroke::new(1.5, egui::Color32::from_rgb(232, 150, 199))
                                        );
                                    }
                                }
                                
                                if select_button_response.clicked() {
                                    if !self.show_child_dropdown {
                                        self.show_child_dropdown = true;
                                        self.child_dropdown_just_opened = true;
                                    } else {
                                        self.show_child_dropdown = false;
                                    }
                                }
                                
                                // Show dropdown if opened
                                if self.show_child_dropdown {
                                    self.render_child_dropdown(ui, select_button_response.rect);
                                }
                            }
                        });
                    });
                }
            );
        });
    }
    
    /// Render child selector dropdown
    pub fn render_child_dropdown(&mut self, ui: &mut egui::Ui, button_rect: egui::Rect) {
        // Calculate dropdown position (below the button)
        let dropdown_pos = egui::pos2(button_rect.left(), button_rect.bottom());
        
        // Create a stable area with a consistent ID
        let area_id = egui::Id::new("child_dropdown_area");
        
        let area_response = egui::Area::new(area_id)
            .order(egui::Order::Foreground)
            .fixed_pos(dropdown_pos)
            .show(ui.ctx(), |ui| {
                let frame = egui::Frame::popup(&ui.style())
                    .fill(egui::Color32::WHITE)  // Pure white background
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 200)))
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::Margin::same(8.0));
                
                frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        // Load children from backend
                        match self.backend.child_service.list_children() {
                            Ok(children_result) => {
                                if children_result.children.is_empty() {
                                    // Set minimum width for empty state
                                    ui.set_min_width(120.0);
                                    ui.label("No children available");
                                } else {
                                    // Calculate the width needed based on actual child names
                                    let mut max_width: f32 = 0.0;
                                    let min_width_raw = ui.fonts(|f| f.layout_no_wrap("Bill Smith".to_string(), 
                                        egui::FontId::new(14.0, egui::FontFamily::Proportional), 
                                        egui::Color32::BLACK)).size().x;
                                    let min_width = min_width_raw * 1.3; // Apply same generous multiplier
                                    
                                    for child in &children_result.children {
                                        let is_current = self.current_child.as_ref()
                                            .map(|c| c.id == child.id)
                                            .unwrap_or(false);
                                        
                                        let display_text = if is_current {
                                            format!("👑 {}", child.name)
                                        } else {
                                            child.name.clone()
                                        };
                                        
                                        let text_width = ui.fonts(|f| f.layout_no_wrap(display_text.clone(), 
                                            egui::FontId::new(14.0, egui::FontFamily::Proportional), 
                                            egui::Color32::BLACK)).size().x;
                                        
                                        // Add extra breathing room - multiply by 1.3 for more generous sizing
                                        let generous_width = text_width * 1.3;
                                        
                                        max_width = max_width.max(generous_width);
                                    }
                                    
                                    // Use the larger of calculated width or minimum width, plus buffer
                                    let dropdown_width = (max_width.max(min_width) + 40.0).max(120.0);
                                    ui.set_min_width(dropdown_width);
                                    
                                    for child in children_result.children {
                                        let is_current = self.current_child.as_ref()
                                            .map(|c| c.id == child.id)
                                            .unwrap_or(false);
                                        
                                        let button_text = if is_current {
                                            format!("👑 {}", child.name)
                                        } else {
                                            child.name.clone()
                                        };
                                        
                                        let button = egui::Button::new(
                                            egui::RichText::new(button_text)
                                                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                                .color(if is_current { 
                                                    egui::Color32::from_rgb(79, 109, 245) 
                                                } else { 
                                                    egui::Color32::from_rgb(60, 60, 60) 
                                                })
                                        )
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::NONE)
                                        .rounding(egui::Rounding::same(4.0));
                                        
                                        // Create hover area with calculated width instead of full available width
                                        let inner_response = ui.allocate_ui_with_layout(
                                            egui::vec2(dropdown_width, 22.0), // Fixed height for consistent row spacing
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add(button)
                                            }
                                        );
                                        let row_response = inner_response.response;
                                        let button_response = inner_response.inner;
                                        
                                        // Add subtle hover effect for the row
                                        if row_response.hovered() {
                                            ui.painter().rect_filled(
                                                row_response.rect,
                                                egui::Rounding::same(2.0),
                                                egui::Color32::from_rgba_unmultiplied(79, 109, 245, 20)
                                            );
                                        }
                                        
                                        if (row_response.clicked() || button_response.clicked()) && !is_current {
                                            // Set this child as active
                                            let command = crate::backend::domain::commands::child::SetActiveChildCommand {
                                                child_id: child.id.clone(),
                                            };
                                            match self.backend.child_service.set_active_child(command) {
                                                Ok(_) => {
                                                    self.current_child = Some(crate::ui::mappers::to_dto(child));
                                                    self.load_balance();
                                                    self.load_calendar_data();
                                                    self.show_child_dropdown = false;
                                                }
                                                Err(e) => {
                                                    self.error_message = Some(format!("Failed to select child: {}", e));
                                                    self.show_child_dropdown = false;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                ui.label(format!("Error loading children: {}", e));
                            }
                        }
                    });
                });
            });
        
        // Close dropdown if mouse is not hovering over either the button or the dropdown
        // (but not on the first frame when it was just opened)
        if !self.child_dropdown_just_opened {
            let mouse_pos = ui.input(|i| i.pointer.latest_pos());
            if let Some(pos) = mouse_pos {
                let hovering_button = button_rect.contains(pos);
                let hovering_dropdown = area_response.response.contains_pointer();
                
                if !hovering_button && !hovering_dropdown {
                    self.show_child_dropdown = false;
                }
            }
        }
        
        // Reset the "just opened" flag after one frame
        if self.child_dropdown_just_opened {
            self.child_dropdown_just_opened = false;
        }
    }
    
    /// Render error and success messages
    pub fn render_messages(&self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error_message {
            ui.colored_label(egui::Color32::RED, format!("❌ {}", error));
        }
        if let Some(success) = &self.success_message {
            ui.colored_label(egui::Color32::GREEN, format!("✅ {}", success));
        }
    }
} 