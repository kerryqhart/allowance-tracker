//! # Calendar Renderer Module
//!
//! This module handles all calendar-related rendering functionality for the allowance tracker app.
//! It provides a visual, interactive calendar view where users can see their transactions
//! displayed on specific dates.
//!
//! ## Key Functions:
//! - `draw_calendar_section_with_toggle()` - Main calendar view with Sunday-first layout
//! - `navigate_month()` - Handle month navigation (previous/next)
//! - `get_day_header_color()` - Calculate gradient colors for day headers
//! - `draw_calendar_days_responsive()` - Render calendar grid with responsive design
//! - `calculate_calendar_grid_height()` - Calculate required height for calendar
//!
//! ## Purpose:
//! This module provides the primary visual interface for the allowance tracker, showing
//! transactions in a calendar format that's intuitive and kid-friendly. It handles:
//! - Month navigation and date calculation
//! - Transaction placement on calendar days
//! - Responsive design for different screen sizes
//! - Visual styling with gradients and hover effects
//!
//! ## Features:
//! - Interactive month navigation
//! - Transaction chips displayed on calendar days
//! - Responsive grid layout
//! - Kid-friendly visual design with gradients
//! - Proper date handling using chrono library

use eframe::egui;
use chrono::{NaiveDate, Datelike, Weekday};
use shared::Transaction;
use crate::ui::app_state::{AllowanceTrackerApp, OverlayType};

/// Represents the different types of day menu glyphs that can be displayed above a selected day
#[derive(Debug, Clone, PartialEq)]
pub enum DayMenuGlyph {
    AddMoney,
    SpendMoney,
}

impl DayMenuGlyph {
    /// Get the text to display for this glyph
    pub fn text(&self) -> &'static str {
        match self {
            DayMenuGlyph::AddMoney => "+$",
            DayMenuGlyph::SpendMoney => "-$",
        }
    }
    
    /// Get the overlay type this glyph should activate
    pub fn overlay_type(&self) -> OverlayType {
        match self {
            DayMenuGlyph::AddMoney => OverlayType::AddMoney,
            DayMenuGlyph::SpendMoney => OverlayType::SpendMoney,
        }
    }
    
    /// Get all available glyphs in order
    pub fn all() -> Vec<DayMenuGlyph> {
        vec![
            DayMenuGlyph::AddMoney,
            DayMenuGlyph::SpendMoney,
        ]
    }
    
    /// Get glyphs that should be shown for a specific date based on business rules
    pub fn for_date(date: NaiveDate) -> Vec<DayMenuGlyph> {
        let today = chrono::Local::now().date_naive();
        
        // Don't show glyphs for future dates (can't future-date transactions)
        if date > today {
            return Vec::new();
        }
        
        // Don't show glyphs for dates older than 45 days (prevent arbitrary backdating)
        let cutoff_date = today - chrono::Duration::days(45);
        if date < cutoff_date {
            return Vec::new();
        }
        
        // For current day and valid past days, show income and expense glyphs
        Self::all()
    }
}

/// Consistent spacing for all calendar elements - controls gaps between day cards, headers, etc.
/// This value determines the visual tightness/looseness of the calendar layout.
const CALENDAR_CARD_SPACING: f32 = 5.0; // Spacing in pixels between calendar elements

/// Represents the type of calendar day for clear distinction between different day types
#[derive(Debug, Clone, PartialEq)]
pub enum CalendarDayType {
    /// A day in the current month being displayed
    CurrentMonth,
    /// A filler day (padding) from previous or next month to fill the calendar grid
    FillerDay,
}

impl CalendarDayType {
    /// Get the background color for this day type
    pub fn background_color(&self, is_today: bool) -> egui::Color32 {
        if is_today {
            // Light yellow tint for today (10% more opacity)
            egui::Color32::from_rgba_unmultiplied(255, 248, 220, 110)
        } else {
            match self {
                CalendarDayType::CurrentMonth => {
                    // Semi-transparent white background (10% more opacity)
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 55)
                }
                CalendarDayType::FillerDay => {
                    // Darker gray for filler days (increased opacity for better visibility)
                    egui::Color32::from_rgba_unmultiplied(120, 120, 120, 120)
                }
            }
        }
    }

    /// Get the border color for this day type
    pub fn border_color(&self, is_today: bool) -> egui::Color32 {
        if is_today {
            // Pink outline for better visibility against gradient background
            egui::Color32::from_rgb(232, 150, 199)
        } else {
            match self {
                CalendarDayType::CurrentMonth => {
                    // Normal border
                    egui::Color32::from_rgba_unmultiplied(200, 200, 200, 100)
                }
                CalendarDayType::FillerDay => {
                    // Lighter border for filler days (increased opacity for better visibility)
                    egui::Color32::from_rgba_unmultiplied(150, 150, 150, 140)
                }
            }
        }
    }

    /// Get the day number text color for this day type
    pub fn day_text_color(&self) -> egui::Color32 {
        // Use normal colors for all days
        match self {
            CalendarDayType::CurrentMonth => {
                // Bold black for current month days (including today)
                egui::Color32::BLACK
            }
            CalendarDayType::FillerDay => {
                // Gray for filler days
                egui::Color32::from_rgb(150, 150, 150)
            }
        }
    }

    /// Get the balance text color for this day type
    pub fn balance_text_color(&self) -> egui::Color32 {
        match self {
            CalendarDayType::CurrentMonth => {
                // Normal gray
                egui::Color32::GRAY
            }
            CalendarDayType::FillerDay => {
                // More subdued gray for filler day balance
                egui::Color32::from_rgb(120, 120, 120)
            }
        }
    }
}

