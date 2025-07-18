# Calendar Vertical Space Utilization Investigation

## Problem Statement
**Goal:** Eliminate unused purple space at the bottom of the calendar, making the calendar extend to fill the full available window height.

**Current Issue:** Despite achieving 100% utilization of `available_rect` (confirmed by debug logs), there remains significant unused vertical space at the bottom of the screen.

## Background Context
This investigation follows the successful completion of independent X/Y scaling (commit 08bf95e). The calendar now scales independently in both dimensions, but we discovered a separate issue: the calendar container doesn't utilize the full window height.

## Current State Analysis

### What We Know
```
🔍 ACTUAL SPACE: available_rect.height=528, desired_calendar=528, actual_card=528, utilization=100.0%
```
- ✅ We're using 100% of the `available_rect` given to the calendar component
- ❌ But `available_rect` itself doesn't represent the full window space
- ❌ Significant purple background space remains unused below the calendar

### Visual Evidence
Looking at the current UI:
- Calendar ends around row 6 (July 31st)
- Large purple background space below the calendar extends to window bottom
- Space appears to be 20-30% of total window height

## Root Cause Hypotheses

### Hypothesis A: Layout Hierarchy Issue
**Theory:** The calendar's `available_rect` is being constrained by parent layout components that reserve too much space.

**Investigation:** 
- `available_rect` comes from parent UI layout
- Parent components may be reserving space for other elements
- Calendar component operates within this constrained rectangle

**Test:** Examine the parent layout chain to understand space allocation.

### Hypothesis B: Container Positioning Issue  
**Theory:** The calendar container is positioned correctly but doesn't extend to the window bottom due to margin/padding constraints.

**Investigation:**
- Calendar container starts at `available_rect.left() + 20.0, available_rect.top() + 20.0`
- Final container size is `final_card_height` but may not reach window bottom
- Gap might be in the positioning logic

**Test:** Remove container margins and padding to see if calendar extends further.

### Hypothesis C: Parent Component Space Reservation
**Theory:** Parent components (header, navigation, etc.) are over-reserving vertical space, leaving less for the calendar.

**Investigation:**
- Header takes some space at top
- Navigation buttons take some space  
- Remaining space passed as `available_rect` to calendar
- One of these might be claiming more space than needed

**Test:** Analyze the component hierarchy that calls `draw_calendar_section_with_toggle()`.

### Hypothesis D: egui Layout Constraint
**Theory:** egui's layout system is imposing automatic constraints that prevent components from using full window height.

**Investigation:**
- egui may reserve space for window borders, title bars, etc.
- Built-in layout policies might prevent 100% height usage
- Framework-level constraints we're not accounting for

**Test:** Try alternative layout approaches or examine egui documentation for height constraints.

## Investigation Plan

### Phase 1: Understand Parent Layout Chain ✅ COMPLETED
- ✅ Find where `draw_calendar_section_with_toggle()` is called → `tab_manager.rs:32`
- ✅ Examine the `available_rect` being passed down → comes from `content_rect` 
- ✅ Identify what parent components reserve space above the calendar → full hierarchy mapped
- ✅ Measure actual vs expected space allocation → found multiple space reservations

**FINDINGS - Complete Layout Hierarchy:**
```
Window
├── Header: 80px                    (app_coordinator.rs)
├── Selection Bar: 0-50px           (app_coordinator.rs, conditional)
├── Subheader: 50px                 (app_coordinator.rs, tab toggle buttons)
├── Content Area (content_rect)
│   ├── Messages (unknown size)     (app_coordinator.rs)
│   └── Tab Manager
│       ├── Reserved: -30px         (tab_manager.rs, artificial reduction)
│       ├── Calendar (available_rect)
│       └── Bottom spacing: +30px   (tab_manager.rs, added after)
```

**SPACE ALLOCATION BREAKDOWN:**
- **Header:** 80px (fixed)
- **Subheader:** 50px (fixed) 
- **Tab Manager Reduction:** 30px (artificial)
- **Tab Manager Bottom Spacing:** 30px (added)
- **Total Reserved:** 190px minimum

**KEY DISCOVERY:** Tab manager artificially reduces height by 30px AND adds 30px spacing = 60px of unnecessary reservation!

### Phase 2: Test Layout Modifications 🧪 IN PROGRESS
- ✅ **Test 1: Remove Tab Manager Double Reservation**
  - **Change:** Removed artificial 30px height reduction + reduced spacing 30px→0px  
  - **Expected gain:** +60px more space for calendar
  - **Files changed:** `tab_manager.rs` lines 31-36 
  - **Status:** 🧪 Testing - app running with 0px bottom spacing

**Additional Padding Sources Discovered:**
- **Calendar internal margins:** 20px on all sides (`calendar_renderer.rs:1049`)
- **Calendar top spacing:** 15px (`calendar_renderer.rs:1015`)  
- **Content width calculation:** -40px for left/right margins (`calendar_renderer.rs:1019`)
- **🎯 ROOT CAUSE:** Height calculation vs positioning mismatch!

**Root Cause Analysis:**
```rust
// HEIGHT CALC: Uses full available_rect.height()
let desired_calendar_height = available_rect.height();

// POSITIONING: Adds 20px top offset  
egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0)

// RESULT: 20px gap at bottom due to mismatch
```

- ✅ **Test 2: Fix Height/Positioning Mismatch**  
  - **Change:** `desired_calendar_height = available_rect.height() - 20.0`
  - **Effect:** Accounts for top positioning offset, eliminates bottom gap
  - **Preserves:** Top, left, right margins remain unchanged at 20px
  - **Status:** 🧪 Testing - app running

**Test 1 Details:**
```rust
// BEFORE (Double reservation):
let mut available_rect = ui.available_rect_before_wrap();
available_rect.max.y -= 30.0; // Artificial reduction
// ... render calendar ...
ui.add_space(30.0); // Extra spacing

// AFTER (Minimal reservation):
let available_rect = ui.available_rect_before_wrap(); // Full space
// ... render calendar ...  
ui.add_space(10.0); // Minimal spacing
```

- [ ] Try removing container margins/padding
- [ ] Test different positioning strategies
- [ ] Experiment with egui layout approaches  
- [ ] Use debug rectangles to visualize space usage

### Phase 3: Implement Solution
- [ ] Based on findings, implement the most promising approach
- [ ] Verify calendar extends to window bottom
- [ ] Ensure no layout breaks or visual artifacts
- [ ] Test across different window sizes

## Success Criteria
- ✅ Calendar extends close to the bottom of the window (minimal unused space)
- ✅ No layout breaks or visual artifacts
- ✅ Maintains current independent X/Y scaling behavior
- ✅ Works across different window sizes
- ✅ No conflicts with header, navigation, or other UI elements

## Implementation Notes
- **Current working file:** `egui-frontend/src/ui/components/calendar_renderer.rs`
- **Parent layout investigation needed:** Find caller of `draw_calendar_section_with_toggle()`
- **Key insight:** This is a space allocation issue, not a dimension calculation issue
- **Success metric:** Visual confirmation that purple space is minimized

## Constraints
- Must maintain the independent scaling behavior achieved in the previous investigation
- Cannot break existing layout or navigation functionality  
- Should work harmoniously with the overall app design 