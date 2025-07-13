use eframe::egui;
use egui_extras::{TableBuilder, Column};
use shared::*;
use crate::ui::components::styling::{draw_card_container, get_table_header_color};

/// Render the transaction table
pub fn render_transaction_table(ui: &mut egui::Ui, transactions: &[Transaction]) {
    if transactions.is_empty() {
        ui.label("No transactions yet!");
        return;
    }

    // Check if Chalkboard font is available (outside of table closures)
    let font_family = if ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("Chalkboard".into()))) {
        egui::FontFamily::Name("Chalkboard".into())
    } else {
        egui::FontFamily::Proportional
    };
    
    // Create a kid-friendly table using proper TableBuilder
    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(150.0))  // DATE column
        .column(Column::exact(200.0))  // DESCRIPTION column  
        .column(Column::exact(100.0))  // AMOUNT column
        .column(Column::exact(100.0))  // BALANCE column
        .header(60.0, |mut header| {
            // Date header with gradient bubble
            header.col(|ui| {
                let button = egui::Button::new(
                    egui::RichText::new("DATE")
                        .font(egui::FontId::new(20.0, font_family.clone()))
                        .color(egui::Color32::WHITE)
                        .strong()
                )
                .fill(get_table_header_color(0))
                .rounding(egui::Rounding::same(8.0));
                
                ui.add_sized([150.0, 60.0], button);
            });
            
            // Description header with gradient bubble
            header.col(|ui| {
                let button = egui::Button::new(
                    egui::RichText::new("DESCRIPTION")
                        .font(egui::FontId::new(20.0, font_family.clone()))
                        .color(egui::Color32::WHITE)
                        .strong()
                )
                .fill(get_table_header_color(1))
                .rounding(egui::Rounding::same(8.0));
                
                ui.add_sized([200.0, 60.0], button);
            });
            
            // Amount header with gradient bubble
            header.col(|ui| {
                let button = egui::Button::new(
                    egui::RichText::new("AMOUNT")
                        .font(egui::FontId::new(20.0, font_family.clone()))
                        .color(egui::Color32::WHITE)
                        .strong()
                )
                .fill(get_table_header_color(2))
                .rounding(egui::Rounding::same(8.0));
                
                ui.add_sized([100.0, 60.0], button);
            });
            
            // Balance header with gradient bubble
            header.col(|ui| {
                let button = egui::Button::new(
                    egui::RichText::new("BALANCE")
                        .font(egui::FontId::new(20.0, font_family.clone()))
                        .color(egui::Color32::WHITE)
                        .strong()
                )
                .fill(get_table_header_color(3))
                .rounding(egui::Rounding::same(8.0));
                
                ui.add_sized([100.0, 60.0], button);
            });
        })
        .body(|mut body| {
            for transaction in transactions {
                body.row(45.0, |mut row| {
                    // Date column (formatted with full month name)
                    row.col(|ui| {
                        let date_str = if let Some(date_part) = transaction.date.split('T').next() {
                            // Parse and format date with full month name
                            if let Ok(parsed_date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                                parsed_date.format("%B %d, %Y").to_string()  // Full month name
                            } else {
                                date_part.to_string()
                            }
                        } else {
                            "Unknown".to_string()
                        };
                        
                        // Add vertical centering and padding
                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(date_str)
                                .font(egui::FontId::new(14.0, font_family.clone()))
                                .strong());
                            ui.add_space(8.0);
                        });
                    });
                    
                    // Description column with bolder text
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(&transaction.description)
                                .font(egui::FontId::new(14.0, font_family.clone()))
                                .strong());
                            ui.add_space(8.0);
                        });
                    });
                    
                    // Amount column with color coding and bold text
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                            ui.add_space(8.0);
                            if transaction.amount >= 0.0 {
                                ui.colored_label(
                                    egui::Color32::from_rgb(34, 139, 34), // Green for positive
                                    egui::RichText::new(format!("+${:.2}", transaction.amount))
                                        .font(egui::FontId::new(14.0, font_family.clone()))
                                        .strong()
                                );
                            } else {
                                ui.colored_label(
                                    egui::Color32::from_rgb(220, 20, 60), // Red for negative
                                    egui::RichText::new(format!("-${:.2}", transaction.amount.abs()))
                                        .font(egui::FontId::new(14.0, font_family.clone()))
                                        .strong()
                                );
                            }
                            ui.add_space(8.0);
                        });
                    });
                    
                    // Balance column with bold text
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(format!("${:.2}", transaction.balance))
                                .font(egui::FontId::new(14.0, font_family.clone()))
                                .strong());
                            ui.add_space(8.0);
                        });
                    });
                });
            }
        });
}

