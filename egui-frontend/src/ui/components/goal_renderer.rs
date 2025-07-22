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
    draw_progress_bar_with_target_completion,
    GoalLayout, 
    GoalContentType
};

/// Shared styling configuration for all section headers in the goal card
#[derive(Debug, Clone)]
struct SectionHeaderStyle {
    font_size: f32,
    font_family: egui::FontFamily,
    color: egui::Color32,
    spacing_below: f32,
}

impl Default for SectionHeaderStyle {
    fn default() -> Self {
        Self {
            font_size: 24.0,
            font_family: egui::FontFamily::Proportional,
            color: egui::Color32::from_rgb(50, 50, 50),
            spacing_below: 20.0,
        }
    }
}

impl SectionHeaderStyle {
    /// Create a styled header label
    fn create_label(&self, text: &str) -> egui::RichText {
        egui::RichText::new(text)
            .font(egui::FontId::new(self.font_size, self.font_family.clone()))
            .color(self.color)
    }
}

impl AllowanceTrackerApp {
    /// Draw the main goal section using the centralized layout system
    pub fn draw_goal_section(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect) {
        log::info!("🎯 GOAL_SECTION: Received {}w x {}h from tab manager", available_rect.width(), available_rect.height());
        
        let layout = GoalLayout::new();
        
        // Check if we should use 3-section layout
        if self.goal.has_active_goal() && self.should_use_three_section_layout() {
            log::info!("✅ Using NEW 3-section layout with direct rendering");
            self.ensure_goal_components_loaded();
            self.draw_three_section_goal_card_direct(ui, available_rect);
        } else {
            // Use normal card_container for other cases  
            layout.card_container(ui, available_rect, |ui| {
                if self.goal.loading {
                    layout.content_spacing(ui, GoalContentType::LoadingState);
                    self.draw_goal_loading_state(ui);
                } else if let Some(error) = &self.goal.error_message.clone() {
                    layout.content_spacing(ui, GoalContentType::ErrorState);
                    self.draw_goal_error_state(ui, error);
                } else if self.goal.has_active_goal() {
                    log::info!("📄 Using legacy layout");
                    self.draw_current_goal_card_with_layout(ui, &layout);
                } else {
                    layout.content_spacing(ui, GoalContentType::CreateGoal);
                    self.draw_create_goal_card(ui);
                }
            });
        }
    }
    
    /// Check if we should use the 3-section layout
    fn should_use_three_section_layout(&self) -> bool {
        // FORCE 3-section layout for debugging positioning (ignore components ready check)
        self.goal.has_active_goal()
    }
    
    /// Ensure goal components are loaded for 3-section layout
    fn ensure_goal_components_loaded(&mut self) {
        self.goal.initialize_components();
        // Data loading will happen during render to avoid borrowing conflicts
    }
    
