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
use crate::ui::components::styling::colors;

use crate::ui::components::goal_progress_bar::{
    draw_progress_bar_with_target, 
    GoalLayout, 
    GoalContentType
};

impl AllowanceTrackerApp {
    /// Draw the main goal section using the centralized layout system
    pub fn draw_goal_section(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect) {
        let layout = GoalLayout::new();
        
        layout.card_container(ui, available_rect, |ui| {
            if self.goal.loading {
                layout.content_spacing(ui, GoalContentType::LoadingState);
                self.draw_goal_loading_state(ui);
            } else if let Some(error) = &self.goal.error_message.clone() {
                layout.content_spacing(ui, GoalContentType::ErrorState);
                self.draw_goal_error_state(ui, error);
            } else if self.goal.has_active_goal() {
                self.draw_current_goal_card_with_layout(ui, &layout);
            } else {
                layout.content_spacing(ui, GoalContentType::CreateGoal);
                self.draw_create_goal_card(ui);
            }
        });
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
    
    /// Draw current goal display card with layout system
    fn draw_current_goal_card_with_layout(&mut self, ui: &mut egui::Ui, layout: &GoalLayout) {
        // Clone the data to avoid borrowing conflicts
        let goal = if let Some(ref g) = self.goal.current_goal { g.clone() } else { return; };
        let calculation = if let Some(ref c) = self.goal.goal_calculation { c.clone() } else { return; };
        
        // Goal title
        layout.content_spacing(ui, GoalContentType::Title);
        ui.label(egui::RichText::new(format!("🎯 You're saving for: {}", goal.description))
            .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
            .color(colors::TEXT_PRIMARY)
            .strong());
        
        // Goal summary
        layout.content_spacing(ui, GoalContentType::Summary);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Target: ${:.2}", goal.target_amount))
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(colors::TEXT_SECONDARY));
            
            layout.element_spacing(ui);
            
            ui.label(egui::RichText::new(format!("Current: ${:.2}", calculation.current_balance))
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(colors::TEXT_SECONDARY));
            
            layout.element_spacing(ui);
            
            ui.label(egui::RichText::new(format!("Needed: ${:.2}", calculation.amount_needed))
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(if calculation.amount_needed <= 0.0 { 
                    egui::Color32::GREEN 
                } else { 
                    colors::TEXT_SECONDARY 
                }));
        });
        
        // Progress bar
        layout.content_spacing(ui, GoalContentType::ProgressBar);
        self.draw_goal_progress_bar_with_layout(ui, &calculation, layout);
        
        // Completion information
        layout.content_spacing(ui, GoalContentType::CompletionInfo);
        if calculation.amount_needed <= 0.0 {
            ui.label(egui::RichText::new("🎉 Goal Complete! Congratulations!")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .color(egui::Color32::GREEN)
                .strong());
        } else {
            self.draw_goal_completion_info(ui, &calculation);
        }
    }
    

    
    /// Draw goal progress bar using the layout system
    fn draw_goal_progress_bar_with_layout(
        &self, 
        ui: &mut egui::Ui, 
        calculation: &shared::GoalCalculation,
        layout: &GoalLayout
    ) {
        let target_amount = if let Some(goal) = &self.goal.current_goal { 
            goal.target_amount 
        } else { 
            return; 
        };
        
        layout.progress_bar_container(ui, |ui, available_width| {
            let layout_config = layout.progress_bar_config();
            draw_progress_bar_with_target(
                ui,
                calculation.current_balance,
                target_amount,
                available_width,
                &layout_config,
            );
        });
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