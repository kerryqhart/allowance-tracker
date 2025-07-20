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
    // Future extensions: ConfigureAllowance, ExportData, etc.
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
    
    /// Current stage in the parental control flow
    pub parental_control_stage: ParentalControlStage,
    
    /// Action waiting for parental control approval
    pub pending_protected_action: Option<ProtectedAction>,
    
    /// User input for parental control challenges
    pub parental_control_input: String,
    
    /// Error message for parental control attempts
    pub parental_control_error: Option<String>,
    
    /// Whether parental control is currently processing
    pub parental_control_loading: bool,
}

impl ModalState {
    /// Create new modal state with all modals hidden
    pub fn new() -> Self {
        Self {
            show_add_money_modal: false,
            show_child_selector: false,
            show_allowance_config_modal: false,
            show_parental_control_modal: false,
            parental_control_stage: ParentalControlStage::Question1,
            pending_protected_action: None,
            parental_control_input: String::new(),
            parental_control_error: None,
            parental_control_loading: false,
        }
    }
    
    /// Hide all modals
    pub fn hide_all_modals(&mut self) {
        self.show_add_money_modal = false;
        self.show_child_selector = false;
        self.show_allowance_config_modal = false;
        self.show_parental_control_modal = false;
    }
    
    /// Reset parental control state
    pub fn reset_parental_control(&mut self) {
        self.parental_control_stage = ParentalControlStage::Question1;
        self.pending_protected_action = None;
        self.parental_control_input.clear();
        self.parental_control_error = None;
        self.parental_control_loading = false;
    }
} 