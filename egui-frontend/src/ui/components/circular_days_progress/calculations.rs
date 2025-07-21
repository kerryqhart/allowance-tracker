//! # Circular Days Progress Calculations
//!
//! This module handles all calculations related to goal timeline progress including
//! days passed, days remaining, and progress percentage computation.

use chrono::NaiveDate;
use crate::backend::domain::models::goal::DomainGoal;
use shared::GoalCalculation;

/// Days progress data structure
#[derive(Debug, Clone)]
pub struct DaysProgress {
    /// Goal creation date
    pub goal_start_date: NaiveDate,
    /// Projected completion date (if available)
    pub projected_completion_date: Option<NaiveDate>,
    /// Current date
    pub current_date: NaiveDate,
    /// Number of days since goal creation
    pub days_passed: i64,
    /// Number of days remaining (if completion date available)
    pub days_remaining: Option<i64>,
    /// Total timeline days (if completion date available)
    pub total_days: Option<i64>,
    /// Progress percentage (0.0 to 1.0)
    pub progress_percentage: f32,
    /// Whether the goal timeline is determined or estimated
    pub is_timeline_determined: bool,
}

impl DaysProgress {
    /// Create a new DaysProgress with calculated values
    pub fn new(
        goal_start_date: NaiveDate,
        projected_completion_date: Option<NaiveDate>,
        current_date: NaiveDate,
    ) -> Self {
        let days_passed = (current_date - goal_start_date).num_days();
        
        let (days_remaining, total_days, progress_percentage, is_timeline_determined) = 
            if let Some(completion_date) = projected_completion_date {
                if completion_date <= current_date {
                    // Goal completion date has passed
                    (Some(0), Some(days_passed), 1.0, true)
                } else {
                    let remaining = (completion_date - current_date).num_days();
                    let total = (completion_date - goal_start_date).num_days();
                    let progress = if total > 0 {
                        (days_passed as f32) / (total as f32)
                    } else {
                        0.0
                    };
                    (Some(remaining), Some(total), progress, true)
                }
            } else {
                // No completion date - use estimated progress based on typical goal timeframes
                let estimated_progress = estimate_progress_without_completion_date(days_passed);
                (None, None, estimated_progress, false)
            };
        
        Self {
            goal_start_date,
            projected_completion_date,
            current_date,
            days_passed,
            days_remaining,
            total_days,
            progress_percentage: progress_percentage.clamp(0.0, 1.0),
            is_timeline_determined,
        }
    }
    
    /// Get display text for the center of the circular progress
    pub fn center_text(&self) -> String {
        if let Some(remaining) = self.days_remaining {
            if let Some(total) = self.total_days {
                if remaining <= 0 {
                    "Complete!".to_string()
                } else {
                    format!("{} of {} days", self.days_passed, total)
                }
            } else {
                format!("{} days passed", self.days_passed)
            }
        } else {
            // No completion date available
            format!("{} days\nsaving", self.days_passed)
        }
    }
    
    /// Get secondary text for additional context
    pub fn secondary_text(&self) -> Option<String> {
        if let Some(remaining) = self.days_remaining {
            if remaining > 0 {
                Some(format!("{} left", remaining))
            } else {
                None
            }
        } else {
            Some("Keep going!".to_string())
        }
    }
    
    /// Check if the goal is completed based on timeline
    pub fn is_timeline_complete(&self) -> bool {
        self.days_remaining.map_or(false, |remaining| remaining <= 0)
    }
    
    /// Get color suggestion for the progress arc
    pub fn progress_color(&self) -> egui::Color32 {
        // Use consistent pink color to match the progress bar
        egui::Color32::from_rgb(200, 120, 200) // Pink matching progress bar
    }
    
    /// Get background color for the unfilled portion
    pub fn background_color(&self) -> egui::Color32 {
        egui::Color32::from_rgb(240, 240, 240)
    }
}

