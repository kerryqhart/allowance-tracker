//! Domain model for parental control attempts
//!
//! This module contains the domain representation of parental control attempts.
//! Unlike the shared DTO, this model can contain domain-specific business logic
//! and validation rules.

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Domain model for a parental control attempt
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParentalControlAttempt {
    /// Unique identifier for the attempt
    pub id: i64,
    /// The value that was attempted for validation
    pub attempted_value: String,
    /// When the attempt was made (RFC 3339 timestamp)
    pub timestamp: String,
    /// Whether the attempt was successful
    pub success: bool,
}

impl ParentalControlAttempt {
    /// Create a new parental control attempt
    pub fn new(id: i64, attempted_value: String, timestamp: String, success: bool) -> Self {
        Self {
            id,
            attempted_value,
            timestamp,
            success,
        }
    }
    
    /// Generate a new timestamp for the current moment
    pub fn generate_current_timestamp() -> String {
        Utc::now().format("%Y-%m-%dT%H:%M:%S%.fZ").to_string()
    }
    
    /// Check if the attempt was successful
    pub fn is_successful(&self) -> bool {
        self.success
    }
    
    /// Get the attempted value (sanitized for logging)
    pub fn get_sanitized_attempted_value(&self) -> String {
        // Don't log the full attempted value for security
        if self.attempted_value.len() > 3 {
            format!("{}...", &self.attempted_value[..3])
        } else {
            "***".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_attempt() {
        let attempt = ParentalControlAttempt::new(
            1,
            "test_value".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            true,
        );
        
        assert_eq!(attempt.id, 1);
        assert_eq!(attempt.attempted_value, "test_value");
        assert_eq!(attempt.timestamp, "2025-01-01T00:00:00Z");
        assert!(attempt.success);
    }
    
    #[test]
    fn test_is_successful() {
        let success_attempt = ParentalControlAttempt::new(
            1,
            "correct".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            true,
        );
        let failed_attempt = ParentalControlAttempt::new(
            2,
            "wrong".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            false,
        );
        
        assert!(success_attempt.is_successful());
        assert!(!failed_attempt.is_successful());
    }
    
    #[test]
    fn test_sanitized_attempted_value() {
        let long_attempt = ParentalControlAttempt::new(
            1,
            "verylongpassword".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            false,
        );
        let short_attempt = ParentalControlAttempt::new(
            2,
            "ab".to_string(),
            "2025-01-01T00:00:00Z".to_string(),
            false,
        );
        
        assert_eq!(long_attempt.get_sanitized_attempted_value(), "ver...");
        assert_eq!(short_attempt.get_sanitized_attempted_value(), "***");
    }
    
    #[test]
    fn test_generate_current_timestamp() {
        let timestamp = ParentalControlAttempt::generate_current_timestamp();
        // Just verify it's a valid RFC 3339 timestamp
        assert!(timestamp.contains("T"));
        assert!(timestamp.contains("Z"));
    }
} 