//! # UI Components Module
//!
//! This module contains reusable UI helper functions for drawing common interface elements
//! throughout the allowance tracker application.
//!
//! ## Key Functions:
//! - `draw_card_background()` - Draws card backgrounds with shadows and styling
//! - `draw_toggle_header()` - Draws expandable/collapsible headers for sections
//! - `draw_card_header_with_toggles()` - Draws complex headers with multiple toggle buttons
//! - `draw_card_with_flat_top()` - Draws cards with flat top edges for integration
//! - `draw_integrated_tabs()` - Draws tab navigation integrated with card styling
//!
//! ## Purpose:
//! These functions provide consistent styling and behavior across different parts of the app,
//! ensuring a cohesive user experience. They handle the visual polish like shadows, gradients,
//! and hover effects that make the interface feel modern and kid-friendly.

use eframe::egui;
use crate::ui::app_state::{AllowanceTrackerApp, MainTab};
use crate::ui::components::theme::colors;

impl AllowanceTrackerApp {
    /// Draw card background with proper styling
    pub fn draw_card_background(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        
        // Draw subtle shadow first
        let shadow_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(2.0, 2.0),
            rect.size(),
        );
        painter.rect_filled(
            shadow_rect, 
            egui::Rounding::same(10.0),
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 20)
        );
        
        // Draw white background
        painter.rect_filled(
            rect, 
            egui::Rounding::same(10.0),
            egui::Color32::WHITE
        );
        
        // Draw border
        painter.rect_stroke(
            rect,
            egui::Rounding::same(10.0),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 220, 220))
        );
    }
    
    /// Draw just the Calendar/Table toggle buttons (for subheader)
    pub fn draw_tab_toggle_buttons(&mut self, ui: &mut egui::Ui) {
        use crate::ui::components::theme::colors;
        
        ui.horizontal(|ui| {
            // Table button (rendered first so it appears on the right in right-to-left layout)
            let table_button = egui::Button::new(
                egui::RichText::new("📋 Table")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Table { 
                        colors::TEXT_WHITE 
                    } else { 
                        colors::TEXT_SECONDARY 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Table {
                colors::ACTIVE_BACKGROUND // Theme active color
            } else {
                colors::INACTIVE_BACKGROUND // Theme inactive color
            })
            .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)); // Purple outline
            
            if ui.add(table_button).clicked() {
                self.current_tab = MainTab::Table;
            }
            
            ui.add_space(8.0);
            
            // Calendar button (rendered second so it appears on the left in right-to-left layout)
            let calendar_button = egui::Button::new(
                egui::RichText::new("📅 Calendar")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Calendar { 
                        colors::TEXT_WHITE 
                    } else { 
                        colors::TEXT_SECONDARY 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Calendar {
                colors::ACTIVE_BACKGROUND // Theme active color
            } else {
                colors::INACTIVE_BACKGROUND // Theme inactive color
            })
            .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)); // Purple outline
            
            if ui.add(calendar_button).clicked() {
                self.current_tab = MainTab::Calendar;
            }
        });
    }

    /// Draw toggle header within card (like old Tauri version) - just title now, toggles moved to subheader
    pub fn draw_toggle_header(&mut self, ui: &mut egui::Ui, card_rect: egui::Rect, title: &str) {
        let header_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(20.0, 15.0),
            egui::vec2(card_rect.width() - 40.0, 40.0),
        );
        
        // Set up UI for header
        let mut header_ui = ui.child_ui(header_rect, egui::Layout::left_to_right(egui::Align::Center), None);
        
        // Just the title now - toggle buttons moved to subheader
        header_ui.label(egui::RichText::new(title)
            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
            .strong()
            .color(egui::Color32::from_rgb(70, 70, 70)));
    }
    
    /// Draw card header with title and toggle buttons
    pub fn draw_card_header_with_toggles(&mut self, ui: &mut egui::Ui, card_rect: egui::Rect) {
        let header_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(20.0, 15.0),
            egui::vec2(card_rect.width() - 40.0, 40.0),
        );
        
        // Set up UI for header
        let mut header_ui = ui.child_ui(header_rect, egui::Layout::left_to_right(egui::Align::Center), None);
        
        // Title on the left
        header_ui.label(egui::RichText::new("Recent Transactions")
            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
            .strong()
            .color(egui::Color32::from_rgb(70, 70, 70)));
        
        // Push toggle buttons to the right
        header_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Chart button
            let chart_button = egui::Button::new(
                egui::RichText::new("📊 Chart")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Calendar { 
                        colors::TEXT_WHITE 
                    } else { 
                        colors::TEXT_SECONDARY 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Calendar {
                colors::ACTIVE_BACKGROUND // Theme active color
            } else {
                colors::INACTIVE_BACKGROUND // Theme inactive color
            })
            .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)); // Purple outline
            
            if ui.add(chart_button).clicked() {
                self.current_tab = MainTab::Calendar;
            }
            
            // Small space between buttons
            ui.add_space(8.0);
            
            // Table button
            let table_button = egui::Button::new(
                egui::RichText::new("📋 Table")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Table { 
                        colors::TEXT_WHITE 
                    } else { 
                        colors::TEXT_SECONDARY 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Table {
                colors::ACTIVE_BACKGROUND // Theme active color
            } else {
                colors::INACTIVE_BACKGROUND // Theme inactive color
            })
            .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)); // Purple outline
            
            if ui.add(table_button).clicked() {
                self.current_tab = MainTab::Table;
            }
        });
    }
    
    /// Draw card with flat top for tab integration
    pub fn draw_card_with_flat_top(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        
        // Draw subtle shadow first (offset slightly)
        let shadow_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(2.0, 2.0),
            rect.size(),
        );
        painter.rect_filled(
            shadow_rect, 
            egui::Rounding { nw: 8.0, ne: 8.0, sw: 10.0, se: 10.0 }, // Rounded top corners, rounded bottom
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 20)
        );
        
        // Draw white background with rounded top corners (tabs will overlap the flat sections)
        painter.rect_filled(
            rect, 
            egui::Rounding { nw: 8.0, ne: 8.0, sw: 10.0, se: 10.0 }, // Rounded top corners, rounded bottom
            egui::Color32::WHITE
        );
    }
    
    /// Draw integrated tabs that connect to cards
    pub fn draw_integrated_tabs(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect) {
        // Calculate the exact positioning to align with calendar card
        let content_width = available_rect.width() - 40.0; // Same margin as calendar
        
        // Position tabs to align with calendar card left edge and sit directly above it
        let tabs_rect = egui::Rect::from_min_size(
            egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0 - 45.0), // Position tabs right above calendar
            egui::vec2(content_width, 45.0)
        );
        
        // Draw tabs in the calculated position
        ui.allocate_ui_at_rect(tabs_rect, |ui| {
            ui.horizontal(|ui| {
                // Add padding to align with calendar content
                ui.add_space(15.0);
                
                // Calendar tab - file folder style with subtle flare
                let calendar_selected = self.current_tab == MainTab::Calendar;
                let calendar_size = if calendar_selected {
                    [145.0, 45.0] // Just slightly wider when active (subtle flare)
                } else {
                    [140.0, 45.0] // Normal width when inactive
                };
                
                let calendar_rounding = if calendar_selected {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 0.0 } // Rounded top, flat bottom for connection
                } else {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 8.0, se: 0.0 } // Rounded top, rounded bottom-left only
                };
                
                let calendar_button = if calendar_selected {
                    egui::Button::new(egui::RichText::new("📅 Calendar")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_PRIMARY))
                        .fill(colors::CARD_BACKGROUND) // Same white as calendar card
                        .rounding(calendar_rounding)
                        .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple border
                } else {
                    egui::Button::new(egui::RichText::new("📅 Calendar")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100)))
                        .fill(colors::INACTIVE_BACKGROUND) // Theme inactive color
                        .rounding(calendar_rounding)
                        .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple border
                };
                
                if ui.add_sized(calendar_size, calendar_button).clicked() {
                    self.current_tab = MainTab::Calendar;
                }
                
                ui.add_space(2.0); // Small gap between tabs
                
                // Table tab - file folder style with subtle flare
                let table_selected = self.current_tab == MainTab::Table;
                let table_size = if table_selected {
                    [145.0, 45.0] // Just slightly wider when active (subtle flare)
                } else {
                    [140.0, 45.0] // Normal width when inactive
                };
                
                let table_rounding = if table_selected {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 0.0 } // Rounded top, flat bottom for connection
                } else {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 8.0 } // Rounded top, rounded bottom-right only
                };
                
                let table_button = if table_selected {
                    egui::Button::new(egui::RichText::new("📋 Table")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_PRIMARY))
                        .fill(colors::CARD_BACKGROUND) // Same white as calendar card
                        .rounding(table_rounding)
                        .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple border
                } else {
                    egui::Button::new(egui::RichText::new("📋 Table")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100)))
                        .fill(colors::INACTIVE_BACKGROUND) // Theme inactive color
                        .rounding(table_rounding)
                        .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple border
                };
                
                if ui.add_sized(table_size, table_button).clicked() {
                    self.current_tab = MainTab::Table;
                }
            });
        });
    }
} 