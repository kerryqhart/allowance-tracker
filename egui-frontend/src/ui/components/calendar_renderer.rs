//! # Calendar Renderer Module
//!
//! This module handles all calendar-related rendering functionality for the allowance tracker app.
//! It provides a visual, interactive calendar view where users can see their transactions
//! displayed on specific dates.
//!
//! ## Key Functions:
//! - `draw_calendar_section_with_toggle()` - Main calendar view with collapsible header
//! - `navigate_month()` - Handle month navigation (previous/next)
//! - `get_day_header_color()` - Calculate gradient colors for day headers
//! - `draw_calendar_days_responsive()` - Render calendar grid with responsive design
//! - `draw_calendar_content()` - Core calendar rendering logic
//! - `draw_calendar_grid_in_rect()` - Draw calendar grid within specific bounds
//! - `calculate_calendar_grid_height()` - Calculate required height for calendar
//! - `render_calendar_grid()` - Render the actual calendar grid with transactions
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
    pub fn border_color(&self) -> egui::Color32 {
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

    /// Get the day number text color for this day type
    pub fn day_text_color(&self, is_today: bool) -> egui::Color32 {
        if is_today {
            // Pink for today
            egui::Color32::from_rgb(219, 112, 147)
        } else {
            match self {
                CalendarDayType::CurrentMonth => {
                    // Bold black for current month days
                    egui::Color32::BLACK
                }
                CalendarDayType::FillerDay => {
                    // Gray for filler days
                    egui::Color32::from_rgb(150, 150, 150)
                }
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
    /// The balance at the end of this day (if any transactions occurred)
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
        
        // Draw subtle background for the day cell using centralized color scheme
        let bg_color = self.day_type.background_color(self.is_today);
        
        ui.painter().rect_filled(
            cell_rect,
            egui::Rounding::same(2.0),
            bg_color
        );
        
        // Draw subtle border around the day cell using centralized color scheme
        let border_color = self.day_type.border_color();
        
        ui.painter().rect_stroke(
            cell_rect,
            egui::Rounding::same(2.0),
            egui::Stroke::new(0.5, border_color)
        );
        
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.set_height(height);
            
            // Add small padding inside the cell
            ui.add_space(4.0);
            
            // Top row: Day number (left) and Balance (right)
            ui.horizontal(|ui| {
                ui.set_width(width - 8.0); // Account for padding
                
                // Day number in upper left (bold black)
                let day_font_size = if config.is_grid_layout {
                    16.0
                } else {
                    (width * 0.15).max(14.0).min(18.0)
                };
                
                // Day number text color using centralized color scheme
                let day_text_color = self.day_type.day_text_color(self.is_today);
                
                ui.label(
                    egui::RichText::new(self.day_number.to_string())
                        .font(egui::FontId::new(day_font_size, egui::FontFamily::Proportional))
                        .color(day_text_color)
                        .strong()
                );
                
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
            let transactions_to_show = if let Some(max) = config.max_transactions {
                self.transactions.iter().take(max)
            } else {
                self.transactions.iter().take(self.transactions.len())
            };
            
            for transaction in transactions_to_show {
                self.render_transaction_chip(ui, transaction, width - 8.0, height, config.is_grid_layout);
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
    
    /// Render a single transaction chip
    fn render_transaction_chip(&self, ui: &mut egui::Ui, transaction: &Transaction, width: f32, height: f32, is_grid_layout: bool) {
        // Determine chip color based on transaction amount
        let (chip_color, text_color) = if transaction.amount > 0.0 {
            (egui::Color32::from_rgb(46, 160, 67), egui::Color32::from_rgb(46, 160, 67))
        } else {
            (egui::Color32::from_rgb(128, 128, 128), egui::Color32::from_rgb(128, 128, 128))
        };
        
        // Create transaction chip text
        let chip_text = if transaction.amount > 0.0 {
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
        
        // Calculate chip dimensions based on layout
        let (chip_width, chip_height, chip_font_size) = if is_grid_layout {
            ((width - 10.0).min(120.0), 18.0, 10.0)
        } else {
            (width * 0.85, height * 0.15, (width * 0.12).max(9.0).min(12.0))
        };
        
        // Check if this is a future transaction
        let is_future = matches!(transaction.transaction_type, shared::TransactionType::FutureAllowance);
        
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            if is_future {
                // Dotted/dashed border for future transactions
                let (rect, _) = ui.allocate_exact_size(egui::vec2(chip_width, chip_height), egui::Sense::hover());
                
                ui.painter().rect_stroke(
                    rect,
                    egui::Rounding::same(4.0),
                    egui::Stroke::new(1.0, chip_color)
                );
                
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    &chip_text,
                    egui::FontId::new(chip_font_size, egui::FontFamily::Proportional),
                    text_color,
                );
            } else {
                // Outlined chip for completed transactions
                let chip_button = egui::Button::new(
                    egui::RichText::new(&chip_text)
                        .font(egui::FontId::new(chip_font_size, egui::FontFamily::Proportional))
                        .color(text_color)
                )
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::new(1.0, chip_color))
                .rounding(egui::Rounding::same(4.0));
                
                ui.add_sized([chip_width, chip_height], chip_button);
            }
        });
    }
}

impl AllowanceTrackerApp {
    /// Create CalendarDay instances for the selected month
    pub fn create_calendar_days(&self, transactions: &[Transaction]) -> Vec<CalendarDay> {
        log::info!("🗓️  Creating calendar days for {}/{} with {} transactions", 
                  self.selected_month, self.selected_year, transactions.len());
        
        let mut calendar_days = Vec::new();
        
        // Create a date for the first day of the selected month
        let first_day = match NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1) {
            Some(date) => date,
            None => {
                log::error!("❌ Failed to create first day of month {}/{}", self.selected_month, self.selected_year);
                return calendar_days;
            }
        };
        
        // Calculate the number of days in the month
        let days_in_month = match first_day.with_day(1) {
            Some(first) => {
                let next_month = if self.selected_month == 12 {
                    first.with_year(self.selected_year + 1).unwrap().with_month(1).unwrap()
                } else {
                    first.with_month(self.selected_month + 1).unwrap()
                };
                (next_month - chrono::Duration::days(1)).day()
            }
            None => {
                log::error!("❌ Failed to calculate days in month {}/{}", self.selected_month, self.selected_year);
                return calendar_days;
            }
        };
        
        log::info!("📅 Month {}/{} has {} days", self.selected_month, self.selected_year, days_in_month);
        
        // Get current date for highlighting today
        let today = chrono::Local::now();
        let is_current_month = today.year() == self.selected_year && today.month() == self.selected_month;
        let today_day = if is_current_month { Some(today.day()) } else { None };
        
        // Group transactions by day
        let mut transactions_by_day: std::collections::HashMap<u32, Vec<Transaction>> = std::collections::HashMap::new();
        let mut processed_transactions = 0;
        let mut filtered_transactions = 0;
        
        for transaction in transactions {
            processed_transactions += 1;
            log::debug!("🔍 Processing transaction {}: {} on {}", 
                       processed_transactions, transaction.description, transaction.date);
            
            // Extract date from DateTime object
            let parsed_date = transaction.date.naive_local().date();
            
            
            log::debug!("📅 Parsed date: {} (year={}, month={}, day={})", 
                       parsed_date, parsed_date.year(), parsed_date.month(), parsed_date.day());
            
            if parsed_date.year() == self.selected_year && parsed_date.month() == self.selected_month {
                filtered_transactions += 1;
                let day = parsed_date.day();
                log::info!("✅ Transaction '{}' matches {}/{} - adding to day {}", 
                          transaction.description, self.selected_month, self.selected_year, day);
                transactions_by_day.entry(day).or_insert_with(Vec::new).push(transaction.clone());
            } else {
                log::debug!("❌ Transaction '{}' date {}/{} doesn't match selected {}/{}", 
                           transaction.description, parsed_date.year(), parsed_date.month(), 
                           self.selected_year, self.selected_month);
            }
        }
        
        log::info!("📊 Transaction processing complete: {}/{} transactions matched {}/{}", 
                  filtered_transactions, processed_transactions, self.selected_month, self.selected_year);
        
        // Log the grouped transactions
        for (day, day_transactions) in &transactions_by_day {
            log::info!("📅 Day {}: {} transactions", day, day_transactions.len());
            for transaction in day_transactions {
                log::debug!("  - {}: ${}", transaction.description, transaction.amount);
            }
        }
        
        // Create CalendarDay instances for each day in the month
        for day in 1..=days_in_month {
            let day_date = match first_day.with_day(day) {
                Some(date) => date,
                None => continue,
            };
            
            let is_today = today_day == Some(day);
            let mut calendar_day = CalendarDay::new(day, day_date, is_today, CalendarDayType::CurrentMonth);
            
            // Add transactions for this day
            if let Some(day_transactions) = transactions_by_day.get(&day) {
                log::info!("📅 Adding {} transactions to day {}", day_transactions.len(), day);
                for transaction in day_transactions {
                    calendar_day.add_transaction(transaction.clone());
                }
            }
            
            calendar_days.push(calendar_day);
        }
        
        // Now create a complete calendar grid with filler days
        let mut complete_calendar_days = Vec::new();
        
        // Calculate first day offset for grid layout
        let first_day_offset = match first_day.weekday() {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        };
        
        // Create filler days for beginning of month if needed
        if first_day_offset > 0 {
            let prev_month_date = if self.selected_month == 1 {
                NaiveDate::from_ymd_opt(self.selected_year - 1, 12, 1)
            } else {
                NaiveDate::from_ymd_opt(self.selected_year, self.selected_month - 1, 1)
            };
            
            if let Some(prev_month_first) = prev_month_date {
                // Get the last day of previous month
                let prev_month_last_day = first_day - chrono::Duration::days(1);
                
                // Create filler days for the end of the previous month
                for i in 0..first_day_offset {
                    let filler_day_num = (prev_month_last_day.day() - (first_day_offset - 1 - i)) as u32;
                    let filler_date = prev_month_last_day - chrono::Duration::days((first_day_offset - 1 - i) as i64);
                    
                    let filler_day = CalendarDay::new(filler_day_num, filler_date, false, CalendarDayType::FillerDay);
                    complete_calendar_days.push(filler_day);
                }
            }
        }
        
        // Add all the current month days
        complete_calendar_days.extend(calendar_days);
        
        // Calculate how many days we need to fill the last row
        let total_days_so_far = complete_calendar_days.len();
        let weeks_needed = ((total_days_so_far + 6) / 7) as u32;
        let total_cells_needed = weeks_needed * 7;
        let filler_days_needed = total_cells_needed - total_days_so_far as u32;
        
        // Create filler days for the end of the calendar if needed
        if filler_days_needed > 0 {
            let next_month_date = if self.selected_month == 12 {
                NaiveDate::from_ymd_opt(self.selected_year + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(self.selected_year, self.selected_month + 1, 1)
            };
            
            if let Some(next_month_first) = next_month_date {
                for i in 0..filler_days_needed {
                    let filler_day_num = i + 1;
                    let filler_date = next_month_first + chrono::Duration::days(i as i64);
                    
                    let filler_day = CalendarDay::new(filler_day_num, filler_date, false, CalendarDayType::FillerDay);
                    complete_calendar_days.push(filler_day);
                }
            }
        }
        
        log::info!("🗓️  Created {} total calendar days ({} current month + {} filler days) for {}/{}", 
                  complete_calendar_days.len(), days_in_month, 
                  complete_calendar_days.len() - days_in_month as usize,
                  self.selected_month, self.selected_year);
        
        complete_calendar_days
    }

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
                                    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
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
    pub fn draw_calendar_days_responsive(&mut self, ui: &mut egui::Ui, transactions: &[Transaction], cell_width: f32, cell_height: f32) {
        ui.spacing_mut().item_spacing.y = CALENDAR_CARD_SPACING; // Vertical spacing between week rows
        // Create complete calendar grid with filler days
        let all_days = self.create_calendar_days(transactions);
        
        // The create_calendar_days() method now returns a complete grid with filler days included
        
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
    
    /// Draw calendar content within the card
    pub fn draw_calendar_content(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect, transactions: &[Transaction]) {
        let mut content_ui = ui.child_ui(content_rect, egui::Layout::top_down(egui::Align::Min), None);
        
        // Calendar navigation
        content_ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                self.navigate_month(-1);
            }
            
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("▶").clicked() {
                    self.navigate_month(1);
                }
                
                // Center the month/year display
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let month_names = [
                        "January", "February", "March", "April", "May", "June",
                        "July", "August", "September", "October", "November", "December"
                    ];
                    
                    let month_name = month_names.get((self.selected_month - 1) as usize).unwrap_or(&"Unknown");
                    ui.label(egui::RichText::new(format!("{} {}", month_name, self.selected_year))
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .strong()
                        .color(egui::Color32::from_rgb(60, 60, 60)));
                });
            });
        });
        
        content_ui.add_space(10.0);
        
        // Draw calendar grid
        self.draw_calendar_grid_in_rect(&mut content_ui, content_rect, transactions);
    }
    
    /// Draw calendar grid within a specific rect
    pub fn draw_calendar_grid_in_rect(&mut self, ui: &mut egui::Ui, rect: egui::Rect, transactions: &[Transaction]) {
        let remaining_height = rect.height() - 50.0; // Account for navigation
        
        // Use chrono for date calculations
        let year = self.selected_year;
        let month = self.selected_month;
        
        // Calculate days in month using chrono
        let first_day = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        let next_month = if month == 12 {
            chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
        } else {
            chrono::NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
        };
        let days_in_month = next_month.signed_duration_since(first_day).num_days() as u32;
        
        let first_day_of_month = first_day.weekday().num_days_from_sunday();
        
        let total_cells = (days_in_month + first_day_of_month + 6) / 7 * 7;
        let rows = (total_cells + 6) / 7;
        
        let cell_width = (rect.width() - 20.0) / 7.0;
        let cell_height = remaining_height / rows as f32;
        
        // Draw weekday headers
        ui.horizontal(|ui| {
            let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            for weekday in weekdays {
                ui.allocate_ui_with_layout(
                    egui::vec2(cell_width, 25.0),
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        ui.label(egui::RichText::new(weekday)
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .strong()
                            .color(self.get_day_header_color(weekdays.iter().position(|&x| x == weekday).unwrap())));
                    },
                );
            }
        });
        
        // Draw calendar days
        self.draw_calendar_days_responsive(ui, transactions, cell_width, cell_height);
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
    
    /// Render the calendar grid for the selected month using CalendarDay components
    pub fn render_calendar_grid(&mut self, ui: &mut egui::Ui, day_width: f32, day_height: f32) {
        // Create CalendarDay instances from calendar_transactions
        let calendar_days = self.create_calendar_days(&self.calendar_transactions);
        
        // Calculate the first day offset for grid layout
        let first_day = match NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1) {
            Some(date) => date,
            None => {
                ui.label("Invalid date");
                return;
            }
        };
        
        // Since create_calendar_days now returns a complete grid, we just need to render all days
        let total_days = calendar_days.len();
        
        // Use the pre-calculated dimensions - remove spacing for grid
        let total_calendar_width = day_width * 7.0;
        
        ui.vertical(|ui| {
            // Center the calendar horizontally
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    // Constrain calendar to calculated width
                    ui.allocate_ui_with_layout(
                        egui::vec2(total_calendar_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            // Day headers with gradient background - dynamic width
                            ui.horizontal(|ui| {
                                let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
                                for (index, day_name) in day_names.iter().enumerate() {
                                    // Create a custom button with gradient background - dynamic width
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(day_width, 40.0), egui::Sense::hover());
                                    
                                    // Draw the gradient background for this day header
                                    crate::ui::draw_day_header_gradient(ui, rect, index);
                                    
                                    // Draw the text on top
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        day_name,
                                        egui::FontId::new(18.0, egui::FontFamily::Proportional),
                                        egui::Color32::WHITE,
                                    );
                                }
                            });
                            
                            // No spacing - grid lines will provide visual separation
            
            // Calendar grid with dynamic sizing using CalendarDay components
            // Since create_calendar_days returns a complete grid, we just iterate through all days
            let total_weeks = (total_days + 6) / 7;
            
            for week in 0..total_weeks {
                ui.horizontal(|ui| {
                    for day_of_week in 0..7 {
                        let day_index = week * 7 + day_of_week;
                        
                        if day_index < total_days {
                            // Render the CalendarDay from the complete grid
                            calendar_days[day_index].render_grid(ui, day_width, day_height);
                        } else {
                            // This shouldn't happen with the complete grid approach, but just in case
                            ui.add_sized([day_width, day_height], egui::Label::new(""));
                        }
                    }
                });
            }
            
            // Draw grid lines after all content is rendered
            // Use a more visible color and thicker stroke for testing
            let grid_color = egui::Color32::from_rgb(100, 100, 100); // Darker gray
            let grid_stroke = egui::Stroke::new(2.0, grid_color); // Thicker stroke
            
            // Get current cursor position for proper coordinate reference
            let current_pos = ui.cursor().min;
            
            // Calculate grid dimensions
            let header_height = 40.0;
            let grid_height = total_weeks as f32 * day_height;
            
            // Calculate the actual grid rectangle based on current position
            let grid_rect = egui::Rect::from_min_size(
                egui::pos2(current_pos.x - total_calendar_width, current_pos.y - header_height - grid_height),
                egui::vec2(total_calendar_width, header_height + grid_height)
            );
            
            println!("Drawing grid at rect: {:?}", grid_rect);
            
            // Draw vertical lines (between columns)
            for i in 1..7 {
                let x = grid_rect.min.x + (i as f32 * day_width);
                let line_start = egui::pos2(x, grid_rect.min.y + header_height);
                let line_end = egui::pos2(x, grid_rect.max.y);
                println!("Drawing vertical line from {:?} to {:?}", line_start, line_end);
                ui.painter().line_segment([line_start, line_end], grid_stroke);
            }
            
            // Draw horizontal lines (between rows)
            for i in 1..=total_weeks {
                let y = grid_rect.min.y + header_height + (i as f32 * day_height);
                let line_start = egui::pos2(grid_rect.min.x, y);
                let line_end = egui::pos2(grid_rect.max.x, y);
                println!("Drawing horizontal line from {:?} to {:?}", line_start, line_end);
                ui.painter().line_segment([line_start, line_end], grid_stroke);
            }
            
            // Draw border around the entire calendar
            println!("Drawing border around rect: {:?}", grid_rect);
            ui.painter().rect_stroke(grid_rect, egui::Rounding::same(2.0), grid_stroke);
                        }
                    );
                }
            );
        });
    }
    
    /// Draw calendar section with proper responsive layout
    /// 
    /// **DEPRECATED**: This function uses egui::Grid approach which limits flexibility for variable row heights.
    /// Use draw_calendar_section_with_toggle() instead which uses the manual approach for better control.
    /// Keeping this in our back-pocket for now but not actively using it.
    #[deprecated(note = "Use draw_calendar_section_with_toggle() instead - egui::Grid limits flexibility")]
    pub fn draw_calendar_section(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        // Responsive approach: size everything as percentages of available space
        
        // Calculate responsive dimensions
        let content_width = available_rect.width() - 40.0; // Leave some margin
        let card_padding = 15.0; // Reduced from 20.0
        let available_calendar_width = content_width - (card_padding * 2.0);
        
        // Calendar takes up 92% of available width, centered (increased from 85%)
        let calendar_width = available_calendar_width * 0.92;
        let calendar_left_margin = (available_calendar_width - calendar_width) / 2.0;
        
        // Calculate cell dimensions proportionally
        let cell_spacing = 1.0; // Reset to reasonable value
        let total_spacing = cell_spacing * 6.0; // 6 gaps between 7 columns
        let cell_width = (calendar_width - total_spacing) / 7.0;
        let cell_height = cell_width * 0.8; // Height is 80% of width for good proportions
        
        // Header height proportional to cell size
        let header_height = cell_height * 0.4;
        
        // Draw the card container
        let card_height = (header_height + cell_height * 6.0) + 150.0; // Space for 6 weeks + padding (reduced from 200.0)
        
        // Ensure card doesn't exceed available rectangle bounds
        let max_available_height = available_rect.height() - 40.0; // Leave 40px margin (20px top + 20px bottom)
        let final_card_height = card_height.min(max_available_height);
        
        let card_rect = egui::Rect::from_min_size(
            egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0),
            egui::vec2(content_width, final_card_height)
        );
        
        // Draw card with flat top to connect to tabs
        self.draw_card_with_flat_top(ui, card_rect);
        
        // Draw calendar content inside the card
        ui.allocate_ui_at_rect(card_rect, |ui| {
            ui.vertical(|ui| {
                ui.add_space(12.0); // Reduced from 15.0
                
                // Month navigation
                ui.horizontal(|ui| {
                    ui.add_space(12.0); // Reduced from 15.0
                    if ui.button("<").clicked() {
                        self.navigate_month(-1);
                    }
                    
                    ui.add_space(20.0);
                    let month_name = match self.selected_month {
                        1 => "January", 2 => "February", 3 => "March", 4 => "April",
                        5 => "May", 6 => "June", 7 => "July", 8 => "August",
                        9 => "September", 10 => "October", 11 => "November", 12 => "December",
                        _ => "Unknown"
                    };
                    ui.label(egui::RichText::new(format!("{} {}", month_name, self.selected_year)).size(18.0));
                    
                    ui.add_space(20.0);
                    if ui.button(">").clicked() {
                        self.navigate_month(1);
                    }
                });
                
                ui.add_space(15.0); // Reduced from 20.0
                
                // Responsive calendar grid
                ui.horizontal(|ui| {
                    ui.add_space(card_padding + calendar_left_margin); // Responsive centering
                    
                    egui::Grid::new("calendar_grid")
                        .num_columns(7)
                        .spacing([cell_spacing, cell_spacing])
                        .min_col_width(cell_width)
                        .max_col_width(cell_width)
                        .striped(false)
                        .show(ui, |ui| {
                            // Day headers - sized proportionally
                            let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
                            for (index, day_name) in day_names.iter().enumerate() {
                                let font_size = (cell_width * 0.15).max(12.0).min(18.0); // Responsive font size
                                let button = egui::Button::new(egui::RichText::new(*day_name).size(font_size).color(egui::Color32::WHITE))
                                    .fill(self.get_day_header_color(index))
                                    .rounding(egui::Rounding::same(4.0));
                                ui.add_sized([cell_width, header_height], button);
                            }
                            ui.end_row();
                            
                            // Calendar days with responsive sizing
                            self.draw_calendar_days_responsive(ui, transactions, cell_width, cell_height);
                        });
                });
            });
        });
    }
} 