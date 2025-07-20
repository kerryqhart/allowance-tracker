//! # Chart Renderer Module
//!
//! This module handles the balance chart visualization for the allowance tracker app.
//! It provides a line chart showing balance over time with tooltips and responsive design.
//!
//! ## Key Functions:
//! - `draw_chart_section()` - Main chart view with data loading and error handling
//! - `render_balance_chart()` - Render the actual plot using egui::plot
//! - `prepare_chart_data()` - Transform transaction data into chart points
//! - `get_date_range_for_period()` - Calculate date ranges for different time periods
//!
//! ## Purpose:
//! This module provides a visual representation of balance changes over time,
//! helping kids understand their spending and saving patterns through an intuitive graph.

use eframe::egui;
use chrono::{NaiveDate, Duration};
use shared::Transaction;
use crate::ui::app_state::AllowanceTrackerApp;
use log::{info, warn};

/// Time period options for the chart
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartPeriod {
    Days30,
    Days90,
    AllTime,
}

impl ChartPeriod {
    pub fn label(&self) -> &'static str {
        match self {
            ChartPeriod::Days30 => "30 Days",
            ChartPeriod::Days90 => "90 Days", 
            ChartPeriod::AllTime => "1 Year",
        }
    }
    
    pub fn button_text(&self) -> &'static str {
        match self {
            ChartPeriod::Days30 => "30 Days",
            ChartPeriod::Days90 => "90 Days",
            ChartPeriod::AllTime => "All Time",
        }
    }
}

/// Data point for the balance chart
#[derive(Debug, Clone)]
pub struct ChartDataPoint {
    pub date: NaiveDate,
    pub balance: f64,
    pub timestamp: f64, // Unix timestamp for plotting
}

