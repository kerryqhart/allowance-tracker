# Batch C: Code Quality / DRY Design

**Date:** 2025-02-04
**Scope:** Extract duplicated code, remove emoji logging, flatten complex closures

---

## Overview

| Item | Issue | Scope | Approach |
|------|-------|-------|----------|
| 2.8 | Button styling repeated 4x | ui_components.rs | Create `ButtonPreset` enum and `styled_button()` helper |
| 2.9 | Column width duplication | transaction_table.rs | Extract constants + `render_table_cell()` helper |
| 2.10 | Emoji logging (352 occurrences) | 43 files | Find-and-replace to remove all emoji prefixes |
| 2.11 | Complex closures in chart_renderer | chart_renderer.rs | Extract 3-4 helper methods to flatten nesting |

---

## 2.8: Button Styling Helper

**Problem:** Same button styling pattern repeated 4+ times with font, fill, stroke, min_size, rounding.

**Solution:** Create `ButtonPreset` enum with variants:
- `Primary` - Green action buttons
- `Secondary` - Gray/neutral buttons
- `Danger` - Red delete buttons
- `Disabled` - Grayed out

Create `styled_button(ui, text, preset) -> Response` helper function.

**File:** `egui-frontend/src/ui/components/ui_components.rs`

---

## 2.9: Column Width Extraction

**Problem:**
1. Column widths defined identically twice (header_widths and row_widths)
2. Cell rendering code duplicated 4 times (~30 lines each)

**Solution:**
1. Extract column percentages to constant: `TABLE_COLUMN_WIDTHS: [f32; 4] = [0.18, 0.48, 0.17, 0.17]`
2. Create `render_table_cell()` helper for common background/border painting

**File:** `egui-frontend/src/ui/components/transaction_table.rs`

---

## 2.10: Remove Emoji Logging

**Problem:** 352 emoji occurrences across 43 files reduce log searchability.

**Solution:** Find-and-replace to remove emoji prefixes:
- `"📧 "` → `""`
- `"🔍 "` → `""`
- `"✅ "` → `""`
- `"❌ "` → `""`
- `"📅 "` → `""`
- `"🎯 "` → `""`
- `"🔒 "` → `""`
- `"📁 "` → `""`
- `"⚠️ "` → `""`
- Plus any others found

**Approach:** Single commit, use sed or manual find-replace, verify with `cargo check`.

**Files:** 43 files across backend and frontend

---

## 2.11: Extract chart_renderer Closures

**Problem:** 487 lines with deeply nested closures, hard to follow control flow.

**Solution:** Extract helper methods:
- `render_no_child_message(ui)` - "Select a child" state
- `render_loading_state(ui)` - Loading spinner/message
- Keep `render_balance_chart(ui)` - actual chart (already exists)

Optionally add `ChartState` enum to clarify the state machine.

**File:** `egui-frontend/src/ui/components/chart_renderer.rs`

---

## Testing Strategy

- All changes are refactoring with no functional changes
- `cargo check` after each task
- `cargo test` after all tasks complete
- Visual inspection of UI to ensure no regressions
