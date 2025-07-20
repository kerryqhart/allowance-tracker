//! # Calendar Renderer Module
//!
//! This module handles all calendar-related rendering functionality for the allowance tracker app.
//! It provides a visual, interactive calendar view where users can see their transactions
//! displayed on specific dates.
//!
//! ## Key Functions:
//! - `draw_calendar_section_with_toggle()` - Main calendar view with Sunday-first layout
//! - `navigate_month()` - Handle month navigation (previous/next)
//! - `get_day_header_color()` - Calculate gradient colors for day headers
//! - `draw_calendar_days_responsive()` - Render calendar grid with responsive design
//! - `calculate_calendar_grid_height()` - Calculate required height for calendar
//!
//! ## Purpose:
//! This module provides the primary visual interface for the allowance tracker, showing
//! transactions in a calendar format that's intuitive and kid-friendly. It handles:
//! - Month navigation and date calculation
//! - Transaction placement on calendar days
//! - Responsive design for different screen sizes
//! - Visual styling with gradients and hover effects
//!
//! ## Features:
//! - Interactive month navigation
//! - Transaction chips displayed on calendar days
//! - Responsive grid layout
//! - Kid-friendly visual design with gradients
//! - Proper date handling using chrono library

use eframe::egui;
use chrono::NaiveDate;
use shared::Transaction;
use crate::ui::app_state::AllowanceTrackerApp;

// Import types, styling, and layout from the same module
use super::types::*;
use super::styling::*;
use super::layout::*;











impl CalendarDay {
    
    /// Render this calendar day with configurable styling
    pub fn render(&self, ui: &mut egui::Ui, width: f32, height: f32) -> egui::Response {
        let (response, _) = self.render_with_config(ui, width, height, &RenderConfig::default());
        response
    }
    
    /// Render this calendar day for grid layout
    pub fn render_grid(&self, ui: &mut egui::Ui, width: f32, height: f32) -> egui::Response {
        let (response, _) = self.render_with_config(ui, width, height, &RenderConfig {
            is_grid_layout: true,
            enable_click_handler: true,
            is_selected: false, // Default to not selected - this method doesn't know about selection
            transaction_selection_mode: false,
            selected_transaction_ids: std::collections::HashSet::new(),
            expanded_day: None,
        });
        response
    }
    
    /// Render this calendar day with specified configuration
    /// Returns (response, clicked_transaction_ids) where clicked_transaction_ids contains IDs of transactions whose checkboxes were clicked
    pub fn render_with_config(&self, ui: &mut egui::Ui, width: f32, height: f32, config: &RenderConfig) -> (egui::Response, Vec<String>) {
        // Initialize variable to collect checkbox clicks
        let mut clicked_transaction_ids = Vec::new();
        
        // Allocate space for this day cell and get hover/click detection - same approach as chips
        let (cell_rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover().union(egui::Sense::click()));
        let is_hovered = response.hovered();
        
        // Draw shadow first (behind everything else) for today's date
        if self.is_today {
            let shadow_rect = egui::Rect::from_min_size(
                cell_rect.min + egui::vec2(2.0, 2.0),
                cell_rect.size()
            );
            ui.painter().rect_filled(
                shadow_rect,
                egui::Rounding::same(2.0),
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 30) // Subtle shadow
            );
        }
        
