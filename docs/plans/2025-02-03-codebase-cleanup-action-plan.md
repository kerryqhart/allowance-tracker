# Codebase Cleanup Action Plan

**Source:** `docs/audits/2025-02-03-codebase-audit.md`
**Goal:** Transform this codebase so a developer picking it up in 3 years won't be lost

---

## Phase 1: Quick Wins (1-2 days total)

Low-risk, high-clarity improvements. Do these first to build momentum.

| Task | Effort | Files |
|------|--------|-------|
| Delete 4 stale TODOs | 10 min | transaction_service.rs, day_action_overlay.rs, settings/mod.rs, interactions.rs |
| Remove ~15 blocks of commented-out code | 30 min | connection.rs, header.rs, dropdown_menu.rs, data_loading.rs, others |
| Remove incorrect `#[allow(dead_code)]` annotations | 10 min | parental_control_repository.rs |
| Remove deprecated `is_empty` field from CalendarDay | 15 min | shared/src/lib.rs |
| Delete unused `generate_id()` from models/child.rs | 5 min | models/child.rs |
| Fix misleading method name `authenticate_parental_control()` | 15 min | app_state.rs |
| Extract magic numbers to constants | 1 hour | Create constants.rs, update references |
| Standardize derive ordering | 30 min | shared/src/lib.rs |
| Add doc comments to most-used shared types | 1 hour | shared/src/lib.rs |

---

## Phase 2: Medium Effort (1-2 weeks total)

Targeted fixes that improve correctness and consistency.

### Correctness Fixes (do first)
| Task | Effort | Impact |
|------|--------|--------|
| Fix EST/EDT timezone bug | 2-3 hours | Timestamps wrong 4 months/year |
| Replace `.unwrap_or_default()` with proper error handling | 3-4 hours | Silent data loss |
| Replace unsafe `.lock().unwrap()` with timeout-based locking | 2-3 hours | Potential panics |
| Externalize parental control answer to config | 1-2 hours | Security hardcoding |

### Consolidation (reduces maintenance burden)
| Task | Effort | Impact |
|------|--------|--------|
| Consolidate `ValidationError` + `MoneyValidationError` | 2-3 hours | Duplicate types |
| Consolidate `ValidationResult` + `MoneyFormValidation` | 1-2 hours | Duplicate types |
| Move `MoneyFormState` to egui-frontend | 1-2 hours | Wrong layer |

### Code Quality
| Task | Effort | Impact |
|------|--------|--------|
| Extract button styling to helper function | 1-2 hours | DRY up ui_components.rs |
| Extract column width calculations | 1 hour | DRY up transaction_table.rs |
| Standardize logging (remove emojis) | 2-3 hours | Searchability |
| Extract complex closures in chart_renderer.rs | 2-3 hours | Readability |

---

## Phase 3: Major Refactors (2-3 weeks total)

Larger structural improvements. Do in priority order.

### Priority 1: Consolidate TransactionMapper (0.5-1 day)
**Why:** Duplicated in 3 places, maintenance nightmare
**Scope:** backend/domain/mappers.rs, calendar.rs, export_service.rs
**Approach:** Single canonical mapper, delete duplicates

### Priority 2: Reduce Deep Nesting (2-3 days)
**Why:** Code is unreadable, afraid to modify
**Scope:** app_state.rs, header.rs, transaction_table.rs, dropdown_menu.rs, goal_renderer.rs, chart_renderer.rs
**Approach:** Extract nested logic to private methods, use early returns

### Priority 3: Add UI Component Tests (2-4 days)
**Why:** Zero test coverage on complex UI logic
**Scope:** Form validation, calendar calculations, chart data prep, state transitions
**Approach:** Unit tests for logic, not full UI integration tests

### Priority 4: Complete State Management Migration (1-2 days)
**Why:** Half-migrated state creates confusion and sync bugs
**Scope:** app_state.rs, form_state.rs (TEMPORARY fields)
**Approach:** Investigate whether to finish migration or revert, then execute

### Priority 5: Fix Allowance Duplicate Detection (1-2 days)
**Why:** String matching on descriptions is a landmine
**Scope:** allowance_service.rs, storage schema
**Approach:** Use TransactionSource enum instead of description parsing

### Priority 6: Create Newtype IDs (2-3 days)
**Why:** Type safety for IDs prevents mix-up bugs
**Scope:** Pervasive - all layers
**Approach:** Create TransactionId, ChildId, GoalId newtypes

---

## Phase 4: Optional/Nice-to-Have

Defer unless time permits or need arises.

- Add separate modules for request/response types in shared
- Create shared `Timestamps` struct
- Add ID generator trait
- Add pagination cursor type
- Add doc comments for all public methods
- Add Windows/Linux font loading
- Add debug feature flag
- Extract form validation to shared utilities
- Add property-based tests for date handling
- Create visual component tests
- Add integration tests
- Document architectural decisions

---

## Immediate Next Steps

1. [ ] Review and discuss Quick Wins (Phase 1)
2. [ ] Review and discuss Medium Effort items (Phase 2)
3. [ ] Start executing Phase 1
4. [ ] Tackle Major Refactors in priority order

---

## Success Criteria

- [ ] No stale TODOs or commented-out code
- [ ] No duplicate type definitions
- [ ] Consistent error handling (no silent failures)
- [ ] UI code readable without deep nesting
- [ ] Test coverage for UI business logic
- [ ] Single source of truth for mappers
- [ ] Allowance detection uses structural data, not string matching
