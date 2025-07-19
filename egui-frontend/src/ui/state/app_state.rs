//! # Core Application State
//!
//! This module contains the essential application state that forms the backbone
//! of the allowance tracker app.
//!
//! ## Responsibilities:
//! - Backend connection management
//! - Current child selection
//! - Current balance tracking
//! - Main tab navigation state
//!
//! ## Purpose:
//! This represents the core "business state" of the application - the fundamental
//! data needed for the app to function, separate from UI-specific state.

use shared::*;
use crate::backend::Backend;

/// Tabs available in the main interface
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Calendar,
    Table,
}

/// Core application state containing essential app data
pub struct CoreAppState {
    /// Backend connection for data access
    pub backend: Backend,
    
    /// Currently selected child
    pub current_child: Option<Child>,
    
    /// Current balance for the selected child
    pub current_balance: f64,
    
    /// Currently active main tab (Calendar or Table)
    pub current_tab: MainTab,
}

impl CoreAppState {
    /// Create new core app state with backend connection
    pub fn new(backend: Backend) -> Self {
        Self {
            backend,
            current_child: None,
            current_balance: 0.0,
            current_tab: MainTab::Calendar, // Default to calendar view
        }
    }
} 