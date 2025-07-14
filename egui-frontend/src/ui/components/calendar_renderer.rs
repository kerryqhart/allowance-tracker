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
use crate::ui::app_state::AllowanceTrackerApp;

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
                    // Darker gray for filler days (matching calendar day opacity)
                    egui::Color32::from_rgba_unmultiplied(120, 120, 120, 55)
                }
            }
        }
    }

    /// Get the border color for this day type
    pub fn border_color(&self, is_today: bool) -> egui::Color32 {
        if is_today {
            // Dark navy blue for high contrast against pink background
            egui::Color32::from_rgb(25, 25, 112) // Navy blue
        } else {
            match self {
                CalendarDayType::CurrentMonth => {
                    // Normal border
                    egui::Color32::from_rgba_unmultiplied(200, 200, 200, 100)
                }
                CalendarDayType::FillerDay => {
                    // Lighter border for filler days
                    egui::Color32::from_rgba_unmultiplied(150, 150, 150, 80)
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
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            is_grid_layout: false,
            max_transactions: Some(2),
            enable_click_handler: false,
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
    pub fn render(&self, ui: &mut egui::Ui, width: f32, height: f32) {
        self.render_with_config(ui, width, height, &RenderConfig::default())
    }
    
    /// Render this calendar day for grid layout
    pub fn render_grid(&self, ui: &mut egui::Ui, width: f32, height: f32) {
        self.render_with_config(ui, width, height, &RenderConfig {
            is_grid_layout: true,
            max_transactions: None,
            enable_click_handler: true,
        })
    }
    
    /// Render this calendar day with specified configuration
    pub fn render_with_config(&self, ui: &mut egui::Ui, width: f32, height: f32, config: &RenderConfig) {
        // Get the available rect for this day cell
        let cell_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(width, height)
        );
        
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
        
        // Draw background for the day cell using centralized color scheme
        let bg_color = self.day_type.background_color(self.is_today);
        
        ui.painter().rect_filled(
            cell_rect,
            egui::Rounding::same(2.0),
            bg_color
        );
        
        // Draw border around the day cell using centralized color scheme
        if self.is_today {
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
        
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.set_height(height);
            
            // Add small padding inside the cell
            ui.add_space(4.0);
            
            // Top row: Day number (left) and Balance (right)
            ui.horizontal(|ui| {
                ui.set_width(width - 8.0); // Account for padding
                
                // Day number in upper left (standard size)
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
                    
                    // Render the text first
                    let response = ui.label(rich_text_bold);
                    
                    // Draw manual underline beneath the text (shorter and thinner)
                    let text_rect = response.rect;
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
                    // Normal day number rendering
                    ui.label(rich_text);
                }
                
                // Balance in upper right (subtle gray)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(balance) = self.balance {
                        let balance_font_size = if config.is_grid_layout {
                            11.0
                        } else {
                            (width * 0.12).max(10.0).min(14.0)
                        };
                        
                        // Balance text color using centralized color scheme
                        let balance_color = self.day_type.balance_text_color();
                        
                        ui.label(
                            egui::RichText::new(format!("${:.2}", balance))
                                .font(egui::FontId::new(balance_font_size, egui::FontFamily::Proportional))
                                .color(balance_color)
                        );
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
            
            for chip in chips_to_show {
                self.render_calendar_chip(ui, chip, width - 8.0, height, config.is_grid_layout);
                ui.add_space(1.0); // Smaller spacing between chips due to padding
            }
            
            // Add click handler for the entire day if enabled
            if config.enable_click_handler {
                let day_rect = ui.available_rect_before_wrap();
                let day_response = ui.allocate_rect(day_rect, egui::Sense::click());
                if day_response.clicked() {
                    println!("Selected day: {}", self.day_number);
                }
            }
        });
    }
    
    /// Render a single calendar chip with unified styling and hover effects
    fn render_calendar_chip(&self, ui: &mut egui::Ui, chip: &CalendarChip, width: f32, _height: f32, is_grid_layout: bool) {
        
        // Get chip styling from the chip type
        let chip_color = chip.chip_type.primary_color();
        let text_color = chip.chip_type.text_color();
        let uses_dotted_border = chip.chip_type.uses_dotted_border();
        
        // Calculate chip dimensions based on layout - use the thinner height for all chips
        let (chip_width, chip_height, chip_font_size) = if is_grid_layout {
            ((width - 10.0).min(120.0), 18.0, 10.0)
        } else {
            (width * 0.85, 18.0, (width * 0.12).max(9.0).min(12.0)) // Force consistent height
        };
        
        // Opaque white background for all transaction chips
        let chip_background = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255);
        
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            // Unified chip rendering with hover effects for all types
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
        });
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
                            ui.label(
                                egui::RichText::new(description)
                                    .font(egui::FontId::new(tooltip_font_size, egui::FontFamily::Proportional))
                                    .color(tooltip_text_color)
                            );
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
    fn convert_backend_calendar_day(&self, backend_day: &shared::CalendarDay) -> CalendarDay {
        // Convert day type from backend to frontend enum
        let day_type = match backend_day.day_type {
            shared::CalendarDayType::MonthDay => CalendarDayType::CurrentMonth,
            shared::CalendarDayType::PaddingBefore | shared::CalendarDayType::PaddingAfter => CalendarDayType::FillerDay,
        };
        
        // Create date for this day
        let date = if backend_day.day == 0 {
            // For filler days, use an arbitrary date (not used for display)
            NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1).unwrap()
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
                                                
                                                // Draw text
                                                ui.label(egui::RichText::new(*day_name)
                                                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                                    .strong()
                                                    .color(egui::Color32::DARK_GRAY));
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
                .map(|day| self.convert_backend_calendar_day(day))
                .collect()
        } else {
            // No calendar data available - return empty calendar
            log::warn!("⚠️ No calendar month data available for {}/{}", self.selected_month, self.selected_year);
            Vec::new()
        };
        
        // Backend calendar data includes complete grid with filler days
        
        // Render the calendar grid (dynamic weeks based on month needs) in proper row layout  
        // Process days in chunks of 7 (one week per row)
        for week_days in all_days.chunks(7) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = CALENDAR_CARD_SPACING; // Horizontal spacing between day cards
                for calendar_day in week_days.iter() {
                    ui.allocate_ui_with_layout(
                        egui::vec2(cell_width, cell_height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            calendar_day.render_with_config(ui, cell_width, cell_height, &RenderConfig {
                                is_grid_layout: true,
                                max_transactions: Some(2), // Limit transactions in grid view
                                enable_click_handler: true,
                            });
                        },
                    );
                    
                    // No manual spacing between day cells - using egui spacing control instead
                }
            });
            
            // No vertical spacing between week rows
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
    

} 