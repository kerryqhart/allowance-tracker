//! # Circular Days Progress Renderer
//!
//! This module handles rendering the donut-style circular progress tracker using egui's
//! painting primitives. It creates a visually appealing circular progress indicator
//! with center text showing days information.

use eframe::egui;
use crate::backend::domain::models::goal::DomainGoal;
use shared::GoalCalculation;
use super::calculations::{DaysProgress, calculate_days_progress};
use std::f32::consts::PI;

/// Configuration for circular progress appearance
#[derive(Debug, Clone)]
pub struct CircularProgressConfig {
    /// Outer radius of the donut
    pub outer_radius: f32,
    /// Inner radius of the donut (creates the "hole")
    pub inner_radius: f32,
    /// Stroke width for the progress arc
    pub stroke_width: f32,
    /// Font size for main center text
    pub center_font_size: f32,
    /// Font size for secondary text
    pub secondary_font_size: f32,
    /// Spacing between text lines
    pub text_line_spacing: f32,
}

impl Default for CircularProgressConfig {
    fn default() -> Self {
        Self {
            outer_radius: 60.0,
            inner_radius: 45.0,
            stroke_width: 15.0,
            center_font_size: 14.0,
            secondary_font_size: 11.0,
            text_line_spacing: 18.0,
        }
    }
}

/// Circular Days Progress component
#[derive(Debug)]
pub struct CircularDaysProgress {
    /// Configuration for appearance
    config: CircularProgressConfig,
    /// Cached days progress data
    days_progress: Option<DaysProgress>,
    /// Error message if calculation failed
    error_message: Option<String>,
}

impl CircularDaysProgress {
    /// Create a new circular days progress component
    pub fn new() -> Self {
        Self {
            config: CircularProgressConfig::default(),
            days_progress: None,
            error_message: None,
        }
    }
    
    /// Create with custom configuration
    pub fn with_config(config: CircularProgressConfig) -> Self {
        Self {
            config,
            days_progress: None,
            error_message: None,
        }
    }
    
    /// Calculate and cache days progress
    pub fn update_progress(&mut self, goal: &DomainGoal, goal_calculation: Option<&GoalCalculation>) {
        match calculate_days_progress(goal, goal_calculation) {
            Ok(progress) => {
                self.days_progress = Some(progress);
                self.error_message = None;
            }
            Err(error) => {
                self.error_message = Some(error);
                self.days_progress = None;
            }
        }
    }
    
    /// Render the circular days progress
    pub fn render(&self, ui: &mut egui::Ui) {
        if let Some(ref error) = self.error_message {
            // Show error state
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.label(egui::RichText::new("❌ Error")
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::RED));
                ui.label(egui::RichText::new(error)
                    .font(egui::FontId::new(10.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
            });
            return;
        }
        
        let Some(ref progress) = self.days_progress else {
            // Show loading/empty state
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.label(egui::RichText::new("⏳ Calculating...")
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
            });
            return;
        };
        
        // Calculate the size and center position
        let available_size = ui.available_size();
        let _size = available_size.x.min(available_size.y).min(self.config.outer_radius * 2.2);
        
        // Allocate space for the circular progress
        let (_rect, _response) = ui.allocate_exact_size(available_size, egui::Sense::hover());
        
        // Calculate center in absolute coordinates relative to the allocated rect
        let center = egui::pos2(
            ui.min_rect().min.x + available_size.x / 2.0,
            ui.min_rect().min.y + available_size.y / 2.0,
        );
        
        // Render the donut chart
        self.render_donut(ui, center, progress);
        