/// Represents the type of calendar transaction chip for visual distinction
#[derive(Debug, Clone, PartialEq)]
pub enum CalendarChipType {
    /// Negative amount transaction (completed)
    Expense,
    /// Positive amount transaction (completed)
    Income,
    /// Future allowance transaction (estimated)
    FutureAllowance,
}

impl CalendarChipType {
    /// Get the primary color for this chip type
    pub fn primary_color(&self) -> egui::Color32 {
        match self {
            CalendarChipType::Expense => egui::Color32::from_rgb(128, 128, 128), // Gray for expenses
            CalendarChipType::Income => egui::Color32::from_rgb(46, 160, 67), // Green for income
            CalendarChipType::FutureAllowance => egui::Color32::from_rgb(46, 160, 67), // Green for future allowances
        }
    }
    
    /// Get the text color for this chip type
    pub fn text_color(&self) -> egui::Color32 {
        self.primary_color() // Use same color as border for text
    }
    
    /// Whether this chip type should use a dotted border
    pub fn uses_dotted_border(&self) -> bool {
        matches!(self, CalendarChipType::FutureAllowance)
    }
}

/// Represents a transaction chip displayed on the calendar
#[derive(Debug, Clone)]
pub struct CalendarChip {
    /// The type of chip (expense, income, or future allowance)
    pub chip_type: CalendarChipType,
    /// The original transaction data
    pub transaction: Transaction,
    /// Pre-formatted display amount (e.g., "+$5.00", "-$2.50")
    pub display_amount: String,
}

impl CalendarChip {
    /// Create a new CalendarChip from a transaction
    pub fn from_transaction(transaction: Transaction, is_grid_layout: bool) -> Self {
        // Determine chip type based on transaction
        let chip_type = match transaction.transaction_type {
            shared::TransactionType::Income => CalendarChipType::Income,
            shared::TransactionType::Expense => CalendarChipType::Expense,
            shared::TransactionType::FutureAllowance => CalendarChipType::FutureAllowance,
        };
        
        // Format display amount based on type and layout
        let display_amount = if transaction.amount > 0.0 {
            if is_grid_layout {
                format!("+${:.2}", transaction.amount)
            } else {
                format!("+${:.0}", transaction.amount)
            }
        } else {
            if is_grid_layout {
                format!("-${:.2}", transaction.amount.abs())
            } else {
                format!("-${:.0}", transaction.amount.abs())
            }
        };
        
        Self {
            chip_type,
            transaction,
            display_amount,
        }
    }
    
    /// Convert a vector of transactions to calendar chips
    pub fn from_transactions(transactions: Vec<Transaction>, is_grid_layout: bool) -> Vec<Self> {
        transactions.into_iter()
            .map(|transaction| Self::from_transaction(transaction, is_grid_layout))
            .collect()
    }
}

/// Represents a single day in the calendar with its associated state and rendering logic
pub struct CalendarDay {
    /// The day number (1-31)
    pub day_number: u32,
    /// The full date for this day
    pub date: NaiveDate,
    /// Whether this day is today
    pub is_today: bool,
    /// The type of day (current month or filler day)
    pub day_type: CalendarDayType,
    /// Transactions that occurred on this day
    pub transactions: Vec<Transaction>,
    /// The balance at the end of this day (for current month days only)
    pub balance: Option<f64>,
}

/// Configuration for calendar day rendering
pub struct RenderConfig {
    pub is_grid_layout: bool,
    pub max_transactions: Option<usize>,
    pub enable_click_handler: bool,
    pub is_selected: bool,
    // Transaction selection state (for deletion mode)
    pub transaction_selection_mode: bool,
    pub selected_transaction_ids: std::collections::HashSet<String>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            is_grid_layout: false,
            max_transactions: Some(2),
            enable_click_handler: false,
            is_selected: false,
            transaction_selection_mode: false,
            selected_transaction_ids: std::collections::HashSet::new(),
        }
    }
}

impl CalendarDay {
    /// Create a new CalendarDay instance
    pub fn new(day_number: u32, date: NaiveDate, is_today: bool, day_type: CalendarDayType) -> Self {
        Self {
            day_number,
            date,
            is_today,
            day_type,
            transactions: Vec::new(),
            balance: None,
        }
    }

    /// Add a transaction to this day
    pub fn add_transaction(&mut self, transaction: Transaction) {
        // Update balance from the transaction (this is the balance after the transaction)
        self.balance = Some(transaction.balance);
        self.transactions.push(transaction);
    }
    
    /// Render this calendar day with configurable styling
    pub fn render(&self, ui: &mut egui::Ui, width: f32, height: f32) -> egui::Response {
        let (response, _) = self.render_with_config(ui, width, height, &RenderConfig::default());
        response
    }
    
    /// Render this calendar day for grid layout
    pub fn render_grid(&self, ui: &mut egui::Ui, width: f32, height: f32) -> egui::Response {
        let (response, _) = self.render_with_config(ui, width, height, &RenderConfig {
            is_grid_layout: true,
            max_transactions: None,
            enable_click_handler: true,
            is_selected: false, // Default to not selected - this method doesn't know about selection
            transaction_selection_mode: false,
            selected_transaction_ids: std::collections::HashSet::new(),
        });
        response
    }
    
    /// Render this calendar day with specified configuration
    /// Returns (response, clicked_transaction_ids) where clicked_transaction_ids contains IDs of transactions whose checkboxes were clicked
    pub fn render_with_config(&self, ui: &mut egui::Ui, width: f32, height: f32, config: &RenderConfig) -> (egui::Response, Vec<String>) {
        // Initialize variable to collect checkbox clicks
        let mut clicked_transaction_ids = Vec::new();
        
        // Allocate space for this day cell and get hover/click detection - same approach as chips
        let (cell_rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover().union(egui::Sense::click()));
        let is_hovered = response.hovered();
        
        // Draw shadow first (behind everything else) for today's date
        if self.is_today {
            let shadow_rect = egui::Rect::from_min_size(
                cell_rect.min + egui::vec2(2.0, 2.0),
                cell_rect.size()
            );
            ui.painter().rect_filled(
                shadow_rect,
                egui::Rounding::same(2.0),
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 30) // Subtle shadow
            );
        }
        
