//! Domain model for an allowance configuration.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllowanceConfig {
    pub child_id: String,
    pub amount: f64,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub is_active: bool,
    pub created_at: String, // RFC 3339 timestamp
    pub updated_at: String, // RFC 3339 timestamp
}

impl AllowanceConfig {
    /// Get the day name for the configured day of week
    pub fn day_name(&self) -> &'static str {
        match self.day_of_week {
            0 => "Sunday",
            1 => "Monday",
            2 => "Tuesday",
            3 => "Wednesday",
            4 => "Thursday",
            5 => "Friday",
            6 => "Saturday",
            _ => "Invalid",
        }
    }
    
    /// Validate day of week value
    pub fn is_valid_day_of_week(day: u8) -> bool {
        day <= 6
    }
} 