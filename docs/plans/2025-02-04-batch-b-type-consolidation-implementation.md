# Batch B: Type Consolidation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Consolidate duplicate validation types and move UI state to the correct layer.

**Architecture:** Rename the more complete Money* types to canonical names, update all callers, move MoneyFormState to frontend.

**Tech Stack:** Rust

---

## Task 1: Consolidate ValidationError Types (2.5)

**Files:**
- Modify: `shared/src/lib.rs:186-195,290-301`
- Modify: `backend/domain/transaction_table.rs:31,180-202,257-268,438-463`
- Modify: `backend/domain/money_management.rs:12,279-314,405-427,764-950`

**Step 1: In shared/src/lib.rs, delete the old ValidationError enum**

Delete lines 186-195:
```rust
/// Specific validation errors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationError {
    EmptyDescription,
    DescriptionTooLong(usize),
    InvalidAmount(String),
    AmountNotPositive,
    AmountTooLarge,
    AmountTooSmall,
}
```

**Step 2: In shared/src/lib.rs, rename MoneyValidationError to ValidationError**

Change line 290-301 from:
```rust
/// Specific validation errors for money forms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MoneyValidationError {
```

To:
```rust
/// Specific validation errors for form input
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationError {
```

**Step 3: Update transaction_table.rs imports**

Change line 31 from:
```rust
use shared::{Transaction, FormattedTransaction, AmountType, ValidationResult, ValidationError};
```

To:
```rust
use shared::{Transaction, FormattedTransaction, AmountType, ValidationResult, ValidationError};
```

(No change needed - name stays the same)

**Step 4: Update transaction_table.rs validate_transaction_input**

In `validate_transaction_input` method (around lines 175-207), update variant names:

Change:
```rust
errors.push(ValidationError::InvalidAmount(parse_error.to_string()));
```
To:
```rust
errors.push(ValidationError::InvalidAmountFormat(parse_error.to_string()));
```

Change:
```rust
errors.push(ValidationError::AmountTooLarge);
```
To:
```rust
errors.push(ValidationError::AmountTooLarge(1_000_000.0));
```

Change:
```rust
errors.push(ValidationError::AmountTooSmall);
```
To:
```rust
errors.push(ValidationError::AmountTooSmall(0.01));
```

**Step 5: Update transaction_table.rs validation_error_message**

In `validation_error_message` method (around lines 257-268), update pattern matching:

Change:
```rust
ValidationError::InvalidAmount(msg) => {
```
To:
```rust
ValidationError::InvalidAmountFormat(msg) => {
```

Change:
```rust
ValidationError::AmountTooLarge => "Amount is too large. Maximum is $1,000,000".to_string(),
ValidationError::AmountTooSmall => "Amount is too small. Minimum is $0.01".to_string(),
```
To:
```rust
ValidationError::AmountTooLarge(max) => format!("Amount is too large. Maximum is ${:.2}", max),
ValidationError::AmountTooSmall(min) => format!("Amount is too small. Minimum is ${:.2}", min),
```

Also add the new variants to the match:
```rust
ValidationError::EmptyAmount => "Please enter an amount".to_string(),
ValidationError::AmountPrecisionTooHigh => "Amount has too many decimal places".to_string(),
```

**Step 6: Update transaction_table.rs tests**

Update test assertions (around lines 438-463):

Change:
```rust
assert!(matches!(result.errors[0], ValidationError::InvalidAmount(_)));
```
To:
```rust
assert!(matches!(result.errors[0], ValidationError::InvalidAmountFormat(_)));
```

**Step 7: Update money_management.rs imports**

Change line 11-12 from:
```rust
    CreateTransactionRequest, MoneyFormState, MoneyFormValidation,
    MoneyManagementConfig, MoneyValidationError,
```
To:
```rust
    CreateTransactionRequest, MoneyFormState, ValidationResult,
    MoneyManagementConfig, ValidationError,
```

**Step 8: Update all MoneyValidationError references in money_management.rs**

Use find-and-replace to change all occurrences:
- `MoneyValidationError::` → `ValidationError::`

This affects approximately 30 occurrences throughout the file.

**Step 9: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 10: Commit**

```bash
git add shared/src/lib.rs backend/domain/transaction_table.rs backend/domain/money_management.rs
git commit -m "refactor: consolidate ValidationError types

Merged ValidationError and MoneyValidationError into single ValidationError.
The Money version was more complete with additional variants:
- EmptyAmount
- InvalidAmountFormat (renamed from InvalidAmount)
- AmountTooSmall(f64) / AmountTooLarge(f64) (now carry values)
- AmountPrecisionTooHigh"
```

---

## Task 2: Consolidate ValidationResult Types (2.6)

**Files:**
- Modify: `shared/src/lib.rs:178-184,281-288`
- Modify: `backend/domain/transaction_table.rs:207-211`
- Modify: `backend/domain/money_management.rs` (MoneyFormValidation → ValidationResult)

**Step 1: In shared/src/lib.rs, delete the old ValidationResult struct**

Delete lines 178-184:
```rust
/// Validation result for transaction form input
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub cleaned_amount: Option<f64>,
}
```

