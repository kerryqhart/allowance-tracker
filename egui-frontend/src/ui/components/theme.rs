//! # Theme Configuration
//!
//! This module provides centralized color and style configuration for the allowance tracker app.
//! All visual styling should use these constants to ensure consistency and easy theme management.
//!
//! ## Future Theming Support
//! This module is designed to support multiple themes/skins in the future. Currently it provides
//! the default "Kid-Friendly" theme, but the structure allows for easy addition of new themes.
//!
//! ## Usage
//! ```rust
//! use crate::ui::components::theme::{Theme, CURRENT_THEME};
//! 
//! // Use theme colors
//! let color = CURRENT_THEME.interactive.hover_border;
//! ```

use eframe::egui::Color32;

/// Main theme configuration structure
#[derive(Debug, Clone)]
pub struct Theme {
    /// Interactive element colors (buttons, dropdowns, etc.)
    pub interactive: InteractiveColors,
    /// Background and layout colors
    pub layout: LayoutColors,
    /// Text and typography colors
    pub typography: TypographyColors,
    /// Calendar-specific colors
    pub calendar: CalendarColors,
    /// Table-specific colors
    pub table: TableColors,
}

/// Colors for interactive elements (buttons, dropdowns, hover states)
#[derive(Debug, Clone)]
pub struct InteractiveColors {
    /// Primary hover border color - used for consistent outline across all interactive elements
    pub hover_border: Color32,
    /// Secondary hover border color (alternative for special cases)
    pub hover_border_secondary: Color32,
    /// Hover background color (semi-transparent)
    pub hover_background: Color32,
    /// Active/selected background color
    pub active_background: Color32,
    /// Inactive background color
    pub inactive_background: Color32,
    /// Button border colors
    pub button_border_normal: Color32,
    pub button_border_active: Color32,
}

/// Layout and container colors
#[derive(Debug, Clone)]
pub struct LayoutColors {
    /// Gradient background colors
    pub gradient_top: Color32,
    pub gradient_bottom: Color32,
    /// Card and container colors
    pub card_background: Color32,
    pub card_shadow: Color32,
    pub card_border: Color32,
}

/// Text and typography colors
#[derive(Debug, Clone)]
pub struct TypographyColors {
    /// Primary text color (main content)
    pub primary: Color32,
    /// Secondary text color (less prominent)
    pub secondary: Color32,
    /// Heading text color
    pub heading: Color32,
    /// Active/selected text color
    pub active: Color32,
    /// White text (for dark backgrounds)
    pub white: Color32,
}

/// Calendar-specific colors
#[derive(Debug, Clone)]
pub struct CalendarColors {
    /// Current day outline color
    pub today_border: Color32,
    /// Selected day colors
    pub selected_background: Color32,
    pub selected_border: Color32,
    /// Day header gradient colors
    pub header_start: Color32,
    pub header_mid: Color32,
    pub header_end: Color32,
    /// Transaction chip colors
    pub income_chip: Color32,
    pub expense_chip: Color32,
    /// Day type backgrounds
    pub current_month_bg: Color32,
    pub filler_day_bg: Color32,
}

/// Table-specific colors
#[derive(Debug, Clone)]
pub struct TableColors {
    /// Header background colors (gradient across columns)
    pub header_colors: [Color32; 4],
    /// Row colors
    pub row_even: Color32,
    pub row_odd: Color32,
    /// Border colors
    pub border: Color32,
}

