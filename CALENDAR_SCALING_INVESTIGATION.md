# Calendar Independent Scaling Investigation

## Problem Statement
**Goal:** Make calendar cells scale independently in X and Y dimensions when window is resized.
**Current Issue:** Horizontal window stretching causes proportional scaling in both X and Y due to locked aspect ratio.

## Root Cause Analysis

### Current Implementation (Original)
```rust
// The coupling chain that causes locked aspect ratio:
cell_width = (calendar_width - total_spacing) / 7.0;           // ✅ From horizontal space  
cell_height = cell_width * 0.8;                               // 🚫 COUPLING POINT 1
header_height = cell_height * 0.4;                            // 🚫 COUPLING POINT 2  
card_height = (header_height + cell_height * 6.0) + 200.0;    // 🚫 COUPLING POINT 3
```

### Cascade Effect
1. User stretches window horizontally
2. `calendar_width` increases  
3. `cell_width` increases (good!)
4. `cell_height = cell_width * 0.8` increases (bad - locked ratio!)
5. `header_height = cell_height * 0.4` increases (compounds the problem)
6. `card_height` increases (entire container grows)
7. **Result:** Everything scales together instead of independently

## Investigation Attempts

### Attempt 1: Pure Independent Calculation (FAILED)
**Approach:** Calculate height directly from available vertical space
**Issue:** Reserved too much space (250px), cells became tiny rectangles
**Learning:** Space accounting is critical - can't be too aggressive with reservations

### Attempt 2: Hybrid Baseline + Available Space (FAILED)  
**Approach:** Use proportional baseline but scale up with available space
**Issue:** Still behaved like locked ratio - horizontal stretch caused vertical growth
**Learning:** The coupling runs deeper than just the cell dimension calculation

### Key Discovery
Multiple layers of coupling exist:
1. **Primary:** `cell_height = cell_width * 0.8`
2. **Secondary:** `header_height = cell_height * 0.4` 
3. **Container:** `card_height = (header_height + cell_height * 6.0) + 200.0`

Must break the chain at multiple strategic points, not just one.

## Hypotheses

### Hypothesis A: Break The Chain at Multiple Points ⬅️ **CURRENT ATTEMPT**
**Strategy:** Surgically break coupling at each link in the chain

```rust
// Keep width calculation (works fine)
cell_width = (calendar_width - total_spacing) / 7.0;

// Break coupling 1: Calculate height independently 
available_height_for_calendar = available_rect.height() - navigation_space - padding;
cell_height = (available_height_for_calendar / 6.5).max(cell_width * 0.6).min(cell_width * 1.2);

// Break coupling 2: Fixed header height
header_height = 30.0;

// Break coupling 3: Calculate container from available space, not cell dimensions
card_height = available_rect.height() - 60.0; // Leave margin for navigation
```

**Pros:** 
- Direct surgical approach
- Addresses root cause at multiple points
- Keeps reasonable aspect ratio limits

**Risks:**
- Complex space accounting
- Need careful tuning of constants
- May feel inconsistent if limits hit too often

### Hypothesis B: Two-Pass Calculation
**Strategy:** Calculate baseline, then optimize if space allows

```rust
// Pass 1: Calculate proportional baseline (current logic)
baseline_cell_width = (calendar_width - total_spacing) / 7.0;
baseline_cell_height = baseline_cell_width * 0.8;

// Pass 2: Check if we can improve vertical usage
available_vertical = calculate_available_vertical_space();
optimized_cell_height = (available_vertical / 6.0).max(baseline_cell_height);

// Use optimized if significantly better, otherwise baseline
cell_height = if optimized_cell_height > baseline_cell_height * 1.2 { 
    optimized_cell_height 
} else { 
    baseline_cell_height 
};
```

**Pros:**
- Preserves current behavior as fallback
- Only optimizes when clear benefit
- Safer approach

**Risks:**
- May not scale horizontally without vertical space
- Threshold logic could feel arbitrary

