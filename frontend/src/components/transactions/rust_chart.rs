use yew::prelude::*;
use shared::FormattedTransaction;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use web_sys::HtmlCanvasElement;
use chrono::{DateTime, FixedOffset, Duration};
use crate::services::DateUtils;

#[derive(Debug, Clone, PartialEq)]
pub enum DateRange {
    Last30Days,
    Last90Days,
    LastYear,
}

impl DateRange {
    fn to_days(&self) -> i64 {
        match self {
            DateRange::Last30Days => 30,
            DateRange::Last90Days => 90,
            DateRange::LastYear => 365,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            DateRange::Last30Days => "30 Days",
            DateRange::Last90Days => "90 Days", 
            DateRange::LastYear => "1 Year",
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct RustChartProps {
    pub transactions: Vec<FormattedTransaction>,
    pub loading: bool,
}

pub enum Msg {
    DrawChart,
    SetDateRange(DateRange),
}

pub struct RustChart {
    canvas_ref: NodeRef,
    selected_range: DateRange,
}

impl Component for RustChart {
    type Message = Msg;
    type Properties = RustChartProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            canvas_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days, // Default to 30 days
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::DrawChart => {
                // Pass full transaction list to draw_chart, it will do its own filtering
                self.draw_chart(&ctx.props().transactions);
                false
            }
            Msg::SetDateRange(range) => {
                self.selected_range = range;
                // Redraw chart with new range, pass full transaction list
                self.draw_chart(&ctx.props().transactions);
                true // Need to re-render to update the UI
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        // Redraw chart if transactions changed
        if ctx.props().transactions != old_props.transactions {
            self.draw_chart(&ctx.props().transactions);
        }
        true
    }

    fn rendered(&mut self, ctx: &Context<Self>, _first_render: bool) {
        // Draw chart when component is rendered
        if !ctx.props().transactions.is_empty() {
            self.draw_chart(&ctx.props().transactions);
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let transaction_count = ctx.props().transactions.len();
        let loading = ctx.props().loading;

        // Get callback to handle range changes
        let link = ctx.link();

        html! {
            <div class="rust-chart-container">
                // Beautiful gradient title matching table header with range selectors
                <div class="chart-title-header" style="background: linear-gradient(135deg, #ff9a9e, #f093fb, #c471ed) !important; padding: 1rem 1.5rem !important; margin: 0 !important; border-radius: 10px 10px 0 0 !important; display: flex !important; justify-content: space-between !important; align-items: center !important;">
                    <h3 class="chart-title" style="color: white !important; font-size: 1.1rem !important; font-weight: 600 !important; text-transform: uppercase !important; letter-spacing: 0.5px !important; margin: 0 !important;">{"Daily Balance History"}</h3>
                    
                    // Range selector buttons
                    <div class="chart-range-selector" style="display: flex !important; gap: 8px !important;">
                        {for [DateRange::Last30Days, DateRange::Last90Days, DateRange::LastYear].iter().map(|range| {
                            let is_active = *range == self.selected_range;
                            let range_clone = range.clone();
                            let onclick = link.callback(move |_| Msg::SetDateRange(range_clone.clone()));
                            
                            html! {
                                <button 
                                    class="range-button"
                                    onclick={onclick}
                                    style={format!(
                                        "background: {} !important; color: {} !important; border: 2px solid white !important; padding: 6px 12px !important; border-radius: 6px !important; font-size: 0.8rem !important; font-weight: 500 !important; cursor: pointer !important; transition: all 0.2s ease !important;",
                                        if is_active { "white" } else { "rgba(255, 255, 255, 0.2)" },
                                        if is_active { "#666" } else { "white" }
                                    )}
                                >
                                    {range.label()}
                                </button>
                            }
                        })}
                    </div>
                </div>
                
                {if transaction_count == 0 && loading {
                    html! {
                        <div class="chart-loading">
                            <div class="loading-spinner"></div>
                            <p>{"Loading chart data..."}</p>
                        </div>
                    }
                } else if transaction_count == 0 {
                    html! {
                        <div class="chart-empty">
                            <i class="fas fa-chart-line chart-empty-icon"></i>
                            <p>{"No transaction data available for chart"}</p>
                        </div>
                    }
                } else {
                    html! {
                        <div class="chart-content">
                            <canvas 
                                ref={self.canvas_ref.clone()}
                                class="rust-chart-canvas"
                                width="800"
                                height="350"
                            ></canvas>
                        </div>
                    }
                }}
            </div>
        }
    }
}

impl RustChart {
    fn draw_chart(&self, all_transactions: &[FormattedTransaction]) {
        if all_transactions.is_empty() {
            return;
        }

        let canvas = match self.canvas_ref.cast::<HtmlCanvasElement>() {
            Some(canvas) => canvas,
            None => return,
        };

        // Set canvas size (reduced height since title is now external)
        canvas.set_width(800);
        canvas.set_height(350);

        let backend = match CanvasBackend::with_canvas_object(canvas.clone()) {
            Some(backend) => backend,
            None => return,
        };

        let root = backend.into_drawing_area();
        
        if root.fill(&WHITE).is_err() {
            return;
        }

        // Filter transactions for the selected range
        let filtered_transactions = self.filter_transactions_by_range(all_transactions);
        
        // Find the first transaction date from the ENTIRE dataset (for zero-balance logic)
        let all_sorted_transactions = {
            let mut all_sorted = all_transactions.to_vec();
            all_sorted.sort_by(|a, b| a.raw_date.cmp(&b.raw_date));
            all_sorted
        };
        
        let first_transaction_date = if all_sorted_transactions.is_empty() {
            None
        } else {
            DateUtils::parse_flexible_rfc3339(&all_sorted_transactions[0].raw_date).ok()
        };

        // Group filtered transactions by day and get the latest balance for each day
        use std::collections::HashMap;
        let mut daily_balances: HashMap<String, f64> = HashMap::new();
        let mut sorted_filtered_transactions = filtered_transactions.to_vec();
        
        // Sort filtered transactions chronologically
        sorted_filtered_transactions.sort_by(|a, b| a.raw_date.cmp(&b.raw_date));
        
        // Group by day and take the latest balance for each day
        for tx in &sorted_filtered_transactions {
            if let Ok(dt) = DateUtils::parse_flexible_rfc3339(&tx.raw_date) {
                let date_key = dt.format("%Y-%m-%d").to_string();
                daily_balances.insert(date_key, tx.raw_balance);
            }
        }
        
        // Calculate date range based on selected time period, not transactions
        let now = js_sys::Date::new_0();
        let current_timestamp = now.get_time() / 1000.0;
        let current_date = match chrono::DateTime::from_timestamp(current_timestamp as i64, 0) {
            Some(dt) => dt.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()),
            None => return, // Can't determine current date
        };
        
        // Calculate start date based on selected range
        let days_back = self.selected_range.to_days();
        let start_date = current_date - chrono::Duration::days(days_back);
        let end_date = current_date;
        
        // Use the calculated date range instead of transaction dates
        let first_date = start_date;
        let last_date = end_date;
        
                 // Create chart data for every day in the range
        let mut chart_data: Vec<(DateTime<FixedOffset>, f64)> = Vec::new();
        let mut current_date = first_date.date_naive();
        let end_date = last_date.date_naive();

        
        // Start with 0.0 balance - we'll track balance changes from the beginning of the selected period
        let mut previous_balance = 0.0;
        
        while current_date <= end_date {
            let date_key = current_date.format("%Y-%m-%d").to_string();
            let current_dt = match DateUtils::parse_flexible_rfc3339(&format!("{}T12:00:00Z", date_key)) {
                Ok(dt) => dt,
                Err(_) => {
                    current_date += chrono::Duration::days(1);
                    continue;
                }
            };
            
            let balance = if let Some(&day_balance) = daily_balances.get(&date_key) {
                // This day has transactions, use the ending balance from that day
                previous_balance = day_balance;
                day_balance
            } else if let Some(first_tx_date) = first_transaction_date {
                // Check if this date is before the first transaction ever
                if current_dt < first_tx_date {
                    0.0 // Show zero before first transaction
                } else {
                    // After first transaction but no transactions on this day, carry forward previous balance
                    previous_balance
                }
            } else {
                // No transactions at all, show zero
                0.0
            };
            
            // Create a consistent timestamp for each day (noon)
            let day_timestamp = format!("{}T12:00:00Z", date_key);
            if let Ok(dt) = DateUtils::parse_flexible_rfc3339(&day_timestamp) {
                chart_data.push((dt, balance));
            }
            
            current_date += chrono::Duration::days(1);
        }

        // Sort by date (should already be sorted, but just to be sure)
        chart_data.sort_by_key(|&(dt, _)| dt);

        // Find data bounds
        let min_date = chart_data.first().unwrap().0;
        let max_date = chart_data.last().unwrap().0;
        let min_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::INFINITY, f64::min);
        let max_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::NEG_INFINITY, f64::max);

        // Add some padding, start from 0 for better balance visualization
        let balance_range = (max_balance - min_balance).max(1.0); // Ensure minimum range
        let padding = balance_range * 0.1;
        let y_min = 0.0_f64.min(min_balance - padding); // Start from 0 or lower if needed
        let y_max = max_balance + padding;

        // Create chart with no title (title is now rendered as HTML)
        let mut chart = match ChartBuilder::on(&root)
            .margin(15)
            .x_label_area_size(45)
            .y_label_area_size(70)
            .build_cartesian_2d(min_date..max_date, y_min..y_max)
        {
            Ok(chart) => chart,
            Err(_) => return,
        };

        // Configure mesh with cleaner, more subtle grid
        if chart
            .configure_mesh()
            .y_desc("Balance ($)")
            .x_desc("Date")
            .y_label_formatter(&|v| format!("${:.2}", v))
            .x_label_formatter(&|v| v.format("%m/%d").to_string())
            .label_style(("sans-serif", 12, &RGBColor(102, 126, 234))) // App's primary color
            .axis_style(&RGBColor(230, 230, 230)) // Subtle gray axes
            .bold_line_style(&RGBColor(245, 245, 245)) // Very light grid lines
            .light_line_style(&RGBColor(250, 250, 250)) // Even lighter secondary grid
            .x_labels(6) // Reduce x-axis label density
            .y_labels(8) // Reduce y-axis label density
            .draw()
            .is_err()
        {
            return;
        }

        // Draw horizontal line at y=0 for better reference
        if chart
            .draw_series(std::iter::once(PathElement::new(
                vec![(min_date, 0.0), (max_date, 0.0)],
                RGBColor(200, 200, 200).stroke_width(1)
            )))
            .is_err()
        {
            return;
        }

        // Draw line chart with connected points for daily balances
        let line_color = RGBColor(102, 126, 234); // App's primary blue color
        let point_color = RGBColor(102, 126, 234); // Same color for consistency
        
        // Draw the main line connecting all points
        if chart
            .draw_series(LineSeries::new(
                chart_data.iter().map(|&(date, balance)| (date, balance)),
                line_color.stroke_width(3)
            ))
            .is_err()
        {
            return;
        }

        // Draw individual points at each data point
        for (date, balance) in chart_data.iter() {
            // Draw main point
            if chart
                .draw_series(std::iter::once(Circle::new((*date, *balance), 4, point_color.filled())))
                .is_err()
            {
                continue;
            }
            
            // Draw white border around point for better visibility
            if chart
                .draw_series(std::iter::once(Circle::new((*date, *balance), 4, WHITE.stroke_width(2))))
                .is_err()
            {
                continue;
            }
        }

        // Present the result
        let _ = root.present();
    }

    fn filter_transactions_by_range(&self, transactions: &[FormattedTransaction]) -> Vec<FormattedTransaction> {
        if transactions.is_empty() {
            return vec![];
        }

        // Get the current date and calculate the cutoff date
        let now = js_sys::Date::new_0();
        let current_timestamp = now.get_time() / 1000.0; // Convert to seconds
        
        // Convert to chrono DateTime for easier date arithmetic
        let current_date = match chrono::DateTime::from_timestamp(current_timestamp as i64, 0) {
            Some(dt) => dt.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()),
            None => return transactions.to_vec(), // Fallback to all transactions if date parsing fails
        };

        let cutoff_days = self.selected_range.to_days();
        let cutoff_date = current_date - Duration::days(cutoff_days);

        // Filter transactions based on the selected range
        transactions
            .iter()
            .filter(|tx| {
                if let Ok(tx_date) = DateUtils::parse_flexible_rfc3339(&tx.raw_date) {
                    tx_date >= cutoff_date
                } else {
                    false // Exclude transactions with invalid dates
                }
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::FormattedTransaction;

    // Basic compilation test - ensures the component can be created
    #[test]
    fn test_rust_chart_compiles() {
        let chart = RustChart {
            canvas_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        // If this compiles, our basic structure is sound
        assert!(std::mem::size_of::<RustChart>() >= std::mem::size_of::<NodeRef>());
    }

    // Test that we can create chart props without issues
    #[test]
    fn test_rust_chart_props_creation() {
        let transactions = vec![];
        let props = RustChartProps {
            transactions,
            loading: false,
        };
        assert!(!props.loading);
        assert_eq!(props.transactions.len(), 0);
    }

    // Test chart data preparation logic
    #[test]
    fn test_chart_data_preparation() {
        let transactions = vec![
            FormattedTransaction {
                id: "test1".to_string(),
                formatted_date: "Jun 15, 2025".to_string(),
                raw_date: "2025-06-15T00:00:00-05:00".to_string(),
                description: "Test transaction".to_string(),
                formatted_amount: "+$25.00".to_string(),
                raw_amount: 25.0,
                formatted_balance: "$25.00".to_string(),
                raw_balance: 25.0,
                amount_type: shared::AmountType::Positive,
            }
        ];

        // Test that our date parsing logic works
        use crate::services::date_utils::DateUtils;
        let result = DateUtils::parse_flexible_rfc3339(&transactions[0].raw_date);
        assert!(result.is_ok(), "Date parsing should succeed for valid RFC 3339 dates");
    }

    // Test that we can call the main chart drawing method without panicking
    #[test]
    fn test_draw_chart_with_empty_transactions() {
        let chart = RustChart {
            canvas_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        let transactions = vec![];
        
        // This should not panic even with empty transactions
        chart.draw_chart(&transactions);
        // If we get here without panicking, the method handles empty data correctly
    }

    // Test that chart handles invalid date formats gracefully
    #[test]
    fn test_chart_with_invalid_dates() {
        let chart = RustChart {
            canvas_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        let transactions = vec![
            FormattedTransaction {
                id: "test1".to_string(),
                formatted_date: "Invalid Date".to_string(),
                raw_date: "invalid-date-format".to_string(),
                description: "Test transaction".to_string(),
                formatted_amount: "+$25.00".to_string(),
                raw_amount: 25.0,
                formatted_balance: "$25.00".to_string(),
                raw_balance: 25.0,
                amount_type: shared::AmountType::Positive,
            }
        ];
        
        // This should not panic even with invalid dates
        chart.draw_chart(&transactions);
        // If we get here, the method handles invalid dates gracefully
    }

    // Test the specific plotters color creation that was causing issues
    #[test]
    fn test_plotters_color_creation() {
        use plotters::prelude::*;
        
        // Test that we can create colors and stroke styles without lifetime issues
        let line_color = RGBColor(102, 126, 234);
        let line_style = line_color.stroke_width(3);
        
        // Test creating color in a closure (similar to legend code)
        let _closure_test = || {
            RGBColor(102, 126, 234).stroke_width(2)
        };
        
        // If this compiles, our color handling approach is sound
        assert_eq!(line_color.0, 102);
        assert_eq!(line_color.1, 126);
        assert_eq!(line_color.2, 234);
    }
}

// Integration tests that require wasm-bindgen-test
#[cfg(test)]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_component_creation_in_wasm() {
        // Test that the component can be created in a WASM environment
        let chart = RustChart {
            canvas_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        let transactions = vec![];
        chart.draw_chart(&transactions);
        
        // If this runs without panicking in WASM, our component is sound
    }
} 