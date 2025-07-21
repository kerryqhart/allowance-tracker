//! Progress Bar Component for Goal Tracking
//! 
//! This module contains the progress bar that shows savings progress with kid-friendly
//! "saved" and "to go!" labels, plus the target amount as part of the same visual unit.

use eframe::egui;
use crate::ui::components::styling::colors;

/// Configuration for progress bar appearance and styling
#[derive(Debug, Clone)]
pub struct ProgressBarConfig {
    // Visual styling
    pub amount_font_size: f32,
    pub label_font_size: f32,
    pub target_font_size: f32,
    pub text_padding: f32,
    pub amount_label_spacing: f32,
    
    // Colors
    pub fill_color_active: egui::Color32,
    pub fill_color_complete: egui::Color32,
    pub background_color: egui::Color32,
}

impl Default for ProgressBarConfig {
    fn default() -> Self {
        Self {
            amount_font_size: 24.0, // Increased from 20.0 for bigger celebration text
            label_font_size: 14.0,
            target_font_size: 60.0,
            text_padding: 20.0,
            amount_label_spacing: 12.0,
            
            fill_color_active: egui::Color32::from_rgb(199, 112, 221), // Pink matching goal chip
            fill_color_complete: egui::Color32::from_rgb(199, 112, 221), // Changed from GREEN to pink
            background_color: egui::Color32::from_rgb(230, 230, 230),
        }
    }
}

// Import layout config from layout module
pub use crate::ui::components::goal_progress_bar::layout::ProgressBarLayoutConfig;

/// Draw a kid-friendly progress bar with "saved" and "to go!" labels
/// 
/// The progress bar and target amount are treated as one visual unit.
/// Layout configuration comes from the layout system for consistency.
pub fn draw_progress_bar_with_target(
    ui: &mut egui::Ui,
    current_balance: f64,
    target_amount: f64,
    available_width: f32,
    layout_config: &ProgressBarLayoutConfig,
) {
    draw_progress_bar_with_target_completion(ui, current_balance, target_amount, available_width, layout_config, false);
}

