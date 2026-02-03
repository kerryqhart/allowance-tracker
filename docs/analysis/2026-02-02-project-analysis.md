# Allowance Tracker - Project Analysis Report

**Date:** 2026-02-02
**Analyst:** Claude
**Commit:** 762d1d2 (main branch, clean)

---

## 1. Project Overview

The allowance-tracker is a native Rust desktop application built with egui for helping families track children's allowances. The project is well-structured with clear separation between frontend, backend domain services, and storage layers.

### Current Statistics
- **99 Rust source files** across the workspace
- **23 files contain tests** (good coverage density)
- **315 commits** in history
- **Last activity:** Extending missed allowance lookback from 7 to 90 days

### Technology Stack
- **GUI:** egui 0.31.1 with eframe
- **Storage:** CSV-based persistence
- **Email:** Lettre 0.11 for Gmail SMTP notifications
- **Date/Time:** Chrono with proper timezone handling

---

## 2. Architecture & Testability Analysis

### 2.1 Overall Assessment: **Good with caveats**

The architecture follows clean separation principles with well-defined layers. Most components are testable, with some areas for improvement in the UI layer.

### 2.2 Backend Layer (Domain Services) - **Excellent Testability**

**Strengths:**
- Services use **dependency injection** via constructor parameters
- `TransactionService` accepts `ChildService`, `AllowanceService`, and `BalanceService` as dependencies
- `MoneyManagementService` is **stateless** and operates on passed-in services
- Business logic is isolated from I/O concerns

**Example of good testability (transaction_service.rs:481-493):**
```rust
fn create_test_service() -> (TransactionService, Arc<CsvConnection>, tempfile::TempDir) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let connection = Arc::new(CsvConnection::new(temp_dir.path()).unwrap());
    // ... services can be constructed with test data
}
```

**Observation:** The services are easy to test in isolation using temp directories for CSV storage.

### 2.3 Storage Layer - **Good Testability**

**Strengths:**
- Well-defined **traits** (`TransactionStorage`, `ChildStorage`, `AllowanceStorage`, `ParentalControlStorage`) in `backend/storage/traits.rs`
- Traits enable mocking for unit tests
- CSV repositories implement these traits cleanly
- Date parsing is **properly encapsulated** in the CSV layer (more on this below)

**Observation:** The trait-based design allows for future SQLite migration without changing domain code.

### 2.4 Frontend Layer - **Mixed Testability**

**Concerns:**

1. **Form validation duplication** (app_state.rs:480-553)
   - `validate_add_money_form()` in `AllowanceTrackerApp` duplicates logic from `MoneyManagementService`
   - Same validation rules exist in two places
   - **Size:** Medium - affects maintainability
   - **Criticality:** Low - currently consistent, but divergence risk

2. **Business logic in UI components** (app_state.rs:649-737)
   - `submit_income_transaction()` and `submit_expense_transaction()` contain transaction creation orchestration
   - These methods create service instances and call multiple services
   - **Testability impact:** Requires full app state to test transaction submission

3. **Large coordinator file** (app_coordinator.rs: 432 lines)
   - Contains rendering logic, tab controls, and refresh coordination
   - Not easily unit testable due to egui dependency
   - **Mitigation:** Most logic delegates to well-tested domain services

**Strengths:**
- State is **modular** - split into CoreAppState, UIState, CalendarState, ModalState, FormState, etc.
- State modules are focused and small (ui_state.rs: 118 lines with tests)
- Navigation logic is separated from rendering

### 2.5 Testability Recommendations

| Area | Issue | Recommendation |
|------|-------|----------------|
| Form validation | Duplicated in UI and domain | Use `MoneyManagementService.validate_*` from UI, remove UI copy |
| Transaction submission | Orchestration in UI | Extract to domain service method or use existing `add_money_complete()` |
| UI state logic | Some testable, some not | The current separation into state modules is good; continue this pattern |

---

## 3. Test Health Assessment

### 3.1 Build Status: **FAILING**

**Critical Issue:** Tests cannot compile due to dependency problem.

```
error[E0658]: `let` expressions in this position are unstable
   --> ar_archive_writer-0.5.1/src/archive_writer.rs:591:20
```

The `ar_archive_writer` crate (transitive dependency via `rav1e` for image encoding) requires unstable Rust features not available in stable Rust 1.87.0.

**Root cause:** This is likely a dependency version mismatch. The `rav1e` crate (for AV1 image encoding) pulls in `ar_archive_writer` which was recently updated to use unstable features.

