use eframe::egui;
use crate::ui::app_state::{AllowanceTrackerApp, MainTab};
use crate::ui::*;
use chrono::{NaiveDate, Datelike, Weekday};
use shared::Transaction;

impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set up kid-friendly styling
        setup_kid_friendly_style(ctx);
        
        // Handle ESC key to close dropdown
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_child_dropdown = false;
        }
        
        // Load initial data on first run
        if self.loading && self.current_child.is_none() {
            self.load_initial_data();
        }
        
        // Clear messages after a delay
        if self.error_message.is_some() || self.success_message.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        
        // Main UI with image background
        egui::CentralPanel::default().show(ctx, |ui| {
            // Draw image background with blue overlay first
            let full_rect = ui.available_rect_before_wrap();
            crate::ui::draw_image_background(ui, full_rect);
            
            if self.loading {
                self.render_loading_screen(ui);
                return;
            }
            
            // Header
            self.render_header(ui);
            
            // Error and success messages
            self.render_messages(ui);
            
            // Main content area
            self.render_main_content(ui);
        });
        
        // Render modals
        self.render_modals(ctx);
    }
}

impl AllowanceTrackerApp {
    /// Render the loading screen
    fn render_loading_screen(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.label("Loading...");
        });
    }
    

    

    

    
    /// Render tab buttons for switching between views

    
    /// Render the main content area
    fn render_main_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Render content based on selected tab with toggle header
            match self.current_tab {
                MainTab::Calendar => {
                    // Reserve space for bottom margin before drawing calendar
                    let mut available_rect = ui.available_rect_before_wrap();
                    available_rect.max.y -= 30.0; // Reserve 30px bottom margin
                    self.draw_calendar_section_with_toggle(ui, available_rect, &self.calendar_transactions.clone());
                    
                    // Add bottom spacing to ensure the calendar doesn't touch the edge
                    ui.add_space(30.0);
                }
                MainTab::Table => {
                    // Reserve space for bottom margin before drawing table
                    let mut available_rect = ui.available_rect_before_wrap();
                    available_rect.max.y -= 30.0; // Reserve 30px bottom margin
                    self.draw_transactions_section_with_toggle(ui, available_rect, &self.calendar_transactions.clone());
                    
                    // Add bottom spacing to ensure the table doesn't touch the edge
                    ui.add_space(30.0);
                }
            }
        });
    }
    
    /// Draw calendar section with toggle header integrated
    fn draw_calendar_section_with_toggle(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        // Use the existing draw_calendar_section method but with toggle header
        ui.add_space(15.0);
        
        // Calculate responsive dimensions - same as original
        let content_width = available_rect.width() - 40.0;
        let card_padding = 15.0;
        let available_calendar_width = content_width - (card_padding * 2.0);
        
        // Calendar takes up 92% of available width, centered
        let calendar_width = available_calendar_width * 0.92;
        let calendar_left_margin = (available_calendar_width - calendar_width) / 2.0;
        
        // Calculate cell dimensions proportionally
        let cell_spacing = 4.0;
        let total_spacing = cell_spacing * 6.0;
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
        
        // Draw card background
        self.draw_card_background(ui, card_rect);
        
        // Draw toggle header
        self.draw_toggle_header(ui, card_rect, "Recent Transactions");
        
        // Draw calendar content inside the card
        ui.allocate_ui_at_rect(card_rect, |ui| {
            ui.vertical(|ui| {
                ui.add_space(60.0); // Space for toggle header
                
                // Month navigation
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
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
                
                ui.add_space(15.0);
                
                // Responsive calendar grid - same as original
                ui.horizontal(|ui| {
                    ui.add_space(card_padding + calendar_left_margin);
                    
                    egui::Grid::new("calendar_grid")
                        .num_columns(7)
                        .spacing([cell_spacing, cell_spacing])
                        .min_col_width(cell_width)
                        .max_col_width(cell_width)
                        .striped(false)
                        .show(ui, |ui| {
                            // Day headers - sized proportionally
                            let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
                            for (i, day_name) in day_names.iter().enumerate() {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(cell_width, header_height),
                                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                    |ui| {
                                        ui.label(egui::RichText::new(*day_name)
                                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                            .strong()
                                            .color(self.get_day_header_color(i)));
                                    },
                                );
                            }
                            ui.end_row();
                            
                            // Calendar days - use original method
                            self.draw_calendar_days_responsive(ui, transactions, cell_width, cell_height);
                        });
                });
            });
        });
    }
    
    /// Draw transactions section with toggle header integrated
    fn draw_transactions_section_with_toggle(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        // Use the existing responsive transaction table
        use crate::ui::components::transaction_table::render_responsive_transaction_table;
        
        ui.add_space(15.0);
        
        // Calculate card dimensions
        let content_width = available_rect.width() - 40.0;
        let card_padding = 15.0;
        
        // Calculate card height based on number of transactions
        let header_height = 60.0;
        let row_height = 45.0;
        let title_height = 100.0; // Include space for toggle header
        let num_rows = transactions.len().min(15); // Show max 15 rows
        let card_height = title_height + header_height + (row_height * num_rows as f32) + card_padding * 2.0;
        
        // Ensure card doesn't exceed available rectangle bounds
        let max_available_height = available_rect.height() - 40.0;
        let final_card_height = card_height.min(max_available_height);
        
        let card_rect = egui::Rect::from_min_size(
            egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0),
            egui::vec2(content_width, final_card_height)
        );
        
        // Draw card background
        self.draw_card_background(ui, card_rect);
        
        // Draw toggle header
        self.draw_toggle_header(ui, card_rect, "Recent Transactions");
        
        // Draw table content with proper spacing for header
        let table_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(0.0, 60.0), // Leave space for toggle header
            egui::vec2(card_rect.width(), card_rect.height() - 60.0)
        );
        
        // Use the existing beautiful table implementation
        render_responsive_transaction_table(ui, table_rect, transactions);
    }
    
    /// Draw toggle header within card (like old Tauri version)
    fn draw_toggle_header(&mut self, ui: &mut egui::Ui, card_rect: egui::Rect, title: &str) {
        let header_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(20.0, 15.0),
            egui::vec2(card_rect.width() - 40.0, 40.0),
        );
        
        // Set up UI for header
        let mut header_ui = ui.child_ui(header_rect, egui::Layout::left_to_right(egui::Align::Center), None);
        
        // Title on the left
        header_ui.label(egui::RichText::new(title)
            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
            .strong()
            .color(egui::Color32::from_rgb(70, 70, 70)));
        
        // Push toggle buttons to the right
        header_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Table button (added first so it appears on the right due to right_to_left layout)
            let table_button = egui::Button::new(
                egui::RichText::new("📋 Table")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Table { 
                        egui::Color32::WHITE 
                    } else { 
                        egui::Color32::from_rgb(70, 70, 70) 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Table {
                egui::Color32::from_rgb(79, 109, 245) // Blue for active
            } else {
                egui::Color32::from_rgb(248, 248, 248) // Light gray for inactive
            });
            
            if ui.add(table_button).clicked() {
                self.current_tab = MainTab::Table;
            }
            
            // Small space between buttons
            ui.add_space(8.0);
            
            // Calendar button (added second so it appears on the left due to right_to_left layout)
            let calendar_button = egui::Button::new(
                egui::RichText::new("📅 Calendar")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Calendar { 
                        egui::Color32::WHITE 
                    } else { 
                        egui::Color32::from_rgb(70, 70, 70) 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Calendar {
                egui::Color32::from_rgb(79, 109, 245) // Blue for active
            } else {
                egui::Color32::from_rgb(248, 248, 248) // Light gray for inactive
            });
            
            if ui.add(calendar_button).clicked() {
                self.current_tab = MainTab::Calendar;
            }
        });
    }
    

    
    /// Draw card background with proper styling
    fn draw_card_background(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        
        // Draw subtle shadow first
        let shadow_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(2.0, 2.0),
            rect.size(),
        );
        painter.rect_filled(
            shadow_rect, 
            egui::Rounding::same(10.0),
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 20)
        );
        
        // Draw white background
        painter.rect_filled(
            rect, 
            egui::Rounding::same(10.0),
            egui::Color32::WHITE
        );
        
        // Draw border
        painter.rect_stroke(
            rect,
            egui::Rounding::same(10.0),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 220, 220))
        );
    }
    
    /// Draw card header with title and toggle buttons
    fn draw_card_header_with_toggles(&mut self, ui: &mut egui::Ui, card_rect: egui::Rect) {
        let header_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(20.0, 15.0),
            egui::vec2(card_rect.width() - 40.0, 40.0),
        );
        
        // Set up UI for header
        let mut header_ui = ui.child_ui(header_rect, egui::Layout::left_to_right(egui::Align::Center), None);
        
        // Title on the left
        header_ui.label(egui::RichText::new("Recent Transactions")
            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
            .strong()
            .color(egui::Color32::from_rgb(70, 70, 70)));
        
        // Push toggle buttons to the right
        header_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Chart button
            let chart_button = egui::Button::new(
                egui::RichText::new("📊 Chart")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Calendar { 
                        egui::Color32::WHITE 
                    } else { 
                        egui::Color32::from_rgb(70, 70, 70) 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Calendar {
                egui::Color32::from_rgb(79, 109, 245) // Blue for active
            } else {
                egui::Color32::from_rgb(248, 248, 248) // Light gray for inactive
            });
            
            if ui.add(chart_button).clicked() {
                self.current_tab = MainTab::Calendar;
            }
            
            // Small space between buttons
            ui.add_space(8.0);
            
            // Table button
            let table_button = egui::Button::new(
                egui::RichText::new("📋 Table")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                    .color(if self.current_tab == MainTab::Table { 
                        egui::Color32::WHITE 
                    } else { 
                        egui::Color32::from_rgb(70, 70, 70) 
                    })
            )
            .min_size(egui::vec2(80.0, 30.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(if self.current_tab == MainTab::Table {
                egui::Color32::from_rgb(79, 109, 245) // Blue for active
            } else {
                egui::Color32::from_rgb(248, 248, 248) // Light gray for inactive
            });
            
            if ui.add(table_button).clicked() {
                self.current_tab = MainTab::Table;
            }
        });
    }
    
    /// Draw calendar content within the card
    fn draw_calendar_content(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect, transactions: &[Transaction]) {
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
    
    /// Draw table content within the card
    fn draw_table_content(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect, transactions: &[Transaction]) {
        let mut content_ui = ui.child_ui(content_rect, egui::Layout::top_down(egui::Align::Min), None);
        
        // Transaction table
        egui::ScrollArea::vertical()
            .max_height(content_rect.height())
            .show(&mut content_ui, |ui| {
                
                // Table header
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Date")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .strong()
                        .color(egui::Color32::from_rgb(100, 100, 100)));
                    
                    ui.separator();
                    
                    ui.label(egui::RichText::new("Description")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .strong()
                        .color(egui::Color32::from_rgb(100, 100, 100)));
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Balance")
                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                            .strong()
                            .color(egui::Color32::from_rgb(100, 100, 100)));
                        
                        ui.separator();
                        
                        ui.label(egui::RichText::new("Amount")
                            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                            .strong()
                            .color(egui::Color32::from_rgb(100, 100, 100)));
                    });
                });
                
                ui.separator();
                
                // Transaction rows
                for transaction in transactions {
                    ui.horizontal(|ui| {
                        // Parse and format the date string
                        let date_display = if let Ok(parsed_date) = chrono::DateTime::parse_from_rfc3339(&transaction.date) {
                            parsed_date.format("%b %d, %Y").to_string()
                        } else {
                            transaction.date.clone() // Fallback to original string if parsing fails
                        };
                        
                        ui.label(egui::RichText::new(date_display)
                            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(80, 80, 80)));
                        
                        ui.separator();
                        
                        ui.label(egui::RichText::new(&transaction.description)
                            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(80, 80, 80)));
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(format!("${:.2}", transaction.balance))
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(egui::Color32::from_rgb(80, 80, 80)));
                            
                            ui.separator();
                            
                            let amount_color = if transaction.amount >= 0.0 {
                                egui::Color32::from_rgb(40, 167, 69) // Green for positive
                            } else {
                                egui::Color32::from_rgb(220, 53, 69) // Red for negative
                            };
                            
                            let amount_text = if transaction.amount >= 0.0 {
                                format!("+${:.2}", transaction.amount)
                            } else {
                                format!("-${:.2}", transaction.amount.abs())
                            };
                            
                            ui.label(egui::RichText::new(amount_text)
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(amount_color));
                        });
                    });
                }
            });
    }
    
    /// Draw calendar grid within a specific rect
    fn draw_calendar_grid_in_rect(&mut self, ui: &mut egui::Ui, rect: egui::Rect, transactions: &[Transaction]) {
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
    fn calculate_calendar_grid_height(&self, day_height: f32) -> f32 {
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
    

    
    /// Render the calendar grid for the selected month
    fn render_calendar_grid(&mut self, ui: &mut egui::Ui, day_width: f32, day_height: f32) {
        // Create a date for the first day of the selected month
        let first_day = match NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1) {
            Some(date) => date,
            None => {
                ui.label("Invalid date");
                return;
            }
        };
        
        // Calculate the number of days in the month
        let days_in_month = match first_day.with_day(1) {
            Some(first) => {
                // Get the first day of the next month and subtract 1 day
                let next_month = if self.selected_month == 12 {
                    first.with_year(self.selected_year + 1).unwrap().with_month(1).unwrap()
                } else {
                    first.with_month(self.selected_month + 1).unwrap()
                };
                (next_month - chrono::Duration::days(1)).day()
            }
            None => return,
        };
        
        // Calculate the offset for the first day of the month
        let first_day_offset = match first_day.weekday() {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        };
        
        // Get current date for highlighting today
        let today = chrono::Local::now();
        let is_current_month = today.year() == self.selected_year && today.month() == self.selected_month;
        let today_day = if is_current_month { Some(today.day()) } else { None };
        
        // Group transactions by day of the month
        let mut transactions_by_day: std::collections::HashMap<u32, Vec<&Transaction>> = std::collections::HashMap::new();
        let mut balance_by_day: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
        
        for transaction in &self.calendar_transactions {
            // Parse the transaction date (RFC 3339 format)
            if let Ok(parsed_date) = chrono::DateTime::parse_from_rfc3339(&transaction.date) {
                let transaction_date = parsed_date.naive_local().date();
                
                // Check if this transaction is in the current month/year
                if transaction_date.year() == self.selected_year && transaction_date.month() == self.selected_month {
                    let day = transaction_date.day();
                    transactions_by_day.entry(day).or_insert_with(Vec::new).push(transaction);
                    
                    // Use the balance from the transaction (this is the balance after the transaction)
                    balance_by_day.insert(day, transaction.balance);
                }
            }
        }
        
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
            
            // Calendar grid with dynamic sizing
            let mut current_day = 1;
            let mut week_count = 0;
            
            // Calculate total number of weeks we'll need
            let total_weeks = ((days_in_month as usize + first_day_offset) + 6) / 7;
            
            while current_day <= days_in_month {
                ui.horizontal(|ui| {
                    for day_of_week in 0..7 {
                        let cell_pos = week_count * 7 + day_of_week;
                        
                        if cell_pos < first_day_offset || current_day > days_in_month {
                            // Empty cell - dynamic size
                            ui.add_sized([day_width, day_height], egui::Label::new(""));
                        } else {
                            // Check if this is today
                            let is_today = today_day == Some(current_day);
                            
                            // Get transactions and balance for this day
                            let day_transactions = transactions_by_day.get(&current_day).cloned().unwrap_or_default();
                            let day_balance = balance_by_day.get(&current_day).copied();
                            
                            // Create a vertical layout for the day cell - dynamic size
                            ui.vertical(|ui| {
                                ui.set_width(day_width);
                                ui.set_height(day_height);
                                
                                // Day number button - dynamic size
                                let button_width = day_width - 10.0;
                                let mut day_button = egui::Button::new(
                                    egui::RichText::new(current_day.to_string())
                                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                                        .color(if is_today { egui::Color32::from_rgb(219, 112, 147) } else { egui::Color32::BLACK })
                                );
                                
                                // Special styling for today - pink outline with shadow
                                if is_today {
                                    day_button = day_button
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(219, 112, 147))) // Pink outline
                                        .rounding(egui::Rounding::same(6.0));
                                } else {
                                    day_button = day_button
                                        .fill(egui::Color32::TRANSPARENT)
                                        .rounding(egui::Rounding::same(4.0));
                                }
                                
                                // Add shadow effect for today
                                if is_today {
                                    let button_rect = egui::Rect::from_min_size(
                                        ui.cursor().min + egui::vec2(2.0, 2.0), // Offset for shadow
                                        egui::vec2(button_width, 25.0)
                                    );
                                    ui.painter().rect_filled(
                                        button_rect,
                                        egui::Rounding::same(6.0),
                                        egui::Color32::from_rgba_premultiplied(219, 112, 147, 40) // Pink shadow
                                    );
                                }
                                
                                let day_response = ui.add_sized([button_width, 25.0], day_button);
                                
                                if day_response.clicked() {
                                    println!("Selected day: {}", current_day);
                                }
                                
                                // Show balance if available - larger font
                                if let Some(balance) = day_balance {
                                    ui.add_space(3.0);
                                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("${:.2}", balance))
                                                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                                .color(egui::Color32::GRAY)
                                        );
                                    });
                                }
                                
                                // Show transaction chips - dynamic size
                                for transaction in day_transactions {
                                    ui.add_space(2.0);
                                    
                                    // Determine chip color based on transaction amount
                                    let (chip_color, text_color) = if transaction.amount > 0.0 {
                                        // Green for positive amounts - matching the green from the image
                                        (egui::Color32::from_rgb(46, 160, 67), egui::Color32::from_rgb(46, 160, 67))
                                    } else {
                                        // Gray for negative amounts - matching the gray from the image
                                        (egui::Color32::from_rgb(128, 128, 128), egui::Color32::from_rgb(128, 128, 128))
                                    };
                                    
                                    // Create transaction chip
                                    let chip_text = if transaction.amount > 0.0 {
                                        format!("+${:.2}", transaction.amount)
                                    } else {
                                        format!("-${:.2}", transaction.amount.abs())
                                    };
                                    
                                    // Different styling for future transactions
                                    let is_future = matches!(transaction.transaction_type, shared::TransactionType::FutureAllowance);
                                    
                                    let chip_width = (day_width - 10.0).min(120.0);
                                    
                                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                                        if is_future {
                                            // Dotted/dashed border for future transactions - dynamic size
                                            let (rect, _) = ui.allocate_exact_size(egui::vec2(chip_width, 18.0), egui::Sense::hover());
                                            
                                            // Draw dotted border
                                            ui.painter().rect_stroke(
                                                rect,
                                                egui::Rounding::same(4.0),
                                                egui::Stroke::new(1.0, chip_color)
                                            );
                                            
                                            // Draw text
                                            ui.painter().text(
                                                rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                &chip_text,
                                                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                                                text_color,
                                            );
                                        } else {
                                            // Outlined chip for completed transactions - matching the style from the image
                                            let chip_button = egui::Button::new(
                                                egui::RichText::new(&chip_text)
                                                    .font(egui::FontId::new(10.0, egui::FontFamily::Proportional))
                                                    .color(text_color)
                                            )
                                            .fill(egui::Color32::TRANSPARENT)
                                            .stroke(egui::Stroke::new(1.0, chip_color))
                                            .rounding(egui::Rounding::same(4.0));
                                            
                                            ui.add_sized([chip_width, 18.0], chip_button);
                                        }
                                    });
                                }
                            });
                            
                            current_day += 1;
                        }
                        
                        // No spacing between day cells - grid lines will provide separation
                    }
                });
                week_count += 1;
                
                // Safety check to prevent infinite loop
                if week_count > 6 {
                    break;
                }
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

    /// Navigate to a different month
    fn navigate_month(&mut self, delta: i32) {
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
        self.load_calendar_data();
    }
    
    fn draw_card_with_flat_top(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter();
        
        // Draw subtle shadow first (offset slightly)
        let shadow_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(2.0, 2.0),
            rect.size(),
        );
        painter.rect_filled(
            shadow_rect, 
            egui::Rounding { nw: 8.0, ne: 8.0, sw: 10.0, se: 10.0 }, // Rounded top corners, rounded bottom
            egui::Color32::from_rgba_premultiplied(0, 0, 0, 20)
        );
        
        // Draw white background with rounded top corners (tabs will overlap the flat sections)
        painter.rect_filled(
            rect, 
            egui::Rounding { nw: 8.0, ne: 8.0, sw: 10.0, se: 10.0 }, // Rounded top corners, rounded bottom
            egui::Color32::WHITE
        );
    }
    
    fn draw_integrated_tabs(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect) {
        // Calculate the exact positioning to align with calendar card
        let content_width = available_rect.width() - 40.0; // Same margin as calendar
        
        // Position tabs to align with calendar card left edge and sit directly above it
        let tabs_rect = egui::Rect::from_min_size(
            egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0 - 45.0), // Position tabs right above calendar
            egui::vec2(content_width, 45.0)
        );
        
        // Draw tabs in the calculated position
        ui.allocate_ui_at_rect(tabs_rect, |ui| {
            ui.horizontal(|ui| {
                // Add padding to align with calendar content
                ui.add_space(15.0);
                
                // Calendar tab - file folder style with subtle flare
                let calendar_selected = self.current_tab == MainTab::Calendar;
                let calendar_size = if calendar_selected {
                    [145.0, 45.0] // Just slightly wider when active (subtle flare)
                } else {
                    [140.0, 45.0] // Normal width when inactive
                };
                
                let calendar_rounding = if calendar_selected {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 0.0 } // Rounded top, flat bottom for connection
                } else {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 8.0, se: 0.0 } // Rounded top, rounded bottom-left only
                };
                
                let calendar_button = if calendar_selected {
                    egui::Button::new(egui::RichText::new("📅 Calendar")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(60, 60, 60)))
                        .fill(egui::Color32::WHITE) // Same white as calendar card
                        .rounding(calendar_rounding)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 220, 220))) // Light border
                } else {
                    egui::Button::new(egui::RichText::new("📅 Calendar")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100)))
                        .fill(egui::Color32::from_rgb(240, 240, 240)) // Lighter gray for inactive
                        .rounding(calendar_rounding)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 200)))
                };
                
                if ui.add_sized(calendar_size, calendar_button).clicked() {
                    self.current_tab = MainTab::Calendar;
                }
                
                ui.add_space(2.0); // Small gap between tabs
                
                // Table tab - file folder style with subtle flare
                let table_selected = self.current_tab == MainTab::Table;
                let table_size = if table_selected {
                    [145.0, 45.0] // Just slightly wider when active (subtle flare)
                } else {
                    [140.0, 45.0] // Normal width when inactive
                };
                
                let table_rounding = if table_selected {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 0.0 } // Rounded top, flat bottom for connection
                } else {
                    egui::Rounding { nw: 8.0, ne: 8.0, sw: 0.0, se: 8.0 } // Rounded top, rounded bottom-right only
                };
                
                let table_button = if table_selected {
                    egui::Button::new(egui::RichText::new("📋 Table")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(60, 60, 60)))
                        .fill(egui::Color32::WHITE) // Same white as calendar card
                        .rounding(table_rounding)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 220, 220)))
                } else {
                    egui::Button::new(egui::RichText::new("📋 Table")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100)))
                        .fill(egui::Color32::from_rgb(240, 240, 240)) // Lighter gray for inactive
                        .rounding(table_rounding)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 200)))
                };
                
                if ui.add_sized(table_size, table_button).clicked() {
                    self.current_tab = MainTab::Table;
                }
            });
        });
    }
    

    
    fn draw_calendar_section(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        // Responsive approach: size everything as percentages of available space
        
        // Calculate responsive dimensions
        let content_width = available_rect.width() - 40.0; // Leave some margin
        let card_padding = 15.0; // Reduced from 20.0
        let available_calendar_width = content_width - (card_padding * 2.0);
        
        // Calendar takes up 92% of available width, centered (increased from 85%)
        let calendar_width = available_calendar_width * 0.92;
        let calendar_left_margin = (available_calendar_width - calendar_width) / 2.0;
        
        // Calculate cell dimensions proportionally
        let cell_spacing = 4.0; // Reduced from 6.0
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
    
    fn get_day_header_color(&self, day_index: usize) -> egui::Color32 {
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
    
    fn draw_calendar_days_responsive(&mut self, ui: &mut egui::Ui, transactions: &[Transaction], cell_width: f32, cell_height: f32) {
        // Create a date for the first day of the selected month
        let first_day = match NaiveDate::from_ymd_opt(self.selected_year, self.selected_month, 1) {
            Some(date) => date,
            None => return,
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
            None => return,
        };
        
        // Calculate the offset for the first day of the month
        let first_day_offset = match first_day.weekday() {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        };
        
        // Get current date for highlighting today
        let today = chrono::Local::now();
        let is_current_month = today.year() == self.selected_year && today.month() == self.selected_month;
        let today_day = if is_current_month { Some(today.day()) } else { None };
        
        // Group transactions by day
        let mut transactions_by_day: std::collections::HashMap<u32, Vec<&Transaction>> = std::collections::HashMap::new();
        for transaction in transactions {
            if let Ok(parsed_date) = chrono::DateTime::parse_from_rfc3339(&transaction.date) {
                let transaction_date = parsed_date.naive_local().date();
                if transaction_date.year() == self.selected_year && transaction_date.month() == self.selected_month {
                    let day = transaction_date.day();
                    transactions_by_day.entry(day).or_insert_with(Vec::new).push(transaction);
                }
            }
        }
        
        // Draw calendar cells
        let mut current_day = 1;
        let mut week_count = 0;
        
        while current_day <= days_in_month {
            for day_of_week in 0..7 {
                let cell_pos = week_count * 7 + day_of_week;
                
                if cell_pos < first_day_offset || current_day > days_in_month {
                    // Empty cell - size it to match other cells
                    ui.add_sized([cell_width, cell_height], egui::Label::new(""));
                } else {
                    // Day cell - responsive sizing
                    let is_today = today_day == Some(current_day);
                    let day_transactions = transactions_by_day.get(&current_day).cloned().unwrap_or_default();
                    
                    ui.vertical(|ui| {
                        ui.set_width(cell_width);
                        ui.set_height(cell_height);
                        
                        // Day number - responsive sizing
                        let day_font_size = (cell_width * 0.2).max(14.0).min(20.0); // Responsive font size
                        let day_button_height = cell_height * 0.3; // 30% of cell height
                        let day_button_width = cell_width * 0.9; // 90% of cell width
                        
                        let day_button = if is_today {
                            // Pink outline with shadow for current day - matching the reference screenshot
                            egui::Button::new(egui::RichText::new(current_day.to_string()).size(day_font_size).color(egui::Color32::from_rgb(219, 112, 147)))
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(219, 112, 147))) // Pink outline
                                .rounding(egui::Rounding::same(6.0))
                        } else {
                            egui::Button::new(egui::RichText::new(current_day.to_string()).size(day_font_size))
                                .fill(egui::Color32::TRANSPARENT)
                                .rounding(egui::Rounding::same(4.0))
                        };
                        
                        // Add shadow effect for today
                        if is_today {
                            let button_rect = egui::Rect::from_min_size(
                                ui.cursor().min + egui::vec2(2.0, 2.0), // Offset for shadow
                                egui::vec2(day_button_width, day_button_height)
                            );
                            ui.painter().rect_filled(
                                button_rect,
                                egui::Rounding::same(6.0),
                                egui::Color32::from_rgba_premultiplied(219, 112, 147, 40) // Pink shadow
                            );
                        }
                        
                        ui.add_sized([day_button_width, day_button_height], day_button);
                        
                        // Transaction chips - responsive sizing
                        let chip_font_size = (cell_width * 0.12).max(9.0).min(12.0); // Responsive chip font
                        let chip_height = cell_height * 0.15; // 15% of cell height
                        let chip_width = cell_width * 0.85; // 85% of cell width
                        
                        for transaction in day_transactions.iter().take(2) { // Show max 2 transactions
                            let (chip_color, text_color) = if transaction.amount > 0.0 {
                                // Green for positive amounts - matching the green from the image
                                (egui::Color32::from_rgb(46, 160, 67), egui::Color32::from_rgb(46, 160, 67))
                            } else {
                                // Gray for negative amounts - matching the gray from the image
                                (egui::Color32::from_rgb(128, 128, 128), egui::Color32::from_rgb(128, 128, 128))
                            };
                            
                            let chip_text = if transaction.amount > 0.0 {
                                format!("+${:.0}", transaction.amount)
                            } else {
                                format!("-${:.0}", transaction.amount.abs())
                            };
                            
                            let chip = egui::Button::new(egui::RichText::new(chip_text).size(chip_font_size).color(text_color))
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::new(1.0, chip_color))
                                .rounding(egui::Rounding::same(4.0));
                            ui.add_sized([chip_width, chip_height], chip);
                        }
                    });
                    
                    current_day += 1;
                }
            }
            ui.end_row();
            week_count += 1;
        }
    }
} 