use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::mappers::to_dto;
use crate::backend::domain::commands::child::SetActiveChildCommand;

impl AllowanceTrackerApp {
    /// Render the spend money modal
    pub fn render_spend_money_modal(&mut self, ctx: &egui::Context) {
        if !self.show_spend_money_modal {
            return;
        }

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

    /// Render the child selector modal
    pub fn render_child_selector_modal(&mut self, ctx: &egui::Context) {
        if !self.show_child_selector {
            return;
        }

        log::info!("🚀 RENDERING CHILD SELECTOR MODAL");

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
                                        ui.label(egui::RichText::new("•")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(0, 120, 215))); // Bullet for active child
                                    } else {
                                        ui.label("   "); // Spacing
                                    }
                                    
                                    // Create hover button using the working chip approach
                                    let button_size = egui::vec2(200.0, 24.0); // Fixed size for consistency
                                    let (button_rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());
                                    
                                    // DEBUG: Log hover state
                                    if response.hovered() {
                                        log::info!("🔍 HOVER DETECTED for child: {}", child.name);
                                    }
                                    
                                    // Determine background color based on hover state (like working chips)
                                    let background_color = if response.hovered() {
                                        log::info!("🎨 PAINTING HOVER BACKGROUND for: {}", child.name);
                                        egui::Color32::from_rgba_unmultiplied(220, 220, 220, 255) // Light gray on hover
                                    } else {
                                        egui::Color32::TRANSPARENT // Transparent when not hovered
                                    };
                                    
                                    // Draw background FIRST (like chips do)
                                    ui.painter().rect_filled(
                                        button_rect,
                                        egui::Rounding::same(4.0),
                                        background_color
                                    );
                                    
