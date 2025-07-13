//! # UI Components Module
//!
//! This module organizes all UI components for the allowance tracker application.
//! Each submodule handles a specific aspect of the user interface.
//!
//! ## Module Organization:
//! - `data_loading` - Backend data loading and state management
//! - `styling` - Visual styling, colors, and theme management
//! - `transaction_table` - Transaction table rendering and formatting
//! - `modals` - Modal dialogs and popup interfaces
//! - `header` - Application header with navigation and balance display
//! - `ui_components` - Reusable UI helper functions and drawing utilities
//! - `tab_manager` - Tab navigation and content routing
//! - `table_renderer` - Table view rendering with responsive design
//! - `calendar_renderer` - Calendar view rendering with transaction display
//!
//! ## Architecture:
//! The components are organized to promote reusability and maintainability.
//! Each module has a clear responsibility and minimal dependencies on others.

pub mod data_loading;
pub mod styling;
pub mod transaction_table;
pub mod modals;
pub mod header;
pub mod ui_components;
pub mod tab_manager;
pub mod table_renderer;
pub mod calendar_renderer;

pub use styling::*;
pub use transaction_table::*; 