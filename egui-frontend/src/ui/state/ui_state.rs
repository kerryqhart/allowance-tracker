//! # UI State Module
//!
//! This module contains general UI state that affects the overall user experience
//! but is not specific to any particular component.
//!
//! ## Responsibilities:
//! - Loading states
//! - User feedback messages (error only)
//! - General UI status indicators
//!
//! ## Purpose:
//! This separates general UI concerns from business logic and component-specific state,
//! making it easier to manage user feedback and loading states consistently.

/// General UI state for loading indicators and user feedback
#[derive(Debug, Default)]
pub struct UIState {
    /// Whether the app is currently loading
    pub loading: bool,
    
    /// Error message to display to the user
    pub error_message: Option<String>,
}

impl UIState {
    /// Create new UI state with default values
    pub fn new() -> Self {
        Self {
            loading: true, // Start with loading=true during app initialization
            error_message: None,
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
} 