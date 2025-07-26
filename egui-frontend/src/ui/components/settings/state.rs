//! # Settings State Module
//!
//! This module contains all state related to settings modals and forms.
//!
//! ## Responsibilities:
//! - Create child form state and validation
//! - Profile editing form state and validation
//! - Export data form state and validation
//! - Future settings form states (allowance config, etc.)
//! - Settings modal visibility flags
//!
//! ## Purpose:
//! This centralizes all settings-related state management, making it easier to
//! maintain consistent form behavior and validation across settings features.

/// Export type selection for export modal
#[derive(Debug, Clone, PartialEq)]
pub enum ExportType {
    /// Export to default Documents folder
    Default,
    /// Export to custom user-specified path
    Custom,
}

impl Default for ExportType {
    fn default() -> Self {
        Self::Default
    }
}

/// Form state for exporting transaction data
#[derive(Debug, Clone)]
pub struct ExportFormState {
    pub export_type: ExportType,
    pub custom_path: String,
    pub selected_file_path: Option<String>, // Path selected via native file dialog
    pub is_exporting: bool,
    pub success_message: Option<String>,
    pub error_message: Option<String>,
    pub preview_filename: String,
    pub preview_location: String,
}

impl ExportFormState {
    /// Create new export form state
    pub fn new() -> Self {
        Self {
            export_type: ExportType::Default,
            custom_path: String::new(),
            selected_file_path: None,
            is_exporting: false,
            success_message: None,
            error_message: None,
            preview_filename: String::new(),
            preview_location: String::new(),
        }
    }

    /// Clear form fields and messages
    pub fn clear(&mut self) {
        self.export_type = ExportType::Default;
        self.custom_path.clear();
        self.selected_file_path = None;
        self.is_exporting = false;
        self.success_message = None;
        self.error_message = None;
        self.preview_filename.clear();
        self.preview_location.clear();
    }

    /// Update preview based on current settings
    pub fn update_preview(&mut self, child_name: Option<&str>) {
        // Generate location preview and filename
        match self.export_type {
            ExportType::Default => {
                // Generate filename preview for default location
                let child_name_formatted = child_name
                    .unwrap_or("child")
                    .replace(" ", "_")
                    .to_lowercase();
                
                let now = chrono::Utc::now();
                self.preview_filename = format!(
                    "{}_transactions_{}.csv",
                    child_name_formatted,
                    now.format("%Y%m%d")
                );

                self.preview_location = if let Some(docs_dir) = dirs::document_dir() {
                    docs_dir.to_string_lossy().to_string()
                } else if let Some(home_dir) = dirs::home_dir() {
                    home_dir.to_string_lossy().to_string()
                } else {
                    "Default location".to_string()
                };
            }
            ExportType::Custom => {
                if let Some(ref selected_path) = self.selected_file_path {
                    // Extract filename and directory from selected path
                    let path = std::path::Path::new(selected_path);
                    if let Some(filename) = path.file_name() {
                        self.preview_filename = filename.to_string_lossy().to_string();
                    } else {
                        self.preview_filename = "export.csv".to_string();
                    }
                    
                    if let Some(parent) = path.parent() {
                        self.preview_location = parent.to_string_lossy().to_string();
                    } else {
                        self.preview_location = selected_path.clone();
                    }
                } else if !self.custom_path.trim().is_empty() {
                    // Fallback to manual custom path entry
                    let child_name_formatted = child_name
                        .unwrap_or("child")
                        .replace(" ", "_")
                        .to_lowercase();
                    
                    let now = chrono::Utc::now();
                    self.preview_filename = format!(
                        "{}_transactions_{}.csv",
                        child_name_formatted,
                        now.format("%Y%m%d")
                    );
                    self.preview_location = self.custom_path.clone();
                } else {
                    self.preview_filename = "Please select a file location".to_string();
                    self.preview_location = "No location selected".to_string();
                }
            }
        };
    }

    /// Clear any previous messages
    pub fn clear_messages(&mut self) {
        self.success_message = None;
        self.error_message = None;
    }

    /// Set success message
    pub fn set_success(&mut self, message: String) {
        self.success_message = Some(message);
        self.error_message = None;
    }

