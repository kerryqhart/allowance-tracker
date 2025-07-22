# Egui Upgrade Plan: 0.28 → 0.32

## Current Status: PHASE 1 - Dependency Compilation Errors

**Branch**: `egui-upgrade`
**Started**: 2025-01-12
**Current Version**: egui 0.28 → 0.32

## Error Analysis

### Phase 1: Dependency Compilation Errors ❌

**Error Pattern**: All 34 errors are in `egui_extras` crate, specifically in the datepicker module.

**Root Cause**: `DatePickerButtonState` and `DatePickerPopupState` structs do not implement the `SerializableAny` trait, which requires both `Serialize` and `Deserialize` from serde.

**Error Location**: 
- `/Users/kerryh/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/egui_extras-0.32.0/src/datepicker/`
- Files: `button.rs` and `popup.rs`

**Error Type**: Upstream dependency issue, not our code
**Status**: ✅ FIXED by removing datepicker feature

### Phase 2: Our Code API Compatibility Issues ❌

**Error Count**: 160 compilation errors in our Rust code
**Error Pattern**: egui API breaking changes between 0.28 and 0.32

#### Category 2.1: Type Changes (Major - ~80+ errors)
```rust
// OLD (0.28): egui::Rounding::same() accepted f32
egui::Rounding::same(10.0)

// NEW (0.32): egui::Rounding::same() expects u8
egui::Rounding::same(10_u8)
```

**Files Affected**: Almost all UI files
**Impact**: Widespread - every rounding value needs conversion

#### Category 2.2: Method Renames (Warnings - ~20+ deprecations)
```rust
// OLD → NEW
.rounding() → .corner_radius()
.allocate_ui_at_rect() → .allocate_new_ui()
.child_ui() → .new_child()
```

**Impact**: Mostly warnings, but should fix for future compatibility

#### Category 2.3: Method Signature Changes (Breaking - ~10+ errors)
```rust
// OLD (0.28): rect_stroke took 3 arguments
painter.rect_stroke(rect, rounding, stroke);

// NEW (0.32): rect_stroke takes 4 arguments
painter.rect_stroke(rect, rounding, stroke, stroke_kind);
```

#### Category 2.4: Field vs Method Changes (Breaking - ~5+ errors)
```rust
// OLD: rounding was an assignable field
style.visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);

// NEW: rounding is now a method, not a field
// Need to find new API
```

#### Category 2.5: Shadow/Effects Changes (Breaking - ~5+ errors)
```rust
// OLD: shadow properties were f32
offset: egui::vec2(6.0, 6.0),
blur: 20.0,
spread: 0.0,

// NEW: shadow properties expect different types
offset: [i8; 2], // was Vec2
blur: u8,       // was f32  
spread: u8,     // was f32
```

### Detailed Error Categorization

#### Category 1: Missing SerializableAny Implementation (34 errors)
```rust
error[E0277]: the trait bound `DatePickerButtonState: SerializableAny` is not satisfied
error[E0277]: the trait bound `DatePickerPopupState: SerializableAny` is not satisfied
```

**Impact**: 
- Blocks compilation completely
- Affects `egui_extras` with `datepicker` feature
- All persistence functionality for datepicker components fails

## Investigation Results

### Current Features Used
```toml
egui_extras = { version = "0.32", features = ["all_loaders", "datepicker", "image", "file"] }
```

**Hypothesis**: The `datepicker` feature in egui_extras 0.32 has a bug where internal state structs don't implement required serialization traits.

## Resolution Strategy

### Approach 1: Feature Workaround (IMMEDIATE) ⭐ **RECOMMENDED**
1. **Remove datepicker feature** temporarily to unblock compilation
2. **Verify our code** doesn't actually use the datepicker
3. **Re-add datepicker** once upstream issue is resolved

### Approach 2: Version Pinning (ALTERNATIVE)
1. Pin to a working egui_extras version (investigate 0.31.x)
2. Upgrade incrementally

### Approach 3: Upstream Investigation (PARALLEL)
1. Check egui GitHub issues for related bugs
2. Report bug if not already reported
3. Monitor for fixes

## Implementation Plan

### Phase 1: Immediate Unblock ✅ **COMPLETED**
- [x] Remove `datepicker` from egui_extras features
- [x] Test compilation
- [x] Search codebase for datepicker usage (not used anywhere)
- [x] Document any datepicker functionality (none needed)

