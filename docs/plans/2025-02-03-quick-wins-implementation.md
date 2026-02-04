# Quick Wins Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove stale TODOs, commented-out code, incorrect annotations, deprecated fields, unused functions, and fix a failing test to clean up technical debt.

**Architecture:** Sequential deletions and minor fixes across backend and frontend layers. No architectural changes - purely hygiene work.

**Tech Stack:** Rust, egui

---

## Verification Cadence

- `cargo check` after each task
- `cargo test` after each group (A, B, C, D)

---

## Group A: Deletions

### Task 1.1: Delete 4 stale TODOs

**Files:**
- Modify: `backend/domain/transaction_service.rs:197`
- Modify: `egui-frontend/src/ui/components/modals/day_action_overlay.rs:376`
- Modify: `egui-frontend/src/ui/components/settings/mod.rs:36`
- Modify: `egui-frontend/src/ui/components/calendar_renderer/interactions.rs:94`

**Step 1: Remove TODO from transaction_service.rs**

Find and delete this line:
```rust
        // TODO: reintegrate future allowances generation once domain models are finished
```

**Step 2: Remove TODO from day_action_overlay.rs**

Find and delete this line:
```rust
                                                    // TODO: Implement add money logic in next phase
```

**Step 3: Remove TODO from settings/mod.rs**

Find and delete this line:
```rust
// TODO: Future modules to implement (allowance_config_modal, data_directory_modal)
```

**Step 4: Remove TODO from interactions.rs**

Find and delete this line:
```rust
        // TODO: Implement actual deletion logic
```

**Step 5: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add backend/domain/transaction_service.rs egui-frontend/src/ui/components/modals/day_action_overlay.rs egui-frontend/src/ui/components/settings/mod.rs egui-frontend/src/ui/components/calendar_renderer/interactions.rs
git commit -m "chore: delete 4 stale TODO comments

These TODOs referenced work that either happened elsewhere or is no longer needed:
- Future allowances generation (already implemented)
- Add money logic (implemented)
- Future modules (already implemented)
- Deletion logic (implemented elsewhere)"
```

---

### Task 1.2: Remove commented-out debug code

**Files:**
- Modify: `backend/storage/csv/connection.rs:75-115`

**Step 1: Remove commented-out debug lines from connection.rs**

Remove these 12 commented-out debug lines from `get_child_directory()`:

```rust
        // debug!("🔍 get_child_directory called for: {}", child_name);
```

```rust
        // debug!("🔍 Base directory locked: {}", base_dir.display());
```

```rust
        // debug!("🔍 Child directory path: {}", child_dir.display());
```

```rust
        // debug!("🔍 Redirect file path: {}", redirect_file.display());
```

```rust
        // debug!("🔍 Checking if redirect file exists...");
```

```rust
              // debug!("📁 Redirect file exists, reading it...");
```

```rust
                                          // debug!("📄 Redirect file content: {}", redirected_path);
```

```rust
                    // debug!("🔍 Checking if redirected path exists: {}", path.display());
```

```rust
            // debug!("📁 No redirect file found");
```

```rust
        // debug!("✅ Using default child directory: {}", child_dir.display());
```

**Step 2: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add backend/storage/csv/connection.rs
git commit -m "chore: remove commented-out debug logging from connection.rs

Removed ~12 lines of commented-out emoji debug statements from get_child_directory().
These were debugging artifacts that should not be in production code."
```

---

### Task 1.3: Remove incorrect #[allow(dead_code)] annotations

**Files:**
- Modify: `backend/storage/csv/parental_control_repository.rs:103,137,165,217`

**Step 1: Remove incorrect annotations**

Remove these 4 `#[allow(dead_code)]` annotations (the methods ARE used internally):

Line 103 - before `get_all_child_directories`:
```rust
    #[allow(dead_code)]
```

Line 137 - before `get_next_id`:
```rust
    #[allow(dead_code)]
```

Line 165 - before `append_parental_control_attempt`:
```rust
    #[allow(dead_code)]
```

Line 217 - before `load_parental_control_attempts_from_directory`:
```rust
    #[allow(dead_code)]
```

**Step 2: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully with NO dead_code warnings for these methods (they are used at lines 272, 283, 303, 308, 314)