                                    // DEBUG: Always paint a small debug marker to verify painting works
                                    if response.hovered() {
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(button_rect.min, egui::vec2(10.0, 10.0)),
                                            egui::Rounding::ZERO,
                                            egui::Color32::RED // Red debug square
                                        );
                                    }
                                    
                                    // Draw text on top
                                    let text_color = if is_active { 
                                        egui::Color32::from_rgb(0, 120, 215) // Active child in blue
                                    } else { 
                                        egui::Color32::from_rgb(60, 60, 60) // Default dark gray
                                    };
                                    
                                    ui.painter().text(
                                        button_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        &child.name,
                                        egui::FontId::new(16.0, egui::FontFamily::Proportional),
                                        text_color,
                                    );
                                    
                                    // Change cursor on hover
                                    if response.hovered() {
                                        ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                                    }
                                    
                                    let child_button_response = response;
                                    
                                    if child_button_response.clicked() {
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

    /// Render day action overlay based on active overlay type
    pub fn render_day_action_overlay(&mut self, ctx: &egui::Context) {
        let overlay_type = match self.active_overlay {
            Some(overlay) => overlay,
            None => return,
        };
        
        // Define colors for each overlay type to match transaction chip themes
        let (overlay_color, title_text, content_text) = match overlay_type {
            crate::ui::app_state::OverlayType::AddMoney => (
                egui::Color32::from_rgb(34, 139, 34), // Green to match income chips
                "Add Money",
                "Enter the amount you want to add to your allowance"
            ),
            crate::ui::app_state::OverlayType::SpendMoney => (
                egui::Color32::from_rgb(128, 128, 128), // Gray to match expense chips
                "Spend Money", 
                "Enter the amount you want to spend from your allowance"
            ),
            crate::ui::app_state::OverlayType::CreateGoal => (
                egui::Color32::from_rgb(199, 112, 221), // Pink for goals
                "Create Goal",
                "Set a savings goal for something special"
            ),
        };
        
        // Use Area with Foreground order to ensure it appears above everything
        egui::Area::new(egui::Id::new("day_action_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Full screen semi-transparent background
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    egui::Rounding::ZERO,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 80) // Subtle dark background
                );
                
                // Center the modal content
                ui.allocate_ui_at_rect(screen_rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        // Modal card with proper styling
                        egui::Frame::window(&ui.style())
                            .fill(egui::Color32::WHITE)
                            .stroke(egui::Stroke::new(3.0, overlay_color))
                            .rounding(egui::Rounding::same(16.0))
                            .inner_margin(egui::Margin::same(25.0))
                            .shadow(egui::Shadow {
                                offset: egui::vec2(6.0, 6.0),
                                blur: 20.0,
                                spread: 0.0,
                                color: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100),
                            })
                            .show(ui, |ui| {
                                // Set modal size
                                ui.set_min_size(egui::vec2(450.0, 350.0));
                                ui.set_max_size(egui::vec2(450.0, 350.0));
                                
                                ui.vertical_centered(|ui| {
                                    ui.add_space(15.0);
                                    
                                    // Title with icon
                                    let title_icon = match overlay_type {
                                        crate::ui::app_state::OverlayType::AddMoney => "💰",
                                        crate::ui::app_state::OverlayType::SpendMoney => "💸",
                                        crate::ui::app_state::OverlayType::CreateGoal => "🎯",
                                    };
                                    
                                    ui.label(egui::RichText::new(format!("{} {}", title_icon, title_text))
                                         .font(egui::FontId::new(28.0, egui::FontFamily::Proportional))
                                         .strong()
                                         .color(overlay_color));
                                    
                                    ui.add_space(15.0);
                                    
                                    // Content - different for each overlay type
                                    match overlay_type {
                                        crate::ui::app_state::OverlayType::AddMoney => {
                                            // Form fields for Add Money
                                            ui.label(egui::RichText::new("Enter the amount you want to add to your allowance")
                                                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                                .color(egui::Color32::from_rgb(80, 80, 80)));
                                            
                                            ui.add_space(20.0);
                                            
                                            // Description field
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("Description:")
                                                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::from_rgb(60, 60, 60)));
                                            });
                                            ui.add_space(5.0);
                                            
                                            let description_response = ui.add(
                                                egui::TextEdit::singleline(&mut self.add_money_description)
                                                    .hint_text("What is this money for?")
                                                    .desired_width(350.0)
                                                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                            );
                                            
                                            ui.add_space(15.0);
                                            
                                            // Amount field  
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("Amount:")
                                                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                                    .color(egui::Color32::from_rgb(60, 60, 60)));
                                            });
                                            ui.add_space(5.0);
                                            
                                            let _amount_response = ui.add(
                                                egui::TextEdit::singleline(&mut self.add_money_amount)
                                                    .hint_text("$0.00")
                                                    .desired_width(150.0)
                                                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                            );
                                            
                                            // Auto-focus description field if empty
                                            if self.add_money_description.is_empty() {
                                                description_response.request_focus();
                                            }
                                        },
                                        _ => {
                                            // Default content for other overlay types
                                            ui.label(egui::RichText::new(content_text)
                                                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                                .color(egui::Color32::from_rgb(80, 80, 80)));
                                        }
                                    }
                                    
                                    ui.add_space(30.0);
                                    
                                    // Buttons
                                    ui.horizontal(|ui| {
                                        ui.add_space(50.0);
                                        
                                        // OK button (text changes based on overlay type)
                                        let ok_text = match overlay_type {
                                            crate::ui::app_state::OverlayType::AddMoney => "Add Money",
                                            crate::ui::app_state::OverlayType::SpendMoney => "Spend Money", 
                                            crate::ui::app_state::OverlayType::CreateGoal => "Create Goal",
                                        };
                                        
                                        let ok_button = egui::Button::new(egui::RichText::new(ok_text)
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::WHITE))
                                            .fill(overlay_color)
                                            .rounding(egui::Rounding::same(10.0))
                                            .min_size(egui::vec2(120.0, 40.0));
                                        
                                        if ui.add(ok_button).clicked() {
                                            match overlay_type {
                                                crate::ui::app_state::OverlayType::AddMoney => {
                                                    // TODO: Implement add money logic in next phase
                                                    log::info!("💰 Add Money clicked - Description: '{}', Amount: '{}'", 
                                                              self.add_money_description, self.add_money_amount);
                                                    self.success_message = Some("Add Money functionality coming in next phase!".to_string());
                                                },
                                                _ => {
                                                    // Default behavior for other overlays
                                                }
                                            }
                                            self.active_overlay = None;
                                            self.selected_day = None;
                                        }
                                        
                                        ui.add_space(30.0);
                                        
                                        // Cancel button
                                        let cancel_button = egui::Button::new(egui::RichText::new("Cancel")
                                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                            .color(egui::Color32::from_rgb(100, 100, 100)))
                                            .fill(egui::Color32::from_rgb(245, 245, 245))
                                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)))
                                            .rounding(egui::Rounding::same(10.0))
                                            .min_size(egui::vec2(90.0, 40.0));
                                        
                                        if ui.add(cancel_button).clicked() {
                                            // Clear form fields when canceling Add Money
                                            if overlay_type == crate::ui::app_state::OverlayType::AddMoney {
                                                self.add_money_description.clear();
                                                self.add_money_amount.clear();
                                            }
                                            self.active_overlay = None;
                                            self.selected_day = None;
                                        }
                                    });
                                    
                                    ui.add_space(15.0);
                                });
                            });
                    });
                });
                
                // Handle backdrop clicks to close modal
                if ui.ctx().input(|i| i.pointer.any_click()) {
                    let pointer_pos = ui.ctx().input(|i| i.pointer.interact_pos());
                    if let Some(pos) = pointer_pos {
                        // Check if the click was outside the modal area
                        let modal_center = screen_rect.center();
                        let modal_rect = egui::Rect::from_center_size(
                            modal_center,
                            egui::vec2(450.0, 350.0)
                        );
                        
                        if !modal_rect.contains(pos) {
                            // Clear form fields when clicking backdrop on Add Money
                            if overlay_type == crate::ui::app_state::OverlayType::AddMoney {
                                self.add_money_description.clear();
                                self.add_money_amount.clear();
                            }
                            self.active_overlay = None;
                            self.selected_day = None;
                        }
                    }
                }
            });
    }

    /// Render the parental control modal
    pub fn render_parental_control_modal(&mut self, ctx: &egui::Context) {
        if !self.show_parental_control_modal {
            return;
        }
        
        log::info!("🔒 Rendering parental control modal - stage: {:?}", self.parental_control_stage);
        
        // Modal window with dark background
        egui::Area::new(egui::Id::new("parental_control_modal_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Dark semi-transparent background
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    egui::Rounding::ZERO,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128)
                );
                
                // Center the modal content
                ui.allocate_ui_at_rect(screen_rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        self.render_parental_control_modal_content(ui);
                    });
                });
            });
    }
    
    /// Render the actual modal content
    fn render_parental_control_modal_content(&mut self, ui: &mut egui::Ui) {
        // Modal card background - compact and constrained
        let modal_size = egui::vec2(300.0, 160.0);
        
        egui::Frame::window(&ui.style())
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(126, 120, 229))) // Purple border
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                // Constrain the modal to exact size
                ui.set_min_size(modal_size);
                ui.set_max_size(modal_size);
                
                match self.parental_control_stage {
                    crate::ui::app_state::ParentalControlStage::Question1 => self.render_question1(ui),
                    crate::ui::app_state::ParentalControlStage::Question2 => self.render_question2(ui),
                    crate::ui::app_state::ParentalControlStage::Authenticated => self.render_success(ui),
                }
            });
    }
    
    /// Render first question: "Are you Mom or Dad?"
    fn render_question1(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(5.0);
            
            // Lock icon - smaller
            ui.label(egui::RichText::new("🔒")
                .font(egui::FontId::new(24.0, egui::FontFamily::Proportional)));
            
            ui.add_space(8.0);
            
            // Question - smaller font
            ui.label(egui::RichText::new("Settings Access")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(8.0);
            
            ui.label(egui::RichText::new("Are you Mom or Dad?")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(80, 80, 80)));
            
            ui.add_space(12.0);
            
            // Buttons - properly centered
            ui.horizontal_centered(|ui| {
                // Calculate total width needed: button + space + button
                let total_width = 70.0 + 12.0 + 70.0; // 152px total
                let available_width = ui.available_width();
                let offset = (available_width - total_width) / 2.0;
                
                if offset > 0.0 {
                    ui.add_space(offset);
                }
                
                // Yes button
                let yes_button = egui::Button::new(
                    egui::RichText::new("Yes")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::BLACK)
                )
                .min_size(egui::vec2(70.0, 32.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(147, 51, 234))); // Purple outline
                
                if ui.add(yes_button).clicked() {
                    self.parental_control_advance_to_question2();
                }
                
                ui.add_space(12.0);
                
                // No button
                let no_button = egui::Button::new(
                    egui::RichText::new("No")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::BLACK)
                )
                .min_size(egui::vec2(70.0, 32.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(147, 51, 234))); // Purple outline
                
                if ui.add(no_button).clicked() {
                    self.cancel_parental_control_challenge();
                }
            });
            
            ui.add_space(10.0);
        });
    }
    
    /// Render second question: "What's cooler than cool?"
    fn render_question2(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(5.0);
            
            // Question mark icon - smaller
            ui.label(egui::RichText::new("❄️")
                .font(egui::FontId::new(24.0, egui::FontFamily::Proportional)));
            
            ui.add_space(8.0);
            
            // Challenge question - smaller fonts
            ui.label(egui::RichText::new("Oh yeah?? If so...")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(4.0);
            
            ui.label(egui::RichText::new("What's cooler than cool?")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(80, 80, 80)));
            
            ui.add_space(10.0);
            
            // Text input - more compact
            let text_input = egui::TextEdit::singleline(&mut self.parental_control_input)
                .hint_text("Enter your answer...")
                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                .desired_width(220.0);
            
            let input_response = ui.add(text_input);
            
            // Auto-focus input field
            if input_response.gained_focus() || self.parental_control_input.is_empty() {
                input_response.request_focus();
            }
            
            // Handle Enter key
            if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.submit_parental_control_answer();
            }
            
            ui.add_space(10.0);
            
            // Error message
            if let Some(error) = &self.parental_control_error {
                ui.label(egui::RichText::new(error)
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(220, 53, 69))); // Red
                ui.add_space(5.0);
            }
            
            // Buttons - properly centered
            ui.horizontal_centered(|ui| {
                // Calculate total width needed: button + space + button
                let total_width = 70.0 + 12.0 + 70.0; // 152px total
                let available_width = ui.available_width();
                let offset = (available_width - total_width) / 2.0;
                
                if offset > 0.0 {
                    ui.add_space(offset);
                }
                
                // Submit button
                let submit_text = if self.parental_control_loading {
                    "⏳ Checking..."
                } else {
                    "Submit"
                };
                
                let submit_button = egui::Button::new(
                    egui::RichText::new(submit_text)
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(if self.parental_control_loading {
                            egui::Color32::GRAY
                        } else {
                            egui::Color32::BLACK
                        })
                )
                .min_size(egui::vec2(70.0, 32.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(1.5, if self.parental_control_loading {
                    egui::Color32::GRAY
                } else {
                    egui::Color32::from_rgb(147, 51, 234) // Purple outline
                }));
                
                if ui.add(submit_button).clicked() && !self.parental_control_loading {
                    self.submit_parental_control_answer();
                }
                
                ui.add_space(12.0);
                
                // Cancel button
                let cancel_button = egui::Button::new(
                    egui::RichText::new("Cancel")
                        .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::BLACK)
                )
                .min_size(egui::vec2(70.0, 32.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::WHITE)
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(147, 51, 234))); // Purple outline
                
                if ui.add(cancel_button).clicked() {
                    self.cancel_parental_control_challenge();
                }
            });
            
            ui.add_space(10.0);
        });
    }
    
    /// Render success state (brief display before closing)
    fn render_success(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            
            // Success icon
            ui.label(egui::RichText::new("✅")
                .font(egui::FontId::new(32.0, egui::FontFamily::Proportional)));
            
            ui.add_space(15.0);
            
            ui.label(egui::RichText::new("Access Granted!")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(34, 139, 34))); // Green
            
            ui.add_space(30.0);
        });
    }

    /// Render all modals
    pub fn render_modals(&mut self, ctx: &egui::Context) {
        self.render_spend_money_modal(ctx);
        self.render_child_selector_modal(ctx);
        self.render_day_action_overlay(ctx);
        self.render_parental_control_modal(ctx);
    }
} 