# Dependency Reduction Design

**Goal:** Reduce dependency footprint by ~16% through removal of unused/redundant crates

**Current state:** 708 dependency lines, 40 duplicate crates

## Changes

### 1. Remove `lettre_email` (~75 deps)

**Rationale:** This crate is listed in Cargo.toml but never used. Only `lettre v0.11` is used (in `email_service.rs`). The old `lettre_email` pulls in ancient versions of multiple crates.

**Duplicates eliminated:**
- `lettre v0.9` (we keep `v0.11`)
- `base64 v0.9`, `v0.10` (we keep `v0.21`, `v0.22`)
- `uuid v0.7` (we keep `v1.17`)
- `rand v0.4`, `v0.6`
- `time v0.1`

**Action:** Remove `lettre_email = "0.9"` from `egui-frontend/Cargo.toml`

### 2. Remove `time` crate, consolidate on chrono (~20 unique deps)

**Rationale:** The `time` crate is only used in one function: `generate_current_timestamp()` in `money_management.rs`. The entire rest of the codebase uses `chrono`. This creates unnecessary duplication.

**Duplicates eliminated:**
- `time v0.3` vs `time v0.1` (from lettre_email)

**Action:**
1. Remove `time = { version = "0.3", features = [...] }` from `egui-frontend/Cargo.toml`
2. Rewrite `generate_current_timestamp()` in `backend/domain/money_management.rs` to use chrono

**Replacement implementation:**
```rust
pub fn generate_current_timestamp(&self) -> Result<String, String> {
    let now = chrono::Local::now();
    Ok(now.to_rfc3339())
}
```

### 3. Remove `env_logger` (~23 deps)

**Rationale:** Used for exactly one line: `env_logger::init()`. The app has 754 log statements, but these are development/debugging logs. For a kid's allowance tracker, no one watches console output in production.

The `log` crate itself remains (used by eframe, egui, git2, etc.). When no logger is initialized, all log macros become zero-cost no-ops.

**Duplicates eliminated:**
- Removes `jiff` (yet another datetime library pulled in by env_logger)
- Removes `regex` (used for log filtering we don't need)

**Action:**
1. Remove `env_logger = "0.11"` from `egui-frontend/Cargo.toml`
2. Delete `use env_logger;` from `egui-frontend/src/main.rs`
3. Delete `env_logger::init();` from `egui-frontend/src/main.rs`

## Summary

| Change | Deps Eliminated | Duplicates Removed |
|--------|-----------------|-------------------|
| Remove `lettre_email` | ~75 | 6+ crate versions |
| Remove `time` | ~20 unique | 1 crate version |
| Remove `env_logger` | ~23 | 2+ crates (jiff, regex) |
| **Total** | **~115-120** | **~8-10 versions** |

## Files Modified

- `egui-frontend/Cargo.toml` - Remove 3 dependencies
- `egui-frontend/src/main.rs` - Delete env_logger import and init
- `backend/domain/money_management.rs` - Rewrite one function

## Not Changed (Considered but Kept)

- **git2** - Heavy (~100 deps) but provides tamper-detection against clever kids manipulating CSV files directly. Worth keeping.
- **eframe features** - Core to the app, not investigated further to keep scope manageable.

## Risk Assessment

**Low risk:**
- Removing unused code (`lettre_email`)
- Consolidating on already-used library (`chrono` instead of `time`)
- Removing debug infrastructure (`env_logger`)

No functional changes to the app's behavior.