        // Draw background for the day cell using centralized color scheme with hover effect
        let base_bg_color = self.day_type.background_color(self.is_today);
        let bg_color = if config.is_selected {
            // Selected day gets a purple-pink tint matching the Create Goal button
            egui::Color32::from_rgba_unmultiplied(230, 190, 235, 140) // Purple-pink for selection
        } else if is_hovered {
            // Make more opaque when hovered - same approach as chips
            if self.is_today {
                // For today, make the yellow background more solid
                egui::Color32::from_rgba_unmultiplied(255, 248, 220, 180) // More opaque yellow
            } else {
                match self.day_type {
                    CalendarDayType::CurrentMonth => {
                        // Make current month days more opaque white
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 120) // More opaque white
                    }
                    CalendarDayType::FillerDay => {
                        // Make filler days more opaque gray
                        egui::Color32::from_rgba_unmultiplied(120, 120, 120, 160) // More opaque gray
                    }
                }
            }
        } else {
            base_bg_color
        };
        
        ui.painter().rect_filled(
            cell_rect,
            egui::Rounding::same(2.0),
            bg_color
        );
        
        // Draw border around the day cell using centralized color scheme
        if config.is_selected {
            // Selected day gets a purple-pink border matching the Create Goal button
            ui.painter().rect_stroke(
                cell_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(2.0, egui::Color32::from_rgb(199, 112, 221)) // Purple-pink border for selection
            );
        } else if self.is_today {
            // Double outline for today: white inner + dark outer for high visibility
            // Draw white inner outline first
            ui.painter().rect_stroke(
                cell_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(2.0, egui::Color32::WHITE)
            );
            
            // Draw dark outer outline
            let outer_rect = egui::Rect::from_min_size(
                cell_rect.min - egui::vec2(1.0, 1.0),
                cell_rect.size() + egui::vec2(2.0, 2.0)
            );
            ui.painter().rect_stroke(
                outer_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(2.0, self.day_type.border_color(self.is_today))
            );
        } else {
            // Normal single outline for other days
            let border_color = self.day_type.border_color(self.is_today);
            ui.painter().rect_stroke(
                cell_rect,
                egui::Rounding::same(2.0),
                egui::Stroke::new(0.5, border_color)
            );
        }
        
        // Draw the content within the allocated cell rectangle
        let ui_result = ui.allocate_ui_at_rect(cell_rect, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.set_height(height);
                
                // Add small padding inside the cell
                ui.add_space(4.0);
                
                // Top row: Day number (left) and Balance (right)
                ui.horizontal(|ui| {
                    ui.set_width(width - 8.0); // Account for padding
                    
                    // Day number in upper left (only for current month days)
                    if matches!(self.day_type, CalendarDayType::CurrentMonth) {
                        let day_font_size = if config.is_grid_layout {
                            16.0
                        } else {
                            (width * 0.15).max(14.0).min(18.0)
                        };
                        
                        // Day number text color using centralized color scheme
                        let day_text_color = self.day_type.day_text_color();
                        
                        // Create the rich text with emphasis for today
                        let rich_text = egui::RichText::new(self.day_number.to_string())
                            .font(egui::FontId::new(day_font_size, egui::FontFamily::Proportional))
                            .color(day_text_color)
                            .strong();
                        
                        if self.is_today {
                            // Use manual underline for today's date (more subtle than native)
                            let rich_text_bold = egui::RichText::new(self.day_number.to_string())
                                .font(egui::FontId::new(day_font_size, egui::FontFamily::Proportional))
                                .color(day_text_color)
                                .strong();
                            
                            // Render the text first - disable selection to prevent dropdown interference
                            let label_response = ui.add(egui::Label::new(rich_text_bold).selectable(false));
                            
                            // Draw manual underline beneath the text (shorter and thinner)
                            let text_rect = label_response.rect;
                            let underline_y = text_rect.bottom() + 1.0; // 1px below text
                            let left_padding = 3.0; // More padding on left side
                            let right_padding = 2.0; // Less padding on right side
                            let underline_color = egui::Color32::from_rgb(80, 80, 80); // Dark gray
                            ui.painter().line_segment(
                                [
                                    egui::pos2(text_rect.left() + left_padding, underline_y),
                                    egui::pos2(text_rect.right() - right_padding, underline_y)
                                ],
                                egui::Stroke::new(0.7, underline_color)
                            );
                        } else {
                            // Normal day number rendering - disable selection to prevent dropdown interference
                            ui.add(egui::Label::new(rich_text).selectable(false));
                        }
                    }
                    
                    // Balance in upper right (subtle gray) - only for current month days
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(balance) = self.balance {
                            // Only show balance for current month days, not for filler days
                            if matches!(self.day_type, CalendarDayType::CurrentMonth) {
                                let balance_font_size = if config.is_grid_layout {
                                    11.0
                                } else {
                                    (width * 0.12).max(10.0).min(14.0)
                                };
                                
                                // Balance text color using centralized color scheme
                                let balance_color = self.day_type.balance_text_color();
                                
                                ui.add(egui::Label::new(
                                    egui::RichText::new(format!("${:.2}", balance))
                                        .font(egui::FontId::new(balance_font_size, egui::FontFamily::Proportional))
                                        .color(balance_color)
                                ).selectable(false)); // Disable selection to prevent dropdown interference
                            }
                        }
                    });
                });
                
                // Add some spacing between header and transaction chips
                ui.add_space(4.0);
                
                // Transaction chips below - vertically stacked
                // Convert transactions to calendar chips
                let chips = CalendarChip::from_transactions(self.transactions.clone(), config.is_grid_layout);
                
                let chips_to_show = if let Some(max) = config.max_transactions {
                    chips.iter().take(max)
                } else {
                    chips.iter().take(chips.len())
                };
                
                let mut local_clicked_ids = Vec::new();
                for chip in chips_to_show {
                    if let Some(transaction_id) = self.render_calendar_chip(ui, chip, width - 8.0, height, config) {
                        local_clicked_ids.push(transaction_id);
                    }
                    ui.add_space(1.0); // Smaller spacing between chips due to padding
                }
                
                local_clicked_ids
            })
        });
        
        // Extract clicked transaction IDs from UI result
        clicked_transaction_ids.extend(ui_result.inner.inner);
        
        // Return the response for click handling by the caller and clicked transaction IDs
        (response, clicked_transaction_ids)
    }
    
    /// Render a single calendar chip with unified styling and hover effects
    /// Returns the transaction ID if the checkbox was clicked (for selection toggle)
    fn render_calendar_chip(&self, ui: &mut egui::Ui, chip: &CalendarChip, width: f32, _height: f32, config: &RenderConfig) -> Option<String> {
        
        // Get chip styling from the chip type
        let chip_color = chip.chip_type.primary_color();
        let text_color = chip.chip_type.text_color();
        let uses_dotted_border = chip.chip_type.uses_dotted_border();
        
        // Calculate chip dimensions based on layout - use the thinner height for all chips
        let (chip_width, chip_height, chip_font_size) = if config.is_grid_layout {
            ((width - 10.0).min(120.0), 18.0, 10.0)
        } else {
            (width * 0.85, 18.0, (width * 0.12).max(9.0).min(12.0)) // Force consistent height
        };
        
        // Check if we should show checkbox (only for deletable transactions in selection mode)
        let show_checkbox = config.transaction_selection_mode && 
                            !matches!(chip.chip_type, CalendarChipType::FutureAllowance);
        let checkbox_width = if show_checkbox { 16.0 } else { 0.0 };
        let checkbox_spacing = if show_checkbox { 4.0 } else { 0.0 };
        
        // Adjust chip width to accommodate checkbox
        let adjusted_chip_width = if show_checkbox {
            chip_width - checkbox_width - checkbox_spacing
        } else {
            chip_width
        };
        
        // Opaque white background for all transaction chips
        let chip_background = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255);
        
        let mut checkbox_clicked = None;
        
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            if show_checkbox {
                // Horizontal layout with checkbox and chip
                ui.horizontal(|ui| {
                    // Checkbox on the left
                    let is_selected = config.selected_transaction_ids.contains(&chip.transaction.id);
                    let checkbox_response = ui.add_sized(
                        [checkbox_width, checkbox_width],
                        egui::Checkbox::new(&mut is_selected.clone(), "")
                    );
                    
                    if checkbox_response.clicked() {
                        checkbox_clicked = Some(chip.transaction.id.clone());
                    }
                    
                    ui.add_space(checkbox_spacing);
                    
                    // Chip on the right
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(adjusted_chip_width, chip_height), egui::Sense::hover());
                    
                    // Determine if we should show hover effect
                    let is_hovered = response.hovered();
                    
                    // Background color - slightly darker when hovered
                    let background_color = if is_hovered {
                        egui::Color32::from_rgba_unmultiplied(245, 245, 245, 255) // Light gray on hover
                    } else {
                        chip_background
                    };
                    
                    // Draw background
                    ui.painter().rect_filled(
                        rect,
                        egui::Rounding::same(4.0),
                        background_color
                    );
                    
                    // Draw border - solid or dotted based on chip type
                    if uses_dotted_border {
                        self.draw_dotted_border(ui, rect, chip_color);
                    } else {
                        // Draw solid border
                        ui.painter().rect_stroke(
                            rect,
                            egui::Rounding::same(4.0),
                            egui::Stroke::new(1.0, chip_color)
                        );
                    }
                    
                    // Draw text
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &chip.display_amount,
                        egui::FontId::new(chip_font_size, egui::FontFamily::Proportional),
                        text_color,
                    );
                    
                    // Show floating tooltip when hovering
                    if is_hovered && !chip.transaction.description.is_empty() {
                        self.show_transaction_tooltip(ui, &chip.transaction.description, rect);
                    }
                });
            } else {
                // Original chip rendering without checkbox
                let (rect, response) = ui.allocate_exact_size(egui::vec2(chip_width, chip_height), egui::Sense::hover());
                
                // Determine if we should show hover effect
                let is_hovered = response.hovered();
                
                // Background color - slightly darker when hovered
                let background_color = if is_hovered {
                    egui::Color32::from_rgba_unmultiplied(245, 245, 245, 255) // Light gray on hover
                } else {
                    chip_background
                };
                
                // Draw background
                ui.painter().rect_filled(
                    rect,
                    egui::Rounding::same(4.0),
                    background_color
                );
                
                // Draw border - solid or dotted based on chip type
                if uses_dotted_border {
                    self.draw_dotted_border(ui, rect, chip_color);
                } else {
                    // Draw solid border
                    ui.painter().rect_stroke(
                        rect,
                        egui::Rounding::same(4.0),
                        egui::Stroke::new(1.0, chip_color)
                    );
                }
                
                // Draw text
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &chip.display_amount,
                    egui::FontId::new(chip_font_size, egui::FontFamily::Proportional),
                    text_color,
                );
                
                // Show floating tooltip when hovering
                if is_hovered && !chip.transaction.description.is_empty() {
                    self.show_transaction_tooltip(ui, &chip.transaction.description, rect);
                }
            }
        });
        
        checkbox_clicked
    }
    
    /// Show a floating tooltip with transaction description
    fn show_transaction_tooltip(&self, ui: &mut egui::Ui, description: &str, chip_rect: egui::Rect) {
        // Get cursor position for tooltip positioning
        let cursor_pos = ui.ctx().pointer_interact_pos().unwrap_or(chip_rect.center());
        
        // Calculate tooltip dimensions (estimate based on text length)
        let tooltip_font_size = 12.0;
        let tooltip_padding = egui::vec2(8.0, 6.0);
        let max_tooltip_width = 200.0;
        
        // Estimate tooltip size (rough approximation)
        let char_width = tooltip_font_size * 0.6; // Approximate character width
        let text_width = (description.len() as f32 * char_width).min(max_tooltip_width);
        let text_height = tooltip_font_size * 1.2; // Line height
        let tooltip_size = egui::vec2(text_width + tooltip_padding.x * 2.0, text_height + tooltip_padding.y * 2.0);
        
        // Smart positioning: offset from cursor, but avoid screen boundaries
        let default_offset = egui::vec2(10.0, -25.0); // Right and up from cursor
        let screen_rect = ui.ctx().screen_rect();
        
        // Calculate initial position
        let mut tooltip_pos = cursor_pos + default_offset;
        
        // Adjust if tooltip would go off-screen
        // Check right boundary
        if tooltip_pos.x + tooltip_size.x > screen_rect.right() {
            tooltip_pos.x = cursor_pos.x - tooltip_size.x - 10.0; // Show to the left instead
        }
        
        // Check left boundary
        if tooltip_pos.x < screen_rect.left() {
            tooltip_pos.x = screen_rect.left() + 5.0; // Keep some margin
        }
        
        // Check top boundary
        if tooltip_pos.y < screen_rect.top() {
            tooltip_pos.y = cursor_pos.y + 25.0; // Show below cursor instead
        }
        
        // Check bottom boundary
        if tooltip_pos.y + tooltip_size.y > screen_rect.bottom() {
            tooltip_pos.y = cursor_pos.y - tooltip_size.y - 10.0; // Show above cursor
        }
        
        // Create tooltip using egui::Area for precise positioning
        egui::Area::new("transaction_tooltip".into())
            .fixed_pos(tooltip_pos)
            .order(egui::Order::Foreground) // Show above everything else
            .show(ui.ctx(), |ui| {
                // Tooltip styling
                let tooltip_bg_color = egui::Color32::from_rgba_unmultiplied(40, 40, 40, 240); // Dark semi-transparent
                let tooltip_text_color = egui::Color32::WHITE;
                let tooltip_border_color = egui::Color32::from_rgba_unmultiplied(100, 100, 100, 200);
                
                // Draw tooltip background with rounded corners
                let tooltip_rect = egui::Rect::from_min_size(ui.cursor().min, tooltip_size);
                
                // Draw shadow first (slightly offset)
                let shadow_rect = tooltip_rect.translate(egui::vec2(1.0, 1.0));
                ui.painter().rect_filled(
                    shadow_rect,
                    egui::Rounding::same(6.0),
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 60)
                );
                
                // Draw main tooltip background
                ui.painter().rect_filled(
                    tooltip_rect,
                    egui::Rounding::same(6.0),
                    tooltip_bg_color
                );
                
                // Draw border
                ui.painter().rect_stroke(
                    tooltip_rect,
                    egui::Rounding::same(6.0),
                    egui::Stroke::new(1.0, tooltip_border_color)
                );
                
                // Add padding and render text
                ui.allocate_ui_with_layout(
                    tooltip_size,
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.add_space(tooltip_padding.y);
                        ui.horizontal(|ui| {
                            ui.add_space(tooltip_padding.x);
                            ui.add(egui::Label::new(
                                egui::RichText::new(description)
                                    .font(egui::FontId::new(tooltip_font_size, egui::FontFamily::Proportional))
                                    .color(tooltip_text_color)
                            ).selectable(false)); // Disable selection to prevent dropdown interference
                        });
                    }
                );
            });
    }
    
    /// Draw a dotted border around a rectangle for future allowance chips
    fn draw_dotted_border(&self, ui: &mut egui::Ui, rect: egui::Rect, color: egui::Color32) {
        let painter = ui.painter();
        let rounding = 4.0;
        let dash_length = 3.0;
        let gap_length = 2.0;
        let stroke_width = 1.0;
        
        // Top border
        let mut x = rect.left() + rounding;
        let y = rect.top();
        while x < rect.right() - rounding {
            let end_x = (x + dash_length).min(rect.right() - rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(end_x, y)],
                egui::Stroke::new(stroke_width, color)
            );
            x = end_x + gap_length;
        }
        
        // Right border
        let mut y = rect.top() + rounding;
        let x = rect.right();
        while y < rect.bottom() - rounding {
            let end_y = (y + dash_length).min(rect.bottom() - rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, end_y)],
                egui::Stroke::new(stroke_width, color)
            );
            y = end_y + gap_length;
        }
        
        // Bottom border
        let mut x = rect.right() - rounding;
        let y = rect.bottom();
        while x > rect.left() + rounding {
            let end_x = (x - dash_length).max(rect.left() + rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(end_x, y)],
                egui::Stroke::new(stroke_width, color)
            );
            x = end_x - gap_length;
        }
        
        // Left border
        let mut y = rect.bottom() - rounding;
        let x = rect.left();
        while y > rect.top() + rounding {
            let end_y = (y - dash_length).max(rect.top() + rounding);
            painter.line_segment(
                [egui::pos2(x, y), egui::pos2(x, end_y)],
                egui::Stroke::new(stroke_width, color)
            );
            y = end_y - gap_length;
        }
        
        // Draw rounded corners as small arcs (simplified as short lines)
        let corner_dash = 2.0;
        
        // Top-left corner
        painter.line_segment(
            [egui::pos2(rect.left() + rounding - corner_dash, rect.top()), 
             egui::pos2(rect.left() + rounding, rect.top())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.left(), rect.top() + rounding - corner_dash), 
             egui::pos2(rect.left(), rect.top() + rounding)],
            egui::Stroke::new(stroke_width, color)
        );
        
        // Top-right corner
        painter.line_segment(
            [egui::pos2(rect.right() - rounding, rect.top()), 
             egui::pos2(rect.right() - rounding + corner_dash, rect.top())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.right(), rect.top() + rounding - corner_dash), 
             egui::pos2(rect.right(), rect.top() + rounding)],
            egui::Stroke::new(stroke_width, color)
        );
        
        // Bottom-right corner
        painter.line_segment(
            [egui::pos2(rect.right() - rounding + corner_dash, rect.bottom()), 
             egui::pos2(rect.right() - rounding, rect.bottom())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.right(), rect.bottom() - rounding), 
             egui::pos2(rect.right(), rect.bottom() - rounding + corner_dash)],
            egui::Stroke::new(stroke_width, color)
        );
        
        // Bottom-left corner
        painter.line_segment(
            [egui::pos2(rect.left() + rounding, rect.bottom()), 
             egui::pos2(rect.left() + rounding - corner_dash, rect.bottom())],
            egui::Stroke::new(stroke_width, color)
        );
        painter.line_segment(
            [egui::pos2(rect.left(), rect.bottom() - rounding + corner_dash), 
             egui::pos2(rect.left(), rect.bottom() - rounding)],
            egui::Stroke::new(stroke_width, color)
        );
    }
}

