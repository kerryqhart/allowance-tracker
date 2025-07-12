use log::{info, warn};
use std::sync::Arc;
use chrono::{Datelike, Local};
use shared::*;
use allowance_tracker_egui::backend::{Backend};
use eframe::egui;

/// Helper function to convert domain child to shared child
pub fn to_dto(child: allowance_tracker_egui::backend::domain::models::child::Child) -> Child {
    Child {
        id: child.id,
        name: child.name,
        birthdate: child.birthdate.to_string(),
        created_at: child.created_at.to_rfc3339(),
        updated_at: child.updated_at.to_rfc3339(),
    }
}

/// Main application struct for the egui allowance tracker
pub struct AllowanceTrackerApp {
    backend: Backend,
    
    // Application state
    current_child: Option<Child>,
    current_balance: f64,
    
    // UI state
    loading: bool,
    error_message: Option<String>,
    success_message: Option<String>,
    
    // Calendar state
    calendar_loading: bool,
    calendar_transactions: Vec<Transaction>,
    selected_month: u32,
    selected_year: i32,
    
    // Modal states
    show_add_money_modal: bool,
    show_spend_money_modal: bool,
    show_child_selector: bool,
    show_settings_menu: bool,
    show_allowance_config_modal: bool,
    
    // Form states
    add_money_amount: String,
    add_money_description: String,
    spend_money_amount: String,
    spend_money_description: String,
}

impl AllowanceTrackerApp {
    /// Create a new allowance tracker app
    pub fn new() -> Result<Self, anyhow::Error> {
        info!("🚀 Initializing AllowanceTrackerApp");
        
        let backend = Backend::new()?;
        
        let now = chrono::Local::now();
        let current_month = now.month();
        let current_year = now.year();
        
        Ok(Self {
            backend,
            
            // Application state
            current_child: None,
            current_balance: 0.0,
            
            // UI state
            loading: true,
            error_message: None,
            success_message: None,
            
            // Calendar state
            calendar_loading: false,
            calendar_transactions: Vec::new(),
            selected_month: current_month,
            selected_year: current_year,
            
            // Modal states
            show_add_money_modal: false,
            show_spend_money_modal: false,
            show_child_selector: false,
            show_settings_menu: false,
            show_allowance_config_modal: false,
            
            // Form states
            add_money_amount: String::new(),
            add_money_description: String::new(),
            spend_money_amount: String::new(),
            spend_money_description: String::new(),
        })
    }
    
    /// Load initial data
    pub fn load_initial_data(&mut self) {
        info!("📊 Loading initial data");
        
        // Load active child
        match self.backend.child_service.get_active_child() {
            Ok(response) => {
                if let Some(child) = response.active_child.child {
                    self.current_child = Some(to_dto(child));
                    self.load_balance();
                    self.load_calendar_data();
                }
                self.loading = false;
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to load active child: {}", e));
                self.loading = false;
            }
        }
    }
    
    /// Load current balance
    fn load_balance(&mut self) {
        // For now, set a placeholder balance
        // TODO: Implement actual balance calculation
        self.current_balance = 42.50;
    }
    
    /// Load calendar data
    fn load_calendar_data(&mut self) {
        info!("📅 Loading calendar data for {}/{}", self.selected_month, self.selected_year);
        
        // For now, load empty transactions
        // TODO: Implement actual calendar data loading
        self.calendar_transactions = Vec::new();
    }
    
    /// Clear success and error messages
    fn clear_messages(&mut self) {
        self.error_message = None;
        self.success_message = None;
    }
}

impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set up kid-friendly styling
        ctx.set_style({
            let mut style = (*ctx.style()).clone();
            
            // Bright, fun colors
            style.visuals.window_fill = egui::Color32::from_rgb(240, 248, 255); // Light blue background
            style.visuals.panel_fill = egui::Color32::from_rgb(250, 250, 250); // Light gray panels
            style.visuals.button_frame = true;
            
            // Larger text for readability
            style.text_styles.insert(
                egui::TextStyle::Heading,
                egui::FontId::new(28.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Body,
                egui::FontId::new(16.0, egui::FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Button,
                egui::FontId::new(18.0, egui::FontFamily::Proportional),
            );
            
            // Rounded corners and padding
            style.spacing.button_padding = egui::vec2(12.0, 8.0);
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
            style.visuals.widgets.active.rounding = egui::Rounding::same(8.0);
            style.visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
            
            style
        });
        
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
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.spinner();
                    ui.label("Loading...");
                });
                return;
            }
            
            // Header
            ui.horizontal(|ui| {
                ui.heading("💰 My Allowance Tracker");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(child) = &self.current_child {
                        ui.label(format!("👤 {}", child.name));
                        ui.label(format!("💵 ${:.2}", self.current_balance));
                    } else {
                        ui.label("No active child");
                    }
                });
            });
            
            ui.separator();
            
            // Error and success messages
            if let Some(error) = &self.error_message {
                ui.colored_label(egui::Color32::RED, format!("❌ {}", error));
            }
            if let Some(success) = &self.success_message {
                ui.colored_label(egui::Color32::GREEN, format!("✅ {}", success));
            }
            
            // Main content area
            ui.horizontal(|ui| {
                // Left side - Calendar
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.heading("📅 Calendar");
                        
                        // Calendar month/year selector
                        ui.horizontal(|ui| {
                            if ui.button("⬅").clicked() {
                                if self.selected_month == 1 {
                                    self.selected_month = 12;
                                    self.selected_year -= 1;
                                } else {
                                    self.selected_month -= 1;
                                }
                                self.load_calendar_data();
                            }
                            
                            ui.label(format!("{}/{}", self.selected_month, self.selected_year));
                            
                            if ui.button("➡").clicked() {
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
                        ui.label("📝 Calendar grid will go here");
                        ui.label("🎯 This will show transaction chips on each day");
                    });
                });
                
                // Right side - Forms and transactions
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.heading("💰 Money Actions");
                        
                        // Add money button
                        if ui.button("💵 Add Money").clicked() {
                            self.show_add_money_modal = true;
                        }
                        
                        // Spend money button
                        if ui.button("🛍️ Spend Money").clicked() {
                            self.show_spend_money_modal = true;
                        }
                        
                        ui.separator();
                        
                        // Recent transactions
                        ui.heading("📋 Recent Transactions");
                        if self.calendar_transactions.is_empty() {
                            ui.label("No transactions yet!");
                        } else {
                            for transaction in &self.calendar_transactions {
                                ui.horizontal(|ui| {
                                    ui.label(&transaction.description);
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(format!("${:.2}", transaction.amount));
                                    });
                                });
                            }
                        }
                    });
                });
            });
            
            // Modals (simplified for now)
            if self.show_add_money_modal {
                egui::Window::new("Add Money")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.text_edit_singleline(&mut self.add_money_amount);
                        ui.text_edit_singleline(&mut self.add_money_description);
                        ui.horizontal(|ui| {
                            if ui.button("Add").clicked() {
                                // TODO: Implement add money logic
                                self.show_add_money_modal = false;
                                self.success_message = Some("Money added!".to_string());
                            }
                            if ui.button("Cancel").clicked() {
                                self.show_add_money_modal = false;
                            }
                        });
                    });
            }
            
            if self.show_spend_money_modal {
                egui::Window::new("Spend Money")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.text_edit_singleline(&mut self.spend_money_amount);
                        ui.text_edit_singleline(&mut self.spend_money_description);
                        ui.horizontal(|ui| {
                            if ui.button("Spend").clicked() {
                                // TODO: Implement spend money logic
                                self.show_spend_money_modal = false;
                                self.success_message = Some("Money spent!".to_string());
                            }
                            if ui.button("Cancel").clicked() {
                                self.show_spend_money_modal = false;
                            }
                        });
                    });
            }
        });
    }
} 