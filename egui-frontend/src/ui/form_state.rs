//! Form state types for UI components.

use shared::ValidationResult;

/// State for managing money input forms
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoneyFormState {
    pub description: String,
    pub amount_input: String,
    pub is_submitting: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub show_success: bool,
}

impl MoneyFormState {
    /// Create a new empty form state
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the form after successful submission
    pub fn clear_with_success(&mut self, message: String) {
        self.description.clear();
        self.amount_input.clear();
        self.is_submitting = false;
        self.error_message = None;
        self.success_message = Some(message);
        self.show_success = true;
    }

    /// Set form to submitting state
    pub fn set_submitting(&mut self) {
        self.is_submitting = true;
        self.error_message = None;
    }

    /// Set an error on the form
    pub fn set_error(&mut self, message: String) {
        self.is_submitting = false;
        self.error_message = Some(message);
    }

    /// Update form state from validation result
    pub fn apply_validation(&mut self, validation: &ValidationResult) {
        if !validation.is_valid {
            if let Some(error) = validation.errors.first() {
                self.error_message = Some(format!("{:?}", error));
            }
        } else {
            self.error_message = None;
        }
    }
}
