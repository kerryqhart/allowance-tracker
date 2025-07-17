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
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::dropdown_menu::{DropdownMenuItem, DropdownButtonConfig, DropdownMenuConfig};

impl AllowanceTrackerApp {
    /// Render the header
    pub fn render_header(&mut self, ui: &mut egui::Ui) {
        log::info!("🏠 RENDER_HEADER called");
        // Use Frame with translucent fill for proper transparency
        let header_height = 60.0;
        
        // Create a frame with translucent background
        let frame = egui::Frame::none()
            .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 30)) // Truly translucent white
            .inner_margin(egui::Margin::symmetric(10.0, 10.0));
        
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
                            if let Some(child) = &self.current_child {
                                // Balance with clean styling (no color coding) - disable text selection
                                ui.add(egui::Label::new(egui::RichText::new(format!("${:.2}", self.current_balance))
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
                                
                                let (child_button_response, should_show_dropdown) = self.child_dropdown.render_button(ui, &button_config);
                                
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
                                
                                let (select_button_response, should_show_dropdown) = self.child_dropdown.render_button(ui, &button_config);
                                
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
    pub fn render_child_dropdown_with_generalized_component(&mut self, ui: &mut egui::Ui, button_rect: egui::Rect) {
        // Load children from backend and build menu items
        let children_list = match self.backend.child_service.list_children() {
            Ok(children_result) => children_result.children,
            Err(_) => vec![],
        };
        
        let menu_items: Vec<DropdownMenuItem> = if children_list.is_empty() {
            vec![DropdownMenuItem {
                label: "No children available".to_string(),
                icon: None,
                is_current: false,
                is_enabled: false,
            }]
        } else {
            children_list.iter().map(|child| {
                let is_current = self.current_child.as_ref()
                    .map(|c| c.id == child.id)
                    .unwrap_or(false);
                
                DropdownMenuItem {
                    label: child.name.clone(),
                    icon: if is_current { Some("👑".to_string()) } else { None },
                    is_current,
                    is_enabled: true,
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
        
        let _clicked_item = self.child_dropdown.render_menu(ui, button_rect, &menu_items, &menu_config, |index| {
            selected_index = Some(index);
        });
        
        // Handle item selection outside the closure to avoid borrowing conflicts
        if let Some(index) = selected_index {
            if index < children_list.len() {
                let selected_child = &children_list[index];
                let is_current = self.current_child.as_ref()
                    .map(|c| c.id == selected_child.id)
                    .unwrap_or(false);
                
                if !is_current {
                    // Set this child as active
                    let command = crate::backend::domain::commands::child::SetActiveChildCommand {
                        child_id: selected_child.id.clone(),
                    };
                    match self.backend.child_service.set_active_child(command) {
                        Ok(_) => {
                            self.current_child = Some(crate::ui::mappers::to_dto(selected_child.clone()));
                            self.load_balance();
                            self.load_calendar_data();
                        }
                        Err(e) => {
                            self.error_message = Some(format!("Failed to select child: {}", e));
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
        
        let (settings_button_response, should_show_dropdown) = self.settings_dropdown.render_button(ui, &button_config);
        
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
                label: "Data directory".to_string(),
                icon: Some("📁".to_string()),
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
        
        let _clicked_item = self.settings_dropdown.render_menu(ui, button_rect, &menu_items, &menu_config, |index| {
            selected_index = Some(index);
        });
        
        // Handle settings menu item selection
        if let Some(index) = selected_index {
            match index {
                0 => {
                    // Profile - placeholder action
                    log::info!("📋 Profile menu item clicked");
                    self.success_message = Some("Profile clicked (not implemented yet)".to_string());
                }
                1 => {
                    // Create child - placeholder action
                    log::info!("👶 Create child menu item clicked");
                    self.success_message = Some("Create child clicked (not implemented yet)".to_string());
                }
                2 => {
                    // Configure allowance - placeholder action
                    log::info!("⚙️ Configure allowance menu item clicked");
                    self.success_message = Some("Configure allowance clicked (not implemented yet)".to_string());
                }
                3 => {
                    // Delete transactions - placeholder action
                    log::info!("🗑️ Delete transactions menu item clicked");
                    self.success_message = Some("Delete transactions clicked (not implemented yet)".to_string());
                }
                4 => {
                    // Export data - placeholder action
                    log::info!("📤 Export data menu item clicked");
                    self.success_message = Some("Export data clicked (not implemented yet)".to_string());
                }
                5 => {
                    // Data directory - placeholder action
                    log::info!("📁 Data directory menu item clicked");
                    self.success_message = Some("Data directory clicked (not implemented yet)".to_string());
                }
                _ => {
                    log::warn!("🚨 Unknown settings menu item clicked: {}", index);
                }
            }
        }
    }

    /// Render error and success messages
    pub fn render_messages(&self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error_message {
            ui.colored_label(egui::Color32::RED, format!("❌ {}", error));
        }
        if let Some(success) = &self.success_message {
            ui.colored_label(egui::Color32::GREEN, format!("✅ {}", success));
        }
    }
} 