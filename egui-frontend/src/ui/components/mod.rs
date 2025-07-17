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

pub mod calendar_renderer;
pub mod data_loading;
pub mod dropdown_menu;
pub mod header;
pub mod modals;
pub mod styling;
pub mod tab_manager;
pub mod table_renderer;
pub mod theme;
pub mod transaction_table;
pub mod ui_components;

pub use styling::{setup_kid_friendly_style, draw_solid_purple_background, draw_image_background, draw_card_container, draw_day_header_gradient, get_table_header_color};
pub use theme::*;
pub use transaction_table::*; 