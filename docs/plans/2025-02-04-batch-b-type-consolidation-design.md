# Batch B: Type Consolidation Design

**Date:** 2025-02-04
**Scope:** Consolidate duplicate validation types and move UI state to correct layer

---

## Overview

| Item | Issue | Fix |
|------|-------|-----|
| 2.5 | `ValidationError` + `MoneyValidationError` duplicates | Keep Money version (more complete), rename to `ValidationError` |
| 2.6 | `ValidationResult` + `MoneyFormValidation` duplicates | Keep Money version (has suggestions), rename to `ValidationResult` |
| 2.7 | `MoneyFormState` in shared layer | Move to `egui-frontend/src/ui/form_state.rs` |

---

## 2.5 + 2.6: Validation Type Consolidation

**Canonical ValidationError (formerly MoneyValidationError):**
```rust
pub enum ValidationError {
    EmptyDescription,
    DescriptionTooLong(usize),
    EmptyAmount,
    InvalidAmountFormat(String),
    AmountNotPositive,
    AmountTooSmall(f64),
    AmountTooLarge(f64),
    AmountPrecisionTooHigh,
}
```

**Canonical ValidationResult (formerly MoneyFormValidation):**
```rust
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub cleaned_amount: Option<f64>,
    pub suggestions: Vec<String>,
}
```

**Migration for old callers:**
- `InvalidAmount(s)` → `InvalidAmountFormat(s)`
- `AmountTooLarge` → `AmountTooLarge(value)`
- `AmountTooSmall` → `AmountTooSmall(value)`
- Add `suggestions: vec![]` when constructing ValidationResult

**Files affected:**
- `shared/src/lib.rs` - delete old types, rename new types
- `backend/domain/transaction_table.rs` - update variant names
- `backend/domain/models/goal.rs` - update variant names
- `backend/domain/money_management.rs` - update type references

---

## 2.7: Move MoneyFormState

**From:** `shared/src/lib.rs`
**To:** `egui-frontend/src/ui/form_state.rs`

```rust
// egui-frontend/src/ui/form_state.rs
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoneyFormState {
    pub description: String,
    pub amount_input: String,
    pub is_submitting: bool,
    pub error_message: Option<String>,
    pub success_message: Option<String>,
    pub show_success: bool,
}
```

**Changes:**
- Remove `Serialize, Deserialize` (not needed for UI state)
- Add `Default` derive for convenience
- Update `app_state.rs` imports

---

## Testing Strategy

- All existing tests should pass after renames
- No new tests needed (pure refactoring)
