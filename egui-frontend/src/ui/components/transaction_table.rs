use eframe::egui;
use egui_extras::{TableBuilder, Column};
use shared::*;
use crate::ui::components::styling::draw_solid_purple_background;

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
            // Date header
            header.col(|ui| {
                let rect = ui.max_rect();
                draw_solid_purple_background(ui, rect);
                
                // Add proper padding around the text
                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::WHITE, 
                        egui::RichText::new("DATE")
                            .font(egui::FontId::new(20.0, font_family.clone()))
                            .strong()
                    );
                    ui.add_space(10.0);
                });
            });
            
            // Description header
            header.col(|ui| {
                let rect = ui.max_rect();
                draw_solid_purple_background(ui, rect);
                
                // Add proper padding around the text
                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::WHITE, 
                        egui::RichText::new("DESCRIPTION")
                            .font(egui::FontId::new(20.0, font_family.clone()))
                            .strong()
                    );
                    ui.add_space(10.0);
                });
            });
            
            // Amount header
            header.col(|ui| {
                let rect = ui.max_rect();
                draw_solid_purple_background(ui, rect);
                
                // Add proper padding around the text
                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::WHITE, 
                        egui::RichText::new("AMOUNT")
                            .font(egui::FontId::new(20.0, font_family.clone()))
                            .strong()
                    );
                    ui.add_space(10.0);
                });
            });
            
            // Balance header
            header.col(|ui| {
                let rect = ui.max_rect();
                draw_solid_purple_background(ui, rect);
                
                // Add proper padding around the text
                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::WHITE, 
                        egui::RichText::new("BALANCE")
                            .font(egui::FontId::new(20.0, font_family.clone()))
                            .strong()
                    );
                    ui.add_space(10.0);
                });
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