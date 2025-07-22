//! # Settings Module
//!
//! This module organizes all settings-related UI components for the allowance tracker app.
//! It consolidates settings functionality that was previously scattered across different modules.
//!
//! ## Module Organization:
//! - `state` - Settings-specific state management
//! - `create_child_modal` - Create new child modal
//! - `profile_modal` - Child profile editing modal (moved from modals/)
//! - `allowance_config_modal` - Configure allowance settings (future)
//! - `export_modal` - Data export functionality (future)
//! - `data_directory_modal` - Data directory management (future)
//! - `shared` - Settings-specific shared utilities
//!
//! ## Architecture:
//! Each settings modal is self-contained with consistent patterns:
//! - Form validation and error handling
//! - Parental control integration
//! - Backend service integration
//! - Consistent styling and UX
//!
//! ## Purpose:
//! This centralizes all settings functionality for better maintainability
//! and provides a consistent user experience across all settings features.

pub mod state;
pub mod create_child_modal;
pub mod profile_modal;
pub mod shared;

// Re-export state for easy access
pub use state::*;
// pub mod allowance_config_modal;
// pub mod export_modal;
// pub mod data_directory_modal; 