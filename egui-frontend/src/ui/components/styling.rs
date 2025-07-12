use eframe::egui;

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