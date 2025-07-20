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
use crate::backend::domain::commands::transactions::TransactionListQuery;
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
        
        // Create the line connecting all points (no name = no tooltip)
        let line_points: PlotPoints = raw_points.iter().copied().collect();
        let line = Line::new(line_points)
            .color(egui::Color32::from_rgb(100, 150, 255))
            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 150, 255)));
        
        // Create individual data point markers (with name for tooltips)
        let marker_points: PlotPoints = raw_points.iter().copied().collect();
        let data_points = Points::new(marker_points)
            .color(egui::Color32::from_rgb(100, 150, 255))
            .filled(true)
            .radius(6.0) // Increased radius for easier hover detection
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
            .show_x(true) // Show x coordinate on hover  
            .show_y(true) // Show y coordinate on hover
            .auto_bounds(egui::Vec2b::TRUE)
            .show_background(false)
            .coordinates_formatter(egui_plot::Corner::LeftBottom, egui_plot::CoordinatesFormatter::new(|point, _bounds| {
                // Convert timestamp to readable date and format balance
                if let Some(datetime) = chrono::DateTime::from_timestamp(point.x as i64, 0) {
                    let date = datetime.format("%m/%d").to_string();
                    format!("📅 {}: ${:.2}", date, point.y)
                } else {
                    format!("${:.2}", point.y)
                }
            }))
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
        
        // For now, let's see if the built-in coordinate display works better
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
        
        info!("📊 Chart date range: {} to {} ({} days)", start_date, end_date, (end_date - start_date).num_days());
        
        // Fetch transactions directly from backend for the specified date range
        // Convert dates to RFC3339 format for backend query
        let start_date_str = start_date.and_hms_opt(0, 0, 0).unwrap().and_utc().to_rfc3339();
        let end_date_str = end_date.and_hms_opt(23, 59, 59).unwrap().and_utc().to_rfc3339();
        
        let query = TransactionListQuery {
            after: None,
            limit: Some(10000), // Get all transactions in the date range
            start_date: Some(start_date_str.clone()),
            end_date: Some(end_date_str.clone()),
        };
        
        info!("📊 Fetching transactions from backend with query: start_date={}, end_date={}, limit=10000", start_date_str, end_date_str);
        
        match self.backend().transaction_service.list_transactions_domain(query) {
            Ok(result) => {
                info!("📊 Successfully loaded {} transactions from backend for chart", result.transactions.len());
                
                // Convert domain transactions to DTO format for chart processing
                let dto_transactions: Vec<shared::Transaction> = result.transactions
                    .into_iter()
                    .map(|tx| shared::Transaction {
                        id: tx.id,
                        child_id: tx.child_id,
                        amount: tx.amount,
                        balance: tx.balance,
                        transaction_type: match tx.transaction_type {
                            crate::backend::domain::models::transaction::TransactionType::Income => shared::TransactionType::Income,
                            crate::backend::domain::models::transaction::TransactionType::Expense => shared::TransactionType::Expense,
                            crate::backend::domain::models::transaction::TransactionType::FutureAllowance => shared::TransactionType::FutureAllowance,
                        },
                        description: tx.description,
                        date: tx.date,
                    })
                    .collect();
                
                info!("📊 Converted to {} DTO transactions for chart", dto_transactions.len());
                
                // Convert to references for prepare_chart_data compatibility
                let transaction_refs: Vec<&shared::Transaction> = dto_transactions.iter().collect();
                
                // Prepare chart data points
                self.chart.chart_data = self.prepare_chart_data(&transaction_refs, start_date, end_date);
                
                info!("📊 Generated {} chart data points", self.chart.chart_data.len());
            }
            Err(e) => {
                warn!("❌ Failed to load chart data from backend: {}", e);
            }
        }
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
                // Weekly balances with intelligent sampling
                let mut sample_dates = Vec::new();
                let mut current_date = start_date;
                
                // Generate weekly sample dates
                while current_date <= end_date {
                    sample_dates.push(current_date);
                    current_date += Duration::days(7);
                }
                
                // CRITICAL: Always include the end date as final sample if not already included
                if let Some(&last_sample) = sample_dates.last() {
                    if last_sample < end_date {
                        sample_dates.push(end_date);
                    }
                }
                
                for &sample_date in &sample_dates {
                    // Find the last transaction on or before this date
                    let mut latest_balance = 0.0;
                    
                    for tx in &sorted_transactions {
                        let tx_date = tx.date.date_naive();
                        if tx_date <= sample_date {
                            latest_balance = tx.balance;
                        } else {
                            break; // Since transactions are sorted by date
                        }
                    }
                    
                    let timestamp = sample_date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp() as f64;
                    data_points.push(ChartDataPoint {
                        date: sample_date,
                        balance: latest_balance,
                        timestamp,
                    });
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
            ChartPeriod::AllTime => {
                // For true "All Time", get the earliest transaction date from backend
                match self.get_earliest_transaction_date() {
                    Some(earliest_date) => earliest_date,
                    None => today - Duration::days(365), // Fallback if no transactions
                }
            }
        };
        
        (start_date, today)
    }
    
    /// Get the earliest transaction date from the backend
    fn get_earliest_transaction_date(&self) -> Option<NaiveDate> {
        // Query backend for ALL transactions (no date filter) to find the earliest
        let query = TransactionListQuery {
            after: None,
            limit: Some(10000), // Get all transactions
            start_date: None, // No start date filter
            end_date: None,   // No end date filter
        };
        
        match self.backend().transaction_service.list_transactions_domain(query) {
            Ok(result) => {
                if result.transactions.is_empty() {
                    return None;
                }
                
                // Find the earliest transaction date
                let earliest_tx = result.transactions
                    .iter()
                    .min_by_key(|tx| tx.date)?;
                
                let earliest_date = earliest_tx.date.date_naive();
                Some(earliest_date)
            }
            Err(e) => {
                warn!("Failed to get earliest transaction date: {}", e);
                None
            }
        }
    }
} 