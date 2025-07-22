//! # Settings Module
//! This module organizes all settings-related UI components for the allowance tracker app.
//! It consolidates settings functionality that was previously scattered across different modules.
//!
//! ## Responsibilities:
//! - **Create Child Modal**: New child creation with validation and backend integration
//! - **Profile Modal**: Child profile editing with form validation
//! - **Export Modal**: Data export functionality with location selection
//! - **Settings State**: Centralized state management for all settings forms
//! - **Shared Utilities**: Common styling and UI helpers for consistent experience
//!
//! ## Architecture:
//! - `state.rs` - All settings-related state management (forms, validation, modal visibility)
//! - `create_child_modal.rs` - Complete create child flow with backend integration
//! - `profile_modal.rs` - Profile editing functionality (moved from modals/)
//! - `export_modal.rs` - Data export functionality with default/custom location options
//! - `shared.rs` - Common styling, validation helpers, and modal utilities
//!
//! ## Design Principles:
//! - **Consistent UX**: All settings modals follow the same visual and interaction patterns
//! - **Form Validation**: Real-time validation with clear error messaging
//! - **Backend Integration**: Proper error handling and loading states
//! - **State Isolation**: Each modal has its own dedicated state management
//! - **Reusable Components**: Shared utilities reduce code duplication

pub mod state;
pub mod create_child_modal;
pub mod profile_modal; // Added in Phase 3
pub mod export_modal; // Added in Phase 2 - Export data functionality
pub mod shared;

pub use state::*;

// TODO: Future modules to implement (allowance_config_modal, data_directory_modal) 