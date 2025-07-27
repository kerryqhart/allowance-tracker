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
    pub card_rounding: u8,         // Card corner rounding
    pub card_internal_margin: f32,  // Space from card edge to content
    pub card_vertical_padding: f32, // Top/bottom padding inside card
    
    // Content spacing
    pub section_spacing: f32,       // Space between major sections
    pub element_spacing: f32,       // Space between related elements
    pub progress_bar_spacing: f32,  // Space around progress bar
    
    // Progress bar layout
    pub progress_bar_height: f32,
    pub progress_bar_rounding: u8,
    pub progress_bar_internal_spacing: f32, // Space between bar and target amount
    
    // NEW: 3-Section Layout Configuration
    pub three_section_enabled: bool,        // Whether to use 3-section layout
    pub top_section_height_ratio: f32,      // Top section as ratio of total height (1/3)
    pub bottom_section_height_ratio: f32,   // Bottom section as ratio of total height (2/3)
    pub bottom_left_width_ratio: f32,       // Bottom-left as ratio of bottom width (2/3)
    pub bottom_right_width_ratio: f32,      // Bottom-right as ratio of bottom width (1/3)
    pub section_gap: f32,                   // Gap between sections
    pub bottom_section_gap: f32,            // Gap between bottom-left and bottom-right
}

impl Default for GoalLayoutConfig {
    fn default() -> Self {
        Self {
            card_margin: 20.0,
            card_rounding: 10,
            card_internal_margin: 35.0,
            card_vertical_padding: 35.0,
            
            section_spacing: 25.0,
            element_spacing: 20.0,
            progress_bar_spacing: 20.0,
            
            progress_bar_height: 70.0,
            progress_bar_rounding: 3,
            progress_bar_internal_spacing: 20.0,
            
            // NEW: 3-Section Layout defaults
            three_section_enabled: false,      // Start with legacy layout by default
            top_section_height_ratio: 1.0/3.0,      // Top takes 1/3 of height
            bottom_section_height_ratio: 2.0/3.0,   // Bottom takes 2/3 of height
            bottom_left_width_ratio: 2.0/3.0,       // Bottom-left takes 2/3 of bottom width
            bottom_right_width_ratio: 1.0/3.0,      // Bottom-right takes 1/3 of bottom width
            section_gap: 15.0,                      // Gap between top and bottom sections
            bottom_section_gap: 15.0,               // Gap between bottom-left and bottom-right
        }
    }
}

/// Layout wrapper that applies consistent margins and spacing
#[derive(Clone)]
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
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card_rect), |ui| {
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
    
    /// Get the layout configuration (for 3-section manual layout)
    pub fn config(&self) -> &GoalLayoutConfig {
        &self.config
    }
    
    /// Get mutable access to the layout configuration
    pub fn config_mut(&mut self) -> &mut GoalLayoutConfig {
        &mut self.config
    }
}

/// Layout configuration specifically for progress bar component
#[derive(Debug, Clone)]
pub struct ProgressBarLayoutConfig {
    pub height: f32,
    pub rounding: u8,
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

/// NEW: Goal card sections for 3-section layout
#[derive(Debug, Clone, Copy)]
pub enum GoalCardSection {
    TopProgressBar,
    BottomLeftGraph,
    BottomRightCircular,
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
    
    // NEW: 3-Section Layout Functions
    
    /// Check if 3-section layout is enabled
    pub fn is_three_section_enabled(&self) -> bool {
        self.config.three_section_enabled
    }
    
    /// Enable or disable 3-section layout
    pub fn set_three_section_enabled(&mut self, enabled: bool) {
        self.config.three_section_enabled = enabled;
    }
    
    /// Create a 3-section layout container with proper section allocation
    pub fn three_section_container<R>(
        &self,
        ui: &mut egui::Ui,
        available_rect: egui::Rect,
        content: impl FnOnce(&mut egui::Ui, egui::Rect, egui::Rect, egui::Rect) -> R
    ) -> R {
        if !self.config.three_section_enabled {
            panic!("three_section_container called but three_section_enabled is false");
        }
        
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
        
        // Calculate internal content area
        let content_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(self.config.card_internal_margin, self.config.card_vertical_padding),
            egui::vec2(
                card_rect.width() - (self.config.card_internal_margin * 2.0),
                card_rect.height() - (self.config.card_vertical_padding * 2.0)
            ),
        );
        
        // Calculate section dimensions
        let top_height = content_rect.height() * self.config.top_section_height_ratio;
        let bottom_height = content_rect.height() * self.config.bottom_section_height_ratio - self.config.section_gap;
        
        // Top section (full width, 1/3 height)
        let top_section_rect = egui::Rect::from_min_size(
            content_rect.min,
            egui::vec2(content_rect.width(), top_height)
        );
        
        // Bottom section positioning (starts after top + gap)
        let bottom_start_y = content_rect.min.y + top_height + self.config.section_gap;
        
        // Bottom-left section (2/3 width, remaining height)
        let bottom_left_width = content_rect.width() * self.config.bottom_left_width_ratio - (self.config.bottom_section_gap / 2.0);
        let bottom_left_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.min.x, bottom_start_y),
            egui::vec2(bottom_left_width, bottom_height)
        );
        
        // Bottom-right section (1/3 width, remaining height)
        let bottom_right_width = content_rect.width() * self.config.bottom_right_width_ratio - (self.config.bottom_section_gap / 2.0);
        let bottom_right_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.min.x + bottom_left_width + self.config.bottom_section_gap, bottom_start_y),
            egui::vec2(bottom_right_width, bottom_height)
        );
        
        content(ui, top_section_rect, bottom_left_rect, bottom_right_rect)
    }
    
    /// Create container for top progress bar section
    pub fn top_progress_bar_container<R>(
        &self,
        ui: &mut egui::Ui,
        section_rect: egui::Rect,
        content: impl FnOnce(&mut egui::Ui) -> R
    ) -> R {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(section_rect), |ui| {
            ui.vertical_centered(|ui| {
                content(ui)
            }).inner
        }).inner
    }
    
    /// Create container for bottom-left graph section
    pub fn bottom_left_graph_container<R>(
        &self,
        ui: &mut egui::Ui,
        section_rect: egui::Rect,
        content: impl FnOnce(&mut egui::Ui) -> R
    ) -> R {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(section_rect), |ui| {
            content(ui)
        }).inner
    }
    
    /// Create container for bottom-right circular section
    pub fn bottom_right_circular_container<R>(
        &self,
        ui: &mut egui::Ui,
        section_rect: egui::Rect,
        content: impl FnOnce(&mut egui::Ui) -> R
    ) -> R {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(section_rect), |ui| {
            ui.vertical_centered(|ui| {
                content(ui)
            }).inner
        }).inner
    }
} 