pub fn draw_progress_bar_with_target_completion(
    ui: &mut egui::Ui,
    current_balance: f64,
    target_amount: f64,
    available_width: f32,
    layout_config: &ProgressBarLayoutConfig,
    is_goal_complete: bool,
) {
    let style_config = ProgressBarConfig::default();
    
    let progress = if target_amount > 0.0 {
        (current_balance / target_amount).clamp(0.0, 1.0)
    } else {
        0.0
    };
    
    let remaining_amount = (target_amount - current_balance).max(0.0);
    
    ui.horizontal(|ui| {
        // Calculate target text size first
        let target_text = format!("${:.0}", target_amount);
        let target_font = egui::FontId::new(style_config.target_font_size, egui::FontFamily::Proportional);
        let target_text_size = ui.painter().layout_no_wrap(
            target_text.clone(),
            target_font.clone(),
            colors::TEXT_SECONDARY
        ).size();
        
        // Progress bar takes remaining space after target text and spacing
        let bar_width = available_width - target_text_size.x - layout_config.internal_spacing;
        let rounding = egui::Rounding::same(layout_config.rounding);
        
        // Colors from configuration
        let fill_color = if progress >= 1.0 { 
            style_config.fill_color_complete
        } else { 
            style_config.fill_color_active
        };
        let background_color = style_config.background_color;
        
        // Calculate progress bar rectangle
        let (rect, _response) = ui.allocate_exact_size(
            egui::vec2(bar_width, layout_config.height), 
            egui::Sense::hover()
        );
        
        // Draw background
        ui.painter().rect_filled(rect, rounding, background_color);
        
        // Draw filled portion
        if progress > 0.0 {
            let filled_width = rect.width() * progress as f32;
            let filled_rect = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(filled_width, rect.height())
            );
            ui.painter().rect_filled(filled_rect, rounding, fill_color);
        }
        
        // Text fonts
        let amount_font = egui::FontId::new(style_config.amount_font_size, egui::FontFamily::Proportional);
        let label_font = egui::FontId::new(style_config.label_font_size, egui::FontFamily::Proportional);
        
        // Text content (conditional based on goal completion)
        let (saved_amount_text, saved_label_text, remaining_amount_text, remaining_label_text) = if is_goal_complete {
            // Goal complete: show celebration message
            (
                format!("You saved ${:.0} of ${:.0}! 🎉✨", current_balance, target_amount),
                "".to_string(), // Empty label for celebration message
                "".to_string(), // No remaining amount text
                "".to_string(), // No remaining label
            )
        } else {
            // Goal in progress: show normal progress
            (
                format!("${:.0}", current_balance),
                "saved".to_string(),
                format!("${:.0}", remaining_amount),
                "to go!".to_string(),
            )
        };
        
        // Draw text based on completion status
        if is_goal_complete {
            // Goal complete: center celebration message across full width
            let celebration_size = ui.painter().layout_no_wrap(
                saved_amount_text.clone(),
                amount_font.clone(),
                egui::Color32::WHITE
            ).size();
            
            if celebration_size.x + style_config.text_padding <= rect.width() {
                let center_x = rect.center().x;
                let center_y = rect.center().y;
                
                // Center the celebration message
                let text_pos = egui::pos2(
                    center_x - celebration_size.x / 2.0,
                    center_y - celebration_size.y / 2.0
                );
                ui.painter().text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    saved_amount_text,
                    amount_font.clone(),
                    egui::Color32::WHITE
                );
            }
        } else {
            // Goal in progress: draw normal "saved" section on filled portion
            if progress > 0.0 {
                let filled_width = rect.width() * progress as f32;
                
                let saved_amount_size = ui.painter().layout_no_wrap(
                    saved_amount_text.clone(),
                    amount_font.clone(),
                    egui::Color32::WHITE
                ).size();
                
                let saved_label_size = ui.painter().layout_no_wrap(
                    saved_label_text.clone(),
                    label_font.clone(),
                    egui::Color32::WHITE
                ).size();
                
                let total_width = saved_amount_size.x.max(saved_label_size.x);
                if total_width + style_config.text_padding <= filled_width {
                    let center_x = rect.min.x + filled_width / 2.0;
                    
                    // Amount (top line)
                    let amount_pos = egui::pos2(
                        center_x - saved_amount_size.x / 2.0,
                        rect.center().y - saved_amount_size.y / 2.0 - style_config.amount_label_spacing
                    );
                    ui.painter().text(
                        amount_pos,
                        egui::Align2::LEFT_TOP,
                        saved_amount_text,
                        amount_font.clone(),
                        egui::Color32::WHITE
                    );
                    
                    // Label (bottom line) - only if we have a label
                    if !saved_label_text.is_empty() {
                        let label_pos = egui::pos2(
                            center_x - saved_label_size.x / 2.0,
                            rect.center().y + saved_label_size.y / 2.0 - 2.0
                        );
                        ui.painter().text(
                            label_pos,
                            egui::Align2::LEFT_TOP,
                            saved_label_text,
                            label_font.clone(),
                            egui::Color32::WHITE
                        );
                    }
                }
            }
        }
        
        // Draw "to go!" section on unfilled portion (if it fits and there's remaining amount)
        if !is_goal_complete && progress < 1.0 && remaining_amount > 0.0 && !remaining_amount_text.is_empty() {
            let filled_width = rect.width() * progress as f32;
            let unfilled_width = rect.width() - filled_width;
            
            let remaining_amount_size = ui.painter().layout_no_wrap(
                remaining_amount_text.clone(),
                amount_font.clone(),
                colors::TEXT_SECONDARY
            ).size();
            
            let remaining_label_size = ui.painter().layout_no_wrap(
                remaining_label_text.clone(),
                label_font.clone(),
                colors::TEXT_SECONDARY
            ).size();
            
            let total_width = remaining_amount_size.x.max(remaining_label_size.x);
            if total_width + style_config.text_padding <= unfilled_width {
                let center_x = rect.min.x + filled_width + unfilled_width / 2.0;
                
                // Amount (top line)
                let amount_pos = egui::pos2(
                    center_x - remaining_amount_size.x / 2.0,
                    rect.center().y - remaining_amount_size.y / 2.0 - style_config.amount_label_spacing
                );
                ui.painter().text(
                    amount_pos,
                    egui::Align2::LEFT_TOP,
                    remaining_amount_text,
                    amount_font,
                    colors::TEXT_SECONDARY
                );
                
                // Label (bottom line)
                let label_pos = egui::pos2(
                    center_x - remaining_label_size.x / 2.0,
                    rect.center().y + remaining_label_size.y / 2.0 - 2.0
                );
                ui.painter().text(
                    label_pos,
                    egui::Align2::LEFT_TOP,
                    remaining_label_text,
                    label_font,
                    colors::TEXT_SECONDARY
                );
            }
        }
        
        // Add spacing between progress bar and target
        ui.add_space(layout_config.internal_spacing);
        
        // Draw target amount with exact size allocation (no wrapping)
        let (target_rect, _response) = ui.allocate_exact_size(
            target_text_size, 
            egui::Sense::hover()
        );
        
        // Center the text vertically within the progress bar height
        let text_y = target_rect.min.y + (layout_config.height / 2.0 - target_text_size.y / 2.0);
        let text_pos = egui::pos2(target_rect.min.x, text_y);
        
        ui.painter().text(
            text_pos,
            egui::Align2::LEFT_TOP,
            target_text,
            target_font,
            colors::TEXT_SECONDARY
        );
    });
} 