        // Render center text
        self.render_center_text(ui, center, progress);
    }
    
    /// Render the donut-shaped progress indicator
    fn render_donut(&self, ui: &mut egui::Ui, center: egui::Pos2, progress: &DaysProgress) {
        let painter = ui.painter();
        
        // Background circle (full donut)
        painter.circle_stroke(
            center,
            self.config.outer_radius,
            egui::Stroke::new(self.config.stroke_width, progress.background_color())
        );
        
        // Progress arc
        if progress.progress_percentage > 0.0 {
            let start_angle = -PI / 2.0; // Start at 12 o'clock
            let end_angle = start_angle + (2.0 * PI * progress.progress_percentage);
            
            self.draw_progress_arc(
                painter,
                center,
                self.config.outer_radius,
                self.config.stroke_width,
                start_angle,
                end_angle,
                progress.progress_color(),
            );
        }
        
        // Inner circle to create the "donut hole" effect
        painter.circle_filled(center, self.config.inner_radius, ui.style().visuals.panel_fill);
    }
    
    /// Draw a progress arc using line segments (since egui doesn't have native arc support)
    fn draw_progress_arc(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        radius: f32,
        stroke_width: f32,
        start_angle: f32,
        end_angle: f32,
        color: egui::Color32,
    ) {
        // Calculate number of segments based on arc length for smooth appearance
        let arc_length = (end_angle - start_angle).abs();
        let num_segments = (arc_length * radius / 3.0).ceil() as i32; // Roughly 3 pixels per segment
        let num_segments = num_segments.max(8).min(100); // Reasonable bounds
        
        let angle_step = (end_angle - start_angle) / num_segments as f32;
        
        // Draw the arc as a series of short line segments
        for i in 0..num_segments {
            let angle1 = start_angle + angle_step * i as f32;
            let angle2 = start_angle + angle_step * (i + 1) as f32;
            
            let point1 = egui::pos2(
                center.x + radius * angle1.cos(),
                center.y + radius * angle1.sin(),
            );
            let point2 = egui::pos2(
                center.x + radius * angle2.cos(),
                center.y + radius * angle2.sin(),
            );
            
            painter.line_segment([point1, point2], egui::Stroke::new(stroke_width, color));
        }
    }
    
    /// Render text in the center of the donut
    fn render_center_text(&self, ui: &mut egui::Ui, center: egui::Pos2, progress: &DaysProgress) {
        let painter = ui.painter();
        
        // Main text
        let main_text = progress.center_text();
        let main_font = egui::FontId::new(self.config.center_font_size, egui::FontFamily::Proportional);
        
        // Handle multi-line text
        let lines: Vec<&str> = main_text.split('\n').collect();
        let line_height = self.config.text_line_spacing;
        
        // Calculate total text height
        let total_height = line_height * (lines.len() as f32 - 1.0);
        let start_y = center.y - total_height / 2.0;
        
        // Draw each line centered
        for (i, line) in lines.iter().enumerate() {
            let text_pos = egui::pos2(center.x, start_y + line_height * i as f32);
            painter.text(
                text_pos,
                egui::Align2::CENTER_CENTER,
                line,
                main_font.clone(),
                ui.style().visuals.strong_text_color(),
            );
        }
        
        // Secondary text (if available)
        if let Some(secondary_text) = progress.secondary_text() {
            let secondary_font = egui::FontId::new(self.config.secondary_font_size, egui::FontFamily::Proportional);
            let secondary_pos = egui::pos2(center.x, start_y + total_height + line_height * 0.7);
            
            painter.text(
                secondary_pos,
                egui::Align2::CENTER_CENTER,
                secondary_text,
                secondary_font,
                ui.style().visuals.weak_text_color(),
            );
        }
    }
    
    /// Clear cached data
    pub fn clear(&mut self) {
        self.days_progress = None;
        self.error_message = None;
    }
    
    /// Check if progress data is available
    pub fn has_progress(&self) -> bool {
        self.days_progress.is_some() && self.error_message.is_none()
    }
    
    /// Get the current progress percentage (for external use)
    pub fn progress_percentage(&self) -> Option<f32> {
        self.days_progress.as_ref().map(|p| p.progress_percentage)
    }
}

impl Default for CircularDaysProgress {
    fn default() -> Self {
        Self::new()
    }
} 