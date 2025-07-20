//! # Unified Styling System
//!
//! This module provides a comprehensive styling system for the allowance tracker app,
//! unifying the previous separate `styling.rs` and `theme.rs` modules into a cohesive,
//! well-organized styling framework.
//!
//! ## Module Organization:
//! - `theme` - Core theme structures and definitions
//! - `colors` - Centralized color constants and theme colors
//! - `functions` - Drawing utility functions and UI helpers
//!
//! ## Architecture:
//! The unified system combines the structured theme approach with practical utility
//! functions, eliminating duplication while maintaining consistency across the app.
//!
//! ## Key Features:
//! - Structured theme system with organized color groups
//! - Utility functions for common drawing operations
//! - Centralized color management
//! - Kid-friendly visual design with gradients and rounded corners
//! - Consistent styling patterns across all components
//!
//! ## Usage:
//! ```rust
//! use crate::ui::components::styling::{CURRENT_THEME, colors, setup_kid_friendly_style};
//! 
//! // Use theme colors
//! let hover_color = CURRENT_THEME.interactive.hover_border;
//! 
//! // Use convenience constants  
//! let bg_color = colors::CARD_BACKGROUND;
//! 
//! // Use drawing functions
//! setup_kid_friendly_style(ctx);
//! ```

pub mod theme;
pub mod colors;
pub mod functions;

// Re-export the most commonly used items for convenience
pub use theme::{Theme, CURRENT_THEME};
pub use colors::*;
pub use functions::*; 