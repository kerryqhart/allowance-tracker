use chrono::{NaiveDate, Datelike, Weekday};

/// Calculate how many transaction chips can fit in the available calendar day space
/// Returns (chips_that_fit, needs_ellipsis)
pub fn calculate_transaction_display_limit(available_height: f32, total_transaction_count: usize) -> (usize, bool) {
    // Constants based on actual chip measurements
    const CHIP_HEIGHT: f32 = 18.0;
    const CHIP_SPACING: f32 = 1.0;
    const HEADER_SPACE: f32 = 38.0; // Realistic estimate for day number + balance + spacing
    const BOTTOM_PADDING: f32 = 8.0; // Reasonable padding
    
    // Calculate available space for transaction area
    let available_for_transactions = available_height - HEADER_SPACE - BOTTOM_PADDING;
    
    // Calculate maximum chips that can physically fit (ignoring ellipsis for now)
    let max_chips_that_fit = if available_for_transactions < CHIP_HEIGHT {
        0
    } else {
        // How many full chips + spacing can fit
        ((available_for_transactions + CHIP_SPACING) / (CHIP_HEIGHT + CHIP_SPACING)).floor() as usize
    };
    
    // Now apply the logic based on physical capacity vs available transactions
    if max_chips_that_fit == 0 {
        // No space for anything
        (0, false)
    } else if max_chips_that_fit == 1 {
        // Space for only 1 chip
        if total_transaction_count <= 1 {
            // 1 or 0 transactions → show them all
            (total_transaction_count, false)
        } else {
            // >1 transactions → show "..." only
            (0, true)
        }
    } else {
        // Space for 2+ chips
        if total_transaction_count <= max_chips_that_fit {
            // All transactions fit → show them all
            (total_transaction_count, false)
        } else {
            // More transactions than space → show (max-1) + ellipsis
            let chips_to_show = max_chips_that_fit - 1;
            (chips_to_show, true)
        }
    }
}

/// Calculate the height needed for the calendar grid
pub fn calculate_calendar_grid_height(selected_year: i32, selected_month: u32, day_height: f32) -> f32 {
    // Calculate number of weeks needed for the current month
    let first_day = match NaiveDate::from_ymd_opt(selected_year, selected_month, 1) {
        Some(date) => date,
        None => return day_height * 6.0, // fallback to 6 weeks
    };
    
    let days_in_month = match first_day.with_day(1) {
        Some(first) => {
            let next_month = if selected_month == 12 {
                first.with_year(selected_year + 1).unwrap().with_month(1).unwrap()
            } else {
                first.with_month(selected_month + 1).unwrap()
            };
            (next_month - chrono::Duration::days(1)).day()
        }
        None => 31, // fallback
    };
    
    let first_day_offset = match first_day.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    
    // Calculate number of weeks needed
    let total_cells = first_day_offset + days_in_month as usize;
    let weeks_needed = (total_cells + 6) / 7; // Round up
    
    weeks_needed as f32 * day_height + 10.0 // Add some spacing
}

/// Calculate optimal calendar dimensions based on available space
pub fn calculate_calendar_dimensions(available_rect: eframe::egui::Rect, calendar_days_count: usize) -> CalendarDimensions {
    let content_width = available_rect.width() - 40.0;
    let calendar_width = content_width;
    
    // Calculate cell width from horizontal space
    let total_spacing = super::styling::CALENDAR_CARD_SPACING * 6.0;
    let cell_width = (calendar_width - total_spacing) / 7.0;
    
    // Calculate optimal calendar height
    let actual_available_rect = available_rect;
    let final_card_height = actual_available_rect.height() - 40.0; // 40px total: 20px bottom margin + 20px internal padding
    
    // Calculate dynamic cell height based on calendar data
    let header_height = super::styling::header::HEADER_HEIGHT;
    let calendar_container_padding = 20.0;
    
    let rows_needed = (calendar_days_count as f32 / 7.0).ceil();
    let vertical_spacing = super::styling::CALENDAR_CARD_SPACING * (rows_needed - 1.0);
    let available_height_for_cells = final_card_height - calendar_container_padding - header_height - vertical_spacing;
    let dynamic_cell_height = (available_height_for_cells / rows_needed).max(40.0).min(200.0);
    
    CalendarDimensions {
        content_width,
        calendar_width,
        cell_width,
        cell_height: dynamic_cell_height,
        final_card_height,
        header_height,
        rows_needed: rows_needed as usize,
    }
}

/// Represents calculated calendar dimensions for responsive layout
pub struct CalendarDimensions {
    /// Total content width available
    pub content_width: f32,
    /// Width allocated for calendar grid
    pub calendar_width: f32,
    /// Width of each calendar cell
    pub cell_width: f32,
    /// Height of each calendar cell (dynamic based on content)
    pub cell_height: f32,
    /// Total height for the calendar card
    pub final_card_height: f32,
    /// Height for day headers
    pub header_height: f32,
    /// Number of rows needed for this month
    pub rows_needed: usize,
}

/// Calculate grid positioning for calendar cells
pub fn calculate_cell_position(day_index: usize, cell_width: f32, cell_height: f32, spacing: f32) -> (f32, f32) {
    let row = day_index / 7;
    let col = day_index % 7;
    
    let x = col as f32 * (cell_width + spacing);
    let y = row as f32 * (cell_height + spacing);
    
    (x, y)
}

/// Calculate expanded row height for calendar days with many transactions
pub fn calculate_expanded_row_height(base_height: f32, transaction_count: usize) -> f32 {
    if transaction_count == 0 {
        return base_height;
    }
    
    let header_space = 45.0; // Day number + balance + spacing
    let bottom_padding = 10.0;
    let collapse_button_height = 25.0; // Space for collapse button
    let chip_height = 18.0;
    let chip_spacing = 1.0;
    
    let chips_height = (transaction_count as f32 * chip_height) + ((transaction_count - 1) as f32 * chip_spacing);
    let needed_height = header_space + chips_height + collapse_button_height + bottom_padding;
    
    needed_height.max(base_height) // Don't go smaller than normal
}

/// Calculate responsive font sizes based on available space
pub fn calculate_responsive_font_sizes(cell_width: f32, is_compact: bool) -> ResponsiveFontSizes {
    let scale_factor = if is_compact { 0.9 } else { 1.0 };
    
    ResponsiveFontSizes {
        day_number: ((cell_width * 0.15).max(14.0).min(18.0) * scale_factor),
        balance: ((cell_width * 0.12).max(10.0).min(14.0) * scale_factor),
        chip: ((cell_width * 0.12).max(9.0).min(12.0) * scale_factor),
        header: (12.0 * scale_factor),
    }
}

/// Represents responsive font sizes for different calendar elements
pub struct ResponsiveFontSizes {
    /// Font size for day numbers
    pub day_number: f32,
    /// Font size for balance text
    pub balance: f32,
    /// Font size for transaction chips
    pub chip: f32,
    /// Font size for day headers
    pub header: f32,
} 