**Step 3: Commit**

```bash
git add backend/storage/csv/parental_control_repository.rs
git commit -m "chore: remove incorrect #[allow(dead_code)] annotations

These 4 private methods ARE used internally in the same file:
- get_all_child_directories (called at line 308)
- get_next_id (called at line 272)
- append_parental_control_attempt (called at line 283)
- load_parental_control_attempts_from_directory (called at lines 303, 314)"
```

---

### Task 1.5: Delete unused generate_id() from models/child.rs

**Files:**
- Modify: `backend/domain/models/child.rs:17-21`

**Step 1: Remove unused function**

Delete the entire `impl Child` block with `generate_id`:

```rust
impl Child {
    /// Generate a unique ID for a child
    pub fn generate_id(timestamp_millis: u64) -> String {
        format!("child::{}", timestamp_millis)
    }
}
```

Note: The shared/src/lib.rs already has `Child::generate_id()` which is the one actually used.

**Step 2: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully (no callers of this function)

**Step 3: Commit**

```bash
git add backend/domain/models/child.rs
git commit -m "chore: delete unused generate_id() from domain Child model

This function was a duplicate of shared::Child::generate_id() which is
the canonical implementation. No code was calling the domain model version."
```

---

### Task: Run Group A tests

**Step 1: Run full test suite**

Run: `cargo test`
Expected: 179 passing, 1 failing (goal_calculation - pre-existing)

---

## Group B: Structural Changes

### Task 1.4: Remove deprecated is_empty field from CalendarDay

**Files:**
- Modify: `shared/src/lib.rs:95-96`
- Modify: `backend/domain/calendar.rs:298,316,337`

**Step 1: Remove the deprecated field from CalendarDay struct**

In `shared/src/lib.rs`, remove these 2 lines from the `CalendarDay` struct:

```rust
    #[deprecated(note = "Use day_type instead of is_empty")]
    pub is_empty: bool, // For padding days before/after month
```

**Step 2: Run cargo check to find usages**

Run: `cargo check 2>&1 | head -50`
Expected: Compilation errors showing where `is_empty` is set

**Step 3: Remove is_empty assignments in calendar.rs**

In `backend/domain/calendar.rs`, remove `is_empty: true,` and `is_empty: false,` from the 3 CalendarDay constructions:

Line ~298 (padding day before month):
```rust
                is_empty: true,
```

Line ~316 (normal day):
```rust
                is_empty: false,
```

Line ~337 (padding day after month):
```rust
                    is_empty: true,
```

**Step 4: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add shared/src/lib.rs backend/domain/calendar.rs
git commit -m "chore: remove deprecated is_empty field from CalendarDay

