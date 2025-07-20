//! # Goal State Module
//!
//! This module contains all state related to goal management and display.
//!
//! ## Responsibilities:
//! - Current goal display state
//! - Goal creation form state
//! - Goal loading and error states
//! - Goal calculation data
//!
//! ## Purpose:
//! This centralizes all goal-related state management, making it easier to
//! coordinate goal behavior and track goal progress.

use shared::GoalCalculation;
use crate::backend::domain::models::goal::DomainGoal;

/// Goal-specific state for the goal tab
#[derive(Debug)]
pub struct GoalUiState {
    /// Current active goal (if any)
    pub current_goal: Option<DomainGoal>,
    
    /// Goal calculation data (progress, completion date, etc.)
    pub goal_calculation: Option<GoalCalculation>,
    
    /// Whether goal data is currently loading
    pub loading: bool,
    
    /// Goal loading error message
    pub error_message: Option<String>,
    
    /// Goal creation form state
    pub creation_form: GoalCreationFormState,
    
    /// Whether the goal creation modal is visible
    pub show_creation_modal: bool,
}

/// State for the goal creation form
#[derive(Debug)]
pub struct GoalCreationFormState {
    /// Goal description input
    pub description: String,
    
    /// Target amount input (as string for validation)
    pub target_amount_text: String,
    
    /// Parsed target amount
    pub target_amount: Option<f64>,
    
    /// Form validation errors
    pub description_error: Option<String>,
    pub amount_error: Option<String>,
    
    /// Whether form is currently submitting
    pub submitting: bool,
    
    /// Form submission error
    pub submission_error: Option<String>,
}

impl GoalUiState {
    /// Create new goal state with default values
    pub fn new() -> Self {
        Self {
            current_goal: None,
            goal_calculation: None,
            loading: false,
            error_message: None,
            creation_form: GoalCreationFormState::new(),
            show_creation_modal: false,
        }
    }
    
    /// Start loading goal data
    pub fn start_loading(&mut self) {
        self.loading = true;
        self.error_message = None;
    }
    
    /// Set goal data from backend
    pub fn set_goal_data(&mut self, goal: Option<DomainGoal>, calculation: Option<GoalCalculation>) {
        self.current_goal = goal;
        self.goal_calculation = calculation;
        self.loading = false;
        self.error_message = None;
    }
    
    /// Set goal loading error
    pub fn set_error(&mut self, error: String) {
        self.loading = false;
        self.error_message = Some(error);
    }
    
    /// Check if there is an active goal
    pub fn has_active_goal(&self) -> bool {
        self.current_goal.is_some()
    }
    
    /// Reset goal creation form
    pub fn reset_creation_form(&mut self) {
        self.creation_form = GoalCreationFormState::new();
    }
    
    /// Show goal creation modal
    pub fn show_creation_modal(&mut self) {
        self.show_creation_modal = true;
        self.reset_creation_form();
    }
    
    /// Hide goal creation modal
    pub fn hide_creation_modal(&mut self) {
        self.show_creation_modal = false;
        self.reset_creation_form();
    }
}

impl GoalCreationFormState {
    /// Create new goal creation form state
    pub fn new() -> Self {
        Self {
            description: String::new(),
            target_amount_text: String::new(),
            target_amount: None,
            description_error: None,
            amount_error: None,
            submitting: false,
            submission_error: None,
        }
    }
    
    /// Validate the form and return true if valid
    pub fn validate(&mut self) -> bool {
        let mut is_valid = true;
        
        // Validate description
        let trimmed_description = self.description.trim();
        if trimmed_description.is_empty() {
            self.description_error = Some("Description cannot be empty".to_string());
            is_valid = false;
        } else if trimmed_description.len() > 256 {
            self.description_error = Some("Description cannot exceed 256 characters".to_string());
            is_valid = false;
        } else {
            self.description_error = None;
        }
        
        // Validate target amount
        match self.target_amount_text.trim().parse::<f64>() {
            Ok(amount) if amount > 0.0 => {
                self.target_amount = Some(amount);
                self.amount_error = None;
            }
            Ok(_) => {
                self.target_amount = None;
                self.amount_error = Some("Amount must be greater than 0".to_string());
                is_valid = false;
            }
            Err(_) => {
                self.target_amount = None;
                self.amount_error = Some("Please enter a valid number".to_string());
                is_valid = false;
            }
        }
        
        is_valid
    }
    
    /// Check if form can be submitted
    pub fn can_submit(&self) -> bool {
        !self.description.trim().is_empty() 
        && !self.target_amount_text.trim().is_empty()
        && self.target_amount.is_some()
        && self.description_error.is_none()
        && self.amount_error.is_none()
        && !self.submitting
    }
    
    /// Start form submission
    pub fn start_submission(&mut self) {
        self.submitting = true;
        self.submission_error = None;
    }
    
    /// Complete form submission successfully
    pub fn complete_submission(&mut self) {
        self.submitting = false;
        // Form will be reset by parent when modal is hidden
    }
    
    /// Set form submission error
    pub fn set_submission_error(&mut self, error: String) {
        self.submitting = false;
        self.submission_error = Some(error);
    }
} 