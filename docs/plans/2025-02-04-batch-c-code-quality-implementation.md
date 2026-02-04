# Batch C: Code Quality Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract duplicated code patterns, remove emoji logging, and flatten complex closures for better maintainability.

**Architecture:** Pure refactoring with no functional changes. Extract helpers, consolidate constants, and simplify control flow.

**Tech Stack:** Rust, egui

---

## Task 1: Button Styling Helper (2.8)

**Files:**
- Modify: `egui-frontend/src/ui/components/ui_components.rs`

**Step 1: Add ButtonPreset enum and styled_button helper at top of file**

After the imports, add:

```rust
/// Preset button styles for consistent UI
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonPreset {
    Primary,    // Green action buttons
    Secondary,  // Gray/neutral buttons
    Danger,     // Red destructive buttons
}

impl ButtonPreset {
    /// Get the fill color for this preset
    pub fn fill_color(&self) -> egui::Color32 {
        match self {
            ButtonPreset::Primary => egui::Color32::from_rgb(76, 175, 80),
            ButtonPreset::Secondary => egui::Color32::from_rgb(158, 158, 158),
            ButtonPreset::Danger => egui::Color32::from_rgb(244, 67, 54),
        }
    }

    /// Get the hover fill color for this preset
    pub fn hover_color(&self) -> egui::Color32 {
        match self {
            ButtonPreset::Primary => egui::Color32::from_rgb(56, 142, 60),
            ButtonPreset::Secondary => egui::Color32::from_rgb(117, 117, 117),
            ButtonPreset::Danger => egui::Color32::from_rgb(211, 47, 47),
        }
    }

    /// Get the text color for this preset
    pub fn text_color(&self) -> egui::Color32 {
        egui::Color32::WHITE
    }
}

/// Create a styled button with consistent appearance
pub fn styled_button(ui: &mut egui::Ui, text: &str, preset: ButtonPreset) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(text)
            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
            .color(preset.text_color())
    )
    .fill(preset.fill_color())
    .min_size(egui::vec2(80.0, 32.0))
    .rounding(egui::Rounding::same(4.0));

    ui.add(button)
}
```

**Step 2: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/ui_components.rs
git commit -m "feat: add ButtonPreset enum and styled_button helper

Provides consistent button styling with Primary, Secondary, and Danger
presets. Future refactoring can update callers to use this helper."
```

---

## Task 2: Column Width Constants (2.9)

**Files:**
- Modify: `egui-frontend/src/ui/components/transaction_table.rs`

**Step 1: Add constants at top of file**

After imports, add:

```rust
/// Column width percentages for transaction table [date, description, amount, balance]
const TABLE_COLUMN_WIDTHS: [f32; 4] = [0.18, 0.48, 0.17, 0.17];

/// Table layout constants
const TABLE_SCROLL_BAR_SPACE: f32 = 30.0;
const TABLE_HEADER_HEIGHT: f32 = 40.0;
const TABLE_ROW_HEIGHT: f32 = 25.0;
const TABLE_CONTENT_MARGIN: f32 = 20.0;
```

**Step 2: Replace header_widths calculation (around line 75)**

Replace:
```rust
let header_widths = [
    content_width_minus_scrollbar * 0.18, // date (reduced from 0.20)
    content_width_minus_scrollbar * 0.48, // description (increased from 0.40)
    content_width_minus_scrollbar * 0.17, // amount (reduced from 0.20)
    content_width_minus_scrollbar * 0.17, // balance (reduced from 0.20)
];
```

With:
```rust
let header_widths: [f32; 4] = TABLE_COLUMN_WIDTHS.map(|w| content_width_minus_scrollbar * w);
```

**Step 3: Replace row_widths calculation (around line 154)**

Replace:
```rust
let row_widths = [
    content_width_minus_scrollbar * 0.18, // date (reduced from 0.20)
    content_width_minus_scrollbar * 0.48, // description (increased from 0.40)
    content_width_minus_scrollbar * 0.17, // amount (reduced from 0.20)
    content_width_minus_scrollbar * 0.17, // balance (reduced from 0.20)
];
```

With:
```rust
let col_widths: [f32; 4] = TABLE_COLUMN_WIDTHS.map(|w| content_width_minus_scrollbar * w);
```

And update references from `row_widths` to `col_widths`.

**Step 4: Replace magic numbers with constants**

Replace throughout the file:
- `30.0` (scroll bar) → `TABLE_SCROLL_BAR_SPACE`
- `40.0` (header height) → `TABLE_HEADER_HEIGHT`
- `25.0` (row height) → `TABLE_ROW_HEIGHT`

**Step 5: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add egui-frontend/src/ui/components/transaction_table.rs
git commit -m "refactor: extract transaction table constants

- TABLE_COLUMN_WIDTHS: single source of truth for column percentages
- Replaced duplicate header_widths/row_widths with shared constant
- Extracted magic numbers for scroll bar, header height, row height"
```

