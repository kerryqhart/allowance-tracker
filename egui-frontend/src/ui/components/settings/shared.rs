//! # Settings Shared Utilities
//!
//! This module contains common utilities and styling functions shared across
//! all settings modals and components.
//!
//! ## Purpose:
//! - Provide consistent modal styling for settings
//! - Common form validation utilities
//! - Shared UI patterns for settings dialogs
//! - Reduce code duplication across settings modals

use eframe::egui;

/// Common styling configuration for settings modals
pub struct SettingsModalStyle {
    pub modal_size: egui::Vec2,
    pub title_font_size: f32,
    pub title_color: egui::Color32,
    pub border_color: egui::Color32,
    pub background_color: egui::Color32,
    pub rounding: f32,
    pub margin: f32,
}

impl SettingsModalStyle {
    /// Default styling for settings modals
    pub fn default_style() -> Self {
        Self {
            modal_size: egui::vec2(450.0, 400.0),
            title_font_size: 28.0,
            title_color: egui::Color32::from_rgb(70, 130, 180), // Steel blue
            border_color: egui::Color32::from_rgb(70, 130, 180),
            background_color: egui::Color32::WHITE,
            rounding: 15.0,
            margin: 25.0,
        }
    }

    /// Apply common modal frame styling
    pub fn apply_frame_styling(&self) -> egui::Frame {
        egui::Frame::window(&egui::Style::default())
            .fill(self.background_color)
            .stroke(egui::Stroke::new(3.0, self.border_color))
            .rounding(egui::CornerRadius::same(self.rounding))
            .inner_margin(egui::Margin::same(self.margin))
            .shadow(egui::Shadow {
                offset: egui::vec2(6.0, 6.0),
                blur: 20.0,
                spread: 0.0,
                color: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100),
            })
    }
}

/// Common form field rendering with error display
pub fn render_form_field_with_error(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    placeholder: &str,
    error: &Option<String>,
    max_length: Option<usize>,
) -> egui::Response {
    ui.vertical(|ui| {
        // Label
        ui.label(egui::RichText::new(label)
            .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
            .strong()
            .color(egui::Color32::from_rgb(60, 60, 60)));

        // Text input with character limit
        let mut text_edit = egui::TextEdit::singleline(value)
            .hint_text(placeholder)
            .desired_width(350.0);

        if let Some(max_len) = max_length {
            text_edit = text_edit.char_limit(max_len);
        }

        let response = ui.add(text_edit);

        // Error message display
        if let Some(error_msg) = error {
            ui.label(egui::RichText::new(format!("❌ {}", error_msg))
                .color(egui::Color32::RED)
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional)));
        } else {
            // Add space to maintain consistent layout
            ui.add_space(20.0);
        }

        response
    }).inner
}

/// Render action buttons with consistent styling
pub fn render_action_buttons<F1, F2>(
    ui: &mut egui::Ui,
    primary_text: &str,
    primary_enabled: bool,
    is_loading: bool,
    on_primary: F1,
    on_cancel: F2,
) where
    F1: FnOnce(),
    F2: FnOnce(),
{
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Cancel button
            if ui.button(egui::RichText::new("Cancel")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional)))
                .clicked() 
            {
                on_cancel();
            }

            ui.add_space(10.0);

            // Primary action button
            let button_text = if is_loading {
                "⏳ Processing..."
            } else {
                primary_text
            };

            let button = egui::Button::new(egui::RichText::new(button_text)
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .strong())
                .fill(if primary_enabled && !is_loading {
                    egui::Color32::from_rgb(70, 130, 180) // Steel blue
                } else {
                    egui::Color32::LIGHT_GRAY
                });

            if ui.add_enabled(primary_enabled && !is_loading, button).clicked() {
                on_primary();
            }
        });
    });
}

/// Render modal backdrop and handle clicks outside modal
pub fn render_modal_backdrop(
    ctx: &egui::Context,
    modal_id: &str,
    on_backdrop_click: impl FnOnce(),
) -> bool {
    let mut backdrop_clicked = false;

    egui::Area::new(egui::Id::new(format!("{}_backdrop", modal_id)))
        .order(egui::Order::Background)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            let screen_rect = ctx.screen_rect();
            
            // Semi-transparent backdrop
            let backdrop_response = ui.allocate_rect(screen_rect, egui::Sense::click());
            ui.painter().rect_filled(
                screen_rect,
                egui::CornerRadius::ZERO,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128),
            );

            if backdrop_response.clicked() {
                backdrop_clicked = true;
            }
        });

    if backdrop_clicked {
        on_backdrop_click();
    }

    backdrop_clicked
} 