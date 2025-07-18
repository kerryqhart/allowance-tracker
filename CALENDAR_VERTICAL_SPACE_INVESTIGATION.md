# Calendar Vertical Space Investigation

## Problem Statement
The calendar has significant unused vertical space at the bottom that doesn't match the nice side margins.

## ✅ Phase 3 COMPLETED: Fix Position-Height Mismatch 

**SUCCESSFUL IMPROVEMENT ACHIEVED:**
- **Before**: 95.6% utilization with 20px artificial height reduction
- **After**: **100.0% utilization** using full available height  
- **Cell height gained**: 59.8px → 63.2px (+3.4px larger cells)
- **Available cell space**: 359px → 379px (+20px gained)
- **Visual confirmation**: Bottom gap noticeably smaller in side-by-side comparison

**Root Cause Fixed**: 
Eliminated position-height mismatch where we were creating double spacing:
- ❌ **Before**: `top + 20px` positioning + `height - 20px` calculation = 40px total gap
- ✅ **After**: `top + 20px` positioning + `full height` calculation = 20px intended top margin only

**Changes Made**:
```rust
// BEFORE: Double spacing
let desired_calendar_height = available_rect.height() - 20.0; // Creates bottom gap
let card_rect = egui::Rect::from_min_size(
    egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0), // Creates top margin
    egui::vec2(content_width, final_card_height)
);

// AFTER: Single intended margin  
let desired_calendar_height = available_rect.height(); // Use full space
let card_rect = egui::Rect::from_min_size(
    egui::pos2(available_rect.left() + 20.0, available_rect.top() + 20.0), // Keep top margin
    egui::vec2(content_width, final_card_height)
);
```

## Phase 4: Parent Layout Hierarchy Investigation (NEXT)

**Remaining Issue**: Visual purple space still exists at bottom, indicating the `available_rect` (454px) doesn't represent full window space.

**Investigation Plan**:
1. **Add layout debugging** to parent components (`app_coordinator.rs`, `tab_manager.rs`)
2. **Trace space allocation** from window → content area → available_rect  
3. **Identify unnecessary reservations** in parent layout hierarchy
4. **Optimize parent space allocation** to give calendar more room

**Known Parent Space Reservations**:
- Header: 80px (fixed, necessary)
- Subheader: 50px (fixed, necessary) 
- Selection bar: 0-50px (conditional, necessary)
- Tab manager: TBD (investigate)
- Messages area: TBD (investigate)

**Success Criteria for Phase 4**:
- Bottom purple space reduced to match side margins
- Calendar extends closer to window bottom
- No layout breaks or visual artifacts 