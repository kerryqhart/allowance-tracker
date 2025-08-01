//! # Shared Modal Utilities
//!
//! This module contains common modal functionality and utilities shared across
//! different modal implementations.
//!
//! ## Purpose:
//! - Provide consistent modal styling and behavior
//! - Reduce code duplication across modal implementations
//! - Ensure uniform user experience across all modals
//!
//! ## Future Extensions:
//! This module is designed to grow as common patterns emerge across modals.
//! Potential additions include:
//! - Common modal layouts
//! - Shared animation utilities
//! - Modal backdrop handling
//! - Common button styles

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    /// Render all modals - main modal coordinator
    pub fn render_modals(&mut self, ctx: &egui::Context) {
        self.render_child_selector_modal(ctx);
        self.render_day_action_overlay(ctx);
        self.render_parental_control_modal(ctx);
        self.render_goal_creation_modal(ctx);
        self.render_profile_modal(ctx);
        self.render_create_child_modal(ctx);
        self.render_export_modal(ctx); // Export data modal from settings
        self.render_data_directory_modal(ctx); // Data directory modal from settings
        self.render_allowance_config_modal(ctx); // Allowance config modal from settings
    }
} 