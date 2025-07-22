//! # Goal Progress Graph Renderer
//!
//! This module handles rendering the goal progress graph using egui_plot.
//! It creates a compact balance progression graph with goal target line,
//! optimized for the smaller space in the goal card layout.

use eframe::egui;
use crate::backend::domain::models::goal::DomainGoal;
use shared::GoalCalculation;
use super::data_preparation::GoalGraphDataPoint;

use log::{info, warn};

/// Goal Progress Graph component
#[derive(Debug)]
pub struct GoalProgressGraph {
    /// Cached graph data points
    data_points: Vec<GoalGraphDataPoint>,
    /// Whether data is currently loading
    is_loading: bool,
    /// Error message if loading failed
    error_message: Option<String>,
}

impl GoalProgressGraph {
    /// Create a new goal progress graph component
    pub fn new() -> Self {
        Self {
            data_points: Vec::new(),
            is_loading: false,
            error_message: None,
        }
    }
    
    /// Check if this component has data loaded
    pub fn has_data(&self) -> bool {
        !self.data_points.is_empty()
    }
    
    /// Clear all data from this component
    pub fn clear_data(&mut self) {
        self.data_points.clear();
        self.is_loading = false;
        self.error_message = None;
    }
    
    /// Load data for the goal progress graph
    pub fn load_data(&mut self, backend: &crate::backend::Backend, goal: &DomainGoal) {
        self.is_loading = true;
        self.error_message = None;
        
        info!("🎯 Loading goal progress graph data for goal: {}", goal.description);
        
        info!("🎯 Using domain APIs to get complete goal progression data");
        
        // Use new domain APIs for complete goal progression data
        match backend.goal_service.get_goal_progression_data(goal) {
            Ok(domain_transactions) => {
                info!("🎯 Successfully loaded {} progression transactions from domain APIs", domain_transactions.len());
                
                // Get balance at goal creation date using domain API
                let goal_creation_date_str = match chrono::DateTime::parse_from_rfc3339(&goal.created_at) {
                    Ok(datetime) => datetime.to_rfc3339(),
                    Err(e) => {
                        self.error_message = Some(format!("Invalid goal creation date: {}", e));
                        self.is_loading = false;
                        return;
                    }
                };
                
                let goal_creation_balance = match backend.balance_service.get_balance_at_date(&goal.child_id, &goal_creation_date_str) {
                    Ok(balance) => balance,
                    Err(e) => {
                        warn!("Failed to get balance at goal creation date, using 0.0: {}", e);
                        0.0
                    }
                };
                
                info!("🎯 Goal creation balance: ${:.2}", goal_creation_balance);
                
                // Convert domain transactions to UI data points using new converter
                self.data_points = crate::ui::components::goal_progress_graph::data_preparation::convert_domain_transactions_to_data_points(
                    &domain_transactions, 
                    goal, 
                    goal_creation_balance
                );
                
                info!("🎯 Generated {} goal graph data points from domain APIs", self.data_points.len());
                
                // Debug logging to see what data points we have
                info!("🎯 GOAL GRAPH DEBUG: Data points breakdown:");
                for (i, point) in self.data_points.iter().enumerate() {
                    info!("  Point {}: {} - ${:.2} (goal_start: {}, goal_target: {}, projection: {})", 
                           i, point.date, point.balance, point.is_goal_start, point.is_goal_target, point.is_projection);
                }
                
                self.is_loading = false;
            }
            Err(e) => {
                warn!("❌ Failed to load goal progression data from domain APIs: {}", e);
                self.error_message = Some(format!("Failed to load goal progression data: {}", e));
                self.is_loading = false;
            }
        }
    }
    
