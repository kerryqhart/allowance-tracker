//! # State Management Module
//!
//! This module organizes all application state into focused, maintainable components.
//! It breaks down the previous monolithic app state into logical categories.
//!
//! ## Module Organization:
//! - `app_state` - Core application state (backend, child, balance, tab)
//! - `ui_state` - General UI state (loading, messages)
//! - `calendar_state` - Calendar-specific state and navigation
//! - `modal_state` - Modal visibility and modal-specific state
//! - `form_state` - Form inputs and validation states
//! - `interaction_state` - User interaction state (selection, dropdowns)
//!
//! ## Architecture:
//! Each state module is focused and has minimal dependencies on others.
//! The main AllowanceTrackerApp struct is rebuilt by composing these state modules.

pub mod app_state;
pub mod ui_state;
pub mod calendar_state;
pub mod modal_state;
pub mod form_state;
pub mod interaction_state;

// Re-export all state components for easy access
pub use app_state::*;
pub use ui_state::*;
pub use calendar_state::*;
pub use modal_state::*;
pub use form_state::*;
pub use interaction_state::*; 