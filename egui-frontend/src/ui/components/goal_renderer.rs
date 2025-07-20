//! # Goal Renderer Module
//!
//! This module handles the rendering of the goal tab content, including:
//! - Current goal display with progress bar
//! - Goal creation interface
//! - Goal completion information
//!
//! ## Key Functions:
//! - `draw_goal_section()` - Main goal content renderer
//! - `draw_goal_card()` - Goal display card
//! - `draw_create_goal_card()` - Goal creation card
//! - `draw_goal_progress_bar()` - Visual progress indicator
//!
//! ## Purpose:
//! This module provides the complete goal management UI, allowing users to
//! create goals and track their progress toward completion.

use eframe::egui;
use log::info;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::styling::{colors, draw_card_container};

impl AllowanceTrackerApp {
    /// Draw the main goal section
    /// 
    /// ## MARGIN STRUCTURE (DO NOT CHANGE WITHOUT USER APPROVAL):
    /// 1. **EXTERNAL MARGINS**: 20px on all sides from window edge to card background
    ///    - Creates `content_rect` with 40px total reduction (20px × 2 sides)
    ///    - Card background drawn at `content_rect` (NOT `available_rect`)
    ///    - This matches calendar/table visual consistency
    /// 
    /// 2. **INTERNAL MARGINS**: 30px left padding inside the card for content positioning
    ///    - All content (text, progress bar, etc.) positioned 30px from card's left edge
    ///    - Creates proper breathing room inside the white card background
    /// 
    /// 3. **VERTICAL PADDING**: 35px top/bottom inside the card for content spacing
    ///    - Ensures content doesn't touch card edges vertically
    pub fn draw_goal_section(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect) {
        // EXTERNAL MARGINS: 20px margins from window edge to card background
        // This creates the white card size that matches other tabs' visual style
        let content_rect = egui::Rect::from_min_size(
            available_rect.min + egui::vec2(20.0, 20.0),
            egui::vec2(available_rect.width() - 40.0, available_rect.height() - 40.0),
        );
        
        // Draw white card background at the content_rect (NOT available_rect)
        draw_card_container(ui, content_rect, 10.0);
        
        // GOAL CONTENT AREA: Position all content inside the white card background
        ui.allocate_ui_at_rect(content_rect, |ui| {
            ui.vertical(|ui| {
                // VERTICAL PADDING: 35px top spacing inside card
                ui.add_space(35.0); 
                
                // INTERNAL MARGINS: Create 30px left padding for content positioning
                // This moves all content (text, progress bar, etc.) away from card's left edge
                ui.horizontal(|ui| {
                    ui.add_space(30.0); // INTERNAL LEFT MARGIN: Content positioning inside card
                    ui.vertical(|ui| {
                
                if self.goal.loading {
                    self.draw_goal_loading_state(ui);
                } else if let Some(error) = &self.goal.error_message.clone() {
                    self.draw_goal_error_state(ui, error);
                } else if self.goal.has_active_goal() {
                    self.draw_current_goal_card(ui);
                } else {
                    self.draw_create_goal_card(ui);
                }
                
                // VERTICAL PADDING: 35px bottom spacing inside card  
                ui.add_space(35.0);
                    }); // End: INTERNAL content vertical layout
                }); // End: INTERNAL left margin horizontal layout  
            }); // End: Card content vertical layout
        }); // End: Goal content area
    }
    