        // Draw background for the day cell using centralized color scheme with hover effect
        let base_bg_color = self.day_type.background_color(self.is_today);
        let bg_color = if config.is_selected {
            // Selected day gets a purple-pink tint matching the Create Goal button
            egui::Color32::from_rgba_unmultiplied(230, 190, 235, 140) // Purple-pink for selection
        } else if is_hovered {
            // Make more opaque when hovered - same approach as chips
            if self.is_today {
                // For today, make the yellow background more solid
                egui::Color32::from_rgba_unmultiplied(255, 248, 220, 180) // More opaque yellow
            } else {
                match self.day_type {
                    CalendarDayType::CurrentMonth => {
                        // Make current month days more opaque white
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 120) // More opaque white
                    }
                    CalendarDayType::FillerDay => {
                        // Make filler days more opaque gray
                        egui::Color32::from_rgba_unmultiplied(120, 120, 120, 160) // More opaque gray
                    }
                }
            }
        } else {
            base_bg_color
        };
        
        ui.painter().rect_filled(
            cell_rect,
            egui::Rounding::same(2.0),
            bg_color
        );
        
        // Draw border around the day cell using centralized color scheme
        if config.is_selected {
            // Selected day gets a purple-pink border matching the Create Goal button
            ui.painter().rect_stroke(
                cell_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(2.0, egui::Color32::from_rgb(199, 112, 221)) // Purple-pink border for selection
            );
        } else if self.is_today {
            // Double outline for today: white inner + dark outer for high visibility
            // Draw white inner outline first
            ui.painter().rect_stroke(
                cell_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(2.0, egui::Color32::WHITE)
            );
            
            // Draw dark outer outline
            let outer_rect = egui::Rect::from_min_size(
                cell_rect.min - egui::vec2(1.0, 1.0),
                cell_rect.size() + egui::vec2(2.0, 2.0)
            );
            ui.painter().rect_stroke(
                outer_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(2.0, self.day_type.border_color(self.is_today))
            );
        } else {
            // Normal single outline for other days
            let border_color = self.day_type.border_color(self.is_today);
            ui.painter().rect_stroke(
                cell_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(0.5, border_color)
            );
        }
        
        // Draw the content within the allocated cell rectangle
        let ui_result = ui.allocate_ui_at_rect(cell_rect, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.set_height(height);
                
                // Add small padding inside the cell
                ui.add_space(4.0);
                
                // Top row: Day number (left) and Balance (right)
                ui.horizontal(|ui| {
                    ui.set_width(width - 8.0); // Account for padding
                    
                    // Get the font family for calendar rendering
                    let font_family = get_calendar_font_family(ui.ctx());
                    
                    // Day number in upper left (only for current month days)
                    if matches!(self.day_type, CalendarDayType::CurrentMonth) {
                        let day_font_size = get_day_number_font_size(config.is_grid_layout, width);
                        
                        // Day number text color using centralized color scheme
                        let day_text_color = self.day_type.day_text_color();
                        
                        // Create the rich text with emphasis for today
                        let rich_text = egui::RichText::new(self.day_number.to_string())
                            .font(egui::FontId::new(day_font_size, font_family.clone()))
                            .color(day_text_color)
                            .strong();
                        
                        if self.is_today {
                            // Use manual underline for today's date (more subtle than native)
                            let rich_text_bold = egui::RichText::new(self.day_number.to_string())
                                .font(egui::FontId::new(day_font_size, font_family.clone()))
                                .color(day_text_color)
                                .strong();
                            
                            // Render the text first - disable selection to prevent dropdown interference
                            let label_response = ui.add(egui::Label::new(rich_text_bold).selectable(false));
                            
                            // Draw manual underline beneath the text (shorter and thinner)
                            let text_rect = label_response.rect;
                            let underline_y = text_rect.bottom() + 1.0; // 1px below text
                            let left_padding = 3.0; // More padding on left side
                            let right_padding = 2.0; // Less padding on right side
                            let underline_color = egui::Color32::from_rgb(80, 80, 80); // Dark gray
                            ui.painter().line_segment(
                                [
                                    egui::pos2(text_rect.left() + left_padding, underline_y),
                                    egui::pos2(text_rect.right() - right_padding, underline_y)
                                ],
                                egui::Stroke::new(0.7, underline_color)
                            );
                        } else {
                            // Normal day number rendering - disable selection to prevent dropdown interference
                            ui.add(egui::Label::new(rich_text).selectable(false));
                        }
                    }
                    
                    // Balance in upper right (subtle gray) - only for current month days
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(balance) = self.balance {
                            // Only show balance for current month days, not for filler days
                            if matches!(self.day_type, CalendarDayType::CurrentMonth) {
                                let balance_font_size = get_balance_font_size(config.is_grid_layout, width);
                                
                                // Balance text color using centralized color scheme
                                let balance_color = self.day_type.balance_text_color();
                                
                                ui.add(egui::Label::new(
                                    egui::RichText::new(format!("${:.2}", balance))
                                        .font(egui::FontId::new(balance_font_size, font_family.clone()))
                                        .color(balance_color)
                                ).selectable(false)); // Disable selection to prevent dropdown interference
                            }
                        }
                    });
                });
                
                // Add some spacing between header and transaction chips
                ui.add_space(4.0);
                
                // Transaction chips below - vertically stacked
                // Convert transactions to calendar chips
                let chips = CalendarChip::from_transactions(self.transactions.clone(), config.is_grid_layout);
                
                // Calculate how many chips can fit dynamically based on available space
                let (chips_to_show_count, needs_ellipsis) = if config.expanded_day == Some(self.date) {
                    // Day is expanded - show all transaction chips, no ellipsis needed
                    (chips.len(), false)
                } else {
                    // Normal calculation based on available space
                    calculate_transaction_display_limit(height, chips.len())
                };
                
                let chips_to_show = chips.iter().take(chips_to_show_count);
                
                let mut local_clicked_ids = Vec::new();
                for chip in chips_to_show {
                    if let Some(transaction_id) = self.render_calendar_chip(ui, chip, width - 8.0, height, config) {
                        local_clicked_ids.push(transaction_id);
                    }
                    ui.add_space(1.0); // Smaller spacing between chips due to padding
                }
                
                // Show ellipsis chip for normal state (not expanded)
                if !config.expanded_day.map_or(false, |expanded_date| expanded_date == self.date) && needs_ellipsis {
                    let ellipsis_chip = CalendarChip::create_ellipsis();
                    if let Some(result) = self.render_calendar_chip(ui, &ellipsis_chip, width - 8.0, height, config) {
                        if result == "ELLIPSIS_CLICKED" {
                            local_clicked_ids.push("ELLIPSIS_CLICKED".to_string());
                        }
                    }
                }
                
                local_clicked_ids
            })
        });
        
        // Render collapse button OUTSIDE content flow if day is expanded
        if config.expanded_day == Some(self.date) {
            let collapse_height = 22.0;
            let collapse_rect = egui::Rect::from_min_size(
                egui::pos2(cell_rect.left(), cell_rect.bottom() - collapse_height),
                egui::vec2(cell_rect.width(), collapse_height)
            );
            
            // Check for hover and click
            let collapse_response = ui.allocate_rect(collapse_rect, egui::Sense::hover().union(egui::Sense::click()));
            
            // Style as solid white bar (no border)
            let collapse_bg_color = if collapse_response.hovered() {
                egui::Color32::from_rgba_unmultiplied(245, 245, 245, 255) // Very light gray on hover
            } else {
                egui::Color32::WHITE // Solid white
            };
            
            // Draw collapse button background - no rounding for perfect border alignment
            ui.painter().rect_filled(
                collapse_rect,
                egui::Rounding::ZERO, // No rounding for perfect border alignment
                collapse_bg_color
            );
            
            // Draw triangle symbol using painter (more reliable than Unicode)
            let triangle_size = 8.0;
            let center = collapse_rect.center();
            
            // Define triangle points (pointing up for "collapse")
            let triangle_points = [
                egui::pos2(center.x, center.y - triangle_size / 2.0), // Top point
                egui::pos2(center.x - triangle_size / 2.0, center.y + triangle_size / 2.0), // Bottom left
                egui::pos2(center.x + triangle_size / 2.0, center.y + triangle_size / 2.0), // Bottom right
            ];
            
            // Draw filled triangle
            ui.painter().add(egui::Shape::convex_polygon(
                triangle_points.to_vec(),
                egui::Color32::from_rgb(120, 120, 120), // Medium gray for visibility
                egui::Stroke::NONE,
            ));
            
            // Handle collapse click
            if collapse_response.clicked() {
                clicked_transaction_ids.push("COLLAPSE_CLICKED".to_string());
            }
        }
        
        // Extract clicked transaction IDs from UI result
        clicked_transaction_ids.extend(ui_result.inner.inner);
        
        // Return the response for click handling by the caller and clicked transaction IDs
        (response, clicked_transaction_ids)
    }
    
    /// Render a single calendar chip with unified styling and hover effects
    /// Returns the transaction ID if the checkbox was clicked (for selection toggle)
    /// Returns "ELLIPSIS_CLICKED" if the ellipsis chip was clicked (for expansion toggle)
    /// Returns "COLLAPSE_CLICKED" if the collapse button was clicked (handled separately)
    fn render_calendar_chip(&self, ui: &mut egui::Ui, chip: &CalendarChip, width: f32, _height: f32, config: &RenderConfig) -> Option<String> {
        
        // Get the font family for calendar rendering
        let font_family = get_calendar_font_family(ui.ctx());
        
        // Get chip styling from the chip type
        let chip_color = chip.chip_type.primary_color();
        let text_color = chip.chip_type.text_color();
        let uses_dotted_border = chip.chip_type.uses_dotted_border();
        
        // Calculate chip dimensions based on layout
        let (chip_width, chip_height, chip_font_size) = calculate_chip_dimensions(config.is_grid_layout, width);
        
        // Check if we should show checkbox (only for deletable transactions in selection mode)
        let show_checkbox = config.transaction_selection_mode && 
                            !matches!(chip.chip_type, CalendarChipType::FutureAllowance);
        let checkbox_width = if show_checkbox { 16.0 } else { 0.0 };
        let checkbox_spacing = if show_checkbox { 4.0 } else { 0.0 };
        
        // Adjust chip width to accommodate checkbox
        let adjusted_chip_width = if show_checkbox {
            chip_width - checkbox_width - checkbox_spacing
        } else {
            chip_width
        };
        
        // Opaque white background for all transaction chips
        let chip_background = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255);
        
        let mut checkbox_clicked = None;
        
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            if show_checkbox {
                // Horizontal layout with checkbox and chip
                ui.horizontal(|ui| {
                    // Checkbox on the left
                    let is_selected = config.selected_transaction_ids.contains(&chip.transaction.id);
                    let checkbox_response = ui.add_sized(
                        [checkbox_width, checkbox_width],
                        egui::Checkbox::new(&mut is_selected.clone(), "")
                    );
                    
                    if checkbox_response.clicked() {
                        checkbox_clicked = Some(chip.transaction.id.clone());
                    }
                    
                    ui.add_space(checkbox_spacing);
                    
                    // Chip on the right
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(adjusted_chip_width, chip_height), egui::Sense::hover());
                    
                    // Determine if we should show hover effect
                    let is_hovered = response.hovered();
                    
                    // Background color - slightly darker when hovered
                    let background_color = if is_hovered {
                        egui::Color32::from_rgba_unmultiplied(245, 245, 245, 255) // Light gray on hover
                    } else {
                        chip_background
                    };
                    
                    // Draw background
                    ui.painter().rect_filled(
                        rect,
                        egui::Rounding::same(4.0),
                        background_color
                    );
                    
                    // Draw border - solid or dotted based on chip type
                    if uses_dotted_border {
                        self.draw_dotted_border(ui, rect, chip_color);
                    } else {
                        // Draw solid border
                        ui.painter().rect_stroke(
                            rect,
                            egui::Rounding::same(4.0),
                            egui::Stroke::new(1.0, chip_color)
                        );
                    }
                    
                    // Draw text
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &chip.display_amount,
                        egui::FontId::new(chip_font_size, font_family.clone()),
                        text_color,
                    );
                    
                    // Show floating tooltip when hovering
                    if is_hovered && !chip.transaction.description.is_empty() {
                        self.show_transaction_tooltip(ui, &chip.transaction.description, rect);
                    }
                });
            } else {
                // Original chip rendering without checkbox
                let sense = if matches!(chip.chip_type, CalendarChipType::Ellipsis) {
                    egui::Sense::hover().union(egui::Sense::click()) // Ellipsis chips are clickable
                } else {
                    egui::Sense::hover() // Regular chips only hover
                };
                let (rect, response) = ui.allocate_exact_size(egui::vec2(chip_width, chip_height), sense);
                
                // Determine if we should show hover effect
                let is_hovered = response.hovered();
                
                // Check for ellipsis click
                if response.clicked() && matches!(chip.chip_type, CalendarChipType::Ellipsis) {
                    checkbox_clicked = Some("ELLIPSIS_CLICKED".to_string());
                }
                
                // Background color - slightly darker when hovered
                let background_color = if is_hovered {
                    egui::Color32::from_rgba_unmultiplied(245, 245, 245, 255) // Light gray on hover
                } else {
                    chip_background
                };
                
                // Draw background
                ui.painter().rect_filled(
                    rect,
                    egui::Rounding::same(4.0),
                    background_color
                );
                
                // Draw border - solid or dotted based on chip type
                if uses_dotted_border {
                    self.draw_dotted_border(ui, rect, chip_color);
                } else {
                    // Draw solid border
                    ui.painter().rect_stroke(
                        rect,
                        egui::Rounding::same(4.0),
                        egui::Stroke::new(1.0, chip_color)
                    );
                }
                
                // Draw text
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &chip.display_amount,
                    egui::FontId::new(chip_font_size, font_family.clone()),
                    text_color,
                );
                
                // Show floating tooltip when hovering
                if is_hovered && !chip.transaction.description.is_empty() {
                    self.show_transaction_tooltip(ui, &chip.transaction.description, rect);
                }
            }
        });
        
        checkbox_clicked
    }
    
    /// Show a floating tooltip with transaction description
    fn show_transaction_tooltip(&self, ui: &mut egui::Ui, description: &str, chip_rect: egui::Rect) {
        // Get the font family for calendar rendering
        let font_family = get_calendar_font_family(ui.ctx());
        
        // Get cursor position for tooltip positioning
        let cursor_pos = ui.ctx().pointer_interact_pos().unwrap_or(chip_rect.center());
        
        // Calculate tooltip dimensions (estimate based on text length)
        let tooltip_font_size = tooltip::FONT_SIZE;
        let tooltip_padding = tooltip::PADDING;
        let max_tooltip_width = tooltip::MAX_WIDTH;
        
        // Estimate tooltip size (rough approximation)
        let char_width = tooltip_font_size * 0.6; // Approximate character width
        let text_width = (description.len() as f32 * char_width).min(max_tooltip_width);
        let text_height = tooltip_font_size * 1.2; // Line height
        let tooltip_size = egui::vec2(text_width + tooltip_padding.x * 2.0, text_height + tooltip_padding.y * 2.0);
        
        // Smart positioning: offset from cursor, but avoid screen boundaries
        let default_offset = tooltip::DEFAULT_OFFSET; // Right and up from cursor
        let screen_rect = ui.ctx().screen_rect();
        
        // Calculate initial position
        let mut tooltip_pos = cursor_pos + default_offset;
        
        // Adjust if tooltip would go off-screen
        // Check right boundary
        if tooltip_pos.x + tooltip_size.x > screen_rect.right() {
            tooltip_pos.x = cursor_pos.x - tooltip_size.x - 10.0; // Show to the left instead
        }
        
        // Check left boundary
        if tooltip_pos.x < screen_rect.left() {
            tooltip_pos.x = screen_rect.left() + 5.0; // Keep some margin
        }
        
        // Check top boundary
        if tooltip_pos.y < screen_rect.top() {
            tooltip_pos.y = cursor_pos.y + 25.0; // Show below cursor instead
        }
        
        // Check bottom boundary
        if tooltip_pos.y + tooltip_size.y > screen_rect.bottom() {
            tooltip_pos.y = cursor_pos.y - tooltip_size.y - 10.0; // Show above cursor
        }
        
        // Create tooltip using egui::Area for precise positioning
        egui::Area::new("transaction_tooltip".into())
            .fixed_pos(tooltip_pos)
            .order(egui::Order::Foreground) // Show above everything else
            .show(ui.ctx(), |ui| {
                // Tooltip styling
                let tooltip_bg_color = tooltip::background_color();
                let tooltip_text_color = tooltip::text_color();
                let tooltip_border_color = tooltip::border_color();
                
                // Draw tooltip background with rounded corners
                let tooltip_rect = egui::Rect::from_min_size(ui.cursor().min, tooltip_size);
                
                // Draw shadow first (slightly offset)
                let shadow_rect = tooltip_rect.translate(egui::vec2(1.0, 1.0));
                ui.painter().rect_filled(
                    shadow_rect,
                    egui::Rounding::same(6.0),
                    tooltip::shadow_color()
                );
                
                // Draw main tooltip background
                ui.painter().rect_filled(
                    tooltip_rect,
                    egui::Rounding::same(6.0),
                    tooltip_bg_color
                );
                
                // Draw border
                ui.painter().rect_stroke(
                    tooltip_rect,
                    egui::Rounding::same(6.0),
                    egui::Stroke::new(1.0, tooltip_border_color)
                );
                
                // Add padding and render text
                ui.allocate_ui_with_layout(
                    tooltip_size,
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.add_space(tooltip_padding.y);
                        ui.horizontal(|ui| {
                            ui.add_space(tooltip_padding.x);
                            ui.add(egui::Label::new(
                                egui::RichText::new(description)
                                    .font(egui::FontId::new(tooltip_font_size, font_family.clone()))
                                    .color(tooltip_text_color)
                            ).selectable(false)); // Disable selection to prevent dropdown interference
                        });
                    }
                );
            });
    }
    
    /// Draw a dotted border around a rectangle for future allowance chips
    fn draw_dotted_border(&self, ui: &mut egui::Ui, rect: egui::Rect, color: egui::Color32) {
        let painter = ui.painter();
        let rounding = 4.0;
        let dash_length = 3.0;
        let gap_length = 2.0;
        let stroke_width = 1.0;
        
        // Top border
        let mut x = rect.left() + rounding;
        let y = rect.top();
        while x < rect.right() - rounding {
            let end_x = (x + dash_length).min(rect.right() - rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(end_x, y)],
                egui::Stroke::new(stroke_width, color)
            );
            x = end_x + gap_length;
        }
        
        // Right border
        let mut y = rect.top() + rounding;
        let x = rect.right();
        while y < rect.bottom() - rounding {
            let end_y = (y + dash_length).min(rect.bottom() - rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, end_y)],
                egui::Stroke::new(stroke_width, color)
            );
            y = end_y + gap_length;
        }
        
        // Bottom border
        let mut x = rect.right() - rounding;
        let y = rect.bottom();
        while x > rect.left() + rounding {
            let end_x = (x - dash_length).max(rect.left() + rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(end_x, y)],
                egui::Stroke::new(stroke_width, color)
            );
            x = end_x - gap_length;
        }
        
        // Left border
        let mut y = rect.bottom() - rounding;
        let x = rect.left();
        while y > rect.top() + rounding {
            let end_y = (y - dash_length).max(rect.top() + rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, end_y)],
                egui::Stroke::new(stroke_width, color)
            );
            y = end_y - gap_length;
        }
        
        // Draw rounded corners as small arcs (simplified as short lines)
        let corner_dash = 2.0;
        
        // Top-left corner
        painter.line_segment(
            [egui::pos2(rect.left() + rounding - corner_dash, rect.top()), 
             egui::pos2(rect.left() + rounding, rect.top())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.left(), rect.top() + rounding - corner_dash), 
             egui::pos2(rect.left(), rect.top() + rounding)],
            egui::Stroke::new(stroke_width, color)
        );
        
        // Top-right corner
        painter.line_segment(
            [egui::pos2(rect.right() - rounding, rect.top()), 
             egui::pos2(rect.right() - rounding + corner_dash, rect.top())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.right(), rect.top() + rounding - corner_dash), 
             egui::pos2(rect.right(), rect.top() + rounding)],
            egui::Stroke::new(stroke_width, color)
        );
        
        // Bottom-right corner
        painter.line_segment(
            [egui::pos2(rect.right() - rounding + corner_dash, rect.bottom()), 
             egui::pos2(rect.right() - rounding, rect.bottom())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.right(), rect.bottom() - rounding), 
             egui::pos2(rect.right(), rect.bottom() - rounding + corner_dash)],
            egui::Stroke::new(stroke_width, color)
        );
        
        // Bottom-left corner
        painter.line_segment(
            [egui::pos2(rect.left() + rounding, rect.bottom()), 
             egui::pos2(rect.left() + rounding - corner_dash, rect.bottom())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.left(), rect.bottom() - rounding + corner_dash), 
             egui::pos2(rect.left(), rect.bottom() - rounding)],
            egui::Stroke::new(stroke_width, color)
        );
    }
    

}

