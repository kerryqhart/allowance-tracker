use log::{info, warn};
use chrono::Datelike;
use shared::*;
use allowance_tracker_egui::backend::{Backend};
use allowance_tracker_egui::backend::domain::commands::transactions::TransactionListQuery;
use allowance_tracker_egui::backend::domain::commands::child::SetActiveChildCommand;
use eframe::egui;
use egui_extras::{TableBuilder, Column};
use std::path::Path;

/// Helper function to load system fonts on macOS
fn load_system_font(font_name: &str) -> Option<Vec<u8>> {
    // macOS system font directories
    let font_paths = [
        format!("/System/Library/Fonts/{}.ttc", font_name),
        format!("/System/Library/Fonts/{}.ttf", font_name),
        format!("/Library/Fonts/{}.ttc", font_name),
        format!("/Library/Fonts/{}.ttf", font_name),
        format!("/System/Library/Fonts/Supplemental/{}.ttf", font_name),
        format!("/System/Library/Fonts/Supplemental/{}.ttc", font_name),
    ];
    
    for path in &font_paths {
        if Path::new(path).exists() {
            match std::fs::read(path) {
                Ok(font_data) => {
                    info!("🎨 Successfully loaded font: {}", path);
                    return Some(font_data);
                }
                Err(e) => {
                    warn!("Failed to read font file {}: {}", path, e);
                }
            }
        }
    }
    
    warn!("Could not find font: {}", font_name);
    None
}

/// Helper function to load Apple Color Emoji font on macOS
fn load_emoji_font() -> Option<Vec<u8>> {
    // Apple Color Emoji is typically in a specific location
    let emoji_paths = [
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        "/System/Library/Fonts/Supplemental/Apple Color Emoji.ttc",
    ];
    
    for path in &emoji_paths {
        if Path::new(path).exists() {
            match std::fs::read(path) {
                Ok(font_data) => {
                    info!("🎨 Successfully loaded emoji font: {}", path);
                    return Some(font_data);
                }
                Err(e) => {
                    warn!("Failed to read emoji font file {}: {}", path, e);
                }
            }
        }
    }
    
    warn!("Could not find Apple Color Emoji font");
    None
}

