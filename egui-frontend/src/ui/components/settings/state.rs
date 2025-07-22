//! # Settings State Module
//!
//! This module contains all state related to settings modals and forms.
//!
//! ## Responsibilities:
//! - Create child form state and validation
//! - Future settings form states (allowance config, export, etc.)
//! - Settings modal visibility flags
//!
//! ## Purpose:
//! This centralizes all settings-related state management, making it easier to
//! maintain consistent form behavior and validation across settings features.

/// Form state for creating a new child
#[derive(Debug, Clone)]
pub struct CreateChildFormState {
    pub name: String,
    pub birthdate: String, // YYYY-MM-DD format
    pub name_error: Option<String>,
    pub birthdate_error: Option<String>,
    pub is_valid: bool,
    pub is_saving: bool,
}

impl CreateChildFormState {
    /// Create new create child form state
    pub fn new() -> Self {
        Self {
            name: String::new(),
            birthdate: String::new(),
            name_error: None,
            birthdate_error: None,
            is_valid: false,
            is_saving: false,
        }
    }

    /// Clear form fields and errors
    pub fn clear(&mut self) {
        self.name.clear();
        self.birthdate.clear();
        self.name_error = None;
        self.birthdate_error = None;
        self.is_valid = false;
        self.is_saving = false;
    }

    /// Validate the form and update error states
    pub fn validate(&mut self) {
        self.name_error = None;
        self.birthdate_error = None;

        // Validate name
        let trimmed_name = self.name.trim();
        if trimmed_name.is_empty() {
            self.name_error = Some("Child name is required".to_string());
        } else if trimmed_name.len() > 100 {
            self.name_error = Some("Name cannot exceed 100 characters".to_string());
        }

        // Validate birthdate format (YYYY-MM-DD)
        if self.birthdate.trim().is_empty() {
            self.birthdate_error = Some("Birthdate is required".to_string());
        } else if !self.is_valid_date_format(&self.birthdate) {
            self.birthdate_error = Some("Use format YYYY-MM-DD (e.g., 2015-03-20)".to_string());
        }

        // Overall form validity
        self.is_valid = self.name_error.is_none() && self.birthdate_error.is_none();
    }

    /// Check if date string is in valid YYYY-MM-DD format with reasonable values
    pub fn is_valid_date_format(&self, date_str: &str) -> bool {
        let parts: Vec<&str> = date_str.split('-').collect();
        if parts.len() != 3 {
            return false;
        }

        // Parse year, month, day
        let year: Result<u32, _> = parts[0].parse();
        let month: Result<u32, _> = parts[1].parse();
        let day: Result<u32, _> = parts[2].parse();

        match (year, month, day) {
            (Ok(y), Ok(m), Ok(d)) => {
                // Basic range validation
                y >= 1900 && y <= 2100 && (1..=12).contains(&m) && (1..=31).contains(&d)
            }
            _ => false,
        }
    }
}

/// Profile editing form state (moved from ModalState for better organization)
#[derive(Debug, Clone)]
pub struct ProfileFormState {
    pub name: String,
    pub birthdate: String, // YYYY-MM-DD format
    pub name_error: Option<String>,
    pub birthdate_error: Option<String>,
    pub is_valid: bool,
    pub is_saving: bool,
}

impl ProfileFormState {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            birthdate: String::new(),
            name_error: None,
            birthdate_error: None,
            is_valid: true,
            is_saving: false,
        }
    }
    
    pub fn clear(&mut self) {
        self.name.clear();
        self.birthdate.clear();
        self.name_error = None;
        self.birthdate_error = None;
        self.is_valid = true;
        self.is_saving = false;
    }
    
    pub fn populate_from_child(&mut self, child: &crate::backend::domain::models::child::Child) {
        self.name = child.name.clone();
        self.birthdate = child.birthdate.to_string();
        self.name_error = None;
        self.birthdate_error = None;
        self.is_valid = true;
        self.is_saving = false;
    }
}

/// All settings-related state for the application
#[derive(Debug)]
pub struct SettingsState {
    /// Whether the create child modal is visible
    pub show_create_child_modal: bool,

    /// Create child form state
    pub create_child_form: CreateChildFormState,

    /// Whether the profile modal is visible (moved from ModalState)
    pub show_profile_modal: bool,

    /// Profile editing form state (moved from ModalState)
    pub profile_form: ProfileFormState,

    // TODO: Future settings modal states
    // pub show_allowance_config_modal: bool,
    // pub allowance_config_form: AllowanceConfigFormState,
    // pub show_export_modal: bool,
    // pub export_form: ExportFormState,
}

impl SettingsState {
    /// Create new settings state
    pub fn new() -> Self {
        Self {
            show_create_child_modal: false,
            create_child_form: CreateChildFormState::new(),
            show_profile_modal: false,
            profile_form: ProfileFormState::new(),
        }
    }

    /// Hide all settings modals
    pub fn hide_all_modals(&mut self) {
        self.show_create_child_modal = false;
        self.show_profile_modal = false;
        // TODO: Hide other settings modals when implemented
    }

    /// Reset all form states
    pub fn reset_all_forms(&mut self) {
        self.create_child_form.clear();
        self.profile_form.clear();
        // TODO: Reset other form states when implemented
    }
}

impl Default for CreateChildFormState {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ProfileFormState {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
} 