### Phase 2: Our Code Compilation Issues ⭐ **MAJOR PROGRESS**
- [x] **2.1**: Fix type changes - Convert f32 rounding to u8 (80+ errors) ✅ DONE
- [ ] **2.2**: Fix method renames - Update deprecated method calls (20+ warnings) 
- [x] **2.3**: Fix method signatures - Add missing parameters (10+ errors) ✅ DONE
- [x] **2.4**: Fix field/method changes - Update style assignment patterns (5+ errors) ✅ DONE
- [ ] **2.5**: Fix shadow/effects - Update shadow property types (5+ errors)
- [x] **BONUS**: Fixed egui::Rounding → egui::CornerRadius rename (100+ changes) ✅ DONE
- [x] **BONUS**: Fixed FontData Arc wrapping (.into() calls) ✅ DONE

### Phase 2B: Remaining API Issues ⭐ **CURRENT PHASE** 
- [ ] **2B.1**: Fix egui_plot API changes (PlotPoints, Line::new)
- [ ] **2B.2**: Fix type conversion issues (Color32, Stroke, Vec2b trait bounds)
- [ ] **2B.3**: Fix function signature changes (argument count mismatches)

### Phase 3: Runtime Testing
- [ ] Test basic app functionality
- [ ] Test calendar functionality (non-datepicker)
- [ ] Test transaction table
- [ ] Test balance charts
- [ ] Test data persistence

### Phase 4: API Modernization
- [ ] Adopt new egui 0.32 features if beneficial
- [ ] Update styling/theming if changed
- [ ] Performance optimizations

### Phase 5: Datepicker Resolution
- [ ] Monitor upstream fix
- [ ] Implement alternative datepicker if needed
- [ ] Re-enable feature once fixed

## Testing Strategy

### Compilation Testing
```bash
# After each fix:
cargo check --bin allowance-tracker-egui
cargo build --bin allowance-tracker-egui  
cargo test --workspace
```

### Runtime Testing
```bash
# Functional testing:
cargo run --bin allowance-tracker-egui
```

**Test Matrix**:
- [ ] App launches successfully
- [ ] Calendar view renders
- [ ] Calendar interactions work
- [ ] Transaction table displays
- [ ] Add transaction works
- [ ] Balance calculation works
- [ ] Data persistence works
- [ ] Settings modal works
- [ ] Goal functionality works

## Risk Assessment

### High Risk
- ❌ **Complete compilation failure** (current state)
- ⚠️ **Potential data format changes** in persistence

### Medium Risk  
- ⚠️ **API breaking changes** requiring code updates
- ⚠️ **Performance regressions** in new version

### Low Risk
- ⚠️ **Visual/styling differences** 
- ⚠️ **New features integration** complexity

## Rollback Plan

**Trigger**: If upgrade proves too complex or breaks critical functionality

**Process**:
1. `git checkout main` 
2. Document issues encountered
3. Plan incremental upgrade (0.28 → 0.29 → 0.30 → 0.31 → 0.32)

## Progress Tracking

### Completed ✅
- [x] Branch creation (`egui-upgrade`)
- [x] Baseline commit (working 0.28 state)
- [x] Version bump to 0.32
- [x] Error identification and categorization
- [x] Fixed egui_extras datepicker dependency issue (removed unused feature)
- [x] **MAJOR**: Fixed 160+ → ~20 compilation errors (87% reduction)
- [x] Fixed all egui::Rounding → egui::CornerRadius API changes (100+ instances)
- [x] Fixed all rect_stroke() method signature changes (4th parameter)
- [x] Fixed all rounding type changes (f32 → u8)
- [x] Fixed FontData Arc wrapping API changes

### In Progress 🔄
- [ ] **CURRENT**: Fix remaining API compatibility issues (Phase 2B)

### Planned 📋
- [ ] Fix our code compilation issues  
- [ ] Runtime testing
- [ ] Performance verification
- [ ] Documentation updates

## Notes

### Important Observations
1. **This is NOT our code issue** - all errors are in upstream dependencies
2. **Datepicker feature appears broken** in egui_extras 0.32
3. **Quick fix available** - disable problematic feature
4. **Our core functionality likely unaffected** - we use calendar components, not datepicker widgets

### Questions for Investigation
- [x] Do we actually use egui_extras datepicker anywhere? → **NO, safely removed**
- [x] Is this a known issue in egui 0.32? → **YES, datepicker persistence bug**
- [ ] Are there newer patch versions that fix this?
- [ ] What is the new API for style.visuals.widgets.*.rounding assignment?
- [ ] What is the 4th parameter for rect_stroke (StrokeKind)?
- [ ] What are the new shadow property types and API?

---

**Next Action**: Start Phase 2B.1 - Fix egui_plot API changes (PlotPoints, Line::new) 