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

### Phase 1: Immediate Unblock ✅ **NEXT STEP**
- [ ] Remove `datepicker` from egui_extras features
- [ ] Test compilation
- [ ] Search codebase for datepicker usage
- [ ] Document any datepicker functionality that needs alternative implementation

### Phase 2: Our Code Compilation Issues
- [ ] Fix any API changes in our egui usage
- [ ] Update immediate mode patterns if needed
- [ ] Fix any breaking changes in egui 0.29-0.32

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

### In Progress 🔄
- [ ] **CURRENT**: Fix egui_extras datepicker issue

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
- [ ] Do we actually use egui_extras datepicker anywhere?
- [ ] Is this a known issue in egui 0.32?
- [ ] Are there newer patch versions that fix this?
- [ ] What are the alternative datepicker solutions?

---

**Next Action**: Remove datepicker feature and test compilation 