### Hypothesis C: Layout-First Approach
**Strategy:** Let egui determine layout space, then calculate cell dimensions

```rust
// Reserve space for headers and navigation first
ui.allocate_ui_with_layout(available_size, |ui| {
    // Headers
    let header_response = ui.horizontal(|ui| { /* render headers */ });
    
    // Calculate cells based on remaining space
    let remaining_rect = ui.available_rect_before_wrap();
    cell_width = (remaining_rect.width() - spacing) / 7.0;
    cell_height = (remaining_rect.height() - spacing) / 6.0;
    
    // Render calendar with actual available space
});
```

**Pros:**
- Most egui-native approach
- Natural responsive behavior
- Eliminates complex space calculations

**Risks:**
- Biggest architectural change
- Unknown rendering order dependencies
- May break existing layout assumptions

## Experiment Log

### Experiment 1: Hypothesis A Implementation
**Date:** [Current]
**Status:** ✅ IMPLEMENTED - Testing in Progress
**Changes:** 
- ✅ Break coupling 1: Independent cell height calculation
  - Calculate from available vertical space: `available_height_for_cells / 6.0`
  - Apply limits: `.max(cell_width * 0.6).min(cell_width * 1.2)`
- ✅ Break coupling 2: Fixed header height  
  - Changed from `cell_height * 0.4` to fixed `30.0`
- ✅ Break coupling 3: Container height from available space
  - Changed from `(header_height + cell_height * 6.0) + 200.0` to `available_rect.height() - 60.0`
- [ ] Test horizontal scaling behavior
- [ ] Test vertical scaling behavior
- [ ] Verify reasonable aspect ratios maintained

**Implementation Details:**
```rust
// BEFORE (Original - Coupled)
cell_height = cell_width * 0.8;                               // 🚫 COUPLING
header_height = cell_height * 0.4;                            // 🚫 COUPLING  
card_height = (header_height + cell_height * 6.0) + 200.0;    // 🚫 COUPLING

// AFTER (Hypothesis A - Independent)
let cell_width = (calendar_width - total_spacing) / 7.0;      // ✅ From horizontal space
let available_height_for_calendar = available_rect.height() - navigation_space - padding_space;
let header_height = 30.0;                                     // ✅ Fixed, not coupled
let available_height_for_cells = available_height_for_calendar - header_height - vertical_spacing;
let calculated_cell_height = available_height_for_cells / 6.0; // ✅ From vertical space
let cell_height = calculated_cell_height.max(cell_width * 0.6).min(cell_width * 1.2); // ✅ Limited but independent
let card_height = available_rect.height() - 60.0;             // ✅ From available space
```

