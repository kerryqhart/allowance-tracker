use eframe::egui;
use shared::*;

/// Render the transaction table (simplified version)
pub fn render_transaction_table(ui: &mut egui::Ui, transactions: &[Transaction]) {
    // Use the responsive version with a default rectangle
    let available_rect = ui.available_rect_before_wrap();
    render_responsive_transaction_table(ui, available_rect, transactions);
}

/// Render responsive transaction table with calendar-style transparent styling
pub fn render_responsive_transaction_table(ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
    if transactions.is_empty() {
        ui.label("No transactions yet!");
        return;
    }

    // Responsive approach: size everything as percentages of available space
    let content_width = available_rect.width() - 40.0; // Leave some margin
    let table_padding = 15.0; // Match calendar padding
    let available_table_width = content_width - (table_padding * 2.0);
    
    // Table takes up 92% of available width, centered (matching calendar approach)
    let table_width = available_table_width * 0.92;
    
    // Calculate column widths to account for scroll bar (shared by headers and rows)
    let scroll_bar_space = 30.0; // Space for scroll bar
    let content_width_with_scroll = table_width - scroll_bar_space;
    
    // Calendar-style dimensions
    let header_height = 40.0; // Similar to calendar day headers
    let row_height = 25.0; // Compact row height
    let row_spacing = 0.0; // No space between rows (very tight)
    let _num_rows = transactions.len().min(25); // Show more rows since we have expanded space
    
    // Use all available space - no height restrictions or borders
    let final_height = available_rect.height() - 20.0; // Small margin to prevent edge touching
    
    let table_rect = egui::Rect::from_min_size(
        egui::pos2(available_rect.left() + 20.0, available_rect.top() + 10.0),
        egui::vec2(content_width, final_height)
    );
    
    // Draw table content using full available area
    ui.allocate_ui_at_rect(table_rect, |ui| {
        ui.vertical(|ui| {
            ui.add_space(8.0); // Reduced spacing since no title
            
            // Use the same default font as calendar (system proportional font)
            let font_family = egui::FontFamily::Proportional;
            
            // Responsive font sizes
            let header_font_size = (table_width * 0.025).max(16.0).min(20.0);
            let content_font_size = (table_width * 0.020).max(12.0).min(16.0);
            
            // Center the table content (matching calendar approach)
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    // Constrain table to calculated width
                    ui.allocate_ui_with_layout(
                        egui::vec2(table_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            // Clean minimal headers (like calendar day-of-week headers)
                            ui.horizontal(|ui| {
                                // Fine-tuned header column spacing
                                ui.spacing_mut().item_spacing.x = 6.5; // Reduced from 7.5px
                                // Fine-tuned vertical spacing to make rows tighter
                                ui.spacing_mut().item_spacing.y = 0.5; // Reduced from 1.0px
                                
                                let header_names = ["DATE", "DESCRIPTION", "AMOUNT", "BALANCE"];
                                // Use reduced width to match row constraints and avoid scroll bar overlap
                                let content_width_minus_scrollbar = content_width_with_scroll - 20.0;
                                let header_widths = [
                                    content_width_minus_scrollbar * 0.18, // date (reduced from 0.20)
                                    content_width_minus_scrollbar * 0.48, // description (increased from 0.40)
                                    content_width_minus_scrollbar * 0.17, // amount (reduced from 0.20)
                                    content_width_minus_scrollbar * 0.17, // balance (reduced from 0.20)
                                ];
                                
                                for (name, width) in header_names.iter().zip(header_widths.iter()) {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(*width, header_height),
                                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                        |ui| {
                                            let header_rect = ui.available_rect_before_wrap();
                                            
                                            // Draw calendar-style header background (like day-of-week)
                                            let bg_color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180);
                                            ui.painter().rect_filled(
                                                header_rect,
                                                egui::Rounding::same(0.0), // No rounding for clean look
                                                bg_color
                                            );
                                            
                                            // Draw border
                                            let border_color = egui::Color32::from_rgba_unmultiplied(150, 150, 150, 200);
                                            ui.painter().rect_stroke(
                                                header_rect,
                                                egui::Rounding::same(0.0), // No rounding for clean look
                                                egui::Stroke::new(1.0, border_color)
                                            );
                                            
                                            // Draw header text
                                            ui.add(egui::Label::new(egui::RichText::new(*name)
                                                .font(egui::FontId::new(header_font_size, font_family.clone()))
                                                .strong()
                                                .color(egui::Color32::DARK_GRAY))
                                                .selectable(false));
                                        },
                                    );
                                    
                                    // No manual spacing - let item_spacing.x handle it
                                }
                            });
                            
                            ui.add_space(row_spacing); // Space after headers
                            
                            // Transaction data with hover detection
                            // Control spacing between transaction rows (vertical spacing)
                            ui.spacing_mut().item_spacing.y = 1.5; // Fine-tuned row spacing (+1px from 0.5px)
                            
                            // Transaction rows (like calendar day cards)
                            for transaction in transactions.iter().take(15) {
                                // 🎯 BUTTON APPROACH: Use egui's built-in button hover instead of manual detection
                                let button_response = ui.add_sized(
                                    [content_width_with_scroll - 20.0, row_height],
                                    egui::Button::new("")
                                        .fill(egui::Color32::TRANSPARENT) // Invisible until hovered
                                        .stroke(egui::Stroke::NONE) // No border
                                        .rounding(egui::Rounding::same(2.0)) // Slight rounding like calendar
                                );
                                
                                // Check if button is hovered for styling
                                let is_hovered = button_response.hovered();
                                
                                // Draw the row content ON TOP of the button
                                let button_rect = button_response.rect;
                                ui.allocate_ui_at_rect(button_rect, |ui| {
                                    ui.horizontal(|ui| {
                                        // Fine-tuned data column spacing
                                        ui.spacing_mut().item_spacing.x = 6.5; // Reduced from 7.5px
                                        // Fine-tuned vertical spacing to make rows tighter
                                        ui.spacing_mut().item_spacing.y = 0.5; // Reduced from 1.0px
                                        
                                        // Use reduced width to match content constraint and avoid scroll bar overlap
                                        let content_width_minus_scrollbar = content_width_with_scroll - 20.0;
                                        let row_widths = [
                                            content_width_minus_scrollbar * 0.18, // date (reduced from 0.20)
                                            content_width_minus_scrollbar * 0.48, // description (increased from 0.40)
                                            content_width_minus_scrollbar * 0.17, // amount (reduced from 0.20)
                                            content_width_minus_scrollbar * 0.17, // balance (reduced from 0.20)
                                        ];
                                        
                                        let cell_bg_color = if is_hovered {
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 120) // More opaque when hovered
                                        } else {
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 55) // Normal transparency
                                        };
                                        let cell_border_color = egui::Color32::from_rgba_unmultiplied(200, 200, 200, 100);
                                        
                                        // Render cells with exact same spacing as headers
                                        for (i, &width) in row_widths.iter().enumerate() {
                                            match i {
                                                0 => {
                                                    // Date cell
                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(width, row_height),
                                                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                                        |ui| {
                                                            let cell_rect = ui.available_rect_before_wrap();
                                                            
                                                            // Draw calendar-style cell background
                                                            ui.painter().rect_filled(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                cell_bg_color
                                                            );
                                                            
                                                            // Draw cell border (minimal)
                                                            ui.painter().rect_stroke(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                egui::Stroke::new(0.0, cell_border_color) // No border
                                                            );
                                                            
                                                            let date_display = transaction.date.format("%b %d, %Y").to_string();
                                                            ui.add(egui::Label::new(egui::RichText::new(date_display)
                                                                .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                                .strong()
                                                                .color(egui::Color32::BLACK))
                                                                .selectable(false)); // Non-interactive
                                                        },
                                                    );
                                                },
                                                1 => {
                                                    // Description cell
                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(width, row_height),
                                                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                                        |ui| {
                                                            let cell_rect = ui.available_rect_before_wrap();
                                                            
                                                            // Draw calendar-style cell background
                                                            ui.painter().rect_filled(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                cell_bg_color
                                                            );
                                                            
                                                            // Draw cell border (minimal)
                                                            ui.painter().rect_stroke(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                egui::Stroke::new(0.0, cell_border_color) // No border
                                                            );
                                                            
                                                            ui.add(egui::Label::new(egui::RichText::new(&transaction.description)
                                                                .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                                .color(egui::Color32::BLACK))
                                                                .selectable(false)); // Non-interactive
                                                        },
                                                    );
                                                },
                                                2 => {
                                                    // Amount cell  
                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(width, row_height),
                                                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                                        |ui| {
                                                            let cell_rect = ui.available_rect_before_wrap();
                                                            
                                                            // Draw calendar-style cell background
                                                            ui.painter().rect_filled(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                cell_bg_color
                                                            );
                                                            
                                                            // Draw cell border (minimal)  
                                                            ui.painter().rect_stroke(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                egui::Stroke::new(0.0, cell_border_color) // No border
                                                            );
                                                            
                                                            // Color-code based on amount (keeping existing logic)
                                                            let amount_color = if transaction.amount > 0.0 {
                                                                egui::Color32::from_rgb(34, 139, 34) // Forest green for positive
                                                            } else {
                                                                egui::Color32::from_rgb(220, 20, 60) // Crimson for negative
                                                            };
                                                            
                                                            ui.add(egui::Label::new(egui::RichText::new(format!("${:.2}", transaction.amount))
                                                                .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                                .strong()
                                                                .color(amount_color))
                                                                .selectable(false)); // Non-interactive
                                                        },
                                                    );
                                                },
                                                3 => {
                                                    // Balance cell
                                                    ui.allocate_ui_with_layout(
                                                        egui::vec2(width, row_height),
                                                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                                        |ui| {
                                                            let cell_rect = ui.available_rect_before_wrap();
                                                            
                                                            // Draw calendar-style cell background
                                                            ui.painter().rect_filled(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                cell_bg_color
                                                            );
                                                            
                                                            // Draw cell border (minimal)
                                                            ui.painter().rect_stroke(
                                                                cell_rect,
                                                                egui::Rounding::same(0.0), // No rounding
                                                                egui::Stroke::new(0.0, cell_border_color) // No border
                                                            );
                                                            
                                                            ui.add(egui::Label::new(egui::RichText::new(format!("${:.2}", transaction.balance))
                                                                    .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                                    .strong()
                                                                    .color(egui::Color32::BLACK))
                                                                    .selectable(false)); // Non-interactive
                                                        },
                                                    );
                                                },
                                                _ => {} // No more columns
                                            }
                                            
                                            // No manual spacing - let item_spacing.x handle it
                                        }
                                    }); // End horizontal layout
                                }); // End allocate_ui_at_rect
                        
                                ui.add_space(row_spacing); // Space between rows
                            }
                        }
                    );
                }
            );
        });
    });
} 