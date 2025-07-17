# Delete Transactions Implementation Plan

## **📋 Project Overview**
Implement parental-controlled transaction deletion functionality with two major phases:
1. **Parental Control Challenge** - Two-stage authentication system
2. **Transaction Selection & Deletion** - UI for selecting and deleting historical transactions

## **✅ Existing Backend Capabilities**

### **Parental Control (Ready to Use)**
- Complete service: `ParentalControlService::validate_answer()`
- Challenge question: **"Oh yeah?? If so, what's cooler than cool?"**
- Expected answer: **"ice cold"** (case-insensitive, whitespace handled)
- Full audit trail stored in CSV
- Location: `backend/domain/parental_control_service.rs`

### **Delete Transactions (Ready to Use)**
- Complete service: `TransactionService::delete_transactions_domain()`
- Supports multiple transaction deletion via `DeleteTransactionsCommand`
- Automatic balance recalculation for remaining transactions
- Full error handling and validation
- Location: `backend/domain/transaction_service.rs`

### **Frontend Structure (Available)**
- Settings menu already has "Delete transactions" item with 🗑️ icon
- Calendar has transaction chips with hover tooltips and selection support
- Three-layer layout with subheader for tab-specific controls
- Existing modal system for user interactions

## **🎯 Implementation Phases**

### **Phase 1: Parental Control Challenge (2-3 hours)**

**1.1 Create Parental Control Modal Component**
- New component: `ParentalControlModal` in `egui-frontend/src/ui/components/modals.rs`
- Two-stage UI process:
  1. "Are you Mom or Dad?" with Yes/No buttons
  2. If Yes → "What's cooler than cool?" with text input + Submit/Cancel
- Wire to existing `ParentalControlService::validate_answer()`
- Success unlocks the protected feature, failure shows error message

**1.2 Add Modal State Management**
- Add to `AllowanceTrackerApp` state:
  - `show_parental_control_modal: bool`
  - `parental_control_stage: ParentalControlStage` (enum: Question1, Question2, Authenticated)
  - `pending_protected_action: Option<ProtectedAction>` (enum for future extensibility)
  - `parental_control_input: String` (for answer input)

**1.3 Wire Settings Menu Integration**
- Modify `render_settings_dropdown_menu()` in `header.rs`
- "Delete transactions" click → opens parental control modal with pending action
- Store action type for execution after successful authentication

### **Phase 2: Transaction Selection System (3-4 hours)**

**2.1 Add Selection State Management**
- Add to `AllowanceTrackerApp`:
  - `selected_transaction_ids: std::collections::HashSet<String>`
  - `transaction_selection_mode: bool`
- Helper methods:
  - `toggle_transaction_selection(id: String)`
  - `clear_all_selections()`
  - `get_selected_count() -> usize`
  - `enter_selection_mode()` / `exit_selection_mode()`

**2.2 Modify Calendar Chip Rendering**
- Update `render_calendar_chip()` in `calendar_renderer.rs`
- Add selection checkbox/checkmark when `transaction_selection_mode = true`
- **Only show selectors for historical transactions** (exclude `FutureAllowance` type)
- Visual changes:
  - Selected chips: purple border + small checkmark overlay
  - Unselected chips: normal styling + empty checkbox
  - Future allowances: no selection UI (grayed out in selection mode)

**2.3 Add Table Selection Support**
- Update `render_responsive_transaction_table()` in `transaction_table.rs`
- Add selection column with checkboxes when `transaction_selection_mode = true`
- Consistent styling with calendar selection
- Same filtering: only historical transactions selectable

### **Phase 3: Subheader Controls (2-3 hours)**

**3.1 Enhance Tab-Specific Controls**
- Modify `draw_tab_specific_controls()` in `app_coordinator.rs`
- **Normal mode**: Show existing controls (month navigation for calendar)
- **Selection mode**: Replace with action buttons

**3.2 Selection Mode Controls Layout**
```
[Clear All] [Delete Selected (3)] [Cancel]     [📅 Calendar] [📋 Table]
```
- **Clear All**: Clear all selections but stay in selection mode
- **Delete Selected (N)**: Show count, only enabled when N > 0
- **Cancel**: Exit selection mode and clear selections
- Buttons styled consistently with existing theme

