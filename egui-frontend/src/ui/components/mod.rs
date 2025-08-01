//! # UI Components Module
//!
//! This module organizes all UI components for the allowance tracker application.
//! Each submodule handles a specific aspect of the user interface.
//!
//! ## Module Organization:
//! - `data_loading` - Backend data loading and state management
//! - `styling` - Unified styling system (theme, colors, functions)
//! - `transaction_table` - Transaction table rendering and formatting
//! - `modals` - Modal dialogs and popup interfaces
//! - `settings` - Settings-related modals and forms (create child, profile, etc.)
//! - `header` - Application header with navigation and balance display
//! - `ui_components` - Reusable UI helper functions and drawing utilities
//! - `tab_manager` - Tab navigation and content routing
//! - `table_renderer` - Table view rendering with responsive design
//! - `calendar_renderer` - Calendar view rendering with transaction display
//! - `goal_progress_graph` - Goal-specific balance progression graph component
//! - `circular_days_progress` - Donut-style circular progress tracker for goal timeline
//!
//! ## Architecture:
//! The components are organized to promote reusability and maintainability.
//! Each module has a clear responsibility and minimal dependencies on others.

pub mod calendar_renderer;
pub mod chart_renderer;
pub mod circular_days_progress;
pub mod data_loading;
pub mod dropdown_menu;
pub mod goal_renderer;
pub mod goal_progress_bar;
pub mod goal_progress_graph;
pub mod header;
pub mod modals;
pub mod settings;
pub mod styling;
pub mod tab_manager;
pub mod table_renderer;
pub mod transaction_table;
pub mod ui_components;

// Re-export the unified styling system
pub use styling::*;
pub use transaction_table::*; 