    /// Render the goal progress graph (data should be loaded beforehand)
    pub fn render(
        &self,
        ui: &mut egui::Ui,
        goal: &DomainGoal,
        _goal_calculation: &GoalCalculation, // For potential future use
    ) {
        // Note: Data loading should happen before calling render
        if self.is_loading {
            // Show loading state
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.spinner();
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Loading progress...")
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
            });
            return;
        }
        
        if let Some(ref error) = self.error_message {
            // Show error state
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.label(egui::RichText::new("❌ Failed to load")
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::RED));
                ui.label(egui::RichText::new(error)
                    .font(egui::FontId::new(10.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
            });
            return;
        }
        
        if self.data_points.is_empty() {
            // Show empty state
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 3.0);
                ui.label(egui::RichText::new("📊 No data yet")
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
                ui.label(egui::RichText::new("Progress will appear as you save!")
                    .font(egui::FontId::new(10.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(120, 120, 120)));
            });
            return;
        }
        
        // Render the actual graph
        self.render_graph(ui, goal);
    }
    
    /// Render the actual egui_plot graph
    fn render_graph(&self, ui: &mut egui::Ui, goal: &DomainGoal) {
        use egui_plot::{Plot, PlotPoints, Line, Points, MarkerShape};
        
        // Separate real data points from projection points
        let real_points: Vec<[f64; 2]> = self.data_points
            .iter()
            .filter(|point| !point.is_projection)
            .map(|point| [point.timestamp, point.balance])
            .collect();
            
        let projection_points: Vec<[f64; 2]> = self.data_points
            .iter()
            .filter(|point| point.is_projection)
            .map(|point| [point.timestamp, point.balance])
            .collect();
        
        // Create lines for different segments
        let pink_color = Color32::from_rgb(200, 120, 200); // Match progress bar color
        let projection_color = Color32::from_rgb(160, 160, 160); // Gray for projection
        
        // Find data range for proper scaling
        let min_balance = self.data_points
            .iter()
            .map(|p| p.balance)
            .fold(f64::INFINITY, f64::min);
        
        let max_balance = self.data_points
            .iter()
            .map(|p| p.balance)
            .fold(f64::NEG_INFINITY, f64::max);
        
        // Include goal target in range calculation and add some padding
        let y_min = (min_balance.min(0.0) * 0.9).min(min_balance - 10.0);
        let y_max = (max_balance.max(goal.target_amount) * 1.1).max(max_balance + 10.0);
        
        // Find goal target point for marker
        let goal_target_point = self.data_points
            .iter()
            .find(|point| point.is_goal_target)
            .map(|point| [point.timestamp, point.balance]);
        
        // Enhanced plot with axis formatting matching the balance chart
        Plot::new("goal_progression_graph")
            .height(ui.available_height()) // Use all available height
            .width(ui.available_width())   // Use all available width
            .show_axes([true, true]) // Show both axes 
            .show_grid([true, true]) // Show both x and y grid lines
            .include_y(y_min)
            .include_y(y_max)
            .allow_boxed_zoom(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show_x(true) // Show x coordinate on hover (same as balance chart)
            .show_y(true) // Show y coordinate on hover (same as balance chart)
            .auto_bounds(egui::Vec2b::TRUE)
            .show_background(false)
            .x_axis_formatter(|mark, _range| {
                // Format x-axis with human readable dates (same as balance chart)
                let timestamp = mark.value as i64;
                if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) {
                    datetime.format("%m/%d").to_string()
                } else {
                    format!("{:.0}", mark.value)
                }
            })
            .y_axis_formatter(|mark, _range| {
                // Format y-axis with dollar signs (same as balance chart)
                let value = mark.value;
                if value.fract() == 0.0 && value >= 0.0 {
                    format!("${:.0}", value)
                } else {
                    format!("${:.2}", value)
                }
            })
            .x_grid_spacer(|input| {
                // Sparser x-axis grid - only show major marks (same as balance chart)
                use egui_plot::GridMark;
                let mut marks = Vec::new();
                let range = input.bounds.1 - input.bounds.0;
                let target_marks = 6; // Aim for about 6 vertical lines
                let step = range / target_marks as f64;
                
                let mut current = input.bounds.0;
                while current <= input.bounds.1 {
                    marks.push(GridMark {
                        value: current,
                        step_size: step,
                    });
                    current += step;
                }
                marks
            })
            .y_grid_spacer(|input| {
                // Clean y-axis intervals (same algorithm as balance chart)
                use egui_plot::GridMark;
                let max_value = input.bounds.1; // Use actual plot bounds only
                let interval_candidate = max_value / 7.0;
                
                // Round to nearest 5
                let clean_interval = ((interval_candidate / 5.0).round() * 5.0).max(5.0); // Minimum $5 interval
                
                let mut marks = Vec::new();
                let mut current = 0.0; // Always start at zero
                
                // Only go up to the actual max, no extra padding here
                while current <= max_value {
                    marks.push(GridMark {
                        value: current,
                        step_size: clean_interval,
                    });
                    current += clean_interval;
                }
                
                marks
            })
            .coordinates_formatter(egui_plot::Corner::LeftBottom, egui_plot::CoordinatesFormatter::new(|point, _bounds| {
                // Convert timestamp to readable date and format balance (same as balance chart)
                if let Some(datetime) = chrono::DateTime::from_timestamp(point.x as i64, 0) {
                    let date = datetime.format("%m/%d").to_string();
                    format!("📅 {}: ${:.2}", date, point.y)
                } else {
                    format!("${:.2}", point.y)
                }
            }))
            .label_formatter(|name, value| {
                // Format tooltips with date and balance (same as balance chart)
                if name == "Balance" {
                    let timestamp = value.x as i64;
                    let balance = value.y;
                    if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) {
                        let date = datetime.format("%m/%d").to_string();
                        format!("{}: ${:.2}", date, balance)
                    } else {
                        format!("${:.2}", balance)
                    }
                } else if name == "Goal" {
                    format!("🎯 Target: ${:.2}", value.y)
                } else {
                    // Return empty string for other elements
                    String::new()
                }
            })
            .show(ui, |plot_ui| {
                
                // Draw solid line connecting all real data points
                if real_points.len() >= 2 {
                    let real_line = Line::new("Progress", PlotPoints::new(real_points.clone()))
                        .color(pink_color)
                        .width(2.0);
                    plot_ui.line(real_line);
                }
                
                // Draw dashed line from last real point to projection point
                if !real_points.is_empty() && !projection_points.is_empty() {
                    let last_real_point = real_points.last().unwrap();
                    let projection_connection: Vec<[f64; 2]> = vec![*last_real_point, projection_points[0]];
                    
                    let projection_line = Line::new("Projection", PlotPoints::new(projection_connection))
                        .color(projection_color)
                        .width(2.0)
                        .style(egui_plot::LineStyle::Dashed { length: 10.0 });
                    plot_ui.line(projection_line);
                }
                
                // Draw interactive data points for tooltips (same as balance chart)
                if !real_points.is_empty() {
                    let data_points = Points::new("Balance", PlotPoints::new(real_points))
                        .color(pink_color)
                        .filled(true)
                        .radius(6.0) // Increased radius for easier hover detection
                        .shape(MarkerShape::Circle); // Name enables tooltips
                    plot_ui.points(data_points);
                }
                
                // Draw goal target marker (gold diamond with tooltip)
                if let Some(target_point) = goal_target_point {
                    let target_points = Points::new("Goal", PlotPoints::new(vec![target_point]))
                        .color(egui::Color32::GOLD)
                        .radius(8.0)
                        .shape(MarkerShape::Diamond); // Name enables tooltips
                    plot_ui.points(target_points);
                }
            });
    }
} 