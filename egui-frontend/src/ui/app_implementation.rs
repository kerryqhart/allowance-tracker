use eframe::egui;
use crate::ui::app_state::{AllowanceTrackerApp, MainTab};
use crate::ui::*;
use chrono::{NaiveDate, Datelike, Weekday};
use shared::Transaction;

impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set up kid-friendly styling
        setup_kid_friendly_style(ctx);
        
        // Load initial data on first run
        if self.loading && self.current_child.is_none() {
            self.load_initial_data();
        }
        
        // Clear messages after a delay
        if self.error_message.is_some() || self.success_message.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        
        // Main UI with gradient background
        egui::CentralPanel::default().show(ctx, |ui| {
            // Draw gradient background first
            let full_rect = ui.available_rect_before_wrap();
            crate::ui::draw_gradient_background(ui, full_rect);
            
            if self.loading {
                self.render_loading_screen(ui);
                return;
            }
            
            // Header
            self.render_header(ui);
            
            ui.separator();
            
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
    
    /// Render the header
    fn render_header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Use Proportional font for emoji-containing text
            ui.label(egui::RichText::new("💰 My Allowance Tracker")
                .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                .strong());
                
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(child) = &self.current_child {
                    ui.label(egui::RichText::new(format!("👤 {}", child.name))
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional)));
                    ui.label(egui::RichText::new(format!("💵 ${:.2}", self.current_balance))
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional)));
                } else {
                    ui.label("No active child");
                }
            });
        });
    }
    
    /// Render error and success messages
    fn render_messages(&self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error_message {
            ui.colored_label(egui::Color32::RED, format!("❌ {}", error));
        }
        if let Some(success) = &self.success_message {
            ui.colored_label(egui::Color32::GREEN, format!("✅ {}", success));
        }
    }
    
    /// Render tab buttons for switching between views
    fn render_tab_buttons(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Center the tabs
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(20.0);
                
                // Calendar tab
                let calendar_selected = self.current_tab == MainTab::Calendar;
                let calendar_button = if calendar_selected {
                    egui::Button::new(egui::RichText::new("📅 Calendar")
                        .font(egui::FontId::new(20.0, egui::FontFamily::Proportional)))
                        .fill(egui::Color32::from_rgb(70, 130, 180)) // Steel blue for selected
                } else {
                    egui::Button::new(egui::RichText::new("📅 Calendar")
                        .font(egui::FontId::new(20.0, egui::FontFamily::Proportional)))
                        .fill(egui::Color32::from_rgb(240, 240, 240)) // Light gray for unselected
                };
                
                if ui.add_sized([150.0, 45.0], calendar_button).clicked() {
                    self.current_tab = MainTab::Calendar;
                }
                
                ui.add_space(10.0);
                
                // Table tab
                let table_selected = self.current_tab == MainTab::Table;
                let table_button = if table_selected {
                    egui::Button::new(egui::RichText::new("📋 Table")
                        .font(egui::FontId::new(20.0, egui::FontFamily::Proportional)))
                        .fill(egui::Color32::from_rgb(70, 130, 180)) // Steel blue for selected
                } else {
                    egui::Button::new(egui::RichText::new("📋 Table")
                        .font(egui::FontId::new(20.0, egui::FontFamily::Proportional)))
                        .fill(egui::Color32::from_rgb(240, 240, 240)) // Light gray for unselected
                };
                
                if ui.add_sized([150.0, 45.0], table_button).clicked() {
                    self.current_tab = MainTab::Table;
                }
            });
        });
    }
    
    /// Render the main content area
    fn render_main_content(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // Render tab buttons
            self.render_tab_buttons(ui);
            
            ui.add_space(10.0);
            
            // Render content based on selected tab
            match self.current_tab {
                MainTab::Calendar => {
                    // Reserve space for bottom margin before drawing calendar
                    let mut available_rect = ui.available_rect_before_wrap();
                    available_rect.max.y -= 30.0; // Reserve 30px bottom margin
                    self.draw_calendar_section(ui, available_rect, &self.calendar_transactions.clone());
                    
                    // Add bottom spacing to ensure the calendar doesn't touch the edge
                    ui.add_space(30.0);
                }
                MainTab::Table => {
                    // Reserve space for bottom margin before drawing table
                    let mut available_rect = ui.available_rect_before_wrap();
                    available_rect.max.y -= 30.0; // Reserve 30px bottom margin
                    self.draw_transactions_section(ui, available_rect, &self.calendar_transactions.clone());
                    
                    // Add bottom spacing to ensure the table doesn't touch the edge
                    ui.add_space(30.0);
                }
            }
        });
    }
    
    /// Render the calendar section
    fn render_calendar_section(&mut self, ui: &mut egui::Ui) {
        // Calculate dynamic calendar dimensions first
        let available_width = ui.available_width();
        let day_spacing = 6.0 * 4.0; // 6 gaps between 7 days, 4px each
        let card_padding = 40.0; // Padding inside the card (left + right = 40px total)
        
        // Calculate usable width for the calendar content (leaving room for card padding)
        let max_calendar_width = (available_width - card_padding - 20.0).max(420.0); // 20px margin for centering
        let usable_width = max_calendar_width - day_spacing;
        let day_width = usable_width / 7.0;
        let day_height = day_width.max(80.0);
        
        // Calculate actual calendar dimensions
        let calendar_width = day_width * 7.0 + day_spacing;
        let header_height = 40.0;
        let title_area_height = 80.0; // Title + month navigation + spacing
        let calendar_grid_height = self.calculate_calendar_grid_height(day_height);
        let calendar_height = title_area_height + header_height + calendar_grid_height + 30.0; // Extra padding
        
        // Calculate final card container size
        let card_width = calendar_width + card_padding;
        let card_height = calendar_height + card_padding;
        
        // Center the card in available space
        let available_rect = ui.available_rect_before_wrap();
        let card_x = (available_rect.width() - card_width).max(0.0) / 2.0 + available_rect.min.x;
        let card_y = available_rect.min.y + 10.0;
        
        let calendar_rect = egui::Rect::from_min_size(
            egui::pos2(card_x, card_y),
            egui::vec2(card_width, card_height),
        );
        
        // Draw the modern card container
        crate::ui::draw_card_container(ui, calendar_rect, 15.0);
        
        // Create a child UI within the card container
        let mut child_ui = ui.child_ui(calendar_rect, *ui.layout(), None);
        
        child_ui.vertical(|ui| {
            ui.add_space(20.0);
            
            // Center the calendar title
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new("📅 Calendar")
                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                    .strong());
            });
            
            ui.add_space(10.0);
                
                // Center the month/year selector
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new("⬅")
                            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))).clicked() {
                            if self.selected_month == 1 {
                                self.selected_month = 12;
                                self.selected_year -= 1;
                            } else {
                                self.selected_month -= 1;
                            }
                            self.load_calendar_data();
                        }
                        
                        ui.add_space(20.0);
                        
                        // Format month name
                        let month_name = match self.selected_month {
                            1 => "January",
                            2 => "February", 
                            3 => "March",
                            4 => "April",
                            5 => "May",
                            6 => "June",
                            7 => "July",
                            8 => "August",
                            9 => "September",
                            10 => "October",
                            11 => "November",
                            12 => "December",
                            _ => "Unknown",
                        };
                        
                        ui.label(egui::RichText::new(format!("{} {}", month_name, self.selected_year))
                            .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                            .strong());
                        
                        ui.add_space(20.0);
                        
                        if ui.button(egui::RichText::new("➡")
                            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))).clicked() {
                            if self.selected_month == 12 {
                                self.selected_month = 1;
                                self.selected_year += 1;
                            } else {
                                self.selected_month += 1;
                            }
                            self.load_calendar_data();
                        }
                    });
                });
                
                ui.add_space(10.0);
                
                // Calendar grid
                self.render_calendar_grid(ui, day_width, day_height);
            });
        
        // Reserve space for the calendar container
        ui.allocate_space(egui::vec2(calendar_rect.width(), calendar_rect.height() + 20.0));
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
    
    /// Render the transactions section
    fn render_transactions_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("📋 Recent Transactions")
                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                    .strong());
                
                render_transaction_table(ui, &self.calendar_transactions);
            });
        });
    }
    
    /// Draw transactions section with responsive table in card container
    fn draw_transactions_section(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        crate::ui::components::transaction_table::render_responsive_transaction_table(ui, available_rect, transactions);
    }
    
    /// Render the money management section
    fn render_money_management_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.vertical(|ui| {
                // Center the title
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("💰 Money Actions")
                        .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                        .strong());
                });
                
                ui.add_space(10.0);
                
                // Center the action buttons
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.horizontal(|ui| {
                        // Add money button - make it larger and more prominent
                        if ui.add_sized([150.0, 50.0], egui::Button::new(
                            egui::RichText::new("💵 Add Money")
                                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        )).clicked() {
                            self.show_add_money_modal = true;
                        }
                        
                        ui.add_space(20.0);
                        
                        // Spend money button - make it larger and more prominent
                        if ui.add_sized([150.0, 50.0], egui::Button::new(
                            egui::RichText::new("🛍️ Spend Money")
                                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        )).clicked() {
                            self.show_spend_money_modal = true;
                        }
                        
                        // Child selector button if no active child
                        if self.current_child.is_none() {
                            ui.add_space(20.0);
                            if ui.add_sized([150.0, 50.0], egui::Button::new(
                                egui::RichText::new("👤 Select Child")
                                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                            )).clicked() {
                                self.show_child_selector = true;
                            }
                        }
                    });
                });
            });
        });
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
        
        // Use the pre-calculated dimensions
        let day_spacing = 6.0 * 4.0; // 6 gaps between 7 days, 4px each
        let total_calendar_width = day_width * 7.0 + day_spacing;
        
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
                    
                    // Add spacing between day headers
                    if index < 6 {
                        ui.add_space(4.0);
                    }
                }
            });
            
            ui.add_space(5.0);
            
            // Calendar grid with dynamic sizing
            let mut current_day = 1;
            let mut week_count = 0;
            
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
                                let button_width = day_width - 5.0;
                                let mut day_button = egui::Button::new(
                                    egui::RichText::new(current_day.to_string())
                                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                                        .color(if is_today { egui::Color32::WHITE } else { egui::Color32::BLACK })
                                );
                                
                                // Special styling for today
                                if is_today {
                                    day_button = day_button.fill(egui::Color32::from_rgb(0, 120, 215)); // Nice blue
                                } else {
                                    day_button = day_button.fill(egui::Color32::WHITE);
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
                                    let chip_color = if transaction.amount > 0.0 {
                                        egui::Color32::from_rgb(34, 139, 34)  // Green for positive
                                    } else {
                                        egui::Color32::from_rgb(220, 20, 60)  // Red for negative
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
                                                chip_color,
                                            );
                                        } else {
                                            // Solid chip for completed transactions - dynamic size
                                            let chip_button = egui::Button::new(
                                                egui::RichText::new(&chip_text)
                                                    .font(egui::FontId::new(10.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::WHITE)
                                            ).fill(chip_color);
                                            
                                            ui.add_sized([chip_width, 18.0], chip_button);
                                        }
                                    });
                                }
                            });
                            
                            current_day += 1;
                        }
                        
                        // Add spacing between day cells
                        if day_of_week < 6 {
                            ui.add_space(4.0);
                        }
                    }
                });
                week_count += 1;
                
                // Safety check to prevent infinite loop
                if week_count > 6 {
                    break;
                }
            }
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
        
        draw_card_container(ui, card_rect, 10.0);
        
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
                            egui::Button::new(egui::RichText::new(current_day.to_string()).size(day_font_size).color(egui::Color32::WHITE))
                                .fill(egui::Color32::from_rgb(0, 120, 215))
                                .rounding(egui::Rounding::same(4.0))
                        } else {
                            egui::Button::new(egui::RichText::new(current_day.to_string()).size(day_font_size))
                                .fill(egui::Color32::WHITE)
                                .rounding(egui::Rounding::same(4.0))
                        };
                        
                        ui.add_sized([day_button_width, day_button_height], day_button);
                        
                        // Transaction chips - responsive sizing
                        let chip_font_size = (cell_width * 0.12).max(9.0).min(12.0); // Responsive chip font
                        let chip_height = cell_height * 0.15; // 15% of cell height
                        let chip_width = cell_width * 0.85; // 85% of cell width
                        
                        for transaction in day_transactions.iter().take(2) { // Show max 2 transactions
                            let chip_color = if transaction.amount > 0.0 {
                                egui::Color32::from_rgb(34, 139, 34)  // Green
                            } else {
                                egui::Color32::from_rgb(220, 20, 60)  // Red
                            };
                            
                            let chip_text = if transaction.amount > 0.0 {
                                format!("+${:.0}", transaction.amount)
                            } else {
                                format!("-${:.0}", transaction.amount.abs())
                            };
                            
                            let chip = egui::Button::new(egui::RichText::new(chip_text).size(chip_font_size).color(egui::Color32::WHITE))
                                .fill(chip_color)
                                .rounding(egui::Rounding::same(2.0));
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