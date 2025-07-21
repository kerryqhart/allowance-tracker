//! # Goal Progress Graph Data Preparation
//!
//! This module handles filtering and preparing transaction data specifically for goal progress graphs.
//! It reuses the chart infrastructure but applies goal-specific filtering and optimization.

use chrono::NaiveDate;
use shared::Transaction;
use crate::backend::domain::models::goal::DomainGoal;

/// Data point for the goal progress graph (similar to ChartDataPoint but goal-specific)
#[derive(Debug, Clone)]
pub struct GoalGraphDataPoint {
    pub date: NaiveDate,
    pub balance: f64,
    pub timestamp: f64, // Unix timestamp for plotting
    pub is_goal_start: bool, // Mark the goal creation point
    pub is_goal_target: bool, // Mark the goal target projection point
    pub is_projection: bool, // Mark if this is a projected (dashed line) point
}

impl GoalGraphDataPoint {
    /// Create a new goal graph data point
    pub fn new(date: NaiveDate, balance: f64, is_goal_start: bool) -> Self {
        let timestamp = date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp() as f64;
        Self {
            date,
            balance,
            timestamp,
            is_goal_start,
            is_goal_target: false,
            is_projection: false,
        }
    }
    
    /// Create a goal target projection point
    pub fn new_goal_target(date: NaiveDate, target_amount: f64) -> Self {
        let timestamp = date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp() as f64;
        Self {
            date,
            balance: target_amount,
            timestamp,
            is_goal_start: false,
            is_goal_target: true,
            is_projection: true,
        }
    }
    
    /// Create a projection point (dashed line)
    pub fn new_projection(date: NaiveDate, balance: f64) -> Self {
        let timestamp = date.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp() as f64;
        Self {
            date,
            balance,
            timestamp,
            is_goal_start: false,
            is_goal_target: false,
            is_projection: true,
        }
    }
}

/// Configuration for goal graph data preparation
#[derive(Debug, Clone)]
pub struct GoalGraphConfig {
    pub max_data_points: usize,  // Limit for performance in smaller graph space
    pub include_pre_goal_context: bool, // Whether to show a few days before goal creation
    pub pre_goal_context_days: i64, // Days of context before goal creation
}

impl Default for GoalGraphConfig {
    fn default() -> Self {
        Self {
            max_data_points: 50, // Smaller limit for compact graph
            include_pre_goal_context: true,
            pre_goal_context_days: 7, // Show 1 week before goal creation for context
        }
    }
}

/// Prepare goal progression graph: historical transactions since goal creation + future allowances
/// Uses domain APIs to get complete progression data with proper balance calculations
pub fn prepare_goal_progression_data(
    _transactions: &[&Transaction], // Not used - getting fresh data from domain
    goal: &DomainGoal,
    _config: &GoalGraphConfig,
) -> Vec<GoalGraphDataPoint> {
    // This function now acts as a simple converter from domain transactions to UI data points
    // The actual business logic is handled by the domain layer
    
    // Note: The UI should call the domain API directly and pass the results here
    // This is a placeholder implementation that the UI layer will replace with domain calls
    log::warn!("prepare_goal_progression_data called with old signature - UI should use domain APIs directly");
    
    let mut data_points = Vec::new();
    
    // Fallback implementation for now - just show current state
    let today = chrono::Local::now().date_naive();
    data_points.push(GoalGraphDataPoint::new(today, 0.0, false));
    data_points.push(GoalGraphDataPoint::new_goal_target(today + chrono::Duration::days(30), goal.target_amount));
    
    data_points
}