    /// Draw the 3-section goal card with DIRECT rendering (no nested UI hierarchies)
    fn draw_three_section_goal_card_direct(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect) {
        let goal = if let Some(ref g) = self.goal.current_goal { g.clone() } else { return; };
        let calculation = if let Some(ref c) = self.goal.goal_calculation { c.clone() } else { return; };
        
        log::info!("🎨 Drawing DIRECT 3-section goal card");
        log::info!("🏠 Direct available space: {}w x {}h", available_rect.width(), available_rect.height());
        
        // Calculate margins and sections with DIRECT rectangle calculations
        let card_margin = 20.0;
        let internal_margin = 35.0;
        let vertical_padding = 35.0;
        let section_gap = 10.0;
        
        // Calculate unified card rectangle
        let unified_card_rect = egui::Rect::from_min_size(
            available_rect.min + egui::vec2(card_margin, card_margin),
            egui::vec2(
                available_rect.width() - (card_margin * 2.0),
                available_rect.height() - (card_margin * 2.0)
            ),
        );
        
        // Draw single unified background
        crate::ui::components::styling::draw_card_container(ui, unified_card_rect, 10.0);
        
        // Calculate internal content area
        let content_rect = egui::Rect::from_min_size(
            unified_card_rect.min + egui::vec2(internal_margin, vertical_padding),
            egui::vec2(
                unified_card_rect.width() - (internal_margin * 2.0),
                unified_card_rect.height() - (vertical_padding * 2.0)
            ),
        );
        
        // Calculate section rectangles DIRECTLY
        let top_height = content_rect.height() * 0.30; // Top section = 30% height
        let bottom_height = content_rect.height() * 0.70 - section_gap; // Bottom = 70% - gap
        
        // Top section (progress bar)
        let top_rect = egui::Rect::from_min_size(
            content_rect.min,
            egui::vec2(content_rect.width(), top_height)
        );
        
        // Bottom sections 
        let bottom_start_y = content_rect.min.y + top_height + section_gap;
        let bottom_left_width = content_rect.width() * (2.0/3.0) - (section_gap / 2.0);
        let bottom_right_width = content_rect.width() * (1.0/3.0) - (section_gap / 2.0);
        
        let bottom_left_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.min.x, bottom_start_y),
            egui::vec2(bottom_left_width, bottom_height)
        );
        
        let bottom_right_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.min.x + bottom_left_width + section_gap, bottom_start_y),
            egui::vec2(bottom_right_width, bottom_height)
        );
        
        log::info!("📐 DIRECT Section rectangles:");
        log::info!("   Top: [{:.1} {:.1}] - [{:.1} {:.1}]", top_rect.min.x, top_rect.min.y, top_rect.max.x, top_rect.max.y);
        log::info!("   Bottom-left: [{:.1} {:.1}] - [{:.1} {:.1}]", bottom_left_rect.min.x, bottom_left_rect.min.y, bottom_left_rect.max.x, bottom_left_rect.max.y);
        log::info!("   Bottom-right: [{:.1} {:.1}] - [{:.1} {:.1}]", bottom_right_rect.min.x, bottom_right_rect.min.y, bottom_right_rect.max.x, bottom_right_rect.max.y);
        log::info!("   📍 Bottom sections should start at same Y: left={:.1}, right={:.1}", bottom_left_rect.min.y, bottom_right_rect.min.y);
        
        // TOP SECTION: Progress bar (DIRECT rendering)
        ui.allocate_ui_at_rect(top_rect, |ui| {
            log::info!("🎯 Rendering progress bar in top section");
            
            let header_style = SectionHeaderStyle::default();
            
            ui.vertical_centered(|ui| {
                // Goal header with consistent styling
                ui.label(header_style.create_label(&format!("Goal: {}", goal.description)));
                
                ui.add_space(header_style.spacing_below);
                
                // Progress bar using existing component (taller to fill 30% section)
                let layout_config = crate::ui::components::goal_progress_bar::ProgressBarLayoutConfig {
                    height: 90.0,  // Increased from 70.0 to better fill the 30% top section
                    rounding: 3.0,
                    internal_spacing: 20.0,
                };
                
                let available_width = top_rect.width() - (internal_margin * 2.0);
                
                crate::ui::components::goal_progress_bar::draw_progress_bar_with_target_completion(
                    ui,
                    calculation.current_balance,
                    goal.target_amount,
                    available_width,
                    &layout_config,
                    self.is_goal_complete()
                );
            });
        });
        
        // BOTTOM-LEFT SECTION: Goal progress graph (with margins)
        log::info!("🎯 Bottom-left section rendering goal progression graph");
        
        // Load graph data if needed using our helper method
        self.load_goal_component_data_if_needed(&goal);
        
        // Apply margins to the graph area 
        let graph_margin = 15.0;
        let graph_rect = egui::Rect::from_min_size(
            bottom_left_rect.min + egui::vec2(graph_margin, graph_margin),
            egui::vec2(
                bottom_left_rect.width() - (graph_margin * 2.0),
                bottom_left_rect.height() - (graph_margin * 2.0)
            ),
        );
        
        log::info!("🎯 Graph rect (with margins): [{:.1} {:.1}] - [{:.1} {:.1}]", 
                   graph_rect.min.x, graph_rect.min.y, graph_rect.max.x, graph_rect.max.y);

        
        // DIRECT RENDERING: No nested UI hierarchy
        ui.allocate_ui_at_rect(graph_rect, |ui| {
            let header_style = SectionHeaderStyle::default();
            
            ui.vertical(|ui| {
                // Graph header with consistent styling - HORIZONTALLY CENTERED
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    let graph_title = if self.is_goal_complete() {
                        "" // Empty title when goal is complete
                    } else {
                        "Are you getting closer?"
                    };
                    if !graph_title.is_empty() {
                        ui.label(header_style.create_label(graph_title));
                    }
                });
                
                // Only add spacing if we have a header
                let graph_title = if self.is_goal_complete() { "" } else { "Are you getting closer?" };
                if !graph_title.is_empty() {
                    ui.add_space(header_style.spacing_below);
                }
                
                // Render the graph in remaining space
                if let Some(ref progress_graph) = self.goal.progress_graph {
                    progress_graph.render(ui, &goal, &calculation);
                } else {
                    // Fallback state - centered message
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 3.0);
                        ui.label(egui::RichText::new("⏳ Initializing graph...")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(120, 120, 120)));
                    });
                }
            });
        });
        
        // BOTTOM-RIGHT SECTION: Circular progress component (centered)
        log::info!("🎯 Bottom-right section rendering circular progress component");
        
        // Apply same margins to the right section for consistent alignment
        let right_margin = 15.0; // Match the graph_margin
        let right_rect_with_margin = egui::Rect::from_min_size(
            bottom_right_rect.min + egui::vec2(right_margin, right_margin),
            egui::vec2(
                bottom_right_rect.width() - (right_margin * 2.0),
                bottom_right_rect.height() - (right_margin * 2.0)
            ),
        );
        

        
        // Update circular progress data if needed and render it
        if let Some(mut circular_progress) = self.goal.circular_progress.take() {
            circular_progress.update_progress(&goal, Some(&calculation));
            
            ui.allocate_ui_at_rect(right_rect_with_margin, |ui| {
                let header_style = SectionHeaderStyle::default();
                
                ui.vertical(|ui| {
                    // Circular progress header with consistent styling - HORIZONTALLY CENTERED
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        let circular_title = if self.is_goal_complete() {
                            "" // Empty title when goal is complete
                        } else {
                            "Days until..."
                        };
                        if !circular_title.is_empty() {
                            ui.label(header_style.create_label(circular_title));
                        }
                    });
                    
                    // Only add spacing if we have a header
                    let circular_title = if self.is_goal_complete() { "" } else { "Days until..." };
                    if !circular_title.is_empty() {
                        ui.add_space(header_style.spacing_below);
                    }
                    
                    // Center just the circular progress component in remaining space
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 2.0 - 60.0); // Center the circle
                        circular_progress.render(ui);
                    });
                });
            });
            
            self.goal.circular_progress = Some(circular_progress);
        } else {
            // Fallback state - show initialization message
            ui.allocate_ui_at_rect(right_rect_with_margin, |ui| {
                let header_style = SectionHeaderStyle::default();
                
                ui.vertical(|ui| {
                    // Header even in fallback state for consistency - HORIZONTALLY CENTERED
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        let circular_title = if self.is_goal_complete() {
                            "" // Empty title when goal is complete
                        } else {
                            "Days until..."
                        };
                        if !circular_title.is_empty() {
                            ui.label(header_style.create_label(circular_title));
                        }
                    });
                    
                    // Only add spacing if we have a header
                    let circular_title = if self.is_goal_complete() { "" } else { "Days until..." };
                    if !circular_title.is_empty() {
                        ui.add_space(header_style.spacing_below);
                    }
                    
                    // Center the fallback message in remaining space
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 3.0);
                        ui.label(egui::RichText::new("⏳ Initializing circular progress...")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(120, 120, 120)));
                    });
                });
            });
        }
    }
    
    /// Load goal component data if needed (handles borrowing conflicts)
    fn load_goal_component_data_if_needed(&mut self, goal: &crate::backend::domain::models::goal::DomainGoal) {
        let needs_data_loading = if let Some(ref progress_graph) = self.goal.progress_graph {
            !progress_graph.has_data()
        } else {
            false
        };
        
        if needs_data_loading {
            let goal_clone = goal.clone();
            if let Some(mut progress_graph) = self.goal.progress_graph.take() {
                let backend = self.backend();
                progress_graph.load_data(backend, &goal_clone);
                self.goal.progress_graph = Some(progress_graph);
            }
        }
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
                .font(egui::FontId::new(22.0, egui::FontFamily::Proportional)) // Increased from 18.0
                .color(egui::Color32::from_rgb(199, 112, 221)) // Changed from GREEN to pink
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
            draw_progress_bar_with_target_completion(
                ui,
                calculation.current_balance,
                target_amount,
                available_width,
                &layout_config,
                self.is_goal_complete(),
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
                .rounding(egui::CornerRadius::same(8))
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
                // Success feedback removed
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
                // Success feedback removed
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