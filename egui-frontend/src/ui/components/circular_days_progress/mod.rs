//! # Circular Days Progress Module
//!
//! This module provides a donut-style circular progress tracker for goal timeline visualization.
//! It shows the number of days that have passed since the goal was set with the number of 
//! days remaining until projected completion.
//!
//! ## Key Components:
//! - `renderer.rs` - Circular/donut progress rendering using egui painting primitives
//! - `calculations.rs` - Days calculation logic and progress percentage computation
//!
//! ## Purpose:
//! This component provides an intuitive visual representation of goal timeline progress
//! in a compact circular format that fits well in the goal card's bottom-right section.

pub mod renderer;
pub mod calculations;

// Re-export main components
pub use renderer::CircularDaysProgress;
pub use calculations::{DaysProgress, calculate_days_progress}; 