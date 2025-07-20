//! # Chart State Module
//!
//! This module manages all chart-related state for the allowance tracker app.
//! It handles chart data, period selection, and chart configuration.

use crate::ui::components::chart_renderer::{ChartPeriod, ChartDataPoint};

/// Chart-specific state for balance visualization
#[derive(Debug)]
pub struct ChartState {
    /// Currently selected time period for the chart
    pub selected_period: ChartPeriod,
    
    /// Chart data points for the current period
    pub chart_data: Vec<ChartDataPoint>,
    
    /// Whether chart data is currently loading
    pub is_loading: bool,
    
    /// Error message if chart loading failed
    pub error_message: Option<String>,
}

impl ChartState {
    /// Create new chart state with default values
    pub fn new() -> Self {
        Self {
            selected_period: ChartPeriod::Days30, // Default to 30-day view
            chart_data: Vec::new(),
            is_loading: false,
            error_message: None,
        }
    }
    
    /// Clear chart data and reset loading state
    pub fn clear_data(&mut self) {
        self.chart_data.clear();
        self.is_loading = false;
        self.error_message = None;
    }
    
    /// Set loading state
    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
        if loading {
            self.error_message = None;
        }
    }
    
    /// Set error message and clear loading state
    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
        self.is_loading = false;
    }
    
    /// Set chart data and clear error/loading states
    pub fn set_data(&mut self, data: Vec<ChartDataPoint>) {
        self.chart_data = data;
        self.is_loading = false;
        self.error_message = None;
    }
} 