    /// Set error message
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
        self.success_message = None;
    }

    /// Check if form is ready for export
    pub fn is_ready_for_export(&self) -> bool {
        !self.is_exporting && match self.export_type {
            ExportType::Default => true,
            ExportType::Custom => {
                // Valid if we have a selected file path from dialog, or manual custom path
                self.selected_file_path.is_some() || !self.custom_path.trim().is_empty()
            }
        }
    }

    /// Get the effective custom path for export
    /// This prioritizes the selected file path from the dialog over the manual custom path
    pub fn get_effective_custom_path(&self) -> Option<String> {
        match self.export_type {
            ExportType::Default => None,
            ExportType::Custom => {
                if let Some(ref selected_path) = self.selected_file_path {
                    Some(selected_path.clone())
                } else if !self.custom_path.trim().is_empty() {
                    Some(self.custom_path.trim().to_string())
                } else {
                    None
                }
            }
        }
    }
}

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

/// Form state for configuring allowance settings
#[derive(Debug, Clone)]
pub struct AllowanceConfigFormState {
    pub amount: String,
    pub day_of_week: u8, // 0 = Sunday, 1 = Monday, ..., 6 = Saturday
    pub amount_error: Option<String>,
    pub is_valid: bool,
    pub is_saving: bool,
    pub success_message: Option<String>,
    pub error_message: Option<String>,
    
    // Original values for change detection
    pub original_amount: Option<f64>,
    pub original_day_of_week: Option<u8>,
    pub has_existing_config: bool,
}

impl AllowanceConfigFormState {
    /// Create new allowance config form state with defaults
    pub fn new() -> Self {
        Self {
            amount: "5.00".to_string(), // Default $5
            day_of_week: 5, // Default Friday
            amount_error: None,
            is_valid: true,
            is_saving: false,
            success_message: None,
            error_message: None,
            original_amount: None,
            original_day_of_week: None,
            has_existing_config: false,
        }
    }
    
    /// Clear all form fields and reset to defaults
    pub fn clear(&mut self) {
        self.amount = "5.00".to_string();
        self.day_of_week = 5;
        self.amount_error = None;
        self.is_valid = true;
        self.is_saving = false;
        self.success_message = None;
        self.error_message = None;
        self.original_amount = None;
        self.original_day_of_week = None;
        self.has_existing_config = false;
    }
    
    /// Load existing allowance config into form
    pub fn load_from_config(&mut self, config: &crate::backend::domain::models::allowance::AllowanceConfig) {
        self.amount = format!("{:.2}", config.amount);
        self.day_of_week = config.day_of_week;
        self.original_amount = Some(config.amount);
        self.original_day_of_week = Some(config.day_of_week);
        self.has_existing_config = true;
        self.amount_error = None;
        self.is_valid = true;
        self.success_message = None;
        self.error_message = None;
        
        log::info!("⚙️ LOADED_CONFIG: amount='{}' (original={:.2}), day={} (original={}), has_existing={}", 
            self.amount, config.amount, self.day_of_week, config.day_of_week, self.has_existing_config);
    }
    
    /// Check if form values have changed from original
    pub fn has_changes(&self) -> bool {
        if !self.has_existing_config {
            log::info!("⚙️ CHANGE_DETECTION: No existing config, allowing changes");
            return true; // Always allow saving for new configs
        }
        
        // Parse current amount
        let current_amount = self.amount.trim().parse::<f64>().unwrap_or(0.0);
        
        // Check if amount or day changed (only log when there are actual changes)
        let amount_changed = self.original_amount.map(|orig| {
            let changed = current_amount != orig;
            if changed {
                log::info!("⚙️ AMOUNT_CHANGE: current={:.2}, original={:.2}, changed={}", 
                    current_amount, orig, changed);
            }
            changed
        }).unwrap_or(true);
        
        let day_changed = self.original_day_of_week.map(|orig| {
            let changed = orig != self.day_of_week;
            if changed {
                log::info!("⚙️ DAY_CHANGE: current={}, original={}, changed={}", 
                    self.day_of_week, orig, changed);
            }
            changed
        }).unwrap_or(true);
        
        let has_changes = amount_changed || day_changed;
        if has_changes {
            log::info!("⚙️ HAS_CHANGES: amount_changed={}, day_changed={}, result={}", 
                amount_changed, day_changed, has_changes);
        }
        
        has_changes
    }
    
