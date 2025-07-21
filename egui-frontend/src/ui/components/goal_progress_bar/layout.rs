//! Goal Layout System
//! 
//! This module provides a centralized, configurable layout system for goal UI components.
//! All spacing, margins, and layout concerns are handled here to ensure consistency
//! and make the system easy to configure and maintain.

use eframe::egui;

/// Centralized configuration for all goal UI spacing and layout
#[derive(Debug, Clone)]
pub struct GoalLayoutConfig {
    // Card layout
    pub card_margin: f32,           // Space from window edge to card
    pub card_rounding: f32,         // Card corner rounding
    pub card_internal_margin: f32,  // Space from card edge to content
    pub card_vertical_padding: f32, // Top/bottom padding inside card
    
    // Content spacing
    pub section_spacing: f32,       // Space between major sections
    pub element_spacing: f32,       // Space between related elements
    pub progress_bar_spacing: f32,  // Space around progress bar
    
    // Progress bar layout
    pub progress_bar_height: f32,
    pub progress_bar_rounding: f32,
    pub progress_bar_internal_spacing: f32, // Space between bar and target amount
}

impl Default for GoalLayoutConfig {
    fn default() -> Self {
        Self {
            card_margin: 20.0,
            card_rounding: 10.0,
            card_internal_margin: 35.0,
            card_vertical_padding: 35.0,
            
            section_spacing: 25.0,
            element_spacing: 20.0,
            progress_bar_spacing: 20.0,
            
            progress_bar_height: 70.0,
            progress_bar_rounding: 3.0,
            progress_bar_internal_spacing: 20.0,
        }
    }
}

/// Layout wrapper that applies consistent margins and spacing
pub struct GoalLayout {
    config: GoalLayoutConfig,
}

impl GoalLayout {
    pub fn new() -> Self {
        Self {
            config: GoalLayoutConfig::default(),
        }
    }
    
    pub fn with_config(config: GoalLayoutConfig) -> Self {
        Self { config }
    }
    
    /// Create the main card container with proper margins
    pub fn card_container<R>(
        &self, 
        ui: &mut egui::Ui, 
        available_rect: egui::Rect,
        content: impl FnOnce(&mut egui::Ui) -> R
    ) -> R {
        // Calculate card rect with external margins
        let card_rect = egui::Rect::from_min_size(
            available_rect.min + egui::vec2(self.config.card_margin, self.config.card_margin),
            egui::vec2(
                available_rect.width() - (self.config.card_margin * 2.0),
                available_rect.height() - (self.config.card_margin * 2.0)
            ),
        );
        
        // Draw card background
        crate::ui::components::styling::draw_card_container(ui, card_rect, self.config.card_rounding);
        
        // Content area with internal margins
        let mut result = None;
        ui.allocate_ui_at_rect(card_rect, |ui| {
            ui.vertical(|ui| {
                // Top padding
                ui.add_space(self.config.card_vertical_padding);
                
                // Content with internal margins
                ui.horizontal(|ui| {
                    ui.add_space(self.config.card_internal_margin);
                    ui.vertical(|ui| {
                        result = Some(content(ui));
                        // Bottom padding
                        ui.add_space(self.config.card_vertical_padding);
                    })
                })
            })
        });
        result.unwrap()
    }
    
    /// Apply consistent spacing between major sections
    pub fn section_spacing(&self, ui: &mut egui::Ui) {
        ui.add_space(self.config.section_spacing);
    }
    
    /// Apply consistent spacing between related elements
    pub fn element_spacing(&self, ui: &mut egui::Ui) {
        ui.add_space(self.config.element_spacing);
    }
    
    /// Apply spacing around progress bar
    pub fn progress_bar_spacing(&self, ui: &mut egui::Ui) {
        ui.add_space(self.config.progress_bar_spacing);
    }
    
    /// Create a progress bar container with proper width allocation
    pub fn progress_bar_container<R>(
        &self,
        ui: &mut egui::Ui,
        content: impl FnOnce(&mut egui::Ui, f32) -> R
    ) -> R {
        // The progress bar should respect right internal margin too
        // Available width minus right internal margin for symmetry with left margin
        let available_width = ui.available_width() - self.config.card_internal_margin;
        content(ui, available_width)
    }
    
    /// Get layout configuration for progress bar component
    pub fn progress_bar_config(&self) -> ProgressBarLayoutConfig {
        ProgressBarLayoutConfig {
            height: self.config.progress_bar_height,
            rounding: self.config.progress_bar_rounding,
            internal_spacing: self.config.progress_bar_internal_spacing,
        }
    }
}

/// Layout configuration specifically for progress bar component
#[derive(Debug, Clone)]
pub struct ProgressBarLayoutConfig {
    pub height: f32,
    pub rounding: f32,
    pub internal_spacing: f32,
}

/// Content type enum for semantic layout application
#[derive(Debug, Clone, Copy)]
pub enum GoalContentType {
    Title,
    Summary,
    ProgressBar,
    CompletionInfo,
    LoadingState,
    ErrorState,
    CreateGoal,
}

impl GoalLayout {
    /// Apply appropriate spacing for different content types
    pub fn content_spacing(&self, ui: &mut egui::Ui, content_type: GoalContentType) {
        match content_type {
            GoalContentType::Title => {
                // No spacing before title (it's first)
            }
            GoalContentType::Summary => {
                self.section_spacing(ui);
            }
            GoalContentType::ProgressBar => {
                self.progress_bar_spacing(ui);
            }
            GoalContentType::CompletionInfo => {
                self.section_spacing(ui);
            }
            GoalContentType::LoadingState | 
            GoalContentType::ErrorState | 
            GoalContentType::CreateGoal => {
                // These are standalone states, no extra spacing needed
            }
        }
    }
} 