    /// Draw goal loading state
    fn draw_goal_loading_state(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.spinner();
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Loading goal...")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(colors::TEXT_SECONDARY));
        });
    }
    
    /// Draw goal error state
    fn draw_goal_error_state(&self, ui: &mut egui::Ui, error: &str) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);
            ui.label(egui::RichText::new("❌ Failed to load goal")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .color(egui::Color32::RED)
                .strong());
            ui.add_space(10.0);
            ui.label(egui::RichText::new(error)
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(colors::TEXT_SECONDARY));
        });
    }
    
    /// Draw current goal display card
    fn draw_current_goal_card(&mut self, ui: &mut egui::Ui) {
        // Clone the data to avoid borrowing conflicts
        let goal = if let Some(ref g) = self.goal.current_goal { g.clone() } else { return; };
        let calculation = if let Some(ref c) = self.goal.goal_calculation { c.clone() } else { return; };
        
        ui.vertical(|ui| {
                // Goal description with kid-friendly labeling
                ui.label(egui::RichText::new(format!("🎯 You're saving for: {}", goal.description))
                    .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                    .color(colors::TEXT_PRIMARY)
                    .strong());
                
                ui.add_space(25.0); // Increased spacing between sections
                
                // Goal amount and progress
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Target: ${:.2}", goal.target_amount))
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_SECONDARY));
                    
                    ui.add_space(20.0);
                    
                    ui.label(egui::RichText::new(format!("Current: ${:.2}", calculation.current_balance))
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_SECONDARY));
                    
                    ui.add_space(20.0);
                    
                    ui.label(egui::RichText::new(format!("Needed: ${:.2}", calculation.amount_needed))
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(if calculation.amount_needed <= 0.0 { 
                            egui::Color32::GREEN 
                        } else { 
                            colors::TEXT_SECONDARY 
                        }));
                });
                
                ui.add_space(20.0); // Better spacing before progress bar
                
                // Progress bar
                self.draw_goal_progress_bar(ui, &calculation);
                
                ui.add_space(25.0); // Increased spacing after progress bar
                
                // Completion information
                if calculation.amount_needed <= 0.0 {
                    ui.label(egui::RichText::new("🎉 Goal Complete! Congratulations!")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::GREEN)
                        .strong());
                } else {
                    self.draw_goal_completion_info(ui, &calculation);
                }
            });
    }
    
    /// Draw goal progress bar
    fn draw_goal_progress_bar(&self, ui: &mut egui::Ui, calculation: &shared::GoalCalculation) {
        let target_amount = if let Some(goal) = &self.goal.current_goal { 
            goal.target_amount 
        } else { 
            return; 
        };
        
        let progress = if target_amount > 0.0 {
            (calculation.current_balance / target_amount).clamp(0.0, 1.0)
        } else {
            0.0
        };
        
        let progress_bar = egui::ProgressBar::new(progress as f32)
            .desired_width(ui.available_width() - 40.0)
            .text(format!("{:.1}%", progress * 100.0))
            .fill(if progress >= 1.0 { 
                egui::Color32::GREEN 
            } else { 
                egui::Color32::from_rgb(100, 150, 255) 
            });
        
        ui.add(progress_bar);
    }
    
    /// Draw goal completion information
    fn draw_goal_completion_info(&self, ui: &mut egui::Ui, calculation: &shared::GoalCalculation) {
        if calculation.is_achievable {
            if let Some(completion_date) = &calculation.projected_completion_date {
                // Parse the date for display
                if let Ok(parsed_date) = chrono::DateTime::parse_from_rfc3339(completion_date) {
                    let formatted_date = parsed_date.format("%B %d, %Y");
                    
                    ui.label(egui::RichText::new(format!("Expected completion: {}", formatted_date))
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_SECONDARY));
                    
                    ui.label(egui::RichText::new(format!("Allowances needed: {}", calculation.allowances_needed))
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(colors::TEXT_SECONDARY));
                }
            }
        } else if calculation.exceeds_time_limit {
            ui.label(egui::RichText::new("⚠️ This goal will take more than a year to complete with current allowance")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::YELLOW));
        } else {
            ui.label(egui::RichText::new("⚠️ No allowance configured - goal completion cannot be calculated")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::YELLOW));
        }
    }
    
    /// Draw create goal card
    fn draw_create_goal_card(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0); // Increased top spacing for better centering
            
            // No goal message
            ui.label(egui::RichText::new("🎯 No Goal Set")
                .font(egui::FontId::new(24.0, egui::FontFamily::Proportional))
                .color(colors::TEXT_PRIMARY)
                .strong());
            
            ui.add_space(15.0); // Increased spacing between title and description
            
            ui.label(egui::RichText::new("Set a savings goal to track your progress!")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(colors::TEXT_SECONDARY));
            
            ui.add_space(40.0); // Increased spacing before button
            
            // Create goal button
            let create_button = egui::Button::new(egui::RichText::new("Create Goal")
                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::WHITE))
                .fill(egui::Color32::from_rgb(100, 150, 255))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(80, 130, 235)))
                .rounding(egui::Rounding::same(8.0))
                .min_size(egui::vec2(120.0, 40.0));
            
            if ui.add(create_button).clicked() {
                self.goal.show_creation_modal();
            }
        });
    }
}

impl AllowanceTrackerApp {
    /// Load current goal data from backend
    pub fn load_goal_data(&mut self) {
        info!("🎯 Loading goal data from backend");
        self.goal.start_loading();
        
        // Create command for getting current goal
        let command = crate::backend::domain::commands::goal::GetCurrentGoalCommand {
            child_id: self.current_child().as_ref().map(|c| c.id.clone()),
        };
        
        // Call backend service
        match self.backend().goal_service.get_current_goal(command) {
            Ok(result) => {
                info!("✅ Successfully loaded goal data");
                self.goal.set_goal_data(result.goal, result.calculation);
            }
            Err(error) => {
                log::error!("❌ Failed to load goal data: {}", error);
                self.goal.set_error(format!("Failed to load goal: {}", error));
            }
        }
    }
    
    /// Cancel the current goal
    pub fn cancel_current_goal(&mut self) {
        info!("🎯 Cancelling current goal");
        
        let command = crate::backend::domain::commands::goal::CancelGoalCommand {
            child_id: self.current_child().as_ref().map(|c| c.id.clone()),
        };
        
        match self.backend().goal_service.cancel_goal(command) {
            Ok(_result) => {
                info!("✅ Successfully cancelled goal");
                self.ui.success_message = Some("Goal cancelled successfully".to_string());
                // Reload goal data to update UI
                self.load_goal_data();
            }
            Err(error) => {
                log::error!("❌ Failed to cancel goal: {}", error);
                self.ui.error_message = Some(format!("Failed to cancel goal: {}", error));
            }
        }
    }
    
    /// Create a new goal
    pub fn create_goal(&mut self, description: String, target_amount: f64) {
        info!("🎯 Creating new goal: {} - ${:.2}", description, target_amount);
        
        let command = crate::backend::domain::commands::goal::CreateGoalCommand {
            child_id: self.current_child().as_ref().map(|c| c.id.clone()),
            description,
            target_amount,
        };
        
        match self.backend().goal_service.create_goal(command) {
            Ok(_result) => {
                info!("✅ Successfully created goal");
                self.ui.success_message = Some("Goal created successfully".to_string());
                self.goal.hide_creation_modal();
                // Reload goal data to update UI
                self.load_goal_data();
            }
            Err(error) => {
                log::error!("❌ Failed to create goal: {}", error);
                self.goal.creation_form.set_submission_error(format!("Failed to create goal: {}", error));
            }
        }
    }
} 