# Allowance Tracker - Cleanup & Documentation Plan

**Date:** 2026-02-02
**Status:** Ready for implementation

---

## Overview

This plan addresses issues identified in the project analysis, focusing on unblocking testing, fixing stale documentation, and reducing code duplication.

---

## Item 1: Fix Dependencies (Critical)

**Problem:** Tests cannot compile due to `ar_archive_writer` requiring unstable Rust features. This is pulled in via `egui_extras` → `all_loaders` → `rav1e` (AV1 encoder).

**Solution:** Remove unused image format support.

**File:** `egui-frontend/Cargo.toml`

**Changes:**
```toml
# Before:
egui_extras = { version = "0.31.1", features = ["all_loaders", "image", "file"] }
image = { version = "0.25", features = ["jpeg", "png", "gif", "webp"] }

# After:
egui_extras = { version = "0.31.1", features = ["image", "file"] }
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }
```

**Verification:**
```bash
cargo test --workspace
```

---

## Item 2: Delete Stale CSV README (High)

**Problem:** `backend/storage/csv/README.md` claims the date invariant is "VIOLATED" but the code shows it's been fixed. The code has good inline comments explaining the pattern.

**Solution:** Delete the file.

**File to delete:** `backend/storage/csv/README.md`

**Verification:** Confirm code comments in `transaction_repository.rs:47-95` adequately document the pattern.

---

## Item 3: Update Main README Features (High)

**Problem:** README feature list is missing 4 implemented features.

**File:** `README.md`

**Changes:** Add to the Features section:

```markdown
- **Email Notifications** - Get alerts when transactions are added (Gmail SMTP)
- **Automatic Allowances** - Scheduled weekly allowance distribution
- **Parental Controls** - Protected access to settings and transaction deletion
- **Custom Data Location** - Choose where your data is stored
```

**Verification:** Read through full README to ensure consistency.

---

## Item 4: Remove Outdated TODOs from .cursorrules (Medium)

**Problem:** "Known Issues & TODOs" section lists completed items, creating noise.

**File:** `.cursorrules`

**Changes:** Delete lines 97-103 (the entire "Known Issues & TODOs" section):

```markdown
## Known Issues & TODOs
- Enhanced calendar interactions
- Transaction editing and deletion
- Data export/import functionality
- Advanced filtering and search
- Multiple child support
- Backup and restore features
```

**Verification:** Ensure no other references to this section exist.

---

## Item 5: Refactor UI Validation to Use Domain Service (Medium)

**Problem:** `app_state.rs` duplicates validation logic from `MoneyManagementService`, violating DRY and making the UI harder to test.

**Files:**
- `egui-frontend/src/ui/app_state.rs` (remove duplicate code)
- Possibly `egui-frontend/src/ui/state/form_state.rs` (if validation state needs adjustment)

**Changes:**

1. **Remove from `app_state.rs` (lines ~480-583):**
   - `validate_add_money_form()`
   - `clean_and_parse_amount()`
   - `has_too_many_decimal_places()`
   - `has_too_many_decimal_places_generic()`
   - `validate_money_transaction_form()`

2. **Update callers to use `MoneyManagementService`:**
   - Create a `MoneyManagementService` instance (it's stateless, cheap to create)
   - Call `service.validate_add_money_form()` or `service.validate_spend_money_form()`
   - Map `MoneyFormValidation` result to UI state

3. **Keep in `app_state.rs`:**
   - `format_currency_amount()` - simple display helper, fine to keep
   - `clear_add_money_form()` - UI state management, appropriate here
   - `auto_format_amount_field()` - UI behavior, appropriate here

**Verification:**
- Test form validation feedback works for add money flow
- Test form validation feedback works for spend money flow
- Test edge cases: empty fields, invalid amounts, too many decimals

---

## Item 6: Remove Duplicate Background Images (Low)

**Problem:** `background.jpg` exists in three locations.

**Files to delete:**
- `/background.jpg` (root)
- `/frontend/assets/background.jpg` (archived WASM frontend)

**File to keep:**
- `/egui-frontend/assets/background.jpg`

**Verification:** App still loads background correctly after deletion.

---

## Item 7: Fix Allowance Refresh Interval Comment (Low)

**Problem:** Comment says "5 minutes" but code sets 60 seconds (1 minute) with note "temporarily for testing".

**File:** `egui-frontend/src/ui/state/ui_state.rs`

**Change to 2 minutes:**
```rust
allowance_refresh_interval: Duration::from_secs(120), // 2 minutes
```

**Verification:** Observe allowance refresh behavior matches expectation.

---

## Item 8: Consolidate TransactionMapper (Low)

**Problem:** `TransactionMapper` is defined inline in `money_management.rs:25-43` but similar mapping exists in `egui-frontend/src/ui/mappers.rs`.

**Files:**
- `backend/domain/money_management.rs` (remove inline mapper)
- `egui-frontend/src/ui/mappers.rs` (verify it has equivalent functionality)

**Changes:**

1. Check if `mappers.rs` has a `to_dto()` function that converts domain Transaction to shared Transaction
2. If yes: Remove inline `TransactionMapper` from `money_management.rs`, import from mappers
3. If no: Move the mapper to a shared location both can use

**Verification:**
- `add_money_complete()` still works
- `spend_money_complete()` still works

---

## Implementation Order

1. **Item 1** - Dependency fix (unblocks everything else)
2. **Run `cargo test --workspace`** - Verify tests pass
3. **Items 2-4** - Documentation cleanup (quick wins)
4. **Item 5** - Validation refactor (most complex)
5. **Items 6-8** - Small cleanup (trivial)
6. **Final test run** - `cargo test --workspace`

---

## Success Criteria

- [ ] `cargo test --workspace` passes
- [ ] All documentation reflects current state
- [ ] No duplicate validation logic between UI and domain
- [ ] No stray duplicate files
