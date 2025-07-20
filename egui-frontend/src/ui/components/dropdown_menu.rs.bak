//! # Dropdown Menu Component
//!
//! This module provides a generalized dropdown menu component that can be reused
//! for different types of dropdowns (child selector, settings menu, etc.).
//!
//! ## Key Features:
//! - Configurable button content and styling
//! - Configurable menu items with actions
//! - Hover effects and visual feedback
//! - Click-outside-to-close behavior
//! - Consistent styling across different dropdown types

use eframe::egui;

/// Represents a single menu item in a dropdown
#[derive(Clone)]
pub struct DropdownMenuItem {
    pub label: String,
    pub icon: Option<String>,  // Optional icon/emoji
    pub is_current: bool,      // Whether this item is currently selected
    pub is_enabled: bool,      // Whether this item is clickable
}

/// Configuration for dropdown button appearance
pub struct DropdownButtonConfig {
    pub text: String,
    pub font_size: f32,
    pub text_color: egui::Color32,
    pub hover_bg_color: egui::Color32,
    pub hover_border_color: egui::Color32,
}

/// Configuration for dropdown menu appearance  
pub struct DropdownMenuConfig {
    pub min_width: f32,
    pub item_height: f32,
    pub item_font_size: f32,
}

/// Generalized dropdown menu component
pub struct DropdownMenu {
    pub is_open: bool,
    pub just_opened: bool,
    pub unique_id: String,
}

impl DropdownMenu {
    pub fn new(unique_id: String) -> Self {
        Self {
            is_open: false,
            just_opened: false,
            unique_id,
        }
    }

    /// Render a dropdown button with the specified configuration
    /// Returns the button response and whether the dropdown should be shown
    pub fn render_button(
        &mut self,
        ui: &mut egui::Ui,
        config: &DropdownButtonConfig,
    ) -> (egui::Response, bool) {
        // Create a clickable area (no selectable text)
        let button_response = ui.allocate_response(
            egui::vec2(
                ui.fonts(|f| f.layout_no_wrap(
                    config.text.clone(), 
                    egui::FontId::new(config.font_size, egui::FontFamily::Proportional), 
                    config.text_color
                )).size().x + 16.0, // Add padding
                config.font_size + 8.0 // Add vertical padding
            ),
            egui::Sense::click()
        );
        
        // Manually draw the text (no text selection possible)
        ui.painter().text(
            button_response.rect.center(),
            egui::Align2::CENTER_CENTER,
            &config.text,
            egui::FontId::new(config.font_size, egui::FontFamily::Proportional),
            config.text_color
        );

        // Add hover and click effects
        if button_response.hovered() || button_response.is_pointer_button_down_on() || self.is_open {
            // Keep hand cursor for hover, click, and dropdown-open states
            ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
            
            // Draw the overlay with different alpha for hover vs click/dropdown
            let expanded_rect = button_response.rect.expand(4.0);
            
            // Use higher alpha when clicked or dropdown is open, lower when just hovered
            let alpha = if button_response.is_pointer_button_down_on() || self.is_open {
                60 // Clicked state or dropdown open - more opaque
            } else {
                20 // Hovered state - subtle
            };
            
            // Draw semi-transparent background
            ui.painter().rect_filled(
                expanded_rect,
                egui::Rounding::same(6.0),
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
            );
            
            // Draw border
            ui.painter().rect_stroke(
                expanded_rect,
                egui::Rounding::same(6.0),
                egui::Stroke::new(1.5, config.hover_border_color)
            );
        }

        // Handle button click
        if button_response.clicked() {
            log::info!("🖱️ DROPDOWN BUTTON CLICKED!");
            if !self.is_open {
                log::info!("🔽 Opening dropdown");
                self.is_open = true;
                self.just_opened = true;
            } else {
                log::info!("🔼 Closing dropdown");
                self.is_open = false;
            }
        }

        (button_response, self.is_open)
    }

    /// Render dropdown menu with the specified items and configuration
    /// Returns the index of the clicked item, if any
    pub fn render_menu<F>(
        &mut self,
        ui: &mut egui::Ui,
        button_rect: egui::Rect,
        items: &[DropdownMenuItem],
        config: &DropdownMenuConfig,
        mut on_item_clicked: F,
    ) -> Option<usize>
    where
        F: FnMut(usize),
    {
        if !self.is_open {
            return None;
        }

        log::info!("📋 RENDER_DROPDOWN_MENU called for {}", self.unique_id);
        
        // Create a stable area with a unique ID based on the dropdown instance
        let area_id = egui::Id::new(&self.unique_id);
        
        let mut clicked_item = None;
        
        // Calculate safe dropdown position that won't need screen boundary adjustment
        let screen_rect = ui.ctx().screen_rect();
        let estimated_dropdown_width = config.min_width + 60.0; // Estimate with padding
        
        // If dropdown would go off-screen, position it to the left of the button instead
        let safe_x = if button_rect.left() + estimated_dropdown_width > screen_rect.max.x {
            // Position dropdown to end at the button's right edge (right-aligned)
            button_rect.right() - estimated_dropdown_width
        } else {
            // Standard left-aligned positioning
            button_rect.left()
        };
        
        let desired_pos = egui::pos2(safe_x, button_rect.bottom());
        log::info!("📍 {} dropdown safe_pos=({:.1},{:.1}), screen_width={:.1}, estimated_width={:.1}", 
            self.unique_id, desired_pos.x, desired_pos.y, screen_rect.width(), estimated_dropdown_width);
        
        let area_response = egui::Area::new(area_id)
            .order(egui::Order::Tooltip)  // Higher than Foreground
            .current_pos(desired_pos)  // Suggest position but allow egui to adjust
            .interactable(true)  // Ensure this area can receive interactions
            .show(ui.ctx(), |ui| {
                let frame = egui::Frame::popup(&ui.style())
                    .fill(egui::Color32::WHITE)  // Pure white background
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 200)))  // Normal border
                    .rounding(egui::Rounding::same(6.0))
                    .inner_margin(egui::Margin::same(8.0));
                
                frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        if items.is_empty() {
                            // Set minimum width for empty state
                            ui.set_min_width(config.min_width);
                            ui.label("No items available");
                        } else {
                            // Calculate the width needed based on actual item labels
                            let mut max_width: f32 = 0.0;
                            
                            for item in items {
                                let display_text = if let Some(icon) = &item.icon {
                                    format!("{} {}", icon, item.label)
                                } else {
                                    item.label.clone()
                                };
                                
                                let text_width = ui.fonts(|f| f.layout_no_wrap(display_text.clone(), 
                                    egui::FontId::new(config.item_font_size, egui::FontFamily::Proportional), 
                                    egui::Color32::BLACK)).size().x;
                                
                                // Add extra breathing room - multiply by 1.3 for more generous sizing
                                let generous_width = text_width * 1.3;
                                max_width = max_width.max(generous_width);
                            }
                            
                            // Use the larger of calculated width or minimum width, plus buffer
                            let dropdown_width = (max_width.max(config.min_width) + 40.0).max(120.0);
                            ui.set_min_width(dropdown_width);
                            
                            for (index, item) in items.iter().enumerate() {
                                let button_text = if let Some(icon) = &item.icon {
                                    format!("{} {}", icon, item.label)
                                } else {
                                    item.label.clone()
                                };
                                
                                log::info!("🚀 RENDERING DROPDOWN ITEM: {}", item.label);
                                
                                // Create a clickable rect with manual text drawing to prevent text selection interference
                                let button_response = ui.allocate_response(
                                    egui::vec2(dropdown_width, config.item_height),
                                    egui::Sense::click()
                                );
                                
                                // Manually draw the text (no text selection possible)
                                let text_color = if item.is_current { 
                                    egui::Color32::from_rgb(79, 109, 245) 
                                } else { 
                                    egui::Color32::from_rgb(60, 60, 60) 
                                };
                                ui.painter().text(
                                    button_response.rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    &button_text,
                                    egui::FontId::new(config.item_font_size, egui::FontFamily::Proportional),
                                    text_color
                                );
                                
                                // Check hover and paint background BEHIND the text
                                if button_response.hovered() && item.is_enabled {
                                    log::info!("🔍 DROPDOWN HOVER DETECTED: {}", item.label);
                                    
                                    // Paint hover background BEHIND by using a lower z-order
                                    ui.painter().rect_filled(
                                        button_response.rect,
                                        egui::Rounding::same(4.0),
                                        egui::Color32::from_rgba_unmultiplied(230, 230, 230, 255)
                                    );
                                    
                                    // Repaint the text on top
                                    ui.painter().text(
                                        button_response.rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        button_text,
                                        egui::FontId::new(config.item_font_size, egui::FontFamily::Proportional),
                                        if item.is_current { 
                                            egui::Color32::from_rgb(79, 109, 245) 
                                        } else { 
                                            egui::Color32::from_rgb(60, 60, 60) 
                                        }
                                    );
                                }
                                
                                // Click detection
                                if button_response.clicked() && item.is_enabled {
                                    log::info!("🖱️ DROPDOWN ITEM CLICKED: {}", item.label);
                                    clicked_item = Some(index);
                                    on_item_clicked(index);
                                    self.is_open = false;
                                }
                            }
                        }
                    });
                });
            });
        
        // Close dropdown if mouse is not hovering over either the button or the dropdown
        // (but not on the first frame when it was just opened)
        if !self.just_opened {
            let mouse_pos = ui.input(|i| i.pointer.latest_pos());
            if let Some(pos) = mouse_pos {
                let hovering_button = button_rect.contains(pos);
                let hovering_dropdown = area_response.response.contains_pointer();
                
                // DEBUG: Log detailed bounds information
                log::info!("🐛 HOVER DEBUG for {}: mouse=({:.1},{:.1}), button_rect=({:.1},{:.1} to {:.1},{:.1}), area_rect=({:.1},{:.1} to {:.1},{:.1}), hovering_button={}, hovering_dropdown={}", 
                    self.unique_id,
                    pos.x, pos.y,
                    button_rect.min.x, button_rect.min.y, button_rect.max.x, button_rect.max.y,
                    area_response.response.rect.min.x, area_response.response.rect.min.y, 
                    area_response.response.rect.max.x, area_response.response.rect.max.y,
                    hovering_button, hovering_dropdown
                );
                
                if !hovering_button && !hovering_dropdown {
                    log::info!("🔴 CLOSING {} dropdown: not hovering button or dropdown", self.unique_id);
                    self.is_open = false;
                }
            }
        }
        
        // Reset the "just opened" flag after one frame
        if self.just_opened {
            self.just_opened = false;
        }
        
        clicked_item
    }
} 