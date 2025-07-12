use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::*;

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
        
        // Main UI
        egui::CentralPanel::default().show(ctx, |ui| {
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
    
    /// Render the main content area
    fn render_main_content(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Left side - Calendar
            self.render_calendar_section(ui);
            
            // Right side - Forms and transactions
            self.render_actions_section(ui);
        });
    }
    
    /// Render the calendar section
    fn render_calendar_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("📅 Calendar")
                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                    .strong());
                
                // Calendar month/year selector
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("⬅")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
                        if self.selected_month == 1 {
                            self.selected_month = 12;
                            self.selected_year -= 1;
                        } else {
                            self.selected_month -= 1;
                        }
                        self.load_calendar_data();
                    }
                    
                    ui.label(format!("{}/{}", self.selected_month, self.selected_year));
                    
                    if ui.button(egui::RichText::new("➡")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
                        if self.selected_month == 12 {
                            self.selected_month = 1;
                            self.selected_year += 1;
                        } else {
                            self.selected_month += 1;
                        }
                        self.load_calendar_data();
                    }
                });
                
                // Calendar grid placeholder
                ui.label(egui::RichText::new("📝 Calendar grid will go here")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional)));
                ui.label(egui::RichText::new("🎯 This will show transaction chips on each day")
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional)));
            });
        });
    }
    
    /// Render the actions section
    fn render_actions_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("💰 Money Actions")
                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                    .strong());
                
                // Add money button
                if ui.button(egui::RichText::new("💵 Add Money")
                    .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))).clicked() {
                    self.show_add_money_modal = true;
                }
                
                // Spend money button
                if ui.button(egui::RichText::new("🛍️ Spend Money")
                    .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))).clicked() {
                    self.show_spend_money_modal = true;
                }
                
                // Child selector button if no active child
                if self.current_child.is_none() {
                    if ui.button(egui::RichText::new("👤 Select Child")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
                        self.show_child_selector = true;
                    }
                }
                
                ui.separator();
                
                // Recent transactions table
                ui.label(egui::RichText::new("📋 Recent Transactions")
                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                    .strong());
                
                render_transaction_table(ui, &self.calendar_transactions);
            });
        });
    }
} 