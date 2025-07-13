use eframe::egui;
use egui::Color32;

/// Setup kid-friendly UI styling
pub fn setup_kid_friendly_style(ctx: &egui::Context) {
    ctx.set_style({
        let mut style = (*ctx.style()).clone();
        
        // Bright, fun colors
        style.visuals.window_fill = egui::Color32::from_rgb(240, 248, 255); // Light blue background
        style.visuals.panel_fill = egui::Color32::from_rgb(250, 250, 250); // Light gray panels
        style.visuals.button_frame = true;
        
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

/// Draw a vertical gradient background
pub fn draw_gradient_background(ui: &mut egui::Ui, rect: egui::Rect) {
    let painter = ui.painter();
    
    // Create a simple linear gradient effect by drawing multiple horizontal stripes
    let num_stripes = 100;
    let stripe_height = rect.height() / num_stripes as f32;
    
    for i in 0..num_stripes {
        let t = i as f32 / (num_stripes - 1) as f32; // 0.0 to 1.0
        
        // Interpolate between top and bottom colors
        let color = Color32::from_rgb(
            (colors::GRADIENT_TOP.r() as f32 * (1.0 - t) + colors::GRADIENT_BOTTOM.r() as f32 * t) as u8,
            (colors::GRADIENT_TOP.g() as f32 * (1.0 - t) + colors::GRADIENT_BOTTOM.g() as f32 * t) as u8,
            (colors::GRADIENT_TOP.b() as f32 * (1.0 - t) + colors::GRADIENT_BOTTOM.b() as f32 * t) as u8,
        );
        
        let stripe_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + i as f32 * stripe_height),
            egui::vec2(rect.width(), stripe_height + 1.0), // +1 to avoid gaps
        );
        
        painter.rect_filled(stripe_rect, egui::Rounding::ZERO, color);
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
    
    // Interpolate between pink, purple, and blue
    let color = if t < 0.5 {
        // Pink to Purple
        let local_t = t * 2.0;
        Color32::from_rgb(
            (colors::DAY_HEADER_START.r() as f32 * (1.0 - local_t) + colors::DAY_HEADER_MID.r() as f32 * local_t) as u8,
            (colors::DAY_HEADER_START.g() as f32 * (1.0 - local_t) + colors::DAY_HEADER_MID.g() as f32 * local_t) as u8,
            (colors::DAY_HEADER_START.b() as f32 * (1.0 - local_t) + colors::DAY_HEADER_MID.b() as f32 * local_t) as u8,
        )
    } else {
        // Purple to Blue
        let local_t = (t - 0.5) * 2.0;
        Color32::from_rgb(
            (colors::DAY_HEADER_MID.r() as f32 * (1.0 - local_t) + colors::DAY_HEADER_END.r() as f32 * local_t) as u8,
            (colors::DAY_HEADER_MID.g() as f32 * (1.0 - local_t) + colors::DAY_HEADER_END.g() as f32 * local_t) as u8,
            (colors::DAY_HEADER_MID.b() as f32 * (1.0 - local_t) + colors::DAY_HEADER_END.b() as f32 * local_t) as u8,
        )
    };
    
    painter.rect_filled(rect, egui::Rounding::same(5.0), color);
} 