**3.3 Action Button Behaviors**
- **Delete Selected**: Validate parental control if not already authenticated → delete transactions
- **Clear All**: Simple state reset
- **Cancel**: Exit mode with state cleanup

### **Phase 4: Backend Integration (1-2 hours)**

**4.1 Create Delete Function**
- Add `delete_selected_transactions()` method to `AllowanceTrackerApp`
- Process: `HashSet<String>` → `Vec<String>` → `DeleteTransactionsCommand`
- Call `backend.transaction_service.delete_transactions()`
- Handle `DeleteTransactionsResult` response

**4.2 State Management & UI Updates**
- **Success path**:
  - Clear selection state
  - Exit selection mode
  - Show success message with count
  - Reload calendar/table data
  - Refresh current balance
- **Error path**:
  - Keep selection state intact
  - Show error message
  - Allow retry

### **Phase 5: UI Polish & Integration (1-2 hours)**

**5.1 Mode Transition Polish**
- Smooth animation/transition into selection mode
- Visual feedback during backend operations (loading states)
- Consistent theming with existing purple/pink color scheme

**5.2 Error Handling & Edge Cases**
- Network/backend failure graceful handling
- Invalid transaction IDs (already deleted by another process)
- Empty selection attempts
- Modal dismissal handling

## **🔧 Technical Implementation Details**

### **New Enums**
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParentalControlStage {
    Question1,  // "Are you Mom or Dad?"
    Question2,  // "What's cooler than cool?"
    Authenticated,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProtectedAction {
    DeleteTransactions,
    // Future: ConfigureAllowance, ExportData, etc.
}
```

### **New State Fields (app_state.rs)**
```rust
// Parental control state
pub show_parental_control_modal: bool,
pub parental_control_stage: ParentalControlStage,
pub pending_protected_action: Option<ProtectedAction>,
pub parental_control_input: String,

// Selection state  
pub selected_transaction_ids: std::collections::HashSet<String>,
pub transaction_selection_mode: bool,
```

### **New Components**
1. **ParentalControlModal** - Two-stage challenge dialog with proper error handling
2. **SelectionControls** - Subheader buttons for selection mode
3. **SelectableTransactionChip** - Calendar chip with selection checkbox overlay

### **Modified Components**
1. **Settings Menu** (`header.rs`) - Wire delete transactions to parental control
2. **Calendar Renderer** (`calendar_renderer.rs`) - Add selection UI to historical transactions
3. **Transaction Table** (`transaction_table.rs`) - Add selection column  
4. **Subheader Controls** (`app_coordinator.rs`) - Context-sensitive action buttons

## **📝 Implementation Decisions (Confirmed)**

1. **Selection Visual**: Simple checkmark overlay on selected transactions
2. **Selection Scope**: Granular only (no "Select All Month" bulk operations)
3. **Authentication Persistence**: Re-authenticate for each delete operation (security over convenience)
4. **Future Allowances**: NOT selectable for deletion (they're generated, not stored)
5. **Final Confirmation**: No additional confirmation dialog (git backup provides safety net)

## **⏱️ Estimated Timeline**
- **Phase 1 (Parental Control)**: 2-3 hours
- **Phase 2 (Selection System)**: 3-4 hours  
- **Phase 3 (Subheader UI)**: 2-3 hours
- **Phase 4 (Backend Integration)**: 1-2 hours
- **Phase 5 (Polish)**: 1-2 hours
- **Total**: 9-14 hours of development time

## **🚀 Implementation Order**
1. Start with **Phase 1** (Parental Control) - establishes security foundation
2. Build **Phase 2** (Selection System) - core functionality
3. Add **Phase 3** (UI Controls) - user interface
4. Integrate **Phase 4** (Backend) - complete the flow
5. Polish **Phase 5** - user experience refinement

## **📋 Success Criteria**
- ✅ Parent can authenticate using two-stage challenge
- ✅ Parent can select individual historical transactions (calendar + table views)
- ✅ Parent can delete selected transactions with immediate UI feedback
- ✅ Balances automatically recalculate after deletion
- ✅ No accidental deletion of future allowances
- ✅ Consistent visual design with existing app theme
- ✅ Graceful error handling for all failure modes 