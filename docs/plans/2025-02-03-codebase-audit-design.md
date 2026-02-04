# Codebase Audit Design

**Goal:** Comprehensive, exhaustive audit of the entire codebase to identify maintainability issues, dead code, inconsistent patterns, and technical debt. Produce a detailed report that enables prioritized remediation.

**Success Criteria:** A future developer picking up this codebase in 3 years won't be lost for context.

---

## Investigation Criteria

For every source file, assess against these categories:

### Code Clarity
- **Naming**: Are functions, variables, types named descriptively? Can you understand intent without reading implementation?
- **Comments**: Are complex sections explained? Are there misleading/stale comments?
- **Function length**: Are functions doing one thing, or sprawling multi-purpose blocks?
- **Cyclomatic complexity**: Deep nesting, long conditional chains?

### Consistency
- **Pattern adherence**: Does this file follow the same patterns as similar files?
- **Error handling**: Consistent use of Result/Option? Panics where there shouldn't be?
- **Logging**: Consistent log levels and message formats?

### Duplication & Abstraction
- **Copy-paste code**: Similar logic repeated across files?
- **Premature abstraction**: Over-engineered for no reason?
- **Missing abstraction**: Should common code be extracted?

### Correctness Risks
- **Unsafe unwrap/expect**: Panics waiting to happen?
- **State management**: Mutable state that could get out of sync?
- **Edge cases**: Obvious unhandled scenarios?

### Testability & Tests
- **Test coverage**: Does the file have corresponding tests?
- **Test quality**: Are tests meaningful or just checking happy path?
- **Testable design**: Is the code structured for easy testing?

### Documentation
- **Module-level docs**: Does the file explain its purpose?
- **Public API docs**: Are public functions documented?
- **Architecture fit**: Is it clear how this file fits the larger system?

### Hygiene
- **Dangling TODOs**: Stale TODO/FIXME/HACK comments to remove
- **Dead code**: Unused functions, unreachable paths, vestigial features
- **Commented-out code**: Old code left in comments

### Dependency Health
- **Version currency**: How far behind latest?
- **Maintenance status**: Actively maintained or abandoned?
- **Security advisories**: Known vulnerabilities?
- **Redundancy**: Multiple crates doing the same thing?

---

## Scope

### Backend Domain Layer (~12 files)
- `backend/domain/transaction_service.rs`
- `backend/domain/child_service.rs`
- `backend/domain/balance_service.rs`
- `backend/domain/allowance_service.rs`
- `backend/domain/goal_service.rs`
- `backend/domain/parental_control_service.rs`
- `backend/domain/email_service.rs`
- `backend/domain/export_service.rs`
- `backend/domain/data_directory_service.rs`
- `backend/domain/calendar.rs`
- `backend/domain/money_management.rs`
- `backend/domain/commands.rs`
- `backend/domain/models/` (any files within)
- `backend/mod.rs`

### Backend Storage Layer (~10 files)
- `backend/storage/traits.rs`
- `backend/storage/csv/connection.rs`
- `backend/storage/csv/transaction_repository.rs`
- `backend/storage/csv/child_repository.rs`
- `backend/storage/csv/allowance_repository.rs`
- `backend/storage/csv/goal_repository.rs`
- `backend/storage/csv/parental_control_repository.rs`
- `backend/storage/csv/global_config_repository.rs`
- `backend/storage/git/mod.rs` (and any subfiles)

### Shared Types (~1 file)
- `shared/src/lib.rs`

### Frontend UI Layer (~25+ files)
- `egui-frontend/src/main.rs`
- `egui-frontend/src/lib.rs`
- `egui-frontend/src/ui/app.rs`
- `egui-frontend/src/ui/app_state.rs`
- `egui-frontend/src/ui/app_coordinator.rs`
- `egui-frontend/src/ui/state/*.rs` (all state modules)
- `egui-frontend/src/ui/components/*.rs` (all components)
- `egui-frontend/src/ui/components/calendar_renderer/*.rs`
- `egui-frontend/src/ui/components/settings/*.rs`
- `egui-frontend/src/ui/components/modals/*.rs`

### Configuration & Dependencies
- `Cargo.toml` (workspace root)
- `egui-frontend/Cargo.toml`
- `shared/Cargo.toml`

### Tests
- Any `tests/` directories
- Any `#[cfg(test)]` modules

---

## Report Structure

```
# Codebase Audit Report

## Executive Summary
- Overall health assessment (1-2 paragraphs)
- Top 5 most urgent issues
- Estimated effort categories (quick wins, medium, major refactors)

## Dependency Health
- Table of all dependencies with version status
- Flagged: outdated, unmaintained, redundant, security issues
- Recommendations

## Dead Code Inventory
- Unused public functions (with evidence)
- Unreachable code paths
- Vestigial features
- Commented-out code blocks
- Stale TODOs/FIXMEs (listed for removal)

## File-by-File Findings

### Backend Domain Layer
#### transaction_service.rs
- **Clarity**: [findings]
- **Consistency**: [findings]
- **Duplication**: [findings]
- **Correctness Risks**: [findings]
- **Tests**: [findings]
- **Documentation**: [findings]
- **Issues**: [numbered list of specific issues]

[...repeat for each file...]

### Backend Storage Layer
[same structure]

### Shared Types
[same structure]

### Frontend UI Layer
[same structure]

## Pattern Summary
- Cross-cutting issues that appear in multiple files
- Systemic problems vs one-off issues

## Prioritized Action Plan

### Quick Wins (< 1 hour each)
### Medium Effort (1-4 hours each)
### Major Refactors (days)
### Optional/Nice-to-Have
```

---

## Execution Approach

1. **Dependency audit first** - Run `cargo outdated`, check crate status, security advisories. Mechanical baseline.

2. **Dead code detection** - Use compiler warnings (`#[warn(dead_code)]`), grep for unused public functions, trace call paths from UI entry points.

3. **File-by-file read** - Systematically read each file against the criteria checklist. Use parallel agents where possible.

4. **Cross-reference pass** - After individual files, look for patterns across files.

5. **Synthesize report** - Compile findings into report structure, prioritize, write recommendations.

**Output location:** `docs/audits/2025-02-03-codebase-audit.md`