impl AllowanceTrackerApp {
    /// Draw the chart section with header and chart content
    pub fn draw_chart_section(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect) {
        info!("📊 CHART: draw_chart_section called with rect height={:.0}", available_rect.height());
        
        // Calculate content area (accounting for card margins)
        let content_margin = 20.0;
        let content_rect = egui::Rect::from_min_size(
            available_rect.min + egui::vec2(content_margin, content_margin),
            available_rect.size() - egui::vec2(content_margin * 2.0, content_margin * 2.0)
        );
        
        // Draw card background
        self.draw_card_background(ui, content_rect);
        
        // Chart content area - use full available space (no internal header)
        let chart_rect = egui::Rect::from_min_size(
            content_rect.min + egui::vec2(60.0, 10.0), // Small top margin
            egui::vec2(content_rect.width() - 80.0, content_rect.height() - 20.0) // Use almost full height
        );
        
        ui.allocate_ui_at_rect(chart_rect, |ui| {
            if let Some(ref _child) = self.current_child() {
                if self.chart.chart_data.is_empty() {
                    // Show loading state
                    ui.vertical_centered(|ui| {
                        ui.add_space(chart_rect.height() / 3.0);
                        ui.label(egui::RichText::new("Loading chart data...")
                            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(120, 120, 120)));
                    });
                    
                    // Load data on first render
                    self.load_chart_data();
                } else {
                    // Render the actual chart
                    self.render_balance_chart(ui);
                }
            } else {
                // No child selected
                ui.vertical_centered(|ui| {
                    ui.add_space(chart_rect.height() / 3.0);
                    ui.label(egui::RichText::new("Select a child to view their balance chart")
                        .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(120, 120, 120)));
                });
            }
        });
    }
    
    /// Render the actual balance chart using egui plotting
    pub fn render_balance_chart(&mut self, ui: &mut egui::Ui) {
        use egui_plot::{Plot, PlotPoints, Line, Points, MarkerShape, GridMark};
        
        if self.chart.chart_data.is_empty() {
            return;
        }
        
        // Create raw point data that can be reused
        let raw_points: Vec<[f64; 2]> = self.chart.chart_data
            .iter()
            .map(|point| [point.timestamp, point.balance])
            .collect();
        
        // Create the line connecting all points
        let line_points: PlotPoints = raw_points.iter().copied().collect();
        let line = Line::new(line_points)
            .color(egui::Color32::from_rgb(100, 150, 255))
            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 150, 255)))
            .name("Balance");
        
        // Create individual data point markers
        let marker_points: PlotPoints = raw_points.iter().copied().collect();
        let data_points = Points::new(marker_points)
            .color(egui::Color32::from_rgb(100, 150, 255))
            .filled(true)
            .radius(4.0)
            .shape(MarkerShape::Circle)
            .name("Balance");
        
        // Find the maximum balance for setting upper bound
        let max_balance = self.chart.chart_data
            .iter()
            .map(|point| point.balance)
            .fold(0.0, f64::max);
        
        // Add some padding above the maximum (10% or at least $5)
        let y_max = (max_balance * 1.1).max(max_balance + 5.0);
        
        Plot::new("balance_chart")
            .show_axes([true, true])
            .show_grid([true, true])
            .include_y(0.0) // Always include zero
            .include_y(y_max) // Include padded maximum
            .allow_boxed_zoom(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .show_x(false) // Remove x-axis crosshair
            .show_y(false) // Remove y-axis crosshair
            .x_grid_spacer(|input| {
                // Sparser x-axis grid - only show major marks
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
                // Clean y-axis intervals using the algorithm:
                // 1. Take max value from the actual plot bounds (not arbitrary minimum)
                // 2. Divide by 7 
                // 3. Round to nearest 5
                // 4. Use that as interval
                
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
            .label_formatter(|name, value| {
                // Only show tooltips for Balance data points
                if name == "Balance" {
                    // Format tooltip with date and balance
                    let timestamp = value.x as i64;
                    let balance = value.y;
                    if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) {
                        let date = datetime.format("%m/%d").to_string();
                        format!("{}: ${:.2}", date, balance)
                    } else {
                        format!("${:.2}", balance)
                    }
                } else {
                    // Return empty string for all other elements to prevent stray text
                    String::new()
                }
            })
            .x_axis_formatter(|mark, _range| {
                // Format x-axis with human readable dates
                let timestamp = mark.value as i64;
                if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp, 0) {
                    datetime.format("%m/%d").to_string()
                } else {
                    format!("{:.0}", mark.value)
                }
            })
            .y_axis_formatter(|mark, _range| {
                // Format y-axis with dollar signs
                let value = mark.value;
                if value.fract() == 0.0 && value >= 0.0 {
                    format!("${:.0}", value)
                } else {
                    format!("${:.2}", value)
                }
            })
            .show(ui, |plot_ui| {
                plot_ui.line(line);
                plot_ui.points(data_points);
            });
    }
    
    /// Load chart data for the selected period and child
    pub fn load_chart_data(&mut self) {
        let Some(ref child) = self.current_child() else {
            warn!("📊 No child selected for chart data loading");
            return;
        };
        
        info!("📊 Loading chart data for child: {} (period: {:?})", child.name, self.chart.selected_period);
        
        // Calculate date range based on selected period
        let (start_date, end_date) = self.get_date_range_for_period(self.chart.selected_period);
        
        info!("📊 Chart date range: {} to {}", start_date, end_date);
        
        // For now, we'll use a simple approach: get all transactions and filter by date
        // In a real implementation, you'd want to call a backend service with date filters
        
        // Get transactions from the table state (which should have recent transactions)
        let all_transactions = &self.table.displayed_transactions;
        
        // Filter transactions within the date range
        let filtered_transactions: Vec<&Transaction> = all_transactions
            .iter()
            .filter(|tx| {
                let tx_date = tx.date.date_naive();
                tx_date >= start_date && tx_date <= end_date
            })
            .collect();
        
        info!("📊 Found {} transactions in date range", filtered_transactions.len());
        
        // Prepare chart data points
        self.chart.chart_data = self.prepare_chart_data(&filtered_transactions, start_date, end_date);
        
        info!("📊 Generated {} chart data points", self.chart.chart_data.len());
    }
    
    /// Prepare chart data points from transactions
    pub fn prepare_chart_data(&self, transactions: &[&Transaction], start_date: NaiveDate, end_date: NaiveDate) -> Vec<ChartDataPoint> {
        let mut data_points = Vec::new();
        
        // Sort transactions by date
        let mut sorted_transactions: Vec<&Transaction> = transactions.iter().copied().collect();
        sorted_transactions.sort_by(|a, b| a.date.cmp(&b.date));
        
        // For 30-day view: show daily balances (last balance of each day)
        // For 90-day view: show weekly balances (last balance of each week)  
        // For all-time view: show monthly balances (last balance of each month)
        
        match self.chart.selected_period {
            ChartPeriod::Days30 => {
                // Daily balances
                let mut current_date = start_date;
                let mut running_balance = 0.0;
                let mut tx_index = 0;
                
                while current_date <= end_date {
                    // Find all transactions for this day
                    let mut day_final_balance = running_balance;
                    
                    while tx_index < sorted_transactions.len() {
                        let tx = sorted_transactions[tx_index];
                        let tx_date = tx.date.date_naive();
                        
                        if tx_date == current_date {
                            day_final_balance = tx.balance;
                            tx_index += 1;
                        } else if tx_date > current_date {
                            break;
                        } else {
                            tx_index += 1;
                        }
                    }
                    
                    // Add data point for this day
                    let timestamp = current_date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp() as f64;
                    data_points.push(ChartDataPoint {
                        date: current_date,
                        balance: day_final_balance,
                        timestamp,
                    });
                    
                    running_balance = day_final_balance;
                    current_date += Duration::days(1);
                }
            }
            ChartPeriod::Days90 => {
                // Weekly balances (simplified - just sample every 7 days)
                let mut current_date = start_date;
                
                while current_date <= end_date {
                    // Find the last transaction on or before this date
                    let mut latest_balance = 0.0;
                    
                    for tx in &sorted_transactions {
                        let tx_date = tx.date.date_naive();
                        if tx_date <= current_date {
                            latest_balance = tx.balance;
                        } else {
                            break;
                        }
                    }
                    
                    let timestamp = current_date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp() as f64;
                    data_points.push(ChartDataPoint {
                        date: current_date,
                        balance: latest_balance,
                        timestamp,
                    });
                    
                    current_date += Duration::days(7);
                }
            }
            ChartPeriod::AllTime => {
                // Monthly balances (simplified - just sample every 30 days)
                let mut current_date = start_date;
                
                while current_date <= end_date {
                    // Find the last transaction on or before this date
                    let mut latest_balance = 0.0;
                    
                    for tx in &sorted_transactions {
                        let tx_date = tx.date.date_naive();
                        if tx_date <= current_date {
                            latest_balance = tx.balance;
                        } else {
                            break;
                        }
                    }
                    
                    let timestamp = current_date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp() as f64;
                    data_points.push(ChartDataPoint {
                        date: current_date,
                        balance: latest_balance,
                        timestamp,
                    });
                    
                    current_date += Duration::days(30);
                }
            }
        }
        
        data_points
    }
    
    /// Get date range for the specified chart period
    pub fn get_date_range_for_period(&self, period: ChartPeriod) -> (NaiveDate, NaiveDate) {
        let today = chrono::Local::now().date_naive();
        
        let start_date = match period {
            ChartPeriod::Days30 => today - Duration::days(30),
            ChartPeriod::Days90 => today - Duration::days(90),
            ChartPeriod::AllTime => today - Duration::days(365), // 1 year
        };
        
        (start_date, today)
    }
} 