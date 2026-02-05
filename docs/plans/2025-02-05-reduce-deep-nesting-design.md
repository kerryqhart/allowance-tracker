# 3.2 Reduce Deep Nesting Design

**Date:** 2025-02-05
**Effort:** ~30 minutes

---

## Problem

Multiple files have deeply nested code (4-6+ levels), reducing readability and maintainability:

1. **data_directory_service.rs** (CRITICAL - 6+ levels)
   - `copy_directory_recursive()` lines 441-461 has nested Unix permissions handling

2. **UI components** (HIGH - 5-6 levels)
   - `allowance_config_modal.rs` - egui callback nesting
   - `money_transaction.rs` - modal rendering callbacks
   - `calendar_renderer/rendering.rs` - chip rendering

3. **connection.rs** (MODERATE - 4 levels)
   - Character mapping with nested match statements

---

## Phase 1: data_directory_service.rs (This PR)

### Current Code (lines 441-461)
```rust
#[cfg(unix)]
{
    if let Ok(source_perms) = std::fs::metadata(&path).map(|m| m.permissions()) {
        let mut new_mode = source_perms.mode();
        if path.to_string_lossy().contains(".git/objects/") {
            new_mode |= 0o600;
        } else {
            new_mode |= 0o200;
        }

        let new_perms = std::fs::Permissions::from_mode(new_mode);
        if let Err(_) = std::fs::set_permissions(&dest_path, new_perms) {
            if let Ok(mut perms) = std::fs::metadata(&dest_path).map(|m| m.permissions()) {
                perms.set_mode(perms.mode() | 0o200);
                let _ = std::fs::set_permissions(&dest_path, perms);
            }
        }
    }
}
```

### Solution: Extract Helper Functions

1. **Create `ensure_writable()` helper** - handles the common pattern of making a file writable
2. **Create `copy_file_with_permissions()` helper** - handles file copy + permission setting
3. **Use early returns** instead of nested else blocks

### Refactored Structure
```rust
/// Ensure a path is writable by owner (Unix only)
#[cfg(unix)]
fn ensure_writable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(mut perms) = std::fs::metadata(path).map(|m| m.permissions()) {
        perms.set_mode(perms.mode() | 0o200);
        let _ = std::fs::set_permissions(path, perms);
    }
}

/// Copy a file and set appropriate permissions
fn copy_file_with_permissions(source: &Path, dest: &Path) -> Result<()> {
    // Make destination writable if it exists
    #[cfg(unix)]
    if dest.exists() {
        ensure_writable(dest);
    }

    std::fs::copy(source, dest)?;

    #[cfg(unix)]
    set_copied_file_permissions(source, dest);

    Ok(())
}

/// Set permissions on a copied file based on source
#[cfg(unix)]
fn set_copied_file_permissions(source: &Path, dest: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let Ok(source_perms) = std::fs::metadata(source).map(|m| m.permissions()) else {
        return;
    };

    let extra_mode = if source.to_string_lossy().contains(".git/objects/") {
        0o600
    } else {
        0o200
    };

    let new_perms = Permissions::from_mode(source_perms.mode() | extra_mode);
    if std::fs::set_permissions(dest, new_perms).is_err() {
        ensure_writable(dest);
    }
}
```

---

## Files Changed

- `backend/domain/data_directory_service.rs` - extract helper functions, reduce nesting

---

## Phase 2: money_transaction.rs

### Problem

`render_money_transaction_modal()` is 236 lines with 6 levels of nesting due to egui's closure-based API. All form rendering is in one method, unlike `allowance_config_modal.rs` which already extracts helpers.

### Current Structure
```
render_money_transaction_modal()          // 236 lines, 6 levels deep
├── Modal overlay + frame setup           // ~30 lines
├── Header (title + hint)                 // ~15 lines
├── Description field + validation        // ~40 lines
├── Amount field + validation             // ~40 lines
├── Buttons (submit + cancel)             // ~55 lines
└── Backdrop click handler                // ~30 lines
```

### Solution: Extract Helper Methods

Mirror the pattern from `allowance_config_modal.rs`:

**1. Extract `render_money_transaction_form_content()`**
```rust
/// Render the form fields (description + amount) for money transaction modal
fn render_money_transaction_form_content(
    &mut self,
    ui: &mut egui::Ui,
    config: &MoneyTransactionModalConfig,
    form_state: &mut MoneyTransactionFormState,
)
```

Contains:
- Description label + character count display
- Description text input + error message
- Amount label + input with `$` prefix + error message
- Validation trigger on field change

**2. Extract `render_money_transaction_buttons()`**
```rust
/// Render action buttons for money transaction modal
/// Returns true if submit was clicked with valid form
fn render_money_transaction_buttons(
    &mut self,
    ui: &mut egui::Ui,
    config: &MoneyTransactionModalConfig,
    form_state: &mut MoneyTransactionFormState,
) -> bool
```

Contains:
- Submit button (styled with config color, disabled when invalid)
- Cancel button (clears form, closes overlay)

Returns `true` if form should be submitted.

### Refactored Structure
```
render_money_transaction_modal()             // ~100 lines
├── Modal overlay + frame setup
├── Header (title + hint)                    // Keep inline - only 15 lines
├── render_money_transaction_form_content()  // NEW - ~80 lines
├── render_money_transaction_buttons()       // NEW - ~55 lines
└── Backdrop click handler
```

### Design Decisions

1. **Keep header inline** - Only 15 lines (two labels). Extracting adds indirection without benefit.

2. **Keep backdrop handler inline** - Modal-specific logic that reads better in context. Matches `allowance_config_modal.rs` pattern.

3. **Follow existing pattern** - Consistency with `allowance_config_modal.rs` means developers familiar with one modal understand the other.

4. **Don't extract per-widget** - In egui, closures represent layout relationships. Extracting at logical component level (form, buttons) preserves this while improving readability.

### Files Changed

- `egui-frontend/src/ui/components/modals/money_transaction.rs`

---

## Phase 2b: calendar_renderer/rendering.rs (Future)

Largest opportunity for improvement. `render_with_config()` is 266 lines. Design TBD after completing money_transaction.rs.

---

## Phase 3: connection.rs - CANCELLED

Original plan mentioned "nested match statements" in character mapping. Upon review, `generate_safe_directory_name()` (lines 622-653) is already clean - just a flat match expression. No refactoring needed.
