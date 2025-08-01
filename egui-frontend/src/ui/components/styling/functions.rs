//! # Styling Functions
//!
//! This module contains drawing utility functions and UI helpers for common styling operations.
//! These functions provide consistent visual styling across the application.
//!
//! ## Key Functions:
//! - `setup_kid_friendly_style()` - Configure global egui styling
//! - `draw_solid_purple_background()` - Draw solid background color
//! - `draw_image_background()` - Draw gradient background with image
//! - `draw_card_container()` - Draw card-style containers with shadows
//! - `draw_day_header_gradient()` - Draw gradient headers for calendar days
//! - `get_table_header_color()` - Get colors for table headers
//!
//! ## Purpose:
//! These functions ensure visual consistency and provide centralized implementations
//! for common styling patterns used throughout the app.

use eframe::egui;
use super::{theme::CURRENT_THEME, colors};

/// Setup kid-friendly UI styling for the entire application
/// 
/// This function configures the global egui style to create a welcoming,
/// kid-friendly interface with appropriate fonts, colors, and spacing.
pub fn setup_kid_friendly_style(ctx: &egui::Context) {
    ctx.set_style({
        let mut style = (*ctx.style()).clone();
        
        // Bright, fun colors
        style.visuals.window_fill = egui::Color32::TRANSPARENT; // Make window transparent so our background shows through
        style.visuals.panel_fill = egui::Color32::TRANSPARENT; // Make panels transparent so our background shows through
        style.visuals.button_frame = true;
        
        // CRITICAL: Set text edit background color so text fields are visible
        // In egui 0.28, text edits use extreme_bg_color (not text_edit_bg_color which was added later)
        style.visuals.extreme_bg_color = CURRENT_THEME.interactive.inactive_background;
        
        // Use Chalkboard font family if available, otherwise fall back to Proportional
        let font_family = if ctx.fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("Chalkboard".into()))) {
            egui::FontFamily::Name("Chalkboard".into())
        } else {
            egui::FontFamily::Proportional
        };
        
        // Larger text for readability with Chalkboard font
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(28.0, font_family.clone()),
        );
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(16.0, font_family.clone()),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(18.0, font_family.clone()),
        );
        
        // Rounded corners and padding
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        // Attempt to set rounding through different API
        style.visuals.widgets.inactive.bg_fill = style.visuals.widgets.inactive.bg_fill;
        style.visuals.widgets.active.bg_fill = style.visuals.widgets.active.bg_fill; 
        style.visuals.widgets.hovered.bg_fill = style.visuals.widgets.hovered.bg_fill;
        
        style
    });
}

/// Draw solid purple background for header columns
/// 
/// Used for table headers and special background sections that need
/// a consistent purple color matching the theme.
pub fn draw_solid_purple_background(ui: &mut egui::Ui, rect: egui::Rect) {
    // Use the nice purple color from the theme
    let purple_color = CURRENT_THEME.calendar.header_mid;
    
    // Draw solid purple background for this column
    ui.painter().rect_filled(rect, egui::CornerRadius::ZERO, purple_color);
}

/// Draw image background with blue overlay (replacing gradient)
/// 
/// This function draws the main application background using the background image
/// with a blue tint. Falls back to a solid color if the image fails to load.
pub fn draw_image_background(ui: &mut egui::Ui, rect: egui::Rect) {
    let painter = ui.painter();
    
    // Load and paint the background image with blue tint
    let image_source = egui::include_image!("../../../../assets/background.jpg");
    let blue_tint = egui::Color32::from_rgba_premultiplied(173, 216, 230, 180); // Light blue tint
    let image = egui::Image::new(image_source)
        .fit_to_exact_size(rect.size())
        .tint(blue_tint); // Apply blue tint directly to the image
    
    // Try to load the image and see if it fails
    match image.load_for_size(ui.ctx(), rect.size()) {
        Ok(_sized_texture) => {
            image.paint_at(ui, rect);
        }
        Err(_e) => {
            // Fallback to a solid color if image fails to load
            let fallback_color = CURRENT_THEME.layout.gradient_bottom; // Light blue
            painter.rect_filled(rect, egui::CornerRadius::ZERO, fallback_color);
        }
    }
}