**Step 2: In shared/src/lib.rs, rename MoneyFormValidation to ValidationResult**

Change lines 281-288 from:
```rust
/// Form validation result specific to money management
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoneyFormValidation {
    pub is_valid: bool,
    pub errors: Vec<MoneyValidationError>,
    pub cleaned_amount: Option<f64>,
    pub suggestions: Vec<String>,
}
```

To:
```rust
/// Form validation result with errors and suggestions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub cleaned_amount: Option<f64>,
    pub suggestions: Vec<String>,
}
```

**Step 3: Update transaction_table.rs to add suggestions field**

In `validate_transaction_input` method, change the return statement (around line 207):

From:
```rust
        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            cleaned_amount,
        }
```

To:
```rust
        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            cleaned_amount,
            suggestions: vec![],
        }
```

**Step 4: Update money_management.rs to use ValidationResult**

Use find-and-replace to change all occurrences:
- `MoneyFormValidation` → `ValidationResult`

This affects approximately 15 occurrences throughout the file, including:
- Function return types
- Variable types
- Struct construction

**Step 5: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 6: Run cargo test**

Run: `cargo test`
Expected: All tests pass

**Step 7: Commit**

```bash
git add shared/src/lib.rs backend/domain/transaction_table.rs backend/domain/money_management.rs
git commit -m "refactor: consolidate ValidationResult types

Merged ValidationResult and MoneyFormValidation into single ValidationResult.
The Money version had an additional 'suggestions' field which is now
part of the canonical type."
```

---

## Task 3: Move MoneyFormState to Frontend (2.7)

**Files:**
- Create: `egui-frontend/src/ui/form_state.rs`
- Modify: `egui-frontend/src/ui/mod.rs`
- Modify: `shared/src/lib.rs:303-312`
- Modify: `backend/domain/money_management.rs:11,260-268,437-466`

**Step 1: Create form_state.rs in frontend**

Create `egui-frontend/src/ui/form_state.rs`:

```rust
//! Form state types for UI components.

/// State for managing money input forms
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoneyFormState {
    pub description: String,
    pub amount_input: String,
    pub is_submitting: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub show_success: bool,
}

impl MoneyFormState {
    /// Create a new empty form state
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the form after successful submission
    pub fn clear_with_success(&mut self, message: String) {
        self.description.clear();
        self.amount_input.clear();
        self.is_submitting = false;
        self.error_message = None;
        self.success_message = Some(message);
        self.show_success = true;
    }

    /// Set form to submitting state
    pub fn set_submitting(&mut self) {
        self.is_submitting = true;
        self.error_message = None;
    }

    /// Set an error on the form
    pub fn set_error(&mut self, message: String) {
        self.is_submitting = false;
        self.error_message = Some(message);
    }

    /// Update form state from validation result
    pub fn apply_validation(&mut self, validation: &shared::ValidationResult) {
        if !validation.is_valid {
            if let Some(error) = validation.errors.first() {
                self.error_message = Some(format!("{:?}", error));
            }
        } else {
            self.error_message = None;
        }
    }
}
```

**Step 2: Add module to ui/mod.rs**

In `egui-frontend/src/ui/mod.rs`, add after line 1:

```rust
pub mod form_state;
```

And add to exports:
```rust
pub use form_state::*;
```

**Step 3: Remove MoneyFormState from shared/src/lib.rs**

Delete lines 303-312:
```rust
/// State for managing money input forms
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoneyFormState {
    pub description: String,
    pub amount_input: String,
    pub is_submitting: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub show_success: bool,
}
```

**Step 4: Update money_management.rs imports**

Change line 11 from:
```rust
    CreateTransactionRequest, MoneyFormState, ValidationResult,
```
To:
```rust
    CreateTransactionRequest, ValidationResult,
```

**Step 5: Remove MoneyFormState helper methods from money_management.rs**

Delete these methods from `MoneyManagementService` (around lines 260-268, 437-466):

- `create_form_state()`
- `update_form_state_with_validation()`
- `clear_form_after_success()`
- `set_form_submitting()`
- `set_form_error()`

These are now methods on `MoneyFormState` itself in the frontend.

**Step 6: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

**Step 7: Run cargo test**

Run: `cargo test`
Expected: All tests pass

**Step 8: Commit**

```bash
git add egui-frontend/src/ui/form_state.rs egui-frontend/src/ui/mod.rs shared/src/lib.rs backend/domain/money_management.rs
git commit -m "refactor: move MoneyFormState to frontend layer

MoneyFormState is UI state (input fields, loading spinners, messages)
and doesn't belong in the shared layer. Moved to egui-frontend/src/ui/form_state.rs
with helper methods as instance methods instead of service methods."
```

---

## Final Verification

**Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | head -20`
Expected: No new warnings

**Step 3: Verify commits**

Run: `git log --oneline -4`
Expected: 3 commits for the 3 tasks

---

## Summary

| Task | Change | Files |
|------|--------|-------|
| 1 | Consolidate ValidationError | 3 |
| 2 | Consolidate ValidationResult | 3 |
| 3 | Move MoneyFormState to frontend | 4 (1 new) |

**Total: 3 tasks, 5 files modified, 1 file created**