/// Setup custom fonts including Chalkboard and Apple Color Emoji
fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // Try to load Chalkboard font
    if let Some(font_data) = load_system_font("Chalkboard") {
        fonts.font_data.insert(
            "Chalkboard".to_owned(),
            egui::FontData::from_owned(font_data),
        );
        
        // Create a new font family for Chalkboard
        fonts.families.insert(
            egui::FontFamily::Name("Chalkboard".into()),
            vec!["Chalkboard".to_owned()],
        );
        
        info!("✅ Chalkboard font loaded successfully!");
    } else {
        warn!("⚠️ Could not load Chalkboard font, using default fonts");
    }
    
    // Try to load Apple Color Emoji font and add it to default fonts (not Chalkboard)
    if let Some(emoji_data) = load_emoji_font() {
        fonts.font_data.insert(
            "AppleColorEmoji".to_owned(),
            egui::FontData::from_owned(emoji_data),
        );
        
        // Add emoji font as fallback to default system fonts only
        if let Some(proportional_fonts) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            proportional_fonts.push("AppleColorEmoji".to_owned());
            info!("✅ Added emoji support to Proportional font family");
        }
        
        if let Some(monospace_fonts) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            monospace_fonts.push("AppleColorEmoji".to_owned());
            info!("✅ Added emoji support to Monospace font family");
        }
        
        info!("✅ Apple Color Emoji font loaded and added to default fonts!");
    } else {
        warn!("⚠️ Could not load Apple Color Emoji font");
    }
    
    // Set the fonts
    ctx.set_fonts(fonts);
}

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
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, anyhow::Error> {
        info!("🚀 Initializing AllowanceTrackerApp");
        
        // Setup custom fonts including Chalkboard
        setup_custom_fonts(&cc.egui_ctx);
        
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
    
    /// Draw solid purple background for header columns
    fn draw_solid_purple_background(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        // Use the nice purple color from the original BALANCE header
        let purple_color = egui::Color32::from_rgb(186, 85, 211);
        
        // Draw solid purple background for this column
        ui.painter().rect_filled(rect, egui::Rounding::ZERO, purple_color);
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
            
            // Use Chalkboard font family if available, otherwise fall back to Proportional
            let font_family = if ctx.fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("Chalkboard".into()))) {
                egui::FontFamily::Name("Chalkboard".into())
            } else {
                egui::FontFamily::Proportional
            };
            
            // Larger text for readability with Chalkboard font
            style.text_styles.insert(
                egui::TextStyle::Heading,
                egui::FontId::new(28.0, font_family.clone()),
            );
            style.text_styles.insert(
                egui::TextStyle::Body,
                egui::FontId::new(16.0, font_family.clone()),
            );
            style.text_styles.insert(
                egui::TextStyle::Button,
                egui::FontId::new(18.0, font_family.clone()),
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
                        if ui.button(egui::RichText::new("👤 Select Child")
                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
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
                
                // Right side - Forms and transactions
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
                        
                        ui.separator();
                        
                        // Recent transactions table
                        ui.label(egui::RichText::new("📋 Recent Transactions")
                            .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                            .strong());
                        if self.calendar_transactions.is_empty() {
                            ui.label("No transactions yet!");
                        } else {
                            // Check if Chalkboard font is available (outside of table closures)
                            let font_family = if ui.ctx().fonts(|fonts| fonts.families().contains(&egui::FontFamily::Name("Chalkboard".into()))) {
                                egui::FontFamily::Name("Chalkboard".into())
                            } else {
                                egui::FontFamily::Proportional
                            };
                            
                            // Create a kid-friendly table using proper TableBuilder
                            TableBuilder::new(ui)
                                .striped(true)
                                .resizable(false)
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                .column(Column::exact(150.0))  // DATE column
                                .column(Column::exact(200.0))  // DESCRIPTION column  
                                .column(Column::exact(100.0))  // AMOUNT column
                                .column(Column::exact(100.0))  // BALANCE column
                                .header(60.0, |mut header| {
                                    
                                    // Solid purple header using header.col()
                                    header.col(|ui| {
                                        // Draw solid purple background for this column
                                        let rect = ui.max_rect();
                                        self.draw_solid_purple_background(ui, rect);
                                        
                                        // Add proper padding around the text
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.add_space(10.0);
                                            ui.colored_label(egui::Color32::WHITE, 
                                                egui::RichText::new("DATE")
                                                    .font(egui::FontId::new(20.0, font_family.clone()))
                                                    .strong()
                                            );
                                            ui.add_space(10.0);
                                        });
                                    });
                                    header.col(|ui| {
                                        let rect = ui.max_rect();
                                        self.draw_solid_purple_background(ui, rect);
                                        
                                        // Add proper padding around the text
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.add_space(10.0);
                                            ui.colored_label(egui::Color32::WHITE, 
                                                egui::RichText::new("DESCRIPTION")
                                                    .font(egui::FontId::new(20.0, font_family.clone()))
                                                    .strong()
                                            );
                                            ui.add_space(10.0);
                                        });
                                    });
                                    header.col(|ui| {
                                        let rect = ui.max_rect();
                                        self.draw_solid_purple_background(ui, rect);
                                        
                                        // Add proper padding around the text
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.add_space(10.0);
                                            ui.colored_label(egui::Color32::WHITE, 
                                                egui::RichText::new("AMOUNT")
                                                    .font(egui::FontId::new(20.0, font_family.clone()))
                                                    .strong()
                                            );
                                            ui.add_space(10.0);
                                        });
                                    });
                                    header.col(|ui| {
                                        let rect = ui.max_rect();
                                        self.draw_solid_purple_background(ui, rect);
                                        
                                        // Add proper padding around the text
                                        ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                            ui.add_space(10.0);
                                            ui.colored_label(egui::Color32::WHITE, 
                                                egui::RichText::new("BALANCE")
                                                    .font(egui::FontId::new(20.0, font_family.clone()))
                                                    .strong()
                                            );
                                            ui.add_space(10.0);
                                        });
                                    });
                                })
                                .body(|mut body| {
                                    for transaction in &self.calendar_transactions {
                                        body.row(45.0, |mut row| {
                                            // Date column (formatted with full month name)
                                            row.col(|ui| {
                                                let date_str = if let Some(date_part) = transaction.date.split('T').next() {
                                                    // Parse and format date with full month name
                                                    if let Ok(parsed_date) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                                                        parsed_date.format("%B %d, %Y").to_string()  // Full month name
                                                    } else {
                                                        date_part.to_string()
                                                    }
                                                } else {
                                                    "Unknown".to_string()
                                                };
                                                
                                                // Add vertical centering and padding
                                                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                                    ui.add_space(8.0);
                                                    ui.label(egui::RichText::new(date_str)
                                                        .font(egui::FontId::new(14.0, font_family.clone()))
                                                        .strong());
                                                    ui.add_space(8.0);
                                                });
                                            });
                                            
                                            // Description column with bolder text
                                            row.col(|ui| {
                                                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                                    ui.add_space(8.0);
                                                    ui.label(egui::RichText::new(&transaction.description)
                                                        .font(egui::FontId::new(14.0, font_family.clone()))
                                                        .strong());
                                                    ui.add_space(8.0);
                                                });
                                            });
                                            
                                            // Amount column with color coding and bold text
                                            row.col(|ui| {
                                                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                                    ui.add_space(8.0);
                                                    if transaction.amount >= 0.0 {
                                                        ui.colored_label(
                                                            egui::Color32::from_rgb(34, 139, 34), // Green for positive
                                                            egui::RichText::new(format!("+${:.2}", transaction.amount))
                                                                .font(egui::FontId::new(14.0, font_family.clone()))
                                                                .strong()
                                                        );
                                                    } else {
                                                        ui.colored_label(
                                                            egui::Color32::from_rgb(220, 20, 60), // Red for negative
                                                            egui::RichText::new(format!("-${:.2}", transaction.amount.abs()))
                                                                .font(egui::FontId::new(14.0, font_family.clone()))
                                                                .strong()
                                                        );
                                                    }
                                                    ui.add_space(8.0);
                                                });
                                            });
                                            
                                            // Balance column with bold text
                                            row.col(|ui| {
                                                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                                                    ui.add_space(8.0);
                                                    ui.label(egui::RichText::new(format!("${:.2}", transaction.balance))
                                                        .font(egui::FontId::new(14.0, font_family.clone()))
                                                        .strong());
                                                    ui.add_space(8.0);
                                                });
                                            });
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
                egui::Window::new("Select Child")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(egui::RichText::new("👤 Available Children:")
                            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                            .strong());
                        
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
                                                ui.label(egui::RichText::new("👑")
                                                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))); // Crown for active child
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
                            
                            if ui.button(egui::RichText::new("🔄 Refresh")
                                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))).clicked() {
                                // Try to reload the active child
                                self.load_initial_data();
                            }
                        });
                    });
            }
        });
    }
}