impl AllowanceTrackerApp {
    /// Convert backend CalendarDay to frontend CalendarDay structure
    fn convert_backend_calendar_day(&self, backend_day: &shared::CalendarDay, day_index: usize) -> CalendarDay {
        // Convert day type from backend to frontend enum
        let day_type = match backend_day.day_type {
            shared::CalendarDayType::MonthDay => CalendarDayType::CurrentMonth,
            shared::CalendarDayType::PaddingBefore | shared::CalendarDayType::PaddingAfter => CalendarDayType::FillerDay,
        };
        
        // Create date for this day
        let date = if backend_day.day == 0 {
            // For filler days, calculate the actual previous/next month days they represent
            match backend_day.day_type {
                shared::CalendarDayType::PaddingBefore => {
                    // Calculate previous month date
                    let (prev_year, prev_month) = if self.selected_month == 1 {
                        (self.selected_year - 1, 12)
                    } else {
                        (self.selected_year, self.selected_month - 1)
                    };
                    
                    // Get the last day of previous month
                    let prev_month_first = NaiveDate::from_ymd_opt(prev_year, prev_month, 1).unwrap();
                    let next_month_first = if prev_month == 12 {
                        NaiveDate::from_ymd_opt(prev_year + 1, 1, 1).unwrap()
                    } else {
                        NaiveDate::from_ymd_opt(prev_year, prev_month + 1, 1).unwrap()
                    };
                    let days_in_prev_month = (next_month_first - prev_month_first).num_days() as u32;
                    
                    // Calculate which day of previous month this represents
                    // First day of current month
                    let current_month_first = NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1).unwrap();
                    let weekday_of_first = current_month_first.weekday().num_days_from_sunday() as usize;
                    
                    // This filler day represents (days_in_prev_month - weekday_of_first + day_index + 1)
                    let prev_day = days_in_prev_month - weekday_of_first as u32 + day_index as u32 + 1;
                    NaiveDate::from_ymd_opt(prev_year, prev_month, prev_day).unwrap()
                }
                shared::CalendarDayType::PaddingAfter => {
                    // Calculate next month date
                    let (next_year, next_month) = if self.selected_month == 12 {
                        (self.selected_year + 1, 1)
                    } else {
                        (self.selected_year, self.selected_month + 1)
                    };
                    
                    // Find how many days we are past the end of current month
                    let current_month_first = NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1).unwrap();
                    let next_month_first = if self.selected_month == 12 {
                        NaiveDate::from_ymd_opt(self.selected_year + 1, 1, 1).unwrap()
                    } else {
                        NaiveDate::from_ymd_opt(self.selected_year, self.selected_month + 1, 1).unwrap()
                    };
                    let days_in_current_month = (next_month_first - current_month_first).num_days() as u32;
                    let weekday_of_first = current_month_first.weekday().num_days_from_sunday() as usize;
                    
                    // Calculate which day of next month this represents
                    let next_day = day_index as u32 - (weekday_of_first + days_in_current_month as usize) as u32 + 1;
                    NaiveDate::from_ymd_opt(next_year, next_month, next_day).unwrap()
                }
                shared::CalendarDayType::MonthDay => {
                    // This shouldn't happen for day == 0, but fallback
                    NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1).unwrap()
                }
            }
        } else {
            NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, backend_day.day).unwrap()
        };
        
        // Check if this is today
        let today = chrono::Local::now();
        let is_today = today.year() == self.selected_year 
            && today.month() == self.selected_month 
            && today.day() == backend_day.day;
        
        CalendarDay {
            day_number: backend_day.day,
            date,
            is_today,
            day_type,
            transactions: backend_day.transactions.clone(),
            balance: Some(backend_day.balance),
        }
    }
    

    
    /// Calculate running balances for all days in the month, carrying forward balances
    /// from previous days when there are no transactions


    /// Draw calendar section with toggle header integrated
    pub fn draw_calendar_section_with_toggle(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        // Use the existing draw_calendar_section method but with toggle header
        ui.add_space(15.0);
        
        // Calculate responsive dimensions - same as original
        let content_width = available_rect.width() - 40.0;
        
        // Calendar takes up full available width to align with navigation buttons
        let calendar_width = content_width;
        
        // Calculate cell dimensions proportionally  
        let total_spacing = CALENDAR_CARD_SPACING * 6.0;
        let cell_width = (calendar_width - total_spacing) / 7.0;
        let cell_height = cell_width * 0.8;
        
        // Header height proportional to cell size
        let header_height = cell_height * 0.4;
        
        // Draw the card container with toggle header
        let card_height = (header_height + cell_height * 6.0) + 200.0; // Space for 6 weeks + toggle header + padding
        
        // Ensure card doesn't exceed available rectangle bounds
        let max_available_height = available_rect.height() - 40.0;
        let final_card_height = card_height.min(max_available_height);
        
        let card_rect = egui::Rect::from_min_size(
            egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0),
            egui::vec2(content_width, final_card_height)
        );
        
        // Draw calendar content (no background card)
        ui.allocate_ui_at_rect(card_rect, |ui| {
            ui.vertical(|ui| {
                // Align the calendar content to the left to match navigation buttons
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        // Constrain calendar to calculated width
                        ui.allocate_ui_with_layout(
                            egui::vec2(calendar_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                // Day headers - consistent layout with automatic spacing matching day cards
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = CALENDAR_CARD_SPACING; // Match day cards spacing
                                    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
                                    for day_name in day_names.iter() {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(cell_width, header_height),
                                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                            |ui| {
                                                // Get the rect for this header
                                                let header_rect = ui.available_rect_before_wrap();
                                                
                                                // Draw card-like background
                                                let bg_color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 180);
                                                ui.painter().rect_filled(
                                                    header_rect,
                                                    egui::Rounding::same(2.0),
                                                    bg_color
                                                );
                                                
                                                // Draw border
                                                let border_color = egui::Color32::from_rgba_unmultiplied(150, 150, 150, 200);
                                                ui.painter().rect_stroke(
                                                    header_rect,
                                                    egui::Rounding::same(2.0),
                                                    egui::Stroke::new(1.0, border_color)
                                                );
                                                
                                                // Draw text - disable selection to prevent dropdown interference
                                                ui.add(egui::Label::new(egui::RichText::new(*day_name)
                                                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                                    .strong()
                                                    .color(egui::Color32::DARK_GRAY))
                                                    .selectable(false));
                                            },
                                        );
                                        
                                        // No manual spacing - using automatic spacing system like day cards
                                    }
                                });
                                
                                ui.add_space(5.0); // Small gap between headers and calendar
                                
                                // Calendar days - use corrected manual method
                                self.draw_calendar_days_responsive(ui, transactions, cell_width, cell_height);
                            }
                        );
                    }
                );
            });
        });
    }
    
    /// Navigate to a different month
    pub fn navigate_month(&mut self, delta: i32) {
        let old_month = self.selected_month;
        let old_year = self.selected_year;
        
        log::info!("🗓️  Navigating from {}/{} with delta {}", old_month, old_year, delta);
        
        if delta > 0 {
            if self.selected_month == 12 {
                self.selected_month = 1;
                self.selected_year += 1;
            } else {
                self.selected_month += 1;
            }
        } else if delta < 0 {
            if self.selected_month == 1 {
                self.selected_month = 12;
                self.selected_year -= 1;
            } else {
                self.selected_month -= 1;
            }
        }
        
        log::info!("🗓️  Navigation complete: {}/{} → {}/{}", 
                  old_month, old_year, self.selected_month, self.selected_year);
        
        if self.selected_month == 6 {
            log::info!("🗓️  🎯 Navigated to June {} - about to load calendar data", self.selected_year);
        }
        
        self.load_calendar_data();
        
        log::info!("🔄 Calendar data reloaded for {}/{}", self.selected_month, self.selected_year);
    }
    
    /// Get color for day header based on index
    pub fn get_day_header_color(&self, day_index: usize) -> egui::Color32 {
        // Use smooth pink-to-purple gradient matching the draw_day_header_gradient function
        let t = day_index as f32 / 6.0; // 0.0 to 1.0
        
        // Interpolate between pink and purple (no blue)
        let pink = egui::Color32::from_rgb(255, 182, 193); // Light pink
        let purple = egui::Color32::from_rgb(186, 85, 211); // Purple
        
        egui::Color32::from_rgb(
            (pink.r() as f32 * (1.0 - t) + purple.r() as f32 * t) as u8,
            (pink.g() as f32 * (1.0 - t) + purple.g() as f32 * t) as u8,
            (pink.b() as f32 * (1.0 - t) + purple.b() as f32 * t) as u8,
        )
    }
    
    /// Draw calendar days with responsive sizing using CalendarDay components
    pub fn draw_calendar_days_responsive(&mut self, ui: &mut egui::Ui, _transactions: &[Transaction], cell_width: f32, cell_height: f32) {
        ui.spacing_mut().item_spacing.y = CALENDAR_CARD_SPACING; // Vertical spacing between week rows
        // Use calendar month data from backend (which includes balance data)
        let all_days: Vec<CalendarDay> = if let Some(ref calendar_month) = self.calendar_month {
            // Convert backend calendar days to frontend calendar days
            calendar_month.days.iter()
                .enumerate()
                .map(|(index, day)| self.convert_backend_calendar_day(day, index))
                .collect()
        } else {
            // No calendar data available - return empty calendar
            log::warn!("⚠️ No calendar month data available for {}/{}", self.selected_month, self.selected_year);
            Vec::new()
        };
        
        // Backend calendar data includes complete grid with filler days
        
        // Render the calendar grid (dynamic weeks based on month needs) in proper row layout  
        // Process days in chunks of 7 (one week per row)
        let mut selected_day_rect: Option<egui::Rect> = None;
        let mut selected_day_date: Option<NaiveDate> = None;
        
        for week_days in all_days.chunks(7) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = CALENDAR_CARD_SPACING; // Horizontal spacing between day cards
                for calendar_day in week_days.iter() {
                    let ui_response = ui.allocate_ui_with_layout(
                        egui::vec2(cell_width, cell_height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            // Check if this day is selected
                            let is_selected = self.selected_day == Some(calendar_day.date);
                            
                            let (response, clicked_transaction_ids) = calendar_day.render_with_config(ui, cell_width, cell_height, &RenderConfig {
                                is_grid_layout: true,
                                max_transactions: Some(2), // Limit transactions in grid view
                                enable_click_handler: true,
                                is_selected,
                                transaction_selection_mode: self.transaction_selection_mode,
                                selected_transaction_ids: self.selected_transaction_ids.clone(),
                            });
                            
                            // Handle checkbox clicks on transactions
                            for transaction_id in clicked_transaction_ids {
                                self.toggle_transaction_selection(&transaction_id);
                            }
                            
                            // Handle click detection for current month days only
                            if response.clicked() && matches!(calendar_day.day_type, CalendarDayType::CurrentMonth) {
                                self.handle_day_click(calendar_day.date);
                            }
                            
                            // Return whether this day is selected for later rect capture
                            is_selected && matches!(calendar_day.day_type, CalendarDayType::CurrentMonth)
                        },
                    );
                    
                    // Store the selected day's rect for icon rendering
                    if ui_response.inner {
                        selected_day_rect = Some(ui_response.response.rect);
                        selected_day_date = Some(calendar_day.date);
                    }
                    
                    // No manual spacing between day cells - using egui spacing control instead
                }
            });
            
            // No vertical spacing between week rows
        }
        
        // Render action icons above the selected day if one is selected
        if let (Some(day_rect), Some(day_date)) = (selected_day_rect, selected_day_date) {
            self.render_day_action_icons(ui, day_rect, day_date);
        }
    }
    
    /// Calculate the height needed for the calendar grid
    pub fn calculate_calendar_grid_height(&self, day_height: f32) -> f32 {
        // Calculate number of weeks needed for the current month
        let first_day = match NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1) {
            Some(date) => date,
            None => return day_height * 6.0, // fallback to 6 weeks
        };
        
        let days_in_month = match first_day.with_day(1) {
            Some(first) => {
                let next_month = if self.selected_month == 12 {
                    first.with_year(self.selected_year + 1).unwrap().with_month(1).unwrap()
                } else {
                    first.with_month(self.selected_month + 1).unwrap()
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
    
    /// Handle clicking on a calendar day - toggle selection and clear overlay
    pub fn handle_day_click(&mut self, clicked_date: NaiveDate) {
        if let Some(selected_date) = self.selected_day {
            if selected_date == clicked_date {
                // Clicking the same day - deselect it
                self.selected_day = None;
                self.active_overlay = None;
                log::info!("📅 Deselected day: {}", clicked_date);
            } else {
                // Clicking a different day - select it and clear overlay
                self.selected_day = Some(clicked_date);
                self.active_overlay = None;
                log::info!("📅 Selected day: {}", clicked_date);
            }
        } else {
            // No day selected - select this day
            self.selected_day = Some(clicked_date);
            self.active_overlay = None;
            log::info!("📅 Selected day: {}", clicked_date);
        }
    }

    /// Render action icons above the selected day
    pub fn render_day_action_icons(&mut self, ui: &mut egui::Ui, day_cell_rect: egui::Rect, selected_date: NaiveDate) {
        // Get glyphs that should be shown for this specific date
        let glyphs = DayMenuGlyph::for_date(selected_date);
        
        // If no glyphs should be shown for this date, return early
        if glyphs.is_empty() {
            return;
        }
        
        // Shared styling for all glyphs - wider to accommodate two characters
        let glyph_size = egui::vec2(48.0, 22.0);
        let glyph_spacing = 6.0;
        
        // Shared colors - using the same pink as selected day
        let outline_color = egui::Color32::from_rgb(199, 112, 221); // Same as selected day
        let background_color = egui::Color32::WHITE;
        let text_color = outline_color; // Same pink as outline
        
        // Calculate the actual width of the glyphs by measuring them
        let total_glyph_width = glyph_size.x * glyphs.len() as f32;
        let total_spacing = glyph_spacing * (glyphs.len() - 1) as f32;
        let total_width = total_glyph_width + total_spacing;
        
        // Position each glyph individually for precise control
        let center_x = day_cell_rect.center().x;
        let start_x = center_x - (total_width / 2.0);
        let glyphs_y = day_cell_rect.top() - glyph_size.y - 25.0; // More space above
        
        // Render each glyph as a separate Area for precise positioning
        for (i, glyph) in glyphs.iter().enumerate() {
            let glyph_x = start_x + (i as f32 * (glyph_size.x + glyph_spacing));
            let glyph_pos = egui::pos2(glyph_x, glyphs_y);
            
            egui::Area::new(egui::Id::new(format!("day_menu_glyph_{}", i)))
                .fixed_pos(glyph_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    let glyph_text = glyph.text();
                    
                    // Create a button with consistent styling
                    let button = egui::Button::new(egui::RichText::new(glyph_text).color(text_color))
                        .fill(background_color)
                        .stroke(egui::Stroke::new(2.0, outline_color))
                        .rounding(egui::Rounding::same(4.0));
                    
                    if ui.add_sized(glyph_size, button).clicked() {
                        self.active_overlay = Some(glyph.overlay_type());
                        log::info!("🎯 Day menu glyph '{}' clicked for date: {}", glyph_text, selected_date);
                    }
                });
        }
    }
    

} 