//! # Goal Progress Graph Module
//!
//! This module provides a specialized balance progression graph for goal tracking.
//! It shows balance changes since the goal was created, with the goal target displayed
//! as a horizontal reference line.
//!
//! ## Key Components:
//! - `graph_renderer.rs` - Graph rendering using egui_plot
//! - `data_preparation.rs` - Data filtering and preparation for goal-specific charts
//!
//! ## Purpose:
//! This component reuses the existing chart infrastructure but customizes it specifically
//! for goal progress tracking in a smaller space with goal-specific context.

pub mod graph_renderer;
pub mod data_preparation;

// Re-export main components
pub use graph_renderer::GoalProgressGraph;
pub use data_preparation::{GoalGraphDataPoint, prepare_goal_graph_data}; 