**Size:** Medium - blocks all testing
**Criticality:** High - must be fixed to validate any changes

**Potential fixes:**
1. Pin `ar_archive_writer` to an older version in Cargo.toml
2. Disable the `rav1e` feature in the `image` crate if AV1 support isn't needed
3. Use nightly Rust (not recommended for production)

### 3.2 Test Inventory (from source inspection)

| Module | Test Count | Coverage Focus |
|--------|------------|----------------|
| `shared/src/lib.rs` | ~10 tests | ID generation, parsing, config validation |
| `money_management.rs` | ~45 tests | Form validation, amount parsing, date validation |
| `transaction_service.rs` | 4 tests | Transaction creation, duplicate prevention |
| `ui_state.rs` | 2 tests | Allowance refresh timing |
| `circular_days_progress/calculations.rs` | Tests present | Goal progress calculations |
| Various repositories | Multiple | CRUD operations, CSV parsing |

### 3.3 Test Quality Observations

**Strengths:**
- **Good naming:** Tests use descriptive names like `test_validate_add_money_form_empty_description`
- **Edge cases covered:** Tests for invalid formats, boundary conditions, empty inputs
- **Proper isolation:** Uses `tempfile::TempDir` for storage tests
- **Assertion quality:** Specific assertions with meaningful values

**Example of well-structured test (money_management.rs:775-784):**
```rust
#[test]
fn test_validate_add_money_form_success() {
    let service = create_test_service();
    let validation = service.validate_add_money_form("Birthday gift", "10.50");
    assert!(validation.is_valid);
    assert!(validation.errors.is_empty());
    assert_eq!(validation.cleaned_amount, Some(10.50));
}
```

**Gaps identified:**
1. **No UI component tests** - Calendar rendering, table formatting untested
2. **No integration tests** - End-to-end flows through multiple services
3. **No property-based tests** - Could benefit from fuzzing amount parsing
4. **Email service untested** - `EmailServiceWrapper` has no unit tests visible

### 3.4 Test Recommendations

| Priority | Recommendation |
|----------|----------------|
| **Blocking** | Fix `ar_archive_writer` dependency to unblock test execution |
| High | Add integration tests for transaction creation flow |
| Medium | Extract testable calculation logic from UI components |
| Low | Add property-based tests for amount parsing edge cases |

---

## 4. Documentation Health

### 4.1 Accuracy Audit

| Document | Status | Issues |
|----------|--------|--------|
| `README.md` | **Partially Stale** | Missing recent features |
| `backend/storage/csv/README.md` | **Stale** | Claims invariant is "VIOLATED" when it's fixed |
| `.cursorrules` | **Partially Stale** | "Known Issues & TODOs" lists completed items |

### 4.2 Specific Documentation Issues

#### README.md - Missing Features

The feature list doesn't mention:
- Email notifications (added in commit a3ffa35)
- Automatic periodic allowance refresh (added in commit e325f51)
- Data directory management with conflict detection

**Current README features:**
```markdown
- **Visual Calendar View**
- **Transaction Management**
- **Balance Tracking**
- **Savings Goals**
- **Child Profiles**
- **CSV Data Storage**
- **Native Desktop UI**
```

**Missing features:**
- Email Notifications (Gmail SMTP integration)
- Automatic Allowance Distribution
- Data Directory Relocation
- Transaction Deletion with Parental Controls

#### CSV README - Incorrect Status

The `backend/storage/csv/README.md` states:

```markdown
### Current Status: VIOLATED
The system currently leaks date strings from CSV -> Domain -> Frontend
```

**Reality:** The code shows this has been **FIXED**:
- `shared/src/lib.rs:12`: `pub date: DateTime<FixedOffset>` (not String)
- `backend/domain/models/transaction.rs:17`: Same proper DateTime type
- `transaction_repository.rs:73-95`: `parse_date_string()` handles conversion at CSV layer

The documentation is outdated by at least one significant refactoring.

#### .cursorrules - Outdated TODOs

The "Known Issues & TODOs" section lists:
- Transaction editing and deletion - **DONE** (deletion implemented)
- Data export/import functionality - **DONE** (export implemented)
- Multiple child support - **DONE** (implemented)

### 4.3 Completeness Audit

**Well documented:**
- Architecture overview in README
- Development commands
- Project structure
- Code guidelines in .cursorrules

**Missing or thin:**
- No API documentation for domain services
- No data model documentation (CSV file formats)
- No deployment/distribution guide
- Email configuration setup not documented

