use eframe::egui;

/// Consistent spacing for all calendar elements - controls gaps between day cards, headers, etc.
/// This value determines the visual tightness/looseness of the calendar layout.
pub const CALENDAR_CARD_SPACING: f32 = 5.0; // Spacing in pixels between calendar elements

/// Get the appropriate font family for calendar rendering
/// Falls back to Proportional if Chalkboard is not available
pub fn get_calendar_font_family(ctx: &egui::Context) -> egui::FontFamily {
    if ctx.fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("Chalkboard".into()))) {
        egui::FontFamily::Name("Chalkboard".into())
    } else {
        egui::FontFamily::Proportional
    }
}

/// Get the day number font size based on layout and cell width
pub fn get_day_number_font_size(is_grid_layout: bool, cell_width: f32) -> f32 {
    if is_grid_layout {
        16.0
    } else {
        (cell_width * 0.15).max(14.0).min(18.0)
    }
}

/// Get the balance font size based on layout and cell width
pub fn get_balance_font_size(is_grid_layout: bool, cell_width: f32) -> f32 {
    if is_grid_layout {
        11.0
    } else {
        (cell_width * 0.12).max(10.0).min(14.0)
    }
}

/// Get the chip font size based on layout and cell width
pub fn get_chip_font_size(is_grid_layout: bool, cell_width: f32) -> f32 {
    if is_grid_layout {
        10.0
    } else {
        (cell_width * 0.12).max(9.0).min(12.0)
    }
}

/// Calculate chip dimensions based on layout
/// Returns (chip_width, chip_height, chip_font_size)
pub fn calculate_chip_dimensions(is_grid_layout: bool, available_width: f32) -> (f32, f32, f32) {
    if is_grid_layout {
        let chip_width = (available_width - 10.0).min(120.0);
        let chip_height = 18.0;
        let font_size = 10.0;
        (chip_width, chip_height, font_size)
    } else {
        let chip_width = available_width * 0.85;
        let chip_height = 18.0; // Force consistent height
        let font_size = (available_width * 0.12).max(9.0).min(12.0);
        (chip_width, chip_height, font_size)
    }
}

/// Calendar header styling constants
pub mod header {
    /// Height for day headers
    pub const HEADER_HEIGHT: f32 = 30.0;
    
    /// Font size for day headers
    pub const HEADER_FONT_SIZE: f32 = 12.0;
    
    /// Background color for day headers
    pub fn background_color() -> eframe::egui::Color32 {
        eframe::egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180)
    }
    
    /// Border color for day headers
    pub fn border_color() -> eframe::egui::Color32 {
        eframe::egui::Color32::from_rgba_unmultiplied(150, 150, 150, 200)
    }
}

/// Tooltip styling constants and functions
pub mod tooltip {
    use eframe::egui;
    
    /// Tooltip font size
    pub const FONT_SIZE: f32 = 12.0;
    
    /// Tooltip padding
    pub const PADDING: egui::Vec2 = egui::vec2(8.0, 6.0);
    
    /// Maximum tooltip width
    pub const MAX_WIDTH: f32 = 200.0;
    
    /// Default offset from cursor
    pub const DEFAULT_OFFSET: egui::Vec2 = egui::vec2(10.0, -25.0);
    
    /// Tooltip background color
    pub fn background_color() -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(40, 40, 40, 240) // Dark semi-transparent
    }
    
    /// Tooltip text color
    pub fn text_color() -> egui::Color32 {
        egui::Color32::WHITE
    }
    
    /// Tooltip border color
    pub fn border_color() -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(100, 100, 100, 200)
    }
    
    /// Tooltip shadow color
    pub fn shadow_color() -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 60)
    }
}

/// Action icon styling constants
pub mod action_icons {
    use eframe::egui;
    
    /// Size of action glyphs
    pub const GLYPH_SIZE: egui::Vec2 = egui::vec2(48.0, 22.0);
    
    /// Spacing between glyphs
    pub const GLYPH_SPACING: f32 = 6.0;
    
    /// Vertical offset from day cell
    pub const VERTICAL_OFFSET: f32 = 25.0;
    
    /// Outline color for action icons (same as selected day)
    pub fn outline_color() -> egui::Color32 {
        egui::Color32::from_rgb(199, 112, 221) // Purple-pink
    }
    
    /// Background color for action icons
    pub fn background_color() -> egui::Color32 {
        egui::Color32::WHITE
    }
    
    /// Text color for action icons
    pub fn text_color() -> egui::Color32 {
        outline_color() // Same pink as outline
    }
} 