/// Prepare goal-specific graph data from transactions (legacy function - kept for compatibility)
pub fn prepare_goal_graph_data(
    transactions: &[&Transaction],
    goal: &DomainGoal,
    config: &GoalGraphConfig,
) -> Vec<GoalGraphDataPoint> {
    let mut data_points = Vec::new();
    
    // Parse goal creation date
    let goal_creation_date = match chrono::DateTime::parse_from_rfc3339(&goal.created_at) {
        Ok(datetime) => datetime.date_naive(),
        Err(e) => {
            log::warn!("Failed to parse goal creation date: {}", e);
            return data_points; // Return empty if we can't parse the date
        }
    };
    
    // Calculate date range for filtering
    let start_date = if config.include_pre_goal_context {
        goal_creation_date - chrono::Duration::days(config.pre_goal_context_days)
    } else {
        goal_creation_date
    };
    
    let end_date = chrono::Local::now().date_naive();
    
    // Filter transactions to the goal timeframe
    let mut filtered_transactions: Vec<&Transaction> = transactions
        .iter()
        .filter(|tx| {
            let tx_date = tx.date.date_naive();
            tx_date >= start_date && tx_date <= end_date
        })
        .copied()
        .collect();
    
    // Sort by date
    filtered_transactions.sort_by(|a, b| a.date.cmp(&b.date));
    
    if filtered_transactions.is_empty() {
        // Create a single point at goal creation with zero balance
        data_points.push(GoalGraphDataPoint::new(goal_creation_date, 0.0, true));
        return data_points;
    }
    
    // Determine sampling strategy based on data volume and timeframe
    let total_days = (end_date - start_date).num_days() as usize;
    
    if total_days <= 30 || filtered_transactions.len() <= config.max_data_points {
        // Daily sampling - use all available data
        create_daily_goal_data_points(
            &filtered_transactions,
            start_date,
            end_date,
            goal_creation_date,
            &mut data_points,
        );
    } else if total_days <= 90 {
        // Weekly sampling for medium-term goals
        create_weekly_goal_data_points(
            &filtered_transactions,
            start_date,
            end_date,
            goal_creation_date,
            &mut data_points,
        );
    } else {
        // Monthly sampling for long-term goals
        create_monthly_goal_data_points(
            &filtered_transactions,
            start_date,
            end_date,
            goal_creation_date,
            &mut data_points,
        );
    }
    
    data_points
}