The is_empty field was deprecated in favor of CalendarDayType::Padding.
Removed the field and all 3 usages in calendar.rs."
```

---

### Task 1.6: Fix misleading method name authenticate_parental_control()

**Files:**
- Modify: `egui-frontend/src/ui/app_state.rs:202-217`

**Step 1: Rename method and update doc comment**

Change:
```rust
    /// Mark authentication as successful
    pub fn authenticate_parental_control(&mut self) {
```

To:
```rust
    /// Mark parental control as authenticated and proceed to question stage
    pub fn mark_parental_control_authenticated(&mut self) {
```

**Step 2: Search for callers**

Run: `grep -rn "authenticate_parental_control" egui-frontend/src/`
Expected: Only the definition (no callers - method appears unused)

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add egui-frontend/src/ui/app_state.rs
git commit -m "chore: rename misleading authenticate_parental_control method

Renamed to mark_parental_control_authenticated to accurately describe
what the method does: it marks auth as complete and transitions state,
rather than performing authentication."
```

---

### Task 1.8: Standardize derive ordering

**Files:**
- Modify: `shared/src/lib.rs`

**Step 1: Identify non-standard derive orderings**

The standard order is: `Debug, Clone, PartialEq, Serialize, Deserialize`

Search for derives that don't follow this order:
```bash
grep -n "#\[derive" shared/src/lib.rs
```

**Step 2: Standardize each derive**

For each `#[derive(...)]` that has these traits in a different order, reorder to:
`#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`

Common reorderings needed:
- `Clone, Debug` → `Debug, Clone`
- `Serialize, Deserialize, Debug, Clone` → `Debug, Clone, PartialEq, Serialize, Deserialize`

Note: Only reorder - don't add traits that aren't already there.

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add shared/src/lib.rs
git commit -m "chore: standardize derive ordering in shared types

Reordered derives to consistent: Debug, Clone, PartialEq, Serialize, Deserialize
This makes the codebase more consistent and easier to scan."
```

---

### Task: Run Group B tests

**Step 1: Run full test suite**

Run: `cargo test`
Expected: 179 passing, 1 failing (goal_calculation - pre-existing)

---

## Group C: Additions

### Task 1.7: Extract magic numbers to constants

**Files:**
- Create: `egui-frontend/src/ui/constants.rs`
- Modify: `egui-frontend/src/ui/mod.rs`

**Step 1: Create constants.rs with UI constants**

Create `egui-frontend/src/ui/constants.rs`:

```rust
//! UI constants for consistent styling across the application.

// Font sizes
pub const FONT_SIZE_SMALL: f32 = 14.0;
pub const FONT_SIZE_NORMAL: f32 = 16.0;
pub const FONT_SIZE_MEDIUM: f32 = 18.0;
pub const FONT_SIZE_LARGE: f32 = 20.0;
pub const FONT_SIZE_XLARGE: f32 = 24.0;
pub const FONT_SIZE_HEADER: f32 = 28.0;

// Spacing
pub const SPACING_SMALL: f32 = 5.0;
pub const SPACING_NORMAL: f32 = 10.0;
pub const SPACING_MEDIUM: f32 = 15.0;
pub const SPACING_LARGE: f32 = 20.0;
pub const SPACING_XLARGE: f32 = 35.0;

// Component dimensions
pub const BUTTON_HEIGHT: f32 = 32.0;
pub const INPUT_HEIGHT: f32 = 28.0;
pub const MODAL_WIDTH: f32 = 400.0;
pub const MODAL_PADDING: f32 = 20.0;

// Animation and timing
pub const ANIMATION_DURATION_MS: u64 = 200;
pub const DEBOUNCE_DELAY_MS: u64 = 300;

// Pagination
pub const DEFAULT_PAGE_SIZE: usize = 50;
pub const MAX_PAGE_SIZE: usize = 100;
```

**Step 2: Add module to mod.rs**

In `egui-frontend/src/ui/mod.rs`, add:

```rust
pub mod constants;
```

**Step 3: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add egui-frontend/src/ui/constants.rs egui-frontend/src/ui/mod.rs
git commit -m "feat: add constants.rs for UI magic numbers

Added centralized constants for:
- Font sizes (14-28px)
- Spacing (5-35px)
- Component dimensions
- Animation timing
- Pagination limits

This provides a single source of truth for UI styling values.
Actual refactoring to use these constants is a separate task."
```

---

### Task 1.9: Add doc comments to most-used shared types

**Files:**
- Modify: `shared/src/lib.rs`

**Step 1: Add doc comments to Transaction**

Add above the Transaction struct:

```rust
/// A financial transaction representing money in or out.
///
/// Transactions are the core data type for tracking allowance money.
/// Each transaction has a unique ID, amount (positive for income, negative for expenses),
/// description, date, and running balance after the transaction.
///
/// # Fields
/// - `id`: Unique identifier in format "transaction::{income|expense}::{timestamp_ms}"
/// - `amount`: Positive for deposits/allowances, negative for spending
/// - `description`: User-provided description of the transaction
/// - `date`: When the transaction occurred (with timezone)
/// - `balance`: Running balance after this transaction
/// - `transaction_type`: Categorizes as Income or Expense for display
/// - `source`: Origin of the transaction (Manual, Allowance, etc.)
```

**Step 2: Add doc comments to Child**

Add above the Child struct:

```rust
/// A child whose allowance is being tracked.
///
/// Each child has their own transaction history, goals, and allowance configuration.
/// The child's data is stored in a dedicated directory named after their sanitized name.
///
/// # Fields
/// - `id`: Unique identifier in format "child::{timestamp_ms}"
/// - `name`: Display name of the child
/// - `birthdate`: Used for age-appropriate features and display
/// - `created_at`/`updated_at`: Audit timestamps
```

**Step 3: Add doc comments to CalendarDay**

Add above the CalendarDay struct:

```rust
/// A single day in the calendar view.
///
/// Represents one cell in the monthly calendar, containing all transactions
/// for that day and summary information for display.
///
/// # Fields
/// - `date`: The calendar date (1-31)
/// - `month`/`year`: For context when rendering across month boundaries
/// - `balance`: Running balance at end of this day
/// - `transactions`: All transactions that occurred on this day
/// - `day_type`: Whether this is a normal day, today, or padding
```

**Step 4: Add doc comments to Goal**

Add above the Goal struct:

```rust
/// A savings goal the child is working toward.
///
/// Goals track progress toward a target amount and can calculate
/// estimated completion dates based on allowance frequency.
///
/// # Fields
/// - `id`: Unique identifier in format "goal::{child_id}::{timestamp_ms}"
/// - `child_id`: The child this goal belongs to
/// - `description`: What the child is saving for
/// - `target_amount`: How much money is needed
/// - `state`: Active, Completed, or Cancelled
/// - `created_at`/`completed_at`: Lifecycle timestamps
```

**Step 5: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Commit**

```bash
git add shared/src/lib.rs
git commit -m "docs: add doc comments to most-used shared types

Added comprehensive documentation to:
- Transaction: core financial data type
- Child: represents a tracked child
- CalendarDay: calendar view cell
- Goal: savings target

These are the most frequently used types and now have clear
documentation for future maintainers."
```

---

### Task: Run Group C tests

**Step 1: Run full test suite**

Run: `cargo test`
Expected: 179 passing, 1 failing (goal_calculation - pre-existing)

---

## Group D: Bug Fix

### Task 1.10: Fix the failing test (goal_calculation)

**Files:**
- Modify: `backend/domain/goal_service.rs:788-809`

**Step 1: Understand the test failure**

The test expects `allowances_needed = 4` but gets `5`.

Test setup:
- Balance: $10.00 (from initial allowance)
- Target: $30.00
- Allowance amount: $5.00
- Amount needed: $30.00 - $10.00 = $20.00
- Expected allowances: ceil($20.00 / $5.00) = 4

**Step 2: Investigate the calculation logic**

Read the `calculate_allowances_needed` function to understand where the off-by-one comes from.

Run: `grep -n "allowances_needed" backend/domain/goal_service.rs | head -20`

**Step 3: Run the test with debug output**

Run: `RUST_BACKTRACE=1 cargo test test_goal_calculation -- --nocapture 2>&1 | tail -50`

**Step 4: Fix the calculation or test expectation**

Based on investigation, either:
- Fix the calculation logic if it's wrong
- Fix the test expectation if the calculation is correct

**Step 5: Run cargo test**

Run: `cargo test test_goal_calculation`
Expected: PASS

**Step 6: Run full test suite**

Run: `cargo test`
Expected: 180 passing, 0 failing

**Step 7: Commit**

```bash
git add backend/domain/goal_service.rs
git commit -m "fix: correct goal calculation test

[Description of what was fixed - calculation or test expectation]"
```

---

## Final Verification

### Run all tests and verify clean state

**Step 1: Run full test suite**

Run: `cargo test`
Expected: 180 passing, 0 failing

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | head -50`
Expected: No warnings (or document any pre-existing warnings)

**Step 3: Verify git status**

Run: `git log --oneline -10`
Expected: 10 commits for Quick Wins tasks

---

## Summary

| Task | Description | Files Modified |
|------|-------------|----------------|
| 1.1 | Delete 4 stale TODOs | 4 files |
| 1.2 | Remove commented-out debug code | 1 file |
| 1.3 | Remove incorrect dead_code annotations | 1 file |
| 1.5 | Delete unused generate_id() | 1 file |
| 1.4 | Remove deprecated is_empty field | 2 files |
| 1.6 | Fix misleading method name | 1 file |
| 1.8 | Standardize derive ordering | 1 file |
| 1.7 | Extract magic numbers to constants | 2 files (1 new) |
| 1.9 | Add doc comments to shared types | 1 file |
| 1.10 | Fix failing test | 1 file |

**Total: 10 tasks, ~15 files modified**
