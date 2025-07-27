//! # UI State Module
//!
//! This module contains general UI state that affects the overall user experience
//! but is not specific to any particular component.
//!
//! ## Responsibilities:
//! - Loading states
//! - User feedback messages (error only)
//! - General UI status indicators
//! - Refresh timing for periodic operations
//!
//! ## Purpose:
//! This separates general UI concerns from business logic and component-specific state,
//! making it easier to manage user feedback and loading states consistently.

use std::time::{Duration, Instant};

/// General UI state for loading indicators and user feedback
#[derive(Debug, Default)]
pub struct UIState {
    /// Whether the app is currently loading
    pub loading: bool,
    
    /// Error message to display to the user
    pub error_message: Option<String>,
    
    /// Last time allowance refresh was performed
    pub last_allowance_refresh: Option<Instant>,
    
    /// How often to check for pending allowances (default: 5 minutes)
    pub allowance_refresh_interval: Duration,
}

impl UIState {
    /// Create new UI state with default values
    pub fn new() -> Self {
        Self {
            loading: true, // Start with loading=true during app initialization
            error_message: None,
            last_allowance_refresh: None,
            allowance_refresh_interval: Duration::from_secs(60), // 1 minute (temporarily for testing)
        }
    }
    
    /// Clear any error messages
    pub fn clear_messages(&mut self) {
        self.error_message = None;
    }
    
    /// Set an error message
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
    }
    
    /// Check if it's time to refresh allowances
    pub fn should_refresh_allowances(&self) -> bool {
        match self.last_allowance_refresh {
            Some(last_refresh) => {
                let now = Instant::now();
                now.duration_since(last_refresh) >= self.allowance_refresh_interval
            }
            None => true, // First time, should refresh
        }
    }
    
    /// Mark that allowances were just refreshed
    pub fn mark_allowance_refresh(&mut self) {
        self.last_allowance_refresh = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_should_refresh_allowances() {
        let mut ui_state = UIState::new();
        
        // Initially should refresh (no previous refresh)
        assert!(ui_state.should_refresh_allowances());
        
        // Mark as just refreshed
        ui_state.mark_allowance_refresh();
        
        // Should not refresh immediately after
        assert!(!ui_state.should_refresh_allowances());
        
        // Wait a bit and test timing
        std::thread::sleep(Duration::from_millis(10));
        
        // Still should not refresh (interval is 5 minutes)
        assert!(!ui_state.should_refresh_allowances());
    }
    
    #[test]
    fn test_refresh_interval_configuration() {
        let mut ui_state = UIState::new();
        
        // Set a very short interval for testing
        ui_state.allowance_refresh_interval = Duration::from_millis(50);
        
        // Initially should refresh
        assert!(ui_state.should_refresh_allowances());
        
        // Mark as refreshed
        ui_state.mark_allowance_refresh();
        
        // Should not refresh immediately
        assert!(!ui_state.should_refresh_allowances());
        
        // Wait longer than the interval
        std::thread::sleep(Duration::from_millis(100));
        
        // Should refresh after interval has passed
        assert!(ui_state.should_refresh_allowances());
    }
} 