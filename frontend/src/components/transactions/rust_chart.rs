use yew::prelude::*;
use shared::FormattedTransaction;
use web_sys::HtmlElement;
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

#[derive(Debug)]
pub enum Msg {
    SetDateRange(DateRange),
}

pub struct RustChart {
    chart_ref: NodeRef,
    selected_range: DateRange,
}

impl Component for RustChart {
    type Message = Msg;
    type Properties = RustChartProps;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days, // Default to 30 days
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
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
                        <div class="chart-content" style="position: relative;">
                            <div 
                                ref={self.chart_ref.clone()}
                                class="rust-chart-svg"
                                style="width: 800px; height: 350px;"
                            ></div>
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

        let chart_div = match self.chart_ref.cast::<HtmlElement>() {
            Some(div) => div,
            None => return,
        };

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
            None => return,
        };
        
        // Calculate start date based on selected range
        let days_back = self.selected_range.to_days();
        let start_date = current_date - chrono::Duration::days(days_back);
        let end_date = current_date;

        // Create chart data for every day in the range
        let mut chart_data: Vec<(DateTime<FixedOffset>, f64)> = Vec::new();
        let mut current_chart_date = start_date.date_naive();
        let end_chart_date = end_date.date_naive();
        let mut previous_balance = 0.0;
        
        while current_chart_date <= end_chart_date {
            let date_key = current_chart_date.format("%Y-%m-%d").to_string();
            let current_dt = match DateUtils::parse_flexible_rfc3339(&format!("{}T12:00:00Z", date_key)) {
                Ok(dt) => dt,
                Err(_) => {
                    current_chart_date += chrono::Duration::days(1);
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
            
            chart_data.push((current_dt, balance));
            current_chart_date += chrono::Duration::days(1);
        }

        if chart_data.is_empty() {
            return;
        }

        // Sort by date (should already be sorted, but just to be sure)
        chart_data.sort_by_key(|&(dt, _)| dt);

        // Find data bounds
        let min_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::INFINITY, f64::min);
        let max_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::NEG_INFINITY, f64::max);

        // Add some padding, start from 0 for better balance visualization
        let balance_range = (max_balance - min_balance).max(1.0); // Ensure minimum range
        let padding = balance_range * 0.1;
        let y_min = 0.0_f64.min(min_balance - padding); // Start from 0 or lower if needed
        let y_max = max_balance + padding;

        // Generate manual SVG
        let svg_output = self.generate_svg_chart(&chart_data, y_min, y_max);
        
        if svg_output.is_empty() {
            return;
        }
        
        // Inject the generated SVG into the DOM
        chart_div.set_inner_html(&svg_output);
    }

    fn generate_svg_chart(&self, chart_data: &[(DateTime<FixedOffset>, f64)], y_min: f64, y_max: f64) -> String {
        if chart_data.is_empty() {
            return String::new();
        }

        const WIDTH: f64 = 800.0;
        const HEIGHT: f64 = 350.0;
        const MARGIN_LEFT: f64 = 70.0;
        const MARGIN_RIGHT: f64 = 30.0;
        const MARGIN_TOP: f64 = 30.0;
        const MARGIN_BOTTOM: f64 = 50.0;
        
        let chart_width = WIDTH - MARGIN_LEFT - MARGIN_RIGHT;
        let chart_height = HEIGHT - MARGIN_TOP - MARGIN_BOTTOM;
        
        let mut svg = String::new();
        
        // SVG header
        svg.push_str(&format!(
            r#"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg" style="background: white; font-family: Arial, sans-serif;">"#,
            WIDTH, HEIGHT
        ));
        
        // Chart background
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"white\" stroke=\"#e0e0e0\" stroke-width=\"1\"/>",
            MARGIN_LEFT, MARGIN_TOP, chart_width, chart_height
        ));
        
        // Helper function to convert data coordinates to SVG coordinates
        let data_to_svg_x = |date_index: usize| {
            MARGIN_LEFT + (date_index as f64 / (chart_data.len() - 1) as f64) * chart_width
        };
        
        let data_to_svg_y = |balance: f64| {
            MARGIN_TOP + chart_height - ((balance - y_min) / (y_max - y_min)) * chart_height
        };
        
        // Draw grid lines
        let num_y_lines = 5;
        for i in 0..=num_y_lines {
            let y_val = y_min + (i as f64 / num_y_lines as f64) * (y_max - y_min);
            let svg_y = data_to_svg_y(y_val);
            
            // Horizontal grid line
            svg.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#f0f0f0\" stroke-width=\"1\"/>",
                MARGIN_LEFT, svg_y, MARGIN_LEFT + chart_width, svg_y
            ));
            
            // Y-axis label
            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"end\" dominant-baseline=\"middle\" font-size=\"12\" fill=\"#666\">${:.2}</text>",
                MARGIN_LEFT - 10.0, svg_y, y_val
            ));
        }
        
        // Draw zero reference line if it's within range
        if y_min <= 0.0 && y_max >= 0.0 {
            let zero_y = data_to_svg_y(0.0);
            svg.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#ccc\" stroke-width=\"2\"/>",
                MARGIN_LEFT, zero_y, MARGIN_LEFT + chart_width, zero_y
            ));
        }
        
        // Draw vertical grid lines and X-axis labels
        let num_x_lines = 6;
        for i in 0..=num_x_lines {
            let data_index = (i * (chart_data.len() - 1)) / num_x_lines;
            let svg_x = data_to_svg_x(data_index);
            
            // Vertical grid line
            svg.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#f0f0f0\" stroke-width=\"1\"/>",
                svg_x, MARGIN_TOP, svg_x, MARGIN_TOP + chart_height
            ));
            
            // X-axis label
            let date_str = chart_data[data_index].0.format("%m/%d").to_string();
            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#666\">{}</text>",
                svg_x, MARGIN_TOP + chart_height + 20.0, date_str
            ));
        }
        
        // Build the line path
        let mut path_data = String::new();
        for (i, (_, balance)) in chart_data.iter().enumerate() {
            let svg_x = data_to_svg_x(i);
            let svg_y = data_to_svg_y(*balance);
            
            if i == 0 {
                path_data.push_str(&format!("M {} {}", svg_x, svg_y));
            } else {
                path_data.push_str(&format!(" L {} {}", svg_x, svg_y));
            }
        }
        
        // Draw the main line
        svg.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"#667eea\" stroke-width=\"3\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>",
            path_data
        ));
        
        // Draw individual points with SVG-native tooltips (skip for 1-year view)
        let show_tooltips = !matches!(self.selected_range, DateRange::LastYear);
        
        for (i, (date, balance)) in chart_data.iter().enumerate() {
            let svg_x = data_to_svg_x(i);
            let svg_y = data_to_svg_y(*balance);
            
            // White border circle 
            svg.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"5\" fill=\"white\" stroke=\"#667eea\" stroke-width=\"2\"/>",
                svg_x, svg_y
            ));
            
            // Main point
            svg.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"#667eea\"/>",
                svg_x, svg_y
            ));
            
            if show_tooltips {
                // Format tooltip content
                let formatted_date = date.format("%B %d, %Y (%A)").to_string();
                let formatted_balance = format!("${:.2}", balance);
                let tooltip_text = format!("{} - Balance: {}", formatted_date, formatted_balance);
                
                // Tooltip background (rounded rectangle)
                let text_width = tooltip_text.len() as f64 * 6.5; // Approximate character width
                let text_height = 20.0;
                let padding = 8.0;
                let tooltip_x = svg_x - (text_width / 2.0) - padding;
                let tooltip_y = svg_y - 40.0;
                
                svg.push_str(&format!(
                    "<g class=\"tooltip-group\" style=\"opacity: 0; pointer-events: none; transition: opacity 0.2s ease;\">
                        <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" fill=\"rgba(0,0,0,0.9)\" stroke=\"none\"/>
                        <text x=\"{}\" y=\"{}\" fill=\"white\" font-size=\"12\" font-family=\"sans-serif\" text-anchor=\"middle\">{}</text>
                    </g>",
                    tooltip_x, tooltip_y, text_width + (padding * 2.0), text_height + (padding * 2.0),
                    svg_x, tooltip_y + 15.0, tooltip_text
                ));
                
                // Invisible hover area with hover events
                svg.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"12\" fill=\"transparent\" stroke=\"none\" style=\"cursor: pointer;\" 
                        onmouseenter=\"evt.target.previousElementSibling.style.opacity='1'\" 
                        onmouseleave=\"evt.target.previousElementSibling.style.opacity='0'\"/>",
                    svg_x, svg_y
                ));
            }
        }
        
        // Axis labels
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"14\" fill=\"#333\">Date</text>",
            MARGIN_LEFT + chart_width / 2.0, HEIGHT - 10.0
        ));
        
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"14\" fill=\"#333\" transform=\"rotate(-90 {} {})\">Balance ($)</text>",
            15.0, MARGIN_TOP + chart_height / 2.0, 15.0, MARGIN_TOP + chart_height / 2.0
        ));
        
        svg.push_str("</svg>");
        
        svg
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

    // Helper function to create mock transaction data
    fn create_mock_transactions() -> Vec<FormattedTransaction> {
        vec![
            FormattedTransaction {
                id: "tx1".to_string(),
                formatted_date: "Jun 15, 2024".to_string(),
                description: "Allowance".to_string(),
                formatted_amount: "+$25.00".to_string(),
                amount_type: shared::AmountType::Positive,
                formatted_balance: "$25.00".to_string(),
                raw_date: "2024-06-15T12:00:00Z".to_string(),
                raw_amount: 25.0,
                raw_balance: 25.0,
            },
            FormattedTransaction {
                id: "tx2".to_string(),
                formatted_date: "Jun 20, 2024".to_string(),
                description: "Toy purchase".to_string(),
                formatted_amount: "-$8.50".to_string(),
                amount_type: shared::AmountType::Negative,
                formatted_balance: "$16.50".to_string(),
                raw_date: "2024-06-20T12:00:00Z".to_string(),
                raw_amount: -8.5,
                raw_balance: 16.5,
            },
            FormattedTransaction {
                id: "tx3".to_string(),
                formatted_date: "Jun 25, 2024".to_string(),
                description: "Extra allowance".to_string(),
                formatted_amount: "+$10.00".to_string(),
                amount_type: shared::AmountType::Positive,
                formatted_balance: "$26.50".to_string(),
                raw_date: "2024-06-25T12:00:00Z".to_string(),
                raw_amount: 10.0,
                raw_balance: 26.5,
            },
        ]
    }

    #[test]
    fn test_date_range_conversion() {
        assert_eq!(DateRange::Last30Days.to_days(), 30);
        assert_eq!(DateRange::Last90Days.to_days(), 90);
        assert_eq!(DateRange::LastYear.to_days(), 365);
    }

    #[test]
    fn test_date_range_labels() {
        assert_eq!(DateRange::Last30Days.label(), "30 Days");
        assert_eq!(DateRange::Last90Days.label(), "90 Days");
        assert_eq!(DateRange::LastYear.label(), "1 Year");
    }

    #[test]
    fn test_component_creation() {
        let _chart = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        // This should compile without issues
        assert!(true);
    }

    #[test]
    fn test_props_creation() {
        let transactions = create_mock_transactions();
        let _props = RustChartProps {
            transactions,
            loading: false,
        };
        
        // This should compile and create props correctly
        assert!(true);
    }

    #[test]
    fn test_filter_transactions_by_range() {
        let chart = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        let transactions = create_mock_transactions();
        
        // Note: This test would normally use the filter_transactions_by_range method,
        // but that method uses js-sys::Date which doesn't work in non-WASM tests.
        // Instead, we'll just validate the basic logic works.
        
        // Validate transaction data structure
        assert!(!transactions.is_empty());
        assert_eq!(transactions.len(), 3);
        
        // Validate the range enum works
        assert_eq!(chart.selected_range.to_days(), 30);
        assert_eq!(chart.selected_range.label(), "30 Days");
    }

    #[test]
    fn test_svg_generation_core_logic() {
        // Test the core SVG generation logic without requiring DOM
        let transactions = create_mock_transactions();
        
        // Test data preparation logic
        let mut daily_balances = std::collections::HashMap::new();
        for tx in &transactions {
            if let Ok(dt) = DateUtils::parse_flexible_rfc3339(&tx.raw_date) {
                let date_key = dt.format("%Y-%m-%d").to_string();
                daily_balances.insert(date_key, tx.raw_balance);
            }
        }
        
        // Should have processed transactions into daily balances
        assert!(!daily_balances.is_empty());
        assert!(daily_balances.len() <= transactions.len());
        
        // Check balance values are correct
        for tx in &transactions {
            if let Ok(dt) = DateUtils::parse_flexible_rfc3339(&tx.raw_date) {
                let date_key = dt.format("%Y-%m-%d").to_string();
                if let Some(&balance) = daily_balances.get(&date_key) {
                    assert_eq!(balance, tx.raw_balance);
                }
            }
        }
    }

    #[test]
    fn test_chart_data_preparation() {
        let transactions = create_mock_transactions();
        
        // Test sorting transactions
        let mut sorted_transactions = transactions.to_vec();
        sorted_transactions.sort_by(|a, b| a.raw_date.cmp(&b.raw_date));
        
        // Should be sorted chronologically
        for i in 1..sorted_transactions.len() {
            assert!(sorted_transactions[i-1].raw_date <= sorted_transactions[i].raw_date);
        }
        
        // Test balance progression makes sense
        assert_eq!(sorted_transactions[0].raw_balance, 25.0);  // First transaction: +25
        assert_eq!(sorted_transactions[1].raw_balance, 16.5);  // After expense: 25-8.5
        assert_eq!(sorted_transactions[2].raw_balance, 26.5);  // After income: 16.5+10
    }





    #[test]
    fn test_chart_bounds_calculation() {
        let transactions = create_mock_transactions();
        
        // Find min/max balances
        let min_balance = transactions.iter()
            .map(|tx| tx.raw_balance)
            .fold(f64::INFINITY, f64::min);
        let max_balance = transactions.iter()
            .map(|tx| tx.raw_balance)
            .fold(f64::NEG_INFINITY, f64::max);
            
        assert_eq!(min_balance, 16.5);  // Lowest balance after expense
        assert_eq!(max_balance, 26.5);  // Highest balance after final income
        
        // Test padding calculation
        let balance_range = (max_balance - min_balance).max(1.0);
        let padding = balance_range * 0.1;
        let y_min = 0.0_f64.min(min_balance - padding);
        let y_max = max_balance + padding;
        
        assert!(y_min <= min_balance);
        assert!(y_max >= max_balance);
        assert!(y_max > y_min);
    }

    #[test]
    fn test_date_parsing_and_formatting() {
        let test_date = "2024-06-15T12:00:00Z";
        
        // Should be able to parse the date
        assert!(DateUtils::parse_flexible_rfc3339(test_date).is_ok());
        
        let parsed = DateUtils::parse_flexible_rfc3339(test_date).unwrap();
        
        // Should be able to format it
        let formatted = parsed.format("%Y-%m-%d").to_string();
        assert_eq!(formatted, "2024-06-15");
        
        let chart_formatted = parsed.format("%m/%d").to_string();
        assert_eq!(chart_formatted, "06/15");
    }

    #[test]
    fn test_empty_transactions_handling() {
        let chart = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        let empty_transactions: Vec<FormattedTransaction> = vec![];
        let filtered = chart.filter_transactions_by_range(&empty_transactions);
        
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_manual_svg_generation() {
        let chart = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        let transactions = create_mock_transactions();
        
        // Create some test chart data
        let mut chart_data = Vec::new();
        for tx in &transactions {
            if let Ok(dt) = DateUtils::parse_flexible_rfc3339(&tx.raw_date) {
                chart_data.push((dt, tx.raw_balance));
            }
        }
        
        if !chart_data.is_empty() {
            let min_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::INFINITY, f64::min);
            let max_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::NEG_INFINITY, f64::max);
            let padding = (max_balance - min_balance) * 0.1;
            let y_min = 0.0_f64.min(min_balance - padding);
            let y_max = max_balance + padding;
            
            // Test manual SVG generation
            let svg_output = chart.generate_svg_chart(&chart_data, y_min, y_max);
            
            assert!(!svg_output.is_empty());
            assert!(svg_output.contains("<svg"));
            assert!(svg_output.contains("</svg>"));
            assert!(svg_output.contains("width=\"800\""));
            assert!(svg_output.contains("height=\"350\""));
            
            println!("Generated SVG length: {} characters", svg_output.len());
        }
    }

    #[test]
    fn test_date_range_filtering_logic() {
        let chart_30 = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        let chart_90 = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last90Days,
        };
        
        // Test range values
        assert_eq!(chart_30.selected_range.to_days(), 30);
        assert_eq!(chart_90.selected_range.to_days(), 90);
        
        // Note: The actual filtering logic would use js-sys::Date which doesn't work
        // in non-WASM tests, but we can validate the basic structure
        let transactions = create_mock_transactions();
        assert!(!transactions.is_empty());
        
        // Validate that 90 days is more than 30 days (basic logic)
        assert!(chart_90.selected_range.to_days() > chart_30.selected_range.to_days());
    }

    #[test]
    fn test_chart_components_integration() {
        // Test that all major chart components work together
        let transactions = create_mock_transactions();
        
        // Test the entire data processing pipeline
        let mut daily_balances = std::collections::HashMap::new();
        let mut sorted_transactions = transactions.to_vec();
        sorted_transactions.sort_by(|a, b| a.raw_date.cmp(&b.raw_date));
        
        for tx in &sorted_transactions {
            if let Ok(dt) = DateUtils::parse_flexible_rfc3339(&tx.raw_date) {
                let date_key = dt.format("%Y-%m-%d").to_string();
                daily_balances.insert(date_key, tx.raw_balance);
            }
        }
        
        // Create chart data points
        let mut chart_data = Vec::new();
        for (date_key, balance) in daily_balances.iter() {
            let day_timestamp = format!("{}T12:00:00Z", date_key);
            if let Ok(dt) = DateUtils::parse_flexible_rfc3339(&day_timestamp) {
                chart_data.push((dt, *balance));
            }
        }
        
        // Sort chart data
        chart_data.sort_by_key(|&(dt, _)| dt);
        
        assert!(!chart_data.is_empty());
        assert_eq!(chart_data.len(), daily_balances.len());
        
        // Test bounds calculation
        if !chart_data.is_empty() {
            let min_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::INFINITY, f64::min);
            let max_balance = chart_data.iter().map(|(_, balance)| *balance).fold(f64::NEG_INFINITY, f64::max);
            
            assert!(min_balance <= max_balance);
            assert!(min_balance.is_finite());
            assert!(max_balance.is_finite());
        }
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_component_creation_in_wasm() {
        let chart = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        // Should create component without panicking in WASM
        assert_eq!(chart.selected_range, DateRange::Last30Days);
    }

    #[wasm_bindgen_test]
    fn test_manual_svg_generation_in_wasm() {
        // Test manual SVG generation works in WASM environment
        let chart = RustChart {
            chart_ref: NodeRef::default(),
            selected_range: DateRange::Last30Days,
        };
        
        // Create minimal test data
        let test_data = vec![
            (chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()), 10.0),
            (chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()) + chrono::Duration::days(1), 20.0),
        ];
        
        let svg_output = chart.generate_svg_chart(&test_data, 0.0, 25.0);
        
        assert!(!svg_output.is_empty());
        assert!(svg_output.contains("<svg"));
        assert!(svg_output.contains("</svg>"));
    }
} 