/// Calculate days progress from goal and calculation data
pub fn calculate_days_progress(
    goal: &DomainGoal,
    goal_calculation: Option<&GoalCalculation>,
) -> Result<DaysProgress, String> {
    // Parse goal creation date
    let goal_start_date = chrono::DateTime::parse_from_rfc3339(&goal.created_at)
        .map_err(|e| format!("Invalid goal creation date: {}", e))?
        .date_naive();
    
    // Get current date
    let current_date = chrono::Local::now().date_naive();
    
    // Parse projected completion date if available
    let projected_completion_date = if let Some(calc) = goal_calculation {
        if let Some(ref completion_date_str) = calc.projected_completion_date {
            match chrono::DateTime::parse_from_rfc3339(completion_date_str) {
                Ok(datetime) => Some(datetime.date_naive()),
                Err(e) => {
                    log::warn!("Failed to parse projected completion date: {}", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };
    
    Ok(DaysProgress::new(goal_start_date, projected_completion_date, current_date))
}

/// Estimate progress percentage when no completion date is available
/// This provides a visual progression that feels natural for ongoing goals
fn estimate_progress_without_completion_date(days_passed: i64) -> f32 {
    // Use a logarithmic-style curve that starts fast then slows down
    // This gives users a sense of progress while avoiding false completion expectations
    
    if days_passed <= 0 {
        return 0.0;
    }
    
    // Define progression milestones
    let milestones = [
        (7, 0.1),    // 1 week = 10%
        (14, 0.2),   // 2 weeks = 20%
        (30, 0.35),  // 1 month = 35%
        (60, 0.5),   // 2 months = 50%
        (90, 0.65),  // 3 months = 65%
        (180, 0.8),  // 6 months = 80%
        (365, 0.9),  // 1 year = 90%
    ];
    
    // Find the appropriate milestone
    for (day_threshold, progress) in milestones.iter() {
        if days_passed <= *day_threshold {
            // Interpolate between previous milestone and current
            let prev_milestone = milestones.iter()
                .rev()
                .find(|(day, _)| *day < days_passed)
                .unwrap_or(&(0, 0.0));
            
            let day_range = day_threshold - prev_milestone.0;
            let progress_range = progress - prev_milestone.1;
            let day_position = days_passed - prev_milestone.0;
            
            if day_range > 0 {
                return prev_milestone.1 + (progress_range * day_position as f32 / day_range as f32);
            } else {
                return prev_milestone.1;
            }
        }
    }
    
    // For goals longer than a year, cap at 95% to avoid suggesting completion
    0.95
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_estimate_progress_without_completion_date() {
        assert_eq!(estimate_progress_without_completion_date(0), 0.0);
        assert_eq!(estimate_progress_without_completion_date(7), 0.1);
        assert_eq!(estimate_progress_without_completion_date(30), 0.35);
        assert_eq!(estimate_progress_without_completion_date(365), 0.9);
        assert_eq!(estimate_progress_without_completion_date(1000), 0.95);
    }
    
    #[test]
    fn test_days_progress_with_completion_date() {
        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let completion_date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(); // 30 days later
        let current_date = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap(); // 15 days in
        
        let progress = DaysProgress::new(start_date, Some(completion_date), current_date);
        
        assert_eq!(progress.days_passed, 15);
        assert_eq!(progress.days_remaining, Some(15));
        assert_eq!(progress.total_days, Some(30));
        assert_eq!(progress.progress_percentage, 0.5);
        assert!(progress.is_timeline_determined);
    }
    
    #[test]
    fn test_days_progress_without_completion_date() {
        let start_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(); // 7 days later
        
        let progress = DaysProgress::new(start_date, None, current_date);
        
        assert_eq!(progress.days_passed, 7);
        assert_eq!(progress.days_remaining, None);
        assert_eq!(progress.total_days, None);
        assert_eq!(progress.progress_percentage, 0.1); // 1 week = 10%
        assert!(!progress.is_timeline_determined);
    }
} 