**Key Constants:**
- `navigation_space = 80.0` (space for navigation buttons and margins)
- `padding_space = 80.0` (space for calendar container padding)  
- `header_height = 30.0` (fixed header height)
- `min_aspect = 0.6` (cells can't be shorter than 60% of width)
- `max_aspect = 1.2` (cells can't be taller than 120% of width)

**Results:** ❌ **FAILED** - Still exhibits proportional scaling behavior

### Experiment 2: Hypothesis A Fixed (Absolute Limits)
**Date:** [Current]
**Status:** 🧪 TESTING - Ready for Validation
**Changes:**
- ✅ **Key Fix:** Replaced proportional limits with absolute limits
  - **Before:** `.max(cell_width * 0.6).min(cell_width * 1.2)` (recreated coupling)
  - **After:** `.max(40.0).min(200.0)` (truly independent)
- ✅ Maintained all other decoupling from Experiment 1
- ✅ Added debug logging to verify behavior

**Expected Results:**
- 🔍 `width_ratio` should vary (not locked to 0.60)
- 🔍 Horizontal stretch → width changes, ratio changes
- 🔍 Vertical stretch → height changes, ratio changes  
- 🔍 Independent scaling in both dimensions

**Test Instructions:**
1. Watch debug logs while resizing
2. Horizontal stretch: `cell_width` should increase, `width_ratio` should decrease
3. Vertical stretch: `cell_height` should increase, `width_ratio` should increase
4. Both: Should scale independently

**Results:** ✅ **SUCCESS!** - Independent scaling achieved!
- ✅ Calendar cells now scale independently in X and Y dimensions
- ✅ Horizontal stretch → cells get wider only
- ✅ Vertical stretch → cells get taller only  
- ✅ Debug logs show varying width ratios (not locked to 0.60)
- ✅ **CORE PROBLEM SOLVED:** Calendar no longer exhibits locked aspect ratio behavior

**Note:** Separate issue discovered during implementation - unused vertical space at bottom of screen. This is a different problem (space utilization) not related to the original coupling issue.
- ✅ Compiles and runs without errors
- ❌ Horizontal stretch still causes proportional X+Y scaling (not independent)
- ❌ Vertical stretch does not change cell dimensions
- ✅ Cells are shorter by default (side effect of new calculation)
- **Conclusion:** The coupling chain was not successfully broken

**Failure Analysis:**
**ROOT CAUSE DISCOVERED via Debug Logs:** 
```
📐 SCALING DEBUG: cell_width=135.9, calculated_height=41.8, final_height=81.5, width_ratio=0.60
```

The limits `.max(cell_width * 0.6).min(cell_width * 1.2)` were **recreating the coupling**! 
- `calculated_height=41.8` (independent calculation working!)
- `final_height=81.5` (forced to `cell_width * 0.6`)
- `width_ratio=0.60` (proves proportional coupling restored)

As `cell_width` increases → `cell_width * 0.6` increases → `final_height` increases proportionally → coupling restored!

**FIX APPLIED:** Replace proportional limits with absolute limits:
```rust
// BEFORE (recreated coupling):
.max(cell_width * 0.6).min(cell_width * 1.2)

// AFTER (truly independent):
.max(40.0).min(200.0)
```

## Success Criteria ✅ ACHIEVED
- ✅ Horizontal window stretch → cells get wider only (no height change)
- ✅ Vertical window stretch → cells get taller only (no width change)  
- ✅ Combined stretch → cells scale independently in both dimensions
- ✅ Reasonable minimum/maximum cell sizes maintained
- ✅ Calendar looks good at different window sizes
- ✅ No layout breaks or visual artifacts
- ✅ **BONUS:** Maximum vertical space utilization (no wasted space)

## Final Solution Summary

**Problem Solved:** Calendar cells now scale independently in X and Y dimensions when window is resized.

**Root Cause:** Multiple coupling points in dimension calculations created a cascade effect where horizontal changes forced proportional vertical changes.

**Solution Strategy:** Hypothesis A - "Break The Chain at Multiple Points" proved successful.

**Key Breakthrough:** Debug logging revealed that "safety limits" were recreating the coupling we tried to eliminate. Replacing proportional limits with absolute limits was crucial.

**Technical Implementation:**
```rust
// BEFORE (Coupled):
cell_height = cell_width * 0.8;  // Direct coupling
header_height = cell_height * 0.4;  // Compounds coupling  
card_height = (header_height + cell_height * 6.0) + 200.0;  // Container coupled

// AFTER (Independent):  
desired_calendar_height = available_rect.height() - 10.0;  // Work backwards from space
cell_height = (available_space / 6.0).max(40.0).min(200.0);  // Absolute limits only
header_height = 30.0;  // Fixed
card_height = calculated_from_actual_usage;  // Based on components, not arbitrary
```

**Lessons Learned:**
1. **Debug logging is essential** - revealed the real failure points
2. **Proportional limits recreate coupling** - use absolute limits for true independence
3. **Work backwards from desired space usage** - more reliable than predicting space needs
4. **Remove artificial constraints** - egui can handle edge cases gracefully

## Implementation Notes
- File: `egui-frontend/src/ui/components/calendar_renderer.rs`
- Function: `draw_calendar_section_with_toggle()`
- Lines: ~1020-1040 (dimension calculations)
- **Status:** ✅ PRODUCTION READY 