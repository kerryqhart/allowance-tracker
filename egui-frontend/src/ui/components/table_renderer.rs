//! # Table Renderer Module
//!
//! This module handles rendering the transaction table view with responsive design and
//! integrated toggle headers.
//!
//! ## Key Functions:
//! - `draw_transactions_section_with_toggle()` - Renders table with collapsible header
//! - `draw_table_content()` - Renders the actual transaction table content
//!
//! ## Purpose:
//! This module provides the table view functionality for viewing transaction history.
//! It integrates with the responsive transaction table component to provide a clean,
//! mobile-friendly interface for browsing transactions.
//!
//! ## Features:
//! - Responsive design that adapts to different screen sizes
//! - Toggle header integration for consistent UI
//! - Reuses the existing transaction table component for consistency

use eframe::egui;
use shared::Transaction;
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    /// Draw transactions section with toggle header integrated
    pub fn draw_transactions_section_with_toggle(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        // Use the existing responsive transaction table
        use crate::ui::components::transaction_table::render_responsive_transaction_table;
        
        ui.add_space(15.0);
        
        // Calculate card dimensions
        let content_width = available_rect.width() - 40.0;
        let card_padding = 15.0;
        
        // Calculate card height based on number of transactions
        let header_height = 60.0;
        let row_height = 45.0;
        let title_height = 100.0; // Include space for toggle header
        let num_rows = transactions.len().min(15); // Show max 15 rows
        let card_height = title_height + header_height + (row_height * num_rows as f32) + card_padding * 2.0;
        
        // Ensure card doesn't exceed available rectangle bounds
        let max_available_height = available_rect.height() - 40.0;
        let final_card_height = card_height.min(max_available_height);
        
        let card_rect = egui::Rect::from_min_size(
            egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0),
            egui::vec2(content_width, final_card_height)
        );
        
        // Draw card background
        self.draw_card_background(ui, card_rect);
        
        // Draw toggle header
        self.draw_toggle_header(ui, card_rect, "Recent Transactions");
        
        // Draw table content with proper spacing for header
        let table_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(0.0, 60.0), // Leave space for toggle header
            egui::vec2(card_rect.width(), card_rect.height() - 60.0)
        );
        
        // Use the existing beautiful table implementation
        render_responsive_transaction_table(ui, table_rect, transactions);
    }
    
    /// Draw table content within the card
    pub fn draw_table_content(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect, transactions: &[Transaction]) {
        let mut content_ui = ui.child_ui(content_rect, egui::Layout::top_down(egui::Align::Min), None);
        
        // Transaction table
        egui::ScrollArea::vertical()
            .max_height(content_rect.height())
            .show(&mut content_ui, |ui| {
                
                // Table header
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Date")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .strong()
                        .color(egui::Color32::from_rgb(100, 100, 100)));
                    
                    ui.separator();
                    
                    ui.label(egui::RichText::new("Description")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .strong()
                        .color(egui::Color32::from_rgb(100, 100, 100)));
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Balance")
                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                            .strong()
                            .color(egui::Color32::from_rgb(100, 100, 100)));
                        
                        ui.separator();
                        
                        ui.label(egui::RichText::new("Amount")
                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                            .strong()
                            .color(egui::Color32::from_rgb(100, 100, 100)));
                    });
                });
                
                ui.separator();
                
                // Transaction rows
                for transaction in transactions {
                    ui.horizontal(|ui| {
                        // Format the date
                        let date_display = transaction.date.format("%b %d, %Y").to_string();
                        
                        ui.label(egui::RichText::new(date_display)
                            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(80, 80, 80)));
                        
                        ui.separator();
                        
                        ui.label(egui::RichText::new(&transaction.description)
                            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(80, 80, 80)));
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!("${:.2}", transaction.balance))
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(egui::Color32::from_rgb(80, 80, 80)));
                            
                            ui.separator();
                            
                            let amount_color = if transaction.amount >= 0.0 {
                                egui::Color32::from_rgb(40, 167, 69) // Green for positive
                            } else {
                                egui::Color32::from_rgb(220, 53, 69) // Red for negative
                            };
                            
                            let amount_text = if transaction.amount >= 0.0 {
                                format!("+${:.2}", transaction.amount)
                            } else {
                                format!("-${:.2}", transaction.amount.abs())
                            };
                            
                            ui.label(egui::RichText::new(amount_text)
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(amount_color));
                        });
                    });
                }
            });
    }
} 