//! # Modal State Module
//!
//! This module contains all state related to modal dialogs and their visibility.
//!
//! ## Responsibilities:
//! - Modal visibility flags
//! - Modal-specific state (parental control flow, etc.)
//! - Protected action management
//!
//! ## Purpose:
//! This centralizes all modal-related state management, making it easier to
//! coordinate modal behavior and prevent conflicts between different modals.

/// Stages of the parental control challenge flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentalControlStage {
    Question1,      // "Are you Mom or Dad?"
    Question2,      // "What's cooler than cool?"
    Authenticated,  // Success state
}

/// Types of protected actions that require parental control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedAction {
    DeleteTransactions,
    AccessSettings, // NEW: Universal protection for all settings menu items
}

/// Specific settings menu actions that can be executed after parental control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    ShowProfile,
    CreateChild,
    ConfigureAllowance,
    DeleteTransactions,
    ExportData,
    DataDirectory,
}

/// Profile editing form state
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

/// Modal visibility and modal-specific state
#[derive(Debug)]
pub struct ModalState {
    /// Whether the add money modal is visible
    pub show_add_money_modal: bool,
    
    /// Whether the child selector modal is visible
    pub show_child_selector: bool,
    
    /// Whether the allowance config modal is visible
    #[allow(dead_code)]
    pub show_allowance_config_modal: bool,
    
    /// Whether the parental control modal is visible
    pub show_parental_control_modal: bool,
    
    /// Whether the profile modal is visible
    pub show_profile_modal: bool,
    
    /// Current stage in the parental control flow
    pub parental_control_stage: ParentalControlStage,
    
    /// Action waiting for parental control approval
    pub pending_protected_action: Option<ProtectedAction>,
    
    /// Settings action waiting to be executed after parental control
    pub pending_settings_action: Option<SettingsAction>,
    
    /// User input for parental control challenges
    pub parental_control_input: String,
    
    /// Error message for parental control attempts
    pub parental_control_error: Option<String>,
    
    /// Whether parental control is currently processing
    pub parental_control_loading: bool,
    
    /// Profile editing form state
    pub profile_form: ProfileFormState,
}

impl ModalState {
    /// Create new modal state with all modals hidden
    pub fn new() -> Self {
        Self {
            show_add_money_modal: false,
            show_child_selector: false,
            show_allowance_config_modal: false,
            show_parental_control_modal: false,
            show_profile_modal: false,
            parental_control_stage: ParentalControlStage::Question1,
            pending_protected_action: None,
            pending_settings_action: None,
            parental_control_input: String::new(),
            parental_control_error: None,
            parental_control_loading: false,
            profile_form: ProfileFormState::new(),
        }
    }
    
    /// Hide all modals
    pub fn hide_all_modals(&mut self) {
        self.show_add_money_modal = false;
        self.show_child_selector = false;
        self.show_allowance_config_modal = false;
        self.show_parental_control_modal = false;
        self.show_profile_modal = false;
    }
    
    /// Reset parental control state
    pub fn reset_parental_control(&mut self) {
        self.parental_control_stage = ParentalControlStage::Question1;
        self.pending_protected_action = None;
        self.pending_settings_action = None;
        self.parental_control_input.clear();
        self.parental_control_error = None;
        self.parental_control_loading = false;
    }
} 