### 4.4 Documentation Recommendations

| Priority | Action |
|----------|--------|
| High | Update CSV README to mark date invariant as FIXED |
| High | Add email notifications and allowance refresh to README features |
| Medium | Update .cursorrules "Known Issues" to remove completed items |
| Low | Add data model documentation for CSV file formats |

---

## 5. Observations (Tech Debt & Cleanup)

### 5.1 Dependency Issues

**Issue:** `ar_archive_writer` requires unstable Rust features
**Location:** Transitive via `image` -> `rav1e`
**Size:** Medium
**Criticality:** High - blocks testing
**Context:** The `image` crate includes AV1 support via `rav1e`, which may not be needed for this application's use case (likely just PNG/JPEG for backgrounds).

### 5.2 Validation Duplication

**Issue:** Form validation logic exists in both UI and domain layers
**Location:**
- `egui-frontend/src/ui/app_state.rs:480-553`
- `backend/domain/money_management.rs:292-347`
**Size:** Small
**Criticality:** Low - currently consistent
**Context:** The UI validation provides immediate feedback; domain validation ensures correctness. Consider having UI call domain validation directly.

### 5.3 Deprecated Field Warning

**Issue:** `CalendarDay.is_empty` is deprecated but still present
**Location:** `shared/src/lib.rs:95-96`
**Size:** Trivial
**Criticality:** Low
**Context:** Marked deprecated in favor of `day_type`, but field still exists for backwards compatibility.

### 5.4 Hardcoded Timezone

**Issue:** Eastern Time (UTC-5) is hardcoded in multiple places
**Locations:**
- `app_state.rs:666`
- `transaction_service.rs:82,436`
- `transaction_repository.rs:85`
- `money_management.rs:661`
**Size:** Small
**Criticality:** Low (app is for personal use)
**Context:** Works fine for Eastern US users; would need refactoring for broader distribution.

### 5.5 Magic Numbers

**Issue:** Various magic numbers without named constants
**Examples:**
- `90` days lookback for missed allowances
- `45` days limit for backdated transactions
- `256` character description limit
- `70` pixel header height
**Size:** Small
**Criticality:** Low
**Context:** Most are reasonable defaults; extracting to named constants would improve readability.

### 5.6 Allowance Refresh Interval Comment Mismatch

**Issue:** Code comment doesn't match value
**Location:** `ui_state.rs:41-42`
```rust
pub allowance_refresh_interval: Duration,
// ...
allowance_refresh_interval: Duration::from_secs(60), // 1 minute (temporarily for testing)
```
**Size:** Trivial
**Criticality:** Trivial
**Context:** Comment says "temporarily for testing" - either restore to 5 minutes or update comment.

### 5.7 Unused Import Warning Potential

**Issue:** `use time::OffsetDateTime` imported but only used in one function
**Location:** `money_management.rs:15`
**Size:** Trivial
**Criticality:** Trivial
**Context:** Minor cleanup opportunity.

### 5.8 Dead Code: Frontend Directory

**Issue:** `frontend/` directory mentioned in README but appears to be archived/unused WASM frontend
**Size:** Small
**Criticality:** Low
**Context:** Could be removed or clearly marked as archived to avoid confusion.

### 5.9 TransactionMapper in Wrong Location

**Issue:** `TransactionMapper` struct defined inline in `money_management.rs:25-43`
**Size:** Small
**Criticality:** Low
**Context:** Would be cleaner in a shared mappers module, especially since similar mapping exists in `egui-frontend/src/ui/mappers.rs`.

---

## 6. Summary

### What's Working Well
1. **Clean architecture** - Good separation between UI, domain, and storage
2. **Testable backend** - Dependency injection and trait-based abstractions
3. **Comprehensive validation** - Both UI feedback and domain-level validation
4. **Modular state management** - UI state split into focused modules
5. **Date handling fixed** - Proper DateTime types throughout (despite stale docs)

### Priority Issues
1. **Critical:** Fix `ar_archive_writer` dependency to enable testing
2. **High:** Update CSV README to reflect fixed date invariant
3. **High:** Add missing features to main README
4. **Medium:** Consider extracting UI validation to use domain service

### Overall Health
The codebase is in good shape architecturally. The main issues are:
- A blocking dependency problem preventing test execution
- Stale documentation that doesn't reflect recent improvements
- Minor code organization opportunities

The project demonstrates good Rust practices and clean architecture principles. Once the dependency issue is resolved and documentation updated, it will be in excellent maintainable state.