/// Create daily data points for short-term goals
fn create_daily_goal_data_points(
    transactions: &[&Transaction],
    start_date: NaiveDate,
    end_date: NaiveDate,
    goal_creation_date: NaiveDate,
    data_points: &mut Vec<GoalGraphDataPoint>,
) {
    let mut current_date = start_date;
    let mut running_balance = 0.0;
    let mut tx_index = 0;
    
    while current_date <= end_date {
        // Find all transactions for this day
        let mut day_final_balance = running_balance;
        
        while tx_index < transactions.len() {
            let tx = transactions[tx_index];
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
        
        // Mark if this is the goal creation date
        let is_goal_start = current_date == goal_creation_date;
        
        // Add data point for this day
        data_points.push(GoalGraphDataPoint::new(current_date, day_final_balance, is_goal_start));
        
        running_balance = day_final_balance;
        current_date += chrono::Duration::days(1);
    }
}

/// Create weekly data points for medium-term goals
fn create_weekly_goal_data_points(
    transactions: &[&Transaction],
    start_date: NaiveDate,
    end_date: NaiveDate,
    goal_creation_date: NaiveDate,
    data_points: &mut Vec<GoalGraphDataPoint>,
) {
    let mut sample_dates = Vec::new();
    let mut current_date = start_date;
    
    // Always include the goal creation date
    if !sample_dates.contains(&goal_creation_date) {
        sample_dates.push(goal_creation_date);
    }
    
    // Generate weekly sample dates
    while current_date <= end_date {
        if !sample_dates.contains(&current_date) {
            sample_dates.push(current_date);
        }
        current_date += chrono::Duration::days(7);
    }
    
    // Always include the end date
    if !sample_dates.contains(&end_date) {
        sample_dates.push(end_date);
    }
    
    // Sort sample dates
    sample_dates.sort();
    
    for &sample_date in &sample_dates {
        let mut latest_balance = 0.0;
        
        // Find the last transaction on or before this date
        for tx in transactions {
            let tx_date = tx.date.date_naive();
            if tx_date <= sample_date {
                latest_balance = tx.balance;
            } else {
                break; // Since transactions are sorted by date
            }
        }
        
        let is_goal_start = sample_date == goal_creation_date;
        data_points.push(GoalGraphDataPoint::new(sample_date, latest_balance, is_goal_start));
    }
}

/// Create monthly data points for long-term goals
fn create_monthly_goal_data_points(
    transactions: &[&Transaction],
    start_date: NaiveDate,
    end_date: NaiveDate,
    goal_creation_date: NaiveDate,
    data_points: &mut Vec<GoalGraphDataPoint>,
) {
    let mut sample_dates = Vec::new();
    let mut current_date = start_date;
    
    // Always include the goal creation date
    sample_dates.push(goal_creation_date);
    
    // Generate monthly sample dates (every 30 days)
    while current_date <= end_date {
        if !sample_dates.contains(&current_date) {
            sample_dates.push(current_date);
        }
        current_date += chrono::Duration::days(30);
    }
    
    // Always include the end date
    if !sample_dates.contains(&end_date) {
        sample_dates.push(end_date);
    }
    
    // Sort sample dates
    sample_dates.sort();
    
    for &sample_date in &sample_dates {
        let mut latest_balance = 0.0;
        
        // Find the last transaction on or before this date
        for tx in transactions {
            let tx_date = tx.date.date_naive();
            if tx_date <= sample_date {
                latest_balance = tx.balance;
            } else {
                break;
            }
        }
        
        let is_goal_start = sample_date == goal_creation_date;
        data_points.push(GoalGraphDataPoint::new(sample_date, latest_balance, is_goal_start));
    }
} 

/// Convert domain transactions to goal graph data points using daily balance calculation
/// Uses the same logic as the calendar to ensure proper same-day transaction ordering
/// Goal creation date determines the starting point for the graph
pub fn convert_domain_transactions_to_data_points(
    transactions: &[crate::backend::domain::models::transaction::Transaction],
    goal: &DomainGoal,
    goal_creation_balance: f64,
) -> Vec<GoalGraphDataPoint> {
    let mut data_points = Vec::new();
    
    // Parse goal creation date
    let goal_creation_date = match chrono::DateTime::parse_from_rfc3339(&goal.created_at) {
        Ok(datetime) => datetime.date_naive(),
        Err(e) => {
            log::error!("Failed to parse goal creation date: {}", e);
            return data_points;
        }
    };
    
    log::info!("Converting {} domain transactions to data points", transactions.len());
    log::info!("Goal created on: {} with balance: ${:.2}", goal_creation_date, goal_creation_balance);
    
    // Add the starting point: balance at goal creation date
    data_points.push(GoalGraphDataPoint::new(goal_creation_date, goal_creation_balance, true));
    
    let today = chrono::Local::now().date_naive();
    
    // Group transactions by date to ensure we use the FINAL balance of each day
    use std::collections::HashMap;
    let mut transactions_by_date: HashMap<chrono::NaiveDate, Vec<&crate::backend::domain::models::transaction::Transaction>> = HashMap::new();
    
    for transaction in transactions {
        let tx_date = transaction.date.date_naive();
        transactions_by_date
            .entry(tx_date)
            .or_insert_with(Vec::new)
            .push(transaction);
    }
    
    // Process each date and use the final balance of the day (like calendar does)
    let mut sorted_dates: Vec<chrono::NaiveDate> = transactions_by_date.keys().copied().collect();
    sorted_dates.sort();
    
    for date in sorted_dates {
        let day_transactions = transactions_by_date.get(&date).unwrap();
        
        // Sort transactions within the day by full timestamp (critical for same-day ordering)
        let mut sorted_day_transactions = day_transactions.clone();
        sorted_day_transactions.sort_by(|a, b| a.date.cmp(&b.date));
        
        // Use the FINAL transaction's balance for this day (last transaction after sorting)
        if let Some(final_transaction) = sorted_day_transactions.last() {
            let is_future = date > today;
            let is_goal_target = final_transaction.balance >= goal.target_amount;
            
            if is_future {
                // Future allowance - use balance calculated by domain layer (BalanceService)
                let mut data_point = GoalGraphDataPoint::new_projection(date, final_transaction.balance);
                if is_goal_target {
                    data_point.is_goal_target = true;
                }
                data_points.push(data_point);
                
                log::info!("  Day {}: final balance ${:.2} (future: {}, goal_target: {}) from {} transactions", 
                           date, final_transaction.balance, is_future, is_goal_target, sorted_day_transactions.len());
            } else {
                // Historical transaction - use final balance from domain layer
                data_points.push(GoalGraphDataPoint::new(date, final_transaction.balance, false));
                
                log::info!("  Day {}: final balance ${:.2} (historical, goal_target: {}) from {} transactions", 
                           date, final_transaction.balance, is_goal_target, sorted_day_transactions.len());
            }
        }
    }
    
    // If goal hasn't been reached in the projection, add explicit goal target point
    let goal_reached = data_points.iter().any(|point| point.is_goal_target);
    if !goal_reached {
        // Find the last future allowance and estimate when goal will be reached
        if let Some(last_future_point) = data_points.iter().filter(|p| p.is_projection).last() {
            if last_future_point.balance < goal.target_amount {
                // Add explicit goal target point some time after last allowance
                let target_date = last_future_point.date + chrono::Duration::days(7); // Estimate
                data_points.push(GoalGraphDataPoint::new_goal_target(target_date, goal.target_amount));
                log::info!("  Added explicit goal target: {} - ${:.2}", target_date, goal.target_amount);
            }
        }
    }
    
    // Sort by date for proper chronological order
    data_points.sort_by(|a, b| a.date.cmp(&b.date));
    
    log::info!("Generated {} total data points for goal graph", data_points.len());
    
    // Debug final data points
    log::info!("🎯 DATA CONVERSION DEBUG: Final data points:");
    for (i, point) in data_points.iter().enumerate() {
        log::info!("  Final point {}: {} - ${:.2} (goal_start: {}, goal_target: {}, projection: {})", 
                   i, point.date, point.balance, point.is_goal_start, point.is_goal_target, point.is_projection);
    }
    
    data_points
} 