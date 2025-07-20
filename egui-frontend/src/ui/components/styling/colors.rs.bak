//! # Color Constants
//!
//! This module provides convenient access to the most commonly used colors from the theme.
//! It serves as a bridge between the structured theme system and quick color access for
//! components that need simple color constants.
//!
//! ## Organization:
//! Colors are grouped by usage context:
//! - Interactive colors (buttons, hover states)
//! - Layout colors (backgrounds, cards)
//! - Typography colors (text)
//! - Calendar colors (day headers, chips)
//! - Legacy colors (for backward compatibility)
//!
//! ## Usage:
//! ```rust
//! use crate::ui::components::styling::colors;
//! 
//! let hover_color = colors::HOVER_BORDER;
//! let bg_color = colors::CARD_BACKGROUND;
//! ```

use eframe::egui::Color32;
use super::theme::CURRENT_THEME;

// ============================================================================
// Interactive Colors - Most commonly used
// ============================================================================

/// Primary hover border color used across all interactive elements
pub const HOVER_BORDER: Color32 = CURRENT_THEME.interactive.hover_border;

/// Secondary hover border color for special cases
pub const HOVER_BORDER_SECONDARY: Color32 = CURRENT_THEME.interactive.hover_border_secondary;

/// Hover background color (semi-transparent)
pub const HOVER_BACKGROUND: Color32 = CURRENT_THEME.interactive.hover_background;

/// Active/selected background color
pub const ACTIVE_BACKGROUND: Color32 = CURRENT_THEME.interactive.active_background;

/// Inactive background color
pub const INACTIVE_BACKGROUND: Color32 = CURRENT_THEME.interactive.inactive_background;

/// Normal button border color
pub const BUTTON_BORDER_NORMAL: Color32 = CURRENT_THEME.interactive.button_border_normal;

/// Active button border color
pub const BUTTON_BORDER_ACTIVE: Color32 = CURRENT_THEME.interactive.button_border_active;

// ============================================================================
// Layout Colors - Backgrounds and containers
// ============================================================================

/// Top color for gradient backgrounds
pub const GRADIENT_TOP: Color32 = CURRENT_THEME.layout.gradient_top;

/// Bottom color for gradient backgrounds
pub const GRADIENT_BOTTOM: Color32 = CURRENT_THEME.layout.gradient_bottom;

/// White card background color
pub const CARD_BACKGROUND: Color32 = CURRENT_THEME.layout.card_background;

/// Card shadow color
pub const CARD_SHADOW: Color32 = CURRENT_THEME.layout.card_shadow;

/// Card border color
pub const CARD_BORDER: Color32 = CURRENT_THEME.layout.card_border;

// ============================================================================
// Typography Colors
// ============================================================================

/// Primary text color (main content)
pub const TEXT_PRIMARY: Color32 = CURRENT_THEME.typography.primary;

/// Secondary text color (less prominent)
pub const TEXT_SECONDARY: Color32 = CURRENT_THEME.typography.secondary;

/// Heading text color
pub const TEXT_HEADING: Color32 = CURRENT_THEME.typography.heading;

/// Active/selected text color
pub const TEXT_ACTIVE: Color32 = CURRENT_THEME.typography.active;

/// White text for dark backgrounds
pub const TEXT_WHITE: Color32 = CURRENT_THEME.typography.white;

// ============================================================================
// Calendar Colors
// ============================================================================

/// Today's date border color
pub const CALENDAR_TODAY_BORDER: Color32 = CURRENT_THEME.calendar.today_border;

/// Selected day background color
pub const CALENDAR_SELECTED_BACKGROUND: Color32 = CURRENT_THEME.calendar.selected_background;

/// Selected day border color
pub const CALENDAR_SELECTED_BORDER: Color32 = CURRENT_THEME.calendar.selected_border;

/// Calendar day header start color (pink)
pub const CALENDAR_HEADER_START: Color32 = CURRENT_THEME.calendar.header_start;

/// Calendar day header middle color (purple)
pub const CALENDAR_HEADER_MID: Color32 = CURRENT_THEME.calendar.header_mid;

/// Calendar day header end color (blue)
pub const CALENDAR_HEADER_END: Color32 = CURRENT_THEME.calendar.header_end;

/// Income transaction chip color (green)
pub const INCOME_CHIP: Color32 = CURRENT_THEME.calendar.income_chip;

/// Expense transaction chip color (red)
pub const EXPENSE_CHIP: Color32 = CURRENT_THEME.calendar.expense_chip;

/// Current month day background
pub const CALENDAR_CURRENT_MONTH_BG: Color32 = CURRENT_THEME.calendar.current_month_bg;

/// Filler day background (grayed out)
pub const CALENDAR_FILLER_DAY_BG: Color32 = CURRENT_THEME.calendar.filler_day_bg;

// ============================================================================
// Table Colors
// ============================================================================

/// Even table row color
pub const TABLE_ROW_EVEN: Color32 = CURRENT_THEME.table.row_even;

/// Odd table row color
pub const TABLE_ROW_ODD: Color32 = CURRENT_THEME.table.row_odd;

/// Table border color
pub const TABLE_BORDER: Color32 = CURRENT_THEME.table.border;

// ============================================================================
// Legacy Color Constants (for backward compatibility)
// ============================================================================

/// Legacy alias for gradient top
pub const GRADIENT_TOP_LEGACY: Color32 = Color32::from_rgb(255, 182, 193);

/// Legacy alias for gradient bottom
pub const GRADIENT_BOTTOM_LEGACY: Color32 = Color32::from_rgb(173, 216, 230);

/// Legacy alias for calendar background
pub const CALENDAR_BACKGROUND: Color32 = Color32::WHITE;

/// Legacy alias for day header start
pub const DAY_HEADER_START: Color32 = Color32::from_rgb(255, 182, 193);

/// Legacy alias for day header mid
pub const DAY_HEADER_MID: Color32 = Color32::from_rgb(186, 85, 211);

/// Legacy alias for day header end
pub const DAY_HEADER_END: Color32 = Color32::from_rgb(135, 206, 235);

/// Legacy alias for balance text
pub const BALANCE_TEXT: Color32 = Color32::from_rgb(128, 128, 128); 