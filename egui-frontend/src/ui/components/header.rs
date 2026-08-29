//! # Header Module
//!
//! This module handles rendering the application header, including child selection,
//! balance display, and action buttons.
//!
//! ## Key Functions:
//! - `render_header()` - Main header rendering with child selector and balance
//! - `render_child_dropdown()` - Child selection dropdown menu
//! - `render_messages()` - Success/error message display
//!
//! ## Purpose:
//! The header provides essential navigation and information display:
//! - Current child selection with dropdown
//! - Current balance display
//! - Quick action buttons (Add Money, Spend Money)
//! - Message display for user feedback
//!
//! ## Features:
//! - Translucent background for modern look
//! - Responsive design
//! - Interactive child selection
//! - Visual feedback for user actions

use eframe::egui;
use log::info;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::dropdown_menu::{DropdownMenuItem, DropdownButtonConfig, DropdownMenuConfig};

impl AllowanceTrackerApp {
    /// Render the header
    pub fn render_header(&mut self, ui: &mut egui::Ui) {
        // info!("RENDER_HEADER called"); // Too verbose
        // Use Frame with translucent fill for proper transparency
        let header_height = 60.0;
        
        // Create a frame with translucent background
        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30)) // Truly translucent white
            .inner_margin(egui::Margin::symmetric(10, 10));
        
        frame.show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), header_height - 20.0), // Account for margin
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.horizontal(|ui| {
                        // Clean title without emoji - disable text selection
                        ui.add(egui::Label::new(egui::RichText::new("Allowance Tracker")
                            .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                            .strong()
                            .color(egui::Color32::from_rgb(60, 60, 60))) // Dark gray for readability
                            .selectable(false)); // Disable text selection
                        
                        // Flexible space to push right content to the right
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Settings menu button (far right)
                            self.render_settings_menu(ui);
                            
                            // Add spacing between settings and child selector
                            ui.add_space(15.0);
                            // FIXED: Use backend as source of truth for header display
                            let current_child_from_backend = self.get_current_child_from_backend();
                            
                            // Header display debug (commented out to reduce noise)
                            // log::info!("HEADER_DISPLAY: Current child from backend: {:?}", 
                            //     current_child_from_backend.as_ref().map(|c| (&c.id, &c.name)));
                            
                            if let Some(child) = &current_child_from_backend {
                                // Balance with clean styling (no color coding) - disable text selection
                                ui.add(egui::Label::new(egui::RichText::new(format!("${:.2}", self.current_balance()))
                                    .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                                    .strong()
                                    .color(egui::Color32::from_rgb(60, 60, 60))) // Same dark gray as title
                                    .selectable(false)); // Disable text selection
                                
                                // Add spacing between balance and name
                                ui.add_space(15.0);
                                
                                // Render child selector using generalized dropdown
                                let button_config = DropdownButtonConfig {
                                    text: child.name.clone(),
                                    font_size: 18.0,
                                    text_color: egui::Color32::from_rgb(80, 80, 80),
                                    hover_bg_color: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                                    hover_border_color: egui::Color32::from_rgb(126, 120, 229),
                                };
                                
                                let (child_button_response, should_show_dropdown) = self.interaction.child_dropdown.render_button(ui, &button_config);
                                
                                // Show dropdown if opened
                                if should_show_dropdown {
                                    self.render_child_dropdown_with_generalized_component(ui, child_button_response.rect);
                                }
                            } else {
                                // No child selected - render select child button using generalized dropdown
                                let button_config = DropdownButtonConfig {
                                    text: "Select Child".to_string(),
                                    font_size: 16.0,
                                    text_color: egui::Color32::GRAY,
                                    hover_bg_color: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                                    hover_border_color: egui::Color32::from_rgb(232, 150, 199),
                                };
                                
                                let (select_button_response, should_show_dropdown) = self.interaction.child_dropdown.render_button(ui, &button_config);
                                
                                // Show dropdown if opened
                                if should_show_dropdown {
                                    self.render_child_dropdown_with_generalized_component(ui, select_button_response.rect);
                                }
                            }
                        });
                    });
                }
            );
        });
    }
    
    /// Render child selector dropdown using generalized component
    ///
    /// Reads the roster, never the filesystem. This is a per-frame render path:
    /// the old `list_children()` here walked every child folder while the menu
    /// was open, which froze the window mid-frame on a cold iCloud folder.
    ///
    /// A child that is not `Available` is still listed — greyed out and
    /// unclickable, annotated with why. Dropping it would make a downloading
    /// child indistinguishable from a missing one, which is the very bug this
    /// design exists to fix.
    pub fn render_child_dropdown_with_generalized_component(&mut self, ui: &mut egui::Ui, button_rect: egui::Rect) {
        use crate::ui::state::roster::status_label;

        // Snapshot what we need so the roster borrow ends before the dropdown
        // widget takes `&mut self.interaction`.
        let rows: Vec<(shared::ChildId, String, bool)> = self
            .roster
            .entries()
            .iter()
            .map(|e| (e.entry.id.clone(), match status_label(&e.status) {
                Some(reason) => format!("{} — {}", e.display_name(), reason),
                None => e.display_name().to_string(),
            }, e.is_available()))
            .collect();

        let current_child_from_backend = self.get_current_child_from_backend();

        let menu_items: Vec<DropdownMenuItem> = if rows.is_empty() {
            vec![DropdownMenuItem {
                label: "No children registered yet".to_string(),
                icon: None,
                is_current: false,
                is_enabled: false,
            }]
        } else {
            rows.iter().map(|(id, label, available)| {
                // Backend remains the source of truth for *which* child is active.
                let is_current = *available
                    && current_child_from_backend.as_ref()
                        .map(|c| c.id.as_str() == id.as_str())
                        .unwrap_or(false);

                DropdownMenuItem {
                    label: label.clone(),
                    icon: if is_current { Some("👑".to_string()) } else { None },
                    is_current,
                    is_enabled: *available,
                }
            }).collect()
        };

        let menu_config = DropdownMenuConfig {
            min_width: 120.0,
            item_height: 22.0,
            item_font_size: 14.0,
        };
        
        // Track which item was clicked to handle selection outside the closure
        let mut selected_index: Option<usize> = None;
        
        let _clicked_item = self.interaction.child_dropdown.render_menu(ui, button_rect, &menu_items, &menu_config, |index| {
            selected_index = Some(index);
        });
        
        // Handle item selection outside the closure to avoid borrowing conflicts
        if let Some(index) = selected_index {
            if let Some((id, _, available)) = rows.get(index) {
                // The dropdown already refuses clicks on disabled items; this
                // is the belt-and-braces half, since selecting an unavailable
                // child would set it active and then fail to load.
                if !available {
                    return;
                }

                let is_current = self
                    .get_current_child_from_backend()
                    .map(|c| c.id.as_str() == id.as_str())
                    .unwrap_or(false);

                if !is_current {
                    let command = crate::backend::domain::commands::child::SetActiveChildCommand {
                        child_id: id.as_str().to_string(),
                    };
                    match self.backend().child_service.set_active_child(command) {
                        Ok(_) => {
                            log::info!("Switched active child to {}", id);
                            self.refresh_all_data_for_current_child();
                        }
                        Err(e) => {
                            log::error!("Failed to switch active child to {}: {}", id, e);
                            self.ui.error_message = Some(format!("Failed to select child: {}", e));
                        }
                    }
                }
            }
        }
    }
    
    /// Render settings menu dropdown with gear icon
    pub fn render_settings_menu(&mut self, ui: &mut egui::Ui) {
        // Settings button with gear icon
        let button_config = DropdownButtonConfig {
            text: "⚙️".to_string(), // Gear icon
            font_size: 20.0,
            text_color: egui::Color32::from_rgb(80, 80, 80),
            hover_bg_color: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            hover_border_color: egui::Color32::from_rgb(126, 120, 229),
        };
        
        let (settings_button_response, should_show_dropdown) = self.interaction.settings_dropdown.render_button(ui, &button_config);
        
        // Show dropdown if opened
        if should_show_dropdown {
            self.render_settings_dropdown_menu(ui, settings_button_response.rect);
        }
    }
    
    /// Render settings dropdown menu items
    pub fn render_settings_dropdown_menu(&mut self, ui: &mut egui::Ui, button_rect: egui::Rect) {
        // Define settings menu items based on the screenshot
        let menu_items = vec![
            DropdownMenuItem {
                label: "Profile".to_string(),
                icon: Some("👤".to_string()),
                is_current: false,
                is_enabled: true,
            },
            DropdownMenuItem {
                label: "Create child".to_string(),
                icon: Some("👶".to_string()),
                is_current: false,
                is_enabled: true,
            },
            DropdownMenuItem {
                label: "Configure allowance".to_string(),
                icon: Some("⚙️".to_string()),
                is_current: false,
                is_enabled: true,
            },
            DropdownMenuItem {
                label: "Delete transactions".to_string(),
                icon: Some("🗑️".to_string()),
                is_current: false,
                is_enabled: true,
            },
            DropdownMenuItem {
                label: "Export data".to_string(),
                icon: Some("📤".to_string()),
                is_current: false,
                is_enabled: true,
            },
            DropdownMenuItem {
                label: "Children".to_string(),
                icon: Some("👶".to_string()),
                is_current: false,
                is_enabled: true,
            },
            DropdownMenuItem {
                label: "Initial sync".to_string(),
                icon: Some("🔄".to_string()),
                is_current: false,
                is_enabled: true,
            },
        ];
        
        let menu_config = DropdownMenuConfig {
            min_width: 180.0, // Wider for settings menu
            item_height: 24.0, // Slightly taller items
            item_font_size: 14.0,
        };
        
        // Track which item was clicked
        let mut selected_index: Option<usize> = None;
        
        info!("🔧 SETTINGS_DROPDOWN: About to render settings menu");
        let _clicked_item = self.interaction.settings_dropdown.render_menu(ui, button_rect, &menu_items, &menu_config, |index| {
            info!("🔧 SETTINGS_DROPDOWN: Item {} clicked!", index);
            selected_index = Some(index);
        });
        
        if selected_index.is_some() {
            info!("🔧 SETTINGS_DROPDOWN: selected_index = {:?}", selected_index);
        }
        
        // Handle settings menu item selection
        if let Some(index) = selected_index {
            // Map index to settings action
            let settings_action = match index {
                0 => crate::ui::state::modal_state::SettingsAction::ShowProfile,
                1 => crate::ui::state::modal_state::SettingsAction::CreateChild,
                2 => crate::ui::state::modal_state::SettingsAction::ConfigureAllowance,
                3 => crate::ui::state::modal_state::SettingsAction::DeleteTransactions,
                4 => crate::ui::state::modal_state::SettingsAction::ExportData,
                5 => crate::ui::state::modal_state::SettingsAction::Children,
                6 => crate::ui::state::modal_state::SettingsAction::InitialSync,
                _ => {
                    log::warn!("🚨 Unknown settings menu item clicked: {}", index);
                    return;
                }
            };
            
            info!("Settings menu item selected: {:?} - triggering parental control", settings_action);
            
            // Store the settings action for execution after parental control
            self.modal.pending_settings_action = Some(settings_action);
            
            // Trigger universal settings parental control
            self.start_parental_control_challenge(crate::ui::state::modal_state::ProtectedAction::AccessSettings);
        }
    }

    /// Render error messages
    pub fn render_messages(&self, ui: &mut egui::Ui) {
        if let Some(error) = &self.ui.error_message {
            ui.colored_label(egui::Color32::RED, format!("{}", error));
        }
    }
    
    /// Render transaction selection controls bar (appears when in selection mode)
    pub fn render_selection_controls_bar(&mut self, ui: &mut egui::Ui) {
        if !self.interaction.transaction_selection_mode {
            return; // Only show when in selection mode
        }
        
        info!("RENDER_SELECTION_CONTROLS_BAR called");
        
        // Selection controls bar with distinct styling
        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(255, 248, 220, 200)) // Light yellow background
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 215, 0))) // Gold border
            .inner_margin(egui::Margin::symmetric(15, 8))
            .corner_radius(egui::CornerRadius::same(8));
        
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Selection mode indicator on the left
                ui.add(egui::Label::new(
                    egui::RichText::new("Delete Mode")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .strong()
                        .color(egui::Color32::from_rgb(180, 100, 0)) // Dark orange
                ).selectable(false));
                
                ui.add_space(10.0);
                
                // Selected count
                let count = self.selected_transaction_count();
                ui.add(egui::Label::new(
                    egui::RichText::new(format!("({} selected)", count))
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100))
                ).selectable(false));
                
                // Push right-side controls to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Exit selection mode button
                    let exit_button = ui.add_sized(
                        [80.0, 28.0],
                        egui::Button::new(
                            egui::RichText::new("Cancel")
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(egui::Color32::WHITE)
                        )
                        .fill(egui::Color32::from_rgb(128, 128, 128)) // Gray background
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 100, 100)))
                        .corner_radius(egui::CornerRadius::same(4))
                    );
                    
                    if exit_button.clicked() {
                        info!("Exit selection mode button clicked");
                        self.exit_transaction_selection_mode();
                    }
                    
                    ui.add_space(10.0);
                    
                    // Delete button (only enabled when transactions are selected)
                    let delete_enabled = self.has_selected_transactions();
                    let delete_color = if delete_enabled {
                        egui::Color32::from_rgb(220, 53, 69) // Red
                    } else {
                        egui::Color32::from_rgb(180, 180, 180) // Disabled gray
                    };
                    
                    let delete_button = ui.add_enabled(
                        delete_enabled,
                        egui::Button::new(
                            egui::RichText::new(format!("Delete ({})", count))
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(egui::Color32::WHITE)
                        )
                        .fill(delete_color)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 50, 60)))
                        .corner_radius(egui::CornerRadius::same(4))
                    );
                    
                    if delete_button.clicked() && delete_enabled {
                        info!("Delete selected transactions button clicked");
                        self.delete_selected_transactions();
                    }
                });
            });
        });
        
        ui.add_space(8.0); // Space below the selection bar
    }
    
    /// Delete the selected transactions
    fn delete_selected_transactions(&mut self) {
        if self.interaction.selected_transaction_ids.is_empty() {
            log::warn!("No transactions selected for deletion");
            return;
        }
        let transaction_ids: Vec<String> = self.interaction.selected_transaction_ids.iter().cloned().collect();
        info!("Attempting to delete {} transactions: {:?}", transaction_ids.len(), transaction_ids);
        let command = crate::backend::domain::commands::transactions::DeleteTransactionsCommand {
            transaction_ids: transaction_ids.clone(),
        };
        match self.backend().transaction_service.as_ref().delete_transactions_domain(command) {
            Ok(result) => {
                info!("Successfully deleted {} transactions", result.deleted_count);
                self.exit_transaction_selection_mode();
                self.load_calendar_data();
                self.load_balance();
            }
            Err(e) => {
                log::error!("Failed to delete transactions: {}", e);
                self.ui.error_message = Some(format!("Failed to delete transactions: {}", e));
            }
        }
    }
} 