---

## Task 3: Remove Emoji Logging (2.10)

**Files:**
- Modify: 43 files across backend and frontend

**Step 1: Remove emojis from log statements using sed**

Run these commands from the worktree root:

```bash
# Find all .rs files and remove common emoji prefixes from log statements
find . -name "*.rs" -type f -exec sed -i '' \
    -e 's/📧 //g' \
    -e 's/🔍 //g' \
    -e 's/✅ //g' \
    -e 's/❌ //g' \
    -e 's/📅 //g' \
    -e 's/🎯 //g' \
    -e 's/🔒 //g' \
    -e 's/📁 //g' \
    -e 's/⚠️ //g' \
    -e 's/📊 //g' \
    -e 's/💾 //g' \
    -e 's/🗑️ //g' \
    -e 's/📋 //g' \
    -e 's/🔄 //g' \
    -e 's/📝 //g' \
    -e 's/🧪 //g' \
    -e 's/💰 //g' \
    -e 's/🎉 //g' \
    -e 's/⬅️ //g' \
    -e 's/➡️ //g' \
    {} \;
```

**Step 2: Verify no emojis remain**

Run: `grep -r "📧\|🔍\|✅\|❌\|📅\|🎯\|🔒\|📁\|⚠️\|📊\|💾" --include="*.rs" . | wc -l`
Expected: 0 (or close to 0, may need additional passes)

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Run cargo test**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add -A
git commit -m "chore: remove emoji prefixes from all log statements

Removed ~350 emoji prefixes from log statements across 43 files.
Plain text logs are more searchable and grep-friendly."
```

---

## Task 4: Extract chart_renderer Helpers (2.11)

**Files:**
- Modify: `egui-frontend/src/ui/components/chart_renderer.rs`

**Step 1: Add helper method for no-child message**

Add this method to the `impl AllowanceTrackerApp` block:

```rust
    /// Render message when no child is selected
    fn render_chart_no_child(&self, ui: &mut egui::Ui, height: f32) {
        ui.vertical_centered(|ui| {
            ui.add_space(height / 3.0);
            ui.label(egui::RichText::new("Select a child to view their balance chart")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));
        });
    }
```

**Step 2: Add helper method for loading state**

Add this method:

```rust
    /// Render loading state for chart
    fn render_chart_loading(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.vertical_centered(|ui| {
            ui.add_space(height / 3.0);
            ui.label(egui::RichText::new("Loading chart data...")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)));
        });

        // Load data on first render
        self.load_chart_data();
    }
```

**Step 3: Simplify draw_chart_section**

In `draw_chart_section`, replace the nested if/else (around lines 78-105) with:

```rust
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(chart_rect), |ui| {
            let chart_height = chart_rect.height();

            if self.get_current_child_from_backend().is_none() {
                self.render_chart_no_child(ui, chart_height);
            } else if self.chart.chart_data.is_empty() {
                self.render_chart_loading(ui, chart_height);
            } else {
                self.render_balance_chart(ui);
            }
        });
```

**Step 4: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Run cargo test**

Run: `cargo test`
Expected: All tests pass

**Step 6: Commit**

```bash
git add egui-frontend/src/ui/components/chart_renderer.rs
git commit -m "refactor: extract chart_renderer helper methods

- render_chart_no_child(): displays 'select a child' message
- render_chart_loading(): displays loading state and triggers data load
- Simplified draw_chart_section() control flow with early returns"
```

---

## Final Verification

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | head -30`
Expected: No new warnings

**Step 3: Verify commits**

Run: `git log --oneline -5`
Expected: 4 commits for the 4 tasks

---

## Summary

| Task | Change | Files |
|------|--------|-------|
| 1 | ButtonPreset enum + styled_button helper | 1 |
| 2 | Column width constants | 1 |
| 3 | Remove emoji logging | ~43 |
| 4 | Extract chart_renderer helpers | 1 |

**Total: 4 tasks, ~46 files modified**
