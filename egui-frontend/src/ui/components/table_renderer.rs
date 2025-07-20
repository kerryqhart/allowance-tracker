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
        // Load initial table data if not loaded yet
        if !self.table.initial_load_complete && !self.table.is_loading_more {
            log::info!("📋 Table not loaded yet, triggering initial load");
            self.load_initial_table_transactions();
        }
        
        ui.add_space(15.0);
        
        // Calculate card dimensions
        let content_width = available_rect.width() - 40.0;
        let card_padding = 15.0;
        
        // Calculate table dimensions to use maximum space (no internal header needed)
        let header_height = 60.0;
        let min_card_height: f32 = header_height + card_padding * 2.0;
        
        // Use more available space - reduce margins to expand table
        let max_available_height = available_rect.height() - 20.0; // Reduced margin 
        let final_card_height = min_card_height.max(max_available_height * 0.85); // Use 85% of available space
        
        let card_rect = egui::Rect::from_min_size(
            egui::pos2(available_rect.left() + 20.0, available_rect.top() + 10.0), // Reduced top margin
            egui::vec2(content_width, final_card_height)
        );
        
        // Draw table content with infinite scroll
        self.draw_infinite_scroll_table_content(ui, card_rect, transactions);
    }
    
    /// Draw infinite scroll table content with loading detection
    pub fn draw_infinite_scroll_table_content(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect, transactions: &[Transaction]) {
        let mut content_ui = ui.child_ui(content_rect, egui::Layout::top_down(egui::Align::Min), None);
        
        // Calculate actual content height to determine if we need scrolling
        let row_height = 25.0; // Match the transaction table row height
        let header_height = 40.0;
        let estimated_content_height = header_height + (transactions.len() as f32 * row_height) + 100.0; // 100px buffer for loading indicators
        
        let needs_scrolling = estimated_content_height > content_rect.height() || self.table.can_load_more();
        
        if needs_scrolling {
            // Use ScrollArea with infinite scroll when content doesn't fit or there's more to load
            let scroll_area_response = egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(content_rect.height())
                .show(&mut content_ui, |ui| {
                    // Use the existing beautiful table implementation
                    use crate::ui::components::transaction_table::render_responsive_transaction_table;
                    render_responsive_transaction_table(ui, content_rect, transactions);
                    
                    // Loading indicator when fetching more
                    if self.table.is_loading_more {
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading more transactions...");
                        });
                    }
                    
                    // Pagination error display
                    if let Some(error) = &self.table.pagination_error {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::RED, format!("Error: {}", error));
                    }
                    
                    // "No more transactions" indicator
                    if !self.table.has_more_transactions && self.table.initial_load_complete && !self.table.is_loading_more && self.table.transaction_count() > 0 {
                        ui.add_space(10.0);
                        ui.colored_label(egui::Color32::GRAY, "No more transactions to load");
                    }
                    
                    // ONLY add trigger area if there are actually more transactions to load
                    if self.table.can_load_more() {
                        let trigger_response = ui.allocate_response(
                            egui::vec2(ui.available_width(), 50.0),
                            egui::Sense::hover()
                        );
                        
                        // Trigger loading when this area becomes visible
                        if trigger_response.rect.intersects(ui.clip_rect()) {
                            log::info!("📋 Infinite scroll trigger area visible - loading more transactions");
                            self.load_more_table_transactions();
                        }
                    }
                });
            
            // Alternative scroll detection: trigger when scrolled near bottom (only if there's more to load)
            if self.table.can_load_more() {
                let scroll_offset = scroll_area_response.state.offset.y;
                let content_height = scroll_area_response.inner_rect.height();
                let visible_height = content_rect.height();
                
                // If scrolled to within 200px of bottom, trigger loading
                if content_height > visible_height && scroll_offset + visible_height + 200.0 >= content_height {
                    log::info!("📋 Near bottom of scroll - loading more transactions (offset: {:.0}, content: {:.0}, visible: {:.0})", 
                              scroll_offset, content_height, visible_height);
                    self.load_more_table_transactions();
                }
            }
        } else {
            // Content fits perfectly - no ScrollArea needed, just render directly
            use crate::ui::components::transaction_table::render_responsive_transaction_table;
            render_responsive_transaction_table(&mut content_ui, content_rect, transactions);
            
            // Show any error messages even without scrolling
            if let Some(error) = &self.table.pagination_error {
                content_ui.add_space(10.0);
                content_ui.colored_label(egui::Color32::RED, format!("Error: {}", error));
            }
        }
    }
} 