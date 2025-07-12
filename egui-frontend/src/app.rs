use log::{info, warn};
use chrono::Datelike;
use shared::*;
use allowance_tracker_egui::backend::{Backend};
use allowance_tracker_egui::backend::domain::commands::transactions::TransactionListQuery;
use allowance_tracker_egui::backend::domain::commands::child::SetActiveChildCommand;
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

/// Simple transaction mapper for converting domain transactions to DTOs
struct TransactionMapper;

impl TransactionMapper {
    fn to_dto(domain_tx: allowance_tracker_egui::backend::domain::models::transaction::Transaction) -> Transaction {
        Transaction {
            id: domain_tx.id,
            child_id: domain_tx.child_id,
            date: domain_tx.date,
            description: domain_tx.description,
            amount: domain_tx.amount,
            balance: domain_tx.balance,
            transaction_type: match domain_tx.transaction_type {
                allowance_tracker_egui::backend::domain::models::transaction::TransactionType::Income => TransactionType::Income,
                allowance_tracker_egui::backend::domain::models::transaction::TransactionType::Expense => TransactionType::Expense,
                allowance_tracker_egui::backend::domain::models::transaction::TransactionType::FutureAllowance => TransactionType::FutureAllowance,
            },
        }
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
    #[allow(dead_code)]
    calendar_loading: bool,
    calendar_transactions: Vec<Transaction>,
    selected_month: u32,
    selected_year: i32,
    
    // Modal states
    show_add_money_modal: bool,
    show_spend_money_modal: bool,
    show_child_selector: bool,
    #[allow(dead_code)]
    show_settings_menu: bool,
    #[allow(dead_code)]
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
        
        // Load recent transactions for the current month
        let query = TransactionListQuery {
            after: None,
            limit: Some(20), // Load last 20 transactions
            start_date: None,
            end_date: None,
        };
        
        match self.backend.transaction_service.list_transactions_domain(query) {
            Ok(result) => {
                info!("📊 Successfully loaded {} transactions", result.transactions.len());
                
                // Convert domain transactions to DTOs
                self.calendar_transactions = result.transactions
                    .into_iter()
                    .map(TransactionMapper::to_dto)
                    .collect();
                
                // Update balance from the most recent transaction
                if let Some(latest_transaction) = self.calendar_transactions.first() {
                    self.current_balance = latest_transaction.balance;
                }
            }
            Err(e) => {
                warn!("❌ Failed to load transactions: {}", e);
                self.error_message = Some(format!("Failed to load transactions: {}", e));
                self.calendar_transactions = Vec::new();
            }
        }
    }
    
    /// Clear success and error messages
    #[allow(dead_code)]
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
                        if ui.button("👤 Select Child").clicked() {
                            self.show_child_selector = true;
                        }
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
                        
                        // Recent transactions table
                        ui.heading("📋 Recent Transactions");
                        if self.calendar_transactions.is_empty() {
                            ui.label("No transactions yet!");
                        } else {
                            // Create a nice table-like display
                            ui.group(|ui| {
                                ui.set_min_width(450.0);
                                
                                // Table header
                                ui.horizontal(|ui| {
                                    ui.set_min_height(30.0);
                                    ui.strong("DATE");
                                    ui.separator();
                                    ui.strong("DESCRIPTION");
                                    ui.separator();
                                    ui.strong("AMOUNT");
                                    ui.separator();
                                    ui.strong("BALANCE");
                                });
                                
                                ui.separator();
                                
                                // Transaction rows
                                for transaction in &self.calendar_transactions {
                                    ui.horizontal(|ui| {
                                        ui.set_min_height(25.0);
                                        
                                        // Date column (simplified)
                                        let date_str = if let Some(date_part) = transaction.date.split('T').next() {
                                            // Parse and format date nicely
                                            if let Ok(parsed_date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                                                parsed_date.format("%b %d, %Y").to_string()
                                            } else {
                                                date_part.to_string()
                                            }
                                        } else {
                                            "Unknown".to_string()
                                        };
                                        ui.label(date_str);
                                        
                                        ui.separator();
                                        
                                        // Description column
                                        ui.label(&transaction.description);
                                        
                                        ui.separator();
                                        
                                        // Amount column with color coding
                                        if transaction.amount >= 0.0 {
                                            ui.colored_label(
                                                egui::Color32::from_rgb(34, 139, 34), // Green for positive
                                                format!("+${:.2}", transaction.amount)
                                            );
                                        } else {
                                            ui.colored_label(
                                                egui::Color32::from_rgb(220, 20, 60), // Red for negative
                                                format!("-${:.2}", transaction.amount.abs())
                                            );
                                        }
                                        
                                        ui.separator();
                                        
                                        // Balance column
                                        ui.label(format!("${:.2}", transaction.balance));
                                    });
                                }
                            });
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
            
            // Child selector modal
            if self.show_child_selector {
                egui::Window::new("👤 Select Child")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.heading("Available Children:");
                        
                        // List all children
                        match self.backend.child_service.list_children() {
                            Ok(children_result) => {
                                if children_result.children.is_empty() {
                                    ui.label("No children found!");
                                    ui.label("Debug: Check if test_data directory exists");
                                } else {
                                    for child in children_result.children {
                                        ui.horizontal(|ui| {
                                            // Show if this is the current active child
                                            let is_active = self.current_child.as_ref()
                                                .map(|c| c.id == child.id)
                                                .unwrap_or(false);
                                            
                                            if is_active {
                                                ui.label("👑"); // Crown for active child
                                            } else {
                                                ui.label("   "); // Spacing
                                            }
                                            
                                            if ui.button(&child.name).clicked() {
                                                // Set this child as active
                                                let command = SetActiveChildCommand {
                                                    child_id: child.id.clone(),
                                                };
                                                match self.backend.child_service.set_active_child(command) {
                                                    Ok(_) => {
                                                        self.current_child = Some(to_dto(child.clone()));
                                                        self.load_balance();
                                                        self.load_calendar_data();
                                                        self.show_child_selector = false;
                                                        self.success_message = Some("Child selected successfully!".to_string());
                                                    }
                                                    Err(e) => {
                                                        self.error_message = Some(format!("Failed to select child: {}", e));
                                                    }
                                                }
                                            }
                                            
                                            ui.label(child.birthdate.to_string());
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                ui.label(format!("Error loading children: {}", e));
                                ui.label("Debug: Check backend initialization");
                            }
                        }
                        
                        ui.separator();
                        
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                self.show_child_selector = false;
                            }
                            
                            if ui.button("🔄 Refresh").clicked() {
                                // Try to reload the active child
                                self.load_initial_data();
                            }
                        });
                    });
            }
        });
    }
}