impl AllowanceTrackerApp {

    

    
    /// Calculate running balances for all days in the month, carrying forward balances
    /// from previous days when there are no transactions


    /// Draw calendar section with toggle header integrated
    pub fn draw_calendar_section_with_toggle(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        // DEBUG: Log calendar entry point
        // Calendar rendering with responsive layout
        
        // Get the font family for calendar rendering
        let font_family = get_calendar_font_family(ui.ctx());
        
        // Use the existing draw_calendar_section method but with toggle header
        ui.add_space(15.0);
        // Add top spacing for visual separation
        
        // Calculate responsive dimensions - same as original
        let content_width = available_rect.width() - 40.0;
        
        // Calendar takes up full available width to align with navigation buttons
        let calendar_width = content_width;
        
        // HYPOTHESIS A: Break the coupling chain at multiple points
        let total_spacing = CALENDAR_CARD_SPACING * 6.0;
        
        // Step 1: Calculate cell width from horizontal space (unchanged - works fine)
        let cell_width = (calendar_width - total_spacing) / 7.0;
        
        // Step 2: Apply TEST RECTANGLE SUCCESS FORMULA 
        // Use simple approach that worked perfectly: available_height - 40px margins
        let actual_available_rect = ui.available_rect_before_wrap();
                // Calculate optimal calendar dimensions
        
        // 🎯 CORRECT CALCULATION: Subtract larger bottom margin to match side margins
        let final_card_height = actual_available_rect.height() - 40.0; // 40px total: 20px bottom margin + 20px internal padding
        
        // Calculate dynamic cell height based on calendar data
        let header_height = header::HEADER_HEIGHT;
        let calendar_container_padding = 20.0;
        
        // Get calendar data to determine row count
        let calendar_days_count = if let Some(ref calendar_month) = self.calendar.calendar_month {
            calendar_month.days.len()
        } else {
            35 // Default fallback
        };
        
        let rows_needed = (calendar_days_count as f32 / 7.0).ceil();
        let vertical_spacing = CALENDAR_CARD_SPACING * (rows_needed - 1.0);
        let available_height_for_cells = final_card_height - calendar_container_padding - header_height - vertical_spacing;
        let dynamic_cell_height = (available_height_for_cells / rows_needed).max(40.0).min(200.0);
        
        // Dynamic cell height calculation complete
        

        
        let card_rect = egui::Rect::from_min_size(
            egui::pos2(actual_available_rect.left() + 20.0, actual_available_rect.top() + 20.0),
            egui::vec2(content_width, final_card_height)
        );
        
        // Calendar container positioned with consistent margins
        
        // Draw calendar content (no background card)
        ui.allocate_ui_at_rect(card_rect, |ui| {
            ui.vertical(|ui| {
                // Align the calendar content to the left to match navigation buttons
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        // Constrain calendar to calculated dimensions  
                                                        // Render calendar with optimal sizing
                        ui.allocate_ui_with_layout(
                            egui::vec2(calendar_width, final_card_height),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                // Day headers - consistent layout with automatic spacing matching day cards
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = CALENDAR_CARD_SPACING; // Match day cards spacing
                                    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
                                    for day_name in day_names.iter() {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(cell_width, header_height),
                                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                            |ui| {
                                                // Get the rect for this header
                                                let header_rect = ui.available_rect_before_wrap();
                                                
                                                // Draw card-like background
                                                let bg_color = header::background_color();
                                                ui.painter().rect_filled(
                                                    header_rect,
                                                    egui::Rounding::same(2.0),
                                                    bg_color
                                                );
                                                
                                                // Draw border
                                                let border_color = header::border_color();
                                                ui.painter().rect_stroke(
                                                    header_rect,
                                                    egui::Rounding::same(2.0),
                                                    egui::Stroke::new(1.0, border_color)
                                                );
                                                
                                                // Draw text - disable selection to prevent dropdown interference
                                                ui.add(egui::Label::new(egui::RichText::new(*day_name)
                                                    .font(egui::FontId::new(header::HEADER_FONT_SIZE, font_family.clone()))
                                                    .strong()
                                                    .color(egui::Color32::DARK_GRAY))
                                                    .selectable(false));
                                            },
                                        );
                                        
                                        // No manual spacing - using automatic spacing system like day cards
                                    }
                                });
                                
                                ui.add_space(5.0); // Small gap between headers and calendar
                                
                                // Calendar days - use corrected manual method with dynamic cell height
                                self.draw_calendar_days_responsive(ui, transactions, cell_width, dynamic_cell_height);
                            }
                        );
                    }
                );
            });
        });
    }
    

    

    
    /// Draw calendar days with responsive sizing using CalendarDay components
    pub fn draw_calendar_days_responsive(&mut self, ui: &mut egui::Ui, _transactions: &[Transaction], cell_width: f32, cell_height: f32) {
        ui.spacing_mut().item_spacing.y = CALENDAR_CARD_SPACING; // Vertical spacing between week rows
        // Use calendar month data from backend (which includes balance data)
        let all_days: Vec<CalendarDay> = if let Some(ref calendar_month) = self.calendar.calendar_month {
            // Convert backend calendar days to frontend calendar days
            calendar_month.days.iter()
                .enumerate()
                .map(|(index, day)| self.convert_backend_calendar_day(day, index))
                .collect()
        } else {
            // No calendar data available - return empty calendar
            println!("⚠️ No calendar month data available for {}/{}", self.calendar.selected_month, self.calendar.selected_year);
            Vec::new()
        };
        
        // Backend calendar data includes complete grid with filler days
        
        // Dynamic cell height already calculated in parent function
        
        // Render the calendar grid (dynamic weeks based on month needs) in proper row layout  
        // Process days in chunks of 7 (one week per row)
        let mut selected_day_rect: Option<egui::Rect> = None;
        let mut selected_day_date: Option<NaiveDate> = None;
        for (_week_index, week_days) in all_days.chunks(7).enumerate() {
            // Calculate row height - use expanded height if any day in this row is expanded
            let row_height = if let Some(expanded_day) = week_days.iter().find(|day| self.calendar.expanded_day == Some(day.date)) {
                // Calculate exact height needed for all chips in the expanded day
                let chip_count = expanded_day.transactions.len();
                let base_height = cell_height;
                let header_space = 45.0; // Day number + balance + spacing
                let bottom_padding = 10.0;
                let collapse_button_height = 25.0; // Space for collapse button
                let chip_height = 18.0;
                let chip_spacing = 1.0;
                
                let chips_height = if chip_count > 0 {
                    (chip_count as f32 * chip_height) + ((chip_count - 1) as f32 * chip_spacing)
                } else {
                    0.0
                };
                
                let needed_height = header_space + chips_height + collapse_button_height + bottom_padding;
                needed_height.max(base_height) // Don't go smaller than normal
            } else {
                cell_height // Normal height
            };
            
            let _week_response = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = CALENDAR_CARD_SPACING; // Horizontal spacing between day cards
                for calendar_day in week_days.iter() {
                    let ui_response = ui.allocate_ui_with_layout(
                        egui::vec2(cell_width, row_height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            // Check if this day is selected
                            let is_selected = self.calendar.selected_day == Some(calendar_day.date);
                            
                            let (response, clicked_transaction_ids) = calendar_day.render_with_config(ui, cell_width, row_height, &RenderConfig {
                                is_grid_layout: true,
                                enable_click_handler: true,
                                is_selected,
                                transaction_selection_mode: self.interaction.transaction_selection_mode,
                                selected_transaction_ids: self.interaction.selected_transaction_ids.clone(),
                                expanded_day: self.calendar.expanded_day,
                            });
                            
                            // Handle checkbox clicks on transactions, ellipsis clicks, and collapse clicks
                            for transaction_id in clicked_transaction_ids {
                                if transaction_id == "ELLIPSIS_CLICKED" {
                                    // Toggle expansion for this day
                                    if self.calendar.expanded_day == Some(calendar_day.date) {
                                        self.calendar.expanded_day = None; // Collapse if already expanded
                                    } else {
                                        self.calendar.expanded_day = Some(calendar_day.date); // Expand this day
                                    }
                                } else if transaction_id == "COLLAPSE_CLICKED" {
                                    // Collapse the expanded day
                                    self.calendar.expanded_day = None;
                                } else {
                                    self.toggle_transaction_selection(&transaction_id);
                                }
                            }
                            
                            // Handle click detection for current month days only
                            if response.clicked() && matches!(calendar_day.day_type, CalendarDayType::CurrentMonth) {
                                self.handle_calendar_day_click(calendar_day.date);
                            }
                            
                            // Return whether this day is selected for later rect capture
                            is_selected && matches!(calendar_day.day_type, CalendarDayType::CurrentMonth)
                        },
                    );
                    
                    // Store the selected day's rect for icon rendering
                    if ui_response.inner {
                        selected_day_rect = Some(ui_response.response.rect);
                        selected_day_date = Some(calendar_day.date);
                    }
                    
                    // No manual spacing between day cells - using egui spacing control instead
                }
            });
            
            // No vertical spacing between week rows
        }
        
        // Calendar grid rendering complete
        
        // Render action icons above the selected day if one is selected
        if let (Some(day_rect), Some(day_date)) = (selected_day_rect, selected_day_date) {
            self.render_day_action_icons(ui, day_rect, day_date);
        }
    }
    

    



    

} 