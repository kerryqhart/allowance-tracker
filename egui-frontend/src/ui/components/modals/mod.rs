//! # Modals Module
//!
//! This module organizes all modal dialog components for the allowance tracker app.
//! It breaks down the previous monolithic modals.rs into focused, maintainable modules.
//!
//! ## Module Organization:
//! - `child_selector` - Child selection modal
//! - `parental_control` - Parental control authentication flow
//! - `money_transaction` - Add/spend money modals  
//! - `day_action_overlay` - Calendar day action overlays
//! - `shared` - Common modal functionality and styling
//!
//! ## Architecture:
//! Each modal is self-contained with its own rendering logic and state handling.
//! Shared functionality is provided by the shared module for consistency.

pub mod child_selector;
pub mod parental_control;
pub mod money_transaction;
pub mod day_action_overlay;
pub mod shared;

// Re-export modal functions for easy access
pub use child_selector::*;
pub use parental_control::*;
pub use money_transaction::*;
pub use day_action_overlay::*;
pub use shared::*; 