/// Draw a modern card container with white background and shadow
/// 
/// This function creates the standard card appearance used throughout the app,
/// with a white background, subtle shadow, and rounded corners.
/// Optionally adds a frosted glass effect using egui 0.31's opacity features.
pub fn draw_card_container(ui: &mut egui::Ui, rect: egui::Rect, rounding: u8) {
    draw_card_container_with_options(ui, rect, rounding, false);
}

/// Draw a card container with frosted glass effect
/// 
/// This creates a modern frosted glass appearance with transparency and enhanced visual depth.
pub fn draw_card_container_frosted(ui: &mut egui::Ui, rect: egui::Rect, rounding: u8) {
    draw_card_container_with_options(ui, rect, rounding, true);
}

/// Draw a card container with configurable frosted glass effect
/// 
/// This function creates the standard card appearance with an optional frosted glass effect
/// using egui 0.31's new opacity features for enhanced visual appeal.
fn draw_card_container_with_options(ui: &mut egui::Ui, rect: egui::Rect, rounding: u8, frosted: bool) {
    let painter = ui.painter();
    
    if frosted {
        // FROSTED GLASS EFFECT
        
        // Draw enhanced shadow with multiple layers for depth
        let shadow_offset = 3.0;
        let shadow_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(shadow_offset, shadow_offset),
            rect.size(),
        );
        
        // Multiple shadow layers for depth
        painter.rect_filled(
            egui::Rect::from_min_size(
                shadow_rect.min + egui::vec2(1.0, 1.0),
                shadow_rect.size()
            ), 
            egui::CornerRadius::same(rounding), 
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 15)
        );
        painter.rect_filled(
            shadow_rect, 
            egui::CornerRadius::same(rounding), 
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 25)
        );
        
        // Frosted glass background - semi-transparent white with subtle tint
        painter.rect_filled(
            rect, 
            egui::CornerRadius::same(rounding), 
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 150) // Much more transparent for noticeable effect
        );
        
        // Add a subtle border for the glass effect
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(rounding),
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 120)),
            egui::StrokeKind::Outside
        );
        
        // Optional: Add a very subtle inner highlight for glass effect
        let inner_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(1.0, 1.0),
            rect.size() - egui::vec2(2.0, 2.0)
        );
        painter.rect_stroke(
            inner_rect,
            egui::CornerRadius::same(rounding - 1),
            egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 60)),
            egui::StrokeKind::Outside
        );
        
    } else {
        // STANDARD SOLID CARD
        
        // Draw subtle shadow first (offset slightly)
        let shadow_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(2.0, 2.0),
            rect.size(),
        );
        painter.rect_filled(shadow_rect, egui::CornerRadius::same(rounding), colors::CARD_SHADOW);
        
        // Draw solid white background
        painter.rect_filled(rect, egui::CornerRadius::same(rounding), colors::CARD_BACKGROUND);
    }
}

/// Draw gradient day headers for calendar
/// 
/// Creates a smooth pink-to-purple gradient across calendar day headers
/// based on the day index (0-6 for Monday-Sunday).
pub fn draw_day_header_gradient(ui: &mut egui::Ui, rect: egui::Rect, day_index: usize) {
    let painter = ui.painter();
    
    // Calculate color based on day index (0-6 for Mon-Sun)
    let t = day_index as f32 / 6.0; // 0.0 to 1.0
    
    // Smooth pink-to-purple gradient across all 7 days (no blue transition)
    let color = egui::Color32::from_rgb(
        (colors::CALENDAR_HEADER_START.r() as f32 * (1.0 - t) + colors::CALENDAR_HEADER_MID.r() as f32 * t) as u8,
        (colors::CALENDAR_HEADER_START.g() as f32 * (1.0 - t) + colors::CALENDAR_HEADER_MID.g() as f32 * t) as u8,
        (colors::CALENDAR_HEADER_START.b() as f32 * (1.0 - t) + colors::CALENDAR_HEADER_MID.b() as f32 * t) as u8,
    );
    
    painter.rect_filled(rect, egui::CornerRadius::same(5), color);
}

/// Get table header color that matches calendar day header style
/// 
/// Returns a color from the pink-to-purple gradient for table headers,
/// ensuring visual consistency between calendar and table styling.
pub fn get_table_header_color(header_index: usize) -> egui::Color32 {
    CURRENT_THEME.table_header_color(header_index)
} 