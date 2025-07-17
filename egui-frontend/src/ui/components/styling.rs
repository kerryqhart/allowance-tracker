//! # Styling Module
//!
//! This module contains all styling functions and color constants for the allowance tracker app.
//! It provides a consistent, kid-friendly visual theme throughout the application.
//!
//! ## Key Functions:
//! - `setup_kid_friendly_style()` - Configure global egui styling
//! - `draw_solid_purple_background()` - Draw solid background color
//! - `draw_image_background()` - Draw gradient background with image
//! - `draw_card_container()` - Draw card-style containers with shadows
//! - `draw_day_header_gradient()` - Draw gradient headers for calendar days
//! - `get_table_header_color()` - Get colors for table headers
//!
//! ## Color Palette:
//! The colors module contains all the color constants used throughout the app:
//! - Gradient backgrounds (pink to blue)
//! - Calendar styling (white backgrounds, subtle shadows)
//! - Transaction chips (green for income, red for expenses)
//! - Interactive elements (buttons, headers)
//!
//! ## Purpose:
//! This module ensures visual consistency and provides a centralized place for all
//! styling concerns. The kid-friendly theme uses bright, welcoming colors and
//! smooth gradients to create an engaging user experience.

use eframe::egui;
use egui::Color32;

/// Setup kid-friendly UI styling for the entire application
pub fn setup_kid_friendly_style(ctx: &egui::Context) {
    ctx.set_style({
        let mut style = (*ctx.style()).clone();
        
        // Bright, fun colors
        style.visuals.window_fill = egui::Color32::TRANSPARENT; // Make window transparent so our background shows through
        style.visuals.panel_fill = egui::Color32::TRANSPARENT; // Make panels transparent so our background shows through
        style.visuals.button_frame = true;
        
        // CRITICAL: Set text edit background color so text fields are visible
        // In egui 0.28, text edits use extreme_bg_color (not text_edit_bg_color which was added later)
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(248, 248, 248); // Light gray background for text edits
        
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
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
        style.visuals.widgets.active.rounding = egui::Rounding::same(8.0);
        style.visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
        
        style
    });
}

/// Draw solid purple background for header columns
pub fn draw_solid_purple_background(ui: &mut egui::Ui, rect: egui::Rect) {
    // Use the nice purple color from the original BALANCE header
    let purple_color = egui::Color32::from_rgb(186, 85, 211);
    
    // Draw solid purple background for this column
    ui.painter().rect_filled(rect, egui::Rounding::ZERO, purple_color);
}

/// Color constants for the kid-friendly theme
pub mod colors {
    use eframe::egui::Color32;
    
    // Background gradient colors (matching the original Tauri design)
    pub const GRADIENT_TOP: Color32 = Color32::from_rgb(255, 182, 193);    // Light pink
    pub const GRADIENT_BOTTOM: Color32 = Color32::from_rgb(173, 216, 230); // Light blue
    
    // Calendar colors
    pub const CALENDAR_BACKGROUND: Color32 = Color32::WHITE;
    pub const CALENDAR_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 20);
    
    // Day header gradient colors
    pub const DAY_HEADER_START: Color32 = Color32::from_rgb(255, 182, 193); // Pink
    pub const DAY_HEADER_MID: Color32 = Color32::from_rgb(186, 85, 211);    // Purple
    pub const DAY_HEADER_END: Color32 = Color32::from_rgb(135, 206, 235);   // Sky blue
    
    // Transaction chip colors
    pub const INCOME_CHIP: Color32 = Color32::from_rgb(34, 139, 34);        // Green
    pub const EXPENSE_CHIP: Color32 = Color32::from_rgb(220, 20, 60);       // Crimson
    
    // Balance text
    pub const BALANCE_TEXT: Color32 = Color32::from_rgb(128, 128, 128);     // Gray
}

/// Draw image background with blue overlay (replacing gradient)
pub fn draw_image_background(ui: &mut egui::Ui, rect: egui::Rect) {
    let painter = ui.painter();
    
    // Load and paint the background image with blue tint
    let image_source = egui::include_image!("../../../assets/background.jpg");
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
            let fallback_color = egui::Color32::from_rgb(173, 216, 230); // Light blue
            painter.rect_filled(rect, egui::Rounding::ZERO, fallback_color);
        }
    }
}

/// Draw a modern card container with white background and shadow
pub fn draw_card_container(ui: &mut egui::Ui, rect: egui::Rect, rounding: f32) {
    let painter = ui.painter();
    
    // Draw subtle shadow first (offset slightly)
    let shadow_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(2.0, 2.0),
        rect.size(),
    );
    painter.rect_filled(shadow_rect, egui::Rounding::same(rounding), colors::CALENDAR_SHADOW);
    
    // Draw white background
    painter.rect_filled(rect, egui::Rounding::same(rounding), colors::CALENDAR_BACKGROUND);
}

/// Draw gradient day headers for calendar
pub fn draw_day_header_gradient(ui: &mut egui::Ui, rect: egui::Rect, day_index: usize) {
    let painter = ui.painter();
    
    // Calculate color based on day index (0-6 for Mon-Sun)
    let t = day_index as f32 / 6.0; // 0.0 to 1.0
    
    // Smooth pink-to-purple gradient across all 7 days (no blue transition)
    let color = Color32::from_rgb(
        (colors::DAY_HEADER_START.r() as f32 * (1.0 - t) + colors::DAY_HEADER_MID.r() as f32 * t) as u8,
        (colors::DAY_HEADER_START.g() as f32 * (1.0 - t) + colors::DAY_HEADER_MID.g() as f32 * t) as u8,
        (colors::DAY_HEADER_START.b() as f32 * (1.0 - t) + colors::DAY_HEADER_MID.b() as f32 * t) as u8,
    );
    
    painter.rect_filled(rect, egui::Rounding::same(5.0), color);
}

/// Get table header color that matches calendar day header style
pub fn get_table_header_color(header_index: usize) -> egui::Color32 {
    // Use the same pink-to-purple gradient as the calendar day headers
    let t = (header_index as f32) / 3.0; // 0.0 to 1.0 across 4 headers (0, 1, 2, 3)
    
    // Same start and end points as the calendar gradient
    let pink = colors::DAY_HEADER_START; // Light pink (255, 182, 193)
    let purple = colors::DAY_HEADER_MID; // Purple (186, 85, 211)
    
    // Interpolate between pink and purple
    Color32::from_rgb(
        (pink.r() as f32 * (1.0 - t) + purple.r() as f32 * t) as u8,
        (pink.g() as f32 * (1.0 - t) + purple.g() as f32 * t) as u8,
        (pink.b() as f32 * (1.0 - t) + purple.b() as f32 * t) as u8,
    )
} 