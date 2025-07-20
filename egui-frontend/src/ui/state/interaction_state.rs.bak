//! # Interaction State Module
//!
//! This module contains all state related to user interactions and selections.
//!
//! ## Responsibilities:
//! - Transaction selection state (for deletion)
//! - Dropdown menu states
//! - User interaction modes
//!
//! ## Purpose:
//! This isolates all user interaction state, making it easier to manage
//! selection modes and interactive UI elements consistently.

use std::collections::HashSet;
use crate::ui::components::dropdown_menu::DropdownMenu;

/// User interaction state for selections and UI modes
pub struct InteractionState {
    /// Whether we're in transaction selection mode (for deletion)
    pub transaction_selection_mode: bool,
    
    /// Set of selected transaction IDs
    pub selected_transaction_ids: HashSet<String>,
    
    /// Child dropdown menu state
    pub child_dropdown: DropdownMenu,
    
    /// Settings dropdown menu state
    pub settings_dropdown: DropdownMenu,
}

impl InteractionState {
    /// Create new interaction state with default values
    pub fn new() -> Self {
        Self {
            transaction_selection_mode: false,
            selected_transaction_ids: HashSet::new(),
            child_dropdown: DropdownMenu::new("child_dropdown".to_string()),
            settings_dropdown: DropdownMenu::new("settings_dropdown".to_string()),
        }
    }
    
    /// Enter transaction selection mode
    pub fn enter_transaction_selection_mode(&mut self) {
        self.transaction_selection_mode = true;
        self.selected_transaction_ids.clear();
    }
    
    /// Exit transaction selection mode
    pub fn exit_transaction_selection_mode(&mut self) {
        self.transaction_selection_mode = false;
        self.selected_transaction_ids.clear();
    }
    
    /// Toggle selection of a transaction
    pub fn toggle_transaction_selection(&mut self, transaction_id: String) {
        if self.selected_transaction_ids.contains(&transaction_id) {
            self.selected_transaction_ids.remove(&transaction_id);
        } else {
            self.selected_transaction_ids.insert(transaction_id);
        }
    }
    
    /// Check if a transaction is selected
    pub fn is_transaction_selected(&self, transaction_id: &str) -> bool {
        self.selected_transaction_ids.contains(transaction_id)
    }
    
    /// Get the number of selected transactions
    pub fn selected_count(&self) -> usize {
        self.selected_transaction_ids.len()
    }
} 