    /// Get day name for current day_of_week
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
    
    /// Get success message based on form state
    pub fn get_success_message(&self) -> String {
        format!("New allowance: ${:.2} every {}", 
            self.amount.parse::<f64>().unwrap_or(0.0), 
            self.day_name())
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

    /// Whether the export data modal is visible
    pub show_export_modal: bool,

    /// Export data form state
    pub export_form: ExportFormState,

    /// Whether the data directory modal is visible
    pub show_data_directory_modal: bool,

    /// Data directory form state
    pub data_directory_form: DataDirectoryFormState,

    /// Whether the allowance config modal is visible
    pub show_allowance_config_modal: bool,

    /// Allowance config form state
    pub allowance_config_form: AllowanceConfigFormState,
}

impl SettingsState {
    /// Create new settings state
    pub fn new() -> Self {
        Self {
            show_create_child_modal: false,
            create_child_form: CreateChildFormState::new(),
            show_profile_modal: false,
            profile_form: ProfileFormState::new(),
            show_export_modal: false,
            export_form: ExportFormState::new(),
            show_data_directory_modal: false,
            data_directory_form: DataDirectoryFormState::new(),
            show_allowance_config_modal: false,
            allowance_config_form: AllowanceConfigFormState::new(),
        }
    }

    /// Hide all settings modals
    pub fn hide_all_modals(&mut self) {
        self.show_create_child_modal = false;
        self.show_profile_modal = false;
        self.show_export_modal = false;
        self.show_data_directory_modal = false;
        self.show_allowance_config_modal = false;
    }

    /// Reset all form states
    pub fn reset_all_forms(&mut self) {
        self.create_child_form.clear();
        self.profile_form.clear();
        self.export_form.clear();
        self.data_directory_form.clear();
        self.allowance_config_form.clear();
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

impl Default for ExportFormState {
    fn default() -> Self {
        Self::new()
    }
}

/// Form state for data directory management
#[derive(Debug, Clone)]
pub struct DataDirectoryFormState {
    pub current_path: String,
    pub new_path: String,
    pub is_loading: bool,
    pub has_conflict: bool,
    pub conflict_details: Option<String>,
    pub user_decision: Option<shared::ConflictResolution>,
    pub success_message: Option<String>,
    pub error_message: Option<String>,
    pub is_redirected: bool, // True if current location is a redirect
}

impl DataDirectoryFormState {
    /// Create new data directory form state
    pub fn new() -> Self {
        Self {
            current_path: String::new(),
            new_path: String::new(),
            is_loading: false,
            has_conflict: false,
            conflict_details: None,
            user_decision: None,
            success_message: None,
            error_message: None,
            is_redirected: false,
        }
    }

    /// Clear form fields and messages
    pub fn clear(&mut self) {
        self.current_path.clear();
        self.new_path.clear();
        self.is_loading = false;
        self.has_conflict = false;
        self.conflict_details = None;
        self.user_decision = None;
        self.success_message = None;
        self.error_message = None;
        self.is_redirected = false;
    }

    /// Set loading state
    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
        if loading {
            self.clear_messages();
        }
    }

    /// Set conflict state
    pub fn set_conflict(&mut self, has_conflict: bool, details: Option<String>) {
        self.has_conflict = has_conflict;
        self.conflict_details = details;
        self.user_decision = None;
    }

    /// Clear any previous messages
    pub fn clear_messages(&mut self) {
        self.success_message = None;
        self.error_message = None;
    }

    /// Set success message
    pub fn set_success(&mut self, message: String) {
        self.success_message = Some(message);
        self.error_message = None;
    }

    /// Set error message
    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
        self.success_message = None;
    }

    /// Check if form is ready for conflict resolution
    pub fn is_ready_for_resolution(&self) -> bool {
        !self.is_loading && self.has_conflict && self.user_decision.is_some()
    }

    /// Check if form is ready for initial path change
    pub fn is_ready_for_change(&self) -> bool {
        !self.is_loading && !self.new_path.trim().is_empty()
    }
}

impl Default for DataDirectoryFormState {
    fn default() -> Self {
        Self::new()
    }
} 