/// Render responsive transaction table with card container
pub fn render_responsive_transaction_table(ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
    if transactions.is_empty() {
        ui.label("No transactions yet!");
        return;
    }

    // Responsive approach: size everything as percentages of available space
    let content_width = available_rect.width() - 40.0; // Leave some margin
    let card_padding = 15.0; // Match calendar padding
    let available_table_width = content_width - (card_padding * 2.0);
    
    // Table takes up 92% of available width, centered (matching calendar approach)
    let table_width = available_table_width * 0.92;
    
    // Calculate responsive column widths
    let date_width = table_width * 0.20; // 20% for date
    let description_width = table_width * 0.40; // 40% for description  
    let amount_width = table_width * 0.20; // 20% for amount
    let balance_width = table_width * 0.20; // 20% for balance
    
    // Calculate card height based on number of transactions
    let header_height = 60.0;
    let row_height = 45.0;
    let title_height = 50.0;
    let num_rows = transactions.len().min(15); // Show max 15 rows
    let card_height = title_height + header_height + (row_height * num_rows as f32) + card_padding * 2.0;
    
    // Ensure card doesn't exceed available rectangle bounds
    let max_available_height = available_rect.height() - 40.0; // Leave 40px margin
    let final_card_height = card_height.min(max_available_height);
    
    let card_rect = egui::Rect::from_min_size(
        egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0),
        egui::vec2(content_width, final_card_height)
    );
    
    // Draw the card container
    draw_card_container(ui, card_rect, 10.0);
    
    // Draw table content inside the card
    ui.allocate_ui_at_rect(card_rect, |ui| {
        ui.vertical(|ui| {
            ui.add_space(12.0); // Match calendar spacing
            
            // Title
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new("📋 Recent Transactions")
                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                    .strong());
            });
            
            ui.add_space(15.0);
            
            // Check if Chalkboard font is available
            let font_family = if ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("Chalkboard".into()))) {
                egui::FontFamily::Name("Chalkboard".into())
            } else {
                egui::FontFamily::Proportional
            };
            
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
                            // Create scrollable area for table
                            egui::ScrollArea::vertical()
                                .max_height(final_card_height - title_height - card_padding * 2.0)
                                .show(ui, |ui| {
                    // Create responsive table
                    TableBuilder::new(ui)
                        .striped(true)
                        .resizable(false)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::exact(date_width))
                        .column(Column::exact(description_width))
                        .column(Column::exact(amount_width))
                        .column(Column::exact(balance_width))
                        .header(header_height, |mut header| {
                            // Date header with gradient bubble
                            header.col(|ui| {
                                let button = egui::Button::new(
                                    egui::RichText::new("DATE")
                                        .font(egui::FontId::new(header_font_size, font_family.clone()))
                                        .color(egui::Color32::WHITE)
                                        .strong()
                                )
                                .fill(get_table_header_color(0))
                                .rounding(egui::Rounding::same(8.0));
                                
                                ui.add_sized([date_width, header_height], button);
                            });
                            
                            // Description header with gradient bubble
                            header.col(|ui| {
                                let button = egui::Button::new(
                                    egui::RichText::new("DESCRIPTION")
                                        .font(egui::FontId::new(header_font_size, font_family.clone()))
                                        .color(egui::Color32::WHITE)
                                        .strong()
                                )
                                .fill(get_table_header_color(1))
                                .rounding(egui::Rounding::same(8.0));
                                
                                ui.add_sized([description_width, header_height], button);
                            });
                            
                            // Amount header with gradient bubble
                            header.col(|ui| {
                                let button = egui::Button::new(
                                    egui::RichText::new("AMOUNT")
                                        .font(egui::FontId::new(header_font_size, font_family.clone()))
                                        .color(egui::Color32::WHITE)
                                        .strong()
                                )
                                .fill(get_table_header_color(2))
                                .rounding(egui::Rounding::same(8.0));
                                
                                ui.add_sized([amount_width, header_height], button);
                            });
                            
                            // Balance header with gradient bubble
                            header.col(|ui| {
                                let button = egui::Button::new(
                                    egui::RichText::new("BALANCE")
                                        .font(egui::FontId::new(header_font_size, font_family.clone()))
                                        .color(egui::Color32::WHITE)
                                        .strong()
                                )
                                .fill(get_table_header_color(3))
                                .rounding(egui::Rounding::same(8.0));
                                
                                ui.add_sized([balance_width, header_height], button);
                            });
                        })
                        .body(|mut body| {
                            for transaction in transactions.iter().take(15) { // Show max 15 transactions
                                body.row(row_height, |mut row| {
                                    // Date column
                                    row.col(|ui| {
                                        let date_str = if let Some(date_part) = transaction.date.split('T').next() {
                                            if let Ok(parsed_date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                                                parsed_date.format("%B %d, %Y").to_string()
                                            } else {
                                                date_part.to_string()
                                            }
                                        } else {
                                            "Unknown".to_string()
                                        };
                                        
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.label(egui::RichText::new(date_str)
                                                .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                .strong());
                                        });
                                    });
                                    
                                    // Description column
                                    row.col(|ui| {
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.label(egui::RichText::new(&transaction.description)
                                                .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                .strong());
                                        });
                                    });
                                    
                                    // Amount column with color coding
                                    row.col(|ui| {
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            if transaction.amount >= 0.0 {
                                                ui.colored_label(
                                                    egui::Color32::from_rgb(34, 139, 34), // Green for positive
                                                    egui::RichText::new(format!("+${:.2}", transaction.amount))
                                                        .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                        .strong()
                                                );
                                            } else {
                                                ui.colored_label(
                                                    egui::Color32::from_rgb(220, 20, 60), // Red for negative
                                                    egui::RichText::new(format!("-${:.2}", transaction.amount.abs()))
                                                        .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                        .strong()
                                                );
                                            }
                                        });
                                    });
                                    
                                    // Balance column
                                    row.col(|ui| {
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.label(egui::RichText::new(format!("${:.2}", transaction.balance))
                                                .font(egui::FontId::new(content_font_size, font_family.clone()))
                                                .strong());
                                        });
                                    });
                                });
                            }
                        });
                            });
                        }
                    );
                }
            );
        });
    });
} 