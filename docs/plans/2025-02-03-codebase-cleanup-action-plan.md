# Codebase Cleanup Action Plan

**Source:** `docs/audits/2025-02-03-codebase-audit.md`
**Goal:** Transform this codebase so a developer picking it up in 3 years won't be lost

**Approach:** Mega-plan. Each item gets brainstormed, detailed plan created, farmed to separate worktree.

---

## Execution Order

1. **Phase 1:** Quick Wins (10 items)
2. **Phase 2:** Medium Effort - Correctness & Consolidation (11 items)
3. **Phase 3:** Major Refactors (6 items)
4. **Phase 4:** Polish & Documentation (12 items)

**Total items:** 39

---

## Phase 1: Quick Wins (1-2 days total)

Low-risk, high-clarity improvements. Do these first to build momentum.

| # | Task | Effort | Files |
|---|------|--------|-------|
| 1.1 | Delete 4 stale TODOs | 10 min | transaction_service.rs, day_action_overlay.rs, settings/mod.rs, interactions.rs |
| 1.2 | Remove ~15 blocks of commented-out code | 30 min | connection.rs, header.rs, dropdown_menu.rs, data_loading.rs, others |
| 1.3 | Remove incorrect `#[allow(dead_code)]` annotations | 10 min | parental_control_repository.rs |
| 1.4 | Remove deprecated `is_empty` field from CalendarDay | 15 min | shared/src/lib.rs |
| 1.5 | Delete unused `generate_id()` from models/child.rs | 5 min | models/child.rs |
| 1.6 | Fix misleading method name `authenticate_parental_control()` | 15 min | app_state.rs |
| 1.7 | Extract magic numbers to constants | 1 hour | Create constants.rs, update references |
| 1.8 | Standardize derive ordering | 30 min | shared/src/lib.rs |
| 1.9 | Add doc comments to most-used shared types | 1 hour | shared/src/lib.rs |
| 1.10 | Fix the 1 failing test (goal_calculation) | 15-30 min | goal_service.rs |

---

## Phase 2: Medium Effort (1-2 weeks total)

Targeted fixes that improve correctness and consistency.

### Correctness Fixes (do first)
| # | Task | Effort | Impact |
|---|------|--------|--------|
| 2.1 | Fix EST/EDT timezone bug | 2-3 hours | Timestamps wrong 4 months/year |
| 2.2 | Replace `.unwrap_or_default()` with proper error handling | 3-4 hours | Silent data loss |
| 2.3 | Replace unsafe `.lock().unwrap()` with timeout-based locking | 2-3 hours | Potential panics |
| 2.4 | Fix path sanitization in export_service.rs | 1-2 hours | No path traversal protection |

### Consolidation (reduces maintenance burden)
| # | Task | Effort | Impact |
|---|------|--------|--------|
| 2.5 | Consolidate `ValidationError` + `MoneyValidationError` | 2-3 hours | Duplicate types |
| 2.6 | Consolidate `ValidationResult` + `MoneyFormValidation` | 1-2 hours | Duplicate types |
| 2.7 | Move `MoneyFormState` to egui-frontend | 1-2 hours | Wrong layer |

### Code Quality
| # | Task | Effort | Impact |
|---|------|--------|--------|
| 2.8 | Extract button styling to helper function | 1-2 hours | DRY up ui_components.rs |
| 2.9 | Extract column width calculations | 1 hour | DRY up transaction_table.rs |
| 2.10 | Standardize logging (remove emojis) | 2-3 hours | Searchability |
| 2.11 | Extract complex closures in chart_renderer.rs | 2-3 hours | Readability |

---

## Phase 3: Major Refactors (2-3 weeks total)

Larger structural improvements. Do in priority order.

### 3.1 Consolidate TransactionMapper (0.5-1 day)
**Why:** Duplicated in 3 places, maintenance nightmare
**Scope:** backend/domain/mappers.rs, calendar.rs, export_service.rs
**Approach:** Single canonical mapper, delete duplicates

### 3.2 Reduce Deep Nesting (2-3 days)
**Why:** Code is unreadable, afraid to modify
**Scope:** app_state.rs, header.rs, transaction_table.rs, dropdown_menu.rs, goal_renderer.rs, chart_renderer.rs
**Approach:** Extract nested logic to private methods, use early returns

### 3.3 Add UI Component Tests (2-4 days)
**Why:** Zero test coverage on complex UI logic
**Scope:** Form validation, calendar calculations, chart data prep, state transitions
**Approach:** Unit tests for logic, not full UI integration tests

### 3.4 Complete State Management Migration (1-2 days)
**Why:** Half-migrated state creates confusion and sync bugs
**Scope:** app_state.rs, form_state.rs (TEMPORARY fields)
**Approach:** Investigate whether to finish migration or revert, then execute

### 3.5 Fix Allowance Duplicate Detection (1-2 days)
**Why:** String matching on descriptions is a landmine
**Scope:** allowance_service.rs, storage schema
**Approach:** Use TransactionSource enum instead of description parsing

### 3.6 Create Newtype IDs (2-3 days)
**Why:** Type safety for IDs prevents mix-up bugs
**Scope:** Pervasive - all layers
**Approach:** Create TransactionId, ChildId, GoalId newtypes

---

## Phase 4: Polish & Documentation (1-2 weeks total)

Final polish to make the codebase exemplary.

| # | Task | Effort | Notes |
|---|------|--------|-------|
| 4.1 | Add separate modules for request/response types in shared | 2-3 hours | Organizational cleanup |
| 4.2 | Create shared `Timestamps` struct for created_at/updated_at | 1-2 hours | DRY pattern |
| 4.3 | Add ID generator trait for consistent ID creation | 2-3 hours | Pairs with newtype IDs |
| 4.4 | Add pagination cursor type for type safety | 1-2 hours | Minor type safety |
| 4.5 | Add doc comments for all public methods | 3-4 hours | Thoroughness |
| 4.6 | Add Windows/Linux font loading | 2-3 hours | Platform support |
| 4.7 | Add debug feature flag for verbose logging | 2-3 hours | Better than emoji prefixes |
| 4.8 | Extract form validation to shared utilities | 2-3 hours | DRY |
| 4.9 | Add property-based tests for date handling | 3-4 hours | Edge case coverage |
| 4.10 | Create visual component tests | 4-6 hours | UI testing |
| 4.11 | Add integration tests for full data flow | 4-6 hours | End-to-end coverage |
| 4.12 | Document all architectural decisions | 3-4 hours | Future maintainer help |

---

## Execution Strategy

For each item:
1. **Brainstorm** - Understand scope, identify edge cases
2. **Plan** - Create detailed implementation plan
3. **Execute** - Farm to separate worktree process
4. **Review** - Verify changes, merge to main

---

## Success Criteria

- [ ] No stale TODOs or commented-out code
- [ ] No duplicate type definitions
- [ ] Consistent error handling (no silent failures)
- [ ] UI code readable without deep nesting
- [ ] Test coverage for UI business logic
- [ ] Single source of truth for mappers
- [ ] Allowance detection uses structural data, not string matching
- [ ] All public APIs documented
- [ ] Cross-platform font support
- [ ] Integration and property-based tests
- [ ] Architectural decisions documented