/// The current active theme - Kid-Friendly theme with purple accents
pub const CURRENT_THEME: Theme = Theme {
    interactive: InteractiveColors {
        // PRIMARY: Purple-blue color for consistent hover outlines across all buttons
        hover_border: Color32::from_rgb(126, 120, 229),
        // SECONDARY: Original pink color (kept for special cases)
        hover_border_secondary: Color32::from_rgb(232, 150, 199), 
        // Semi-transparent white for hover backgrounds
        hover_background: Color32::from_rgba_premultiplied(255, 255, 255, 20),
        // Active button background (blue)
        active_background: Color32::from_rgb(79, 109, 245),
        // Inactive button background (light gray)
        inactive_background: Color32::from_rgb(248, 248, 248),
        // Button borders
        button_border_normal: Color32::from_rgb(220, 220, 220),
        button_border_active: Color32::from_rgb(200, 200, 200),
    },
    layout: LayoutColors {
        // Background gradient (pink to blue)
        gradient_top: Color32::from_rgb(255, 182, 193),
        gradient_bottom: Color32::from_rgb(173, 216, 230),
        // Card styling
        card_background: Color32::WHITE,
        card_shadow: Color32::from_rgba_premultiplied(0, 0, 0, 20),
        card_border: Color32::from_rgb(220, 220, 220),
    },
    typography: TypographyColors {
        // Text colors
        primary: Color32::from_rgb(60, 60, 60),
        secondary: Color32::from_rgb(80, 80, 80),
        heading: Color32::from_rgb(70, 70, 70),
        active: Color32::from_rgb(79, 109, 245),
        white: Color32::WHITE,
    },
    calendar: CalendarColors {
        // Today's date gets the pink outline for visibility
        today_border: Color32::from_rgb(232, 150, 199),
        // Selected day styling
        selected_background: Color32::from_rgba_premultiplied(230, 190, 235, 140),
        selected_border: Color32::from_rgb(199, 112, 221),
        // Day headers gradient
        header_start: Color32::from_rgb(255, 182, 193),
        header_mid: Color32::from_rgb(186, 85, 211),
        header_end: Color32::from_rgb(135, 206, 235),
        // Transaction chips
        income_chip: Color32::from_rgb(34, 139, 34),
        expense_chip: Color32::from_rgb(220, 20, 60),
        // Day backgrounds
        current_month_bg: Color32::from_rgba_premultiplied(255, 255, 255, 55),
        filler_day_bg: Color32::from_rgba_premultiplied(120, 120, 120, 120),
    },
    table: TableColors {
        // Header gradient colors (pink to purple across 4 columns)
        header_colors: [
            Color32::from_rgb(255, 182, 193), // Pink
            Color32::from_rgb(232, 150, 199), // Light pink-purple
            Color32::from_rgb(208, 117, 205), // Medium pink-purple  
            Color32::from_rgb(186, 85, 211),  // Purple
        ],
        // Row styling
        row_even: Color32::WHITE,
        row_odd: Color32::from_rgba_premultiplied(248, 248, 248, 255),
        border: Color32::from_rgb(220, 220, 220),
    },
};

/// Helper functions for common styling patterns
impl Theme {
    /// Get hover border color for interactive elements
    pub fn hover_border(&self) -> Color32 {
        self.interactive.hover_border
    }
    
    /// Get hover background color for interactive elements
    pub fn hover_background(&self) -> Color32 {
        self.interactive.hover_background
    }
    
    /// Get calendar day header color by index (0-6)
    pub fn calendar_header_color(&self, day_index: usize) -> Color32 {
        let t = (day_index as f32 / 6.0).clamp(0.0, 1.0);
        
        // Interpolate between pink and purple
        Color32::from_rgb(
            (self.calendar.header_start.r() as f32 * (1.0 - t) + self.calendar.header_mid.r() as f32 * t) as u8,
            (self.calendar.header_start.g() as f32 * (1.0 - t) + self.calendar.header_mid.g() as f32 * t) as u8,
            (self.calendar.header_start.b() as f32 * (1.0 - t) + self.calendar.header_mid.b() as f32 * t) as u8,
        )
    }
    
    /// Get table header color by index (0-3)
    pub fn table_header_color(&self, header_index: usize) -> Color32 {
        let index = header_index.min(3); // Clamp to valid range
        self.table.header_colors[index]
    }
}

/// Convenience constants for the most commonly used colors
pub mod colors {
    use super::CURRENT_THEME;
    use eframe::egui::Color32;
    
    // Interactive colors - most commonly used
    pub const HOVER_BORDER: Color32 = CURRENT_THEME.interactive.hover_border;
    pub const HOVER_BACKGROUND: Color32 = CURRENT_THEME.interactive.hover_background;
    pub const ACTIVE_BACKGROUND: Color32 = CURRENT_THEME.interactive.active_background;
    pub const INACTIVE_BACKGROUND: Color32 = CURRENT_THEME.interactive.inactive_background;
    
    // Typography colors
    pub const TEXT_PRIMARY: Color32 = CURRENT_THEME.typography.primary;
    pub const TEXT_SECONDARY: Color32 = CURRENT_THEME.typography.secondary;
    pub const TEXT_WHITE: Color32 = CURRENT_THEME.typography.white;
    
    // Layout colors
    pub const CARD_BACKGROUND: Color32 = CURRENT_THEME.layout.card_background;
    pub const CARD_BORDER: Color32 = CURRENT_THEME.layout.card_border;
} 