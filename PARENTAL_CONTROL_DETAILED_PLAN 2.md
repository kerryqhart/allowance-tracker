# Parental Control System - Detailed Implementation Plan

## **🎯 Overview**
Implement a two-stage parental control challenge system that gates access to administrative functions like transaction deletion. This provides child-safe protection while maintaining parent accessibility.

## **🏗️ Architecture Overview**

### **Flow Diagram**
```
Settings Menu → "Delete transactions" → Parental Control Modal
                                            ↓
Stage 1: "Are you Mom or Dad?" [Yes/No]
                                            ↓ (Yes)
Stage 2: "What's cooler than cool?" [Text Input + Submit]
                                            ↓ (Correct Answer)
Authentication Success → Execute Protected Action
```

### **Backend Integration**
- **Service**: `ParentalControlService::validate_answer()`
- **Command**: `ValidateParentalControlCommand { answer: String }`
- **Response**: `ValidateParentalControlResult { success: bool, message: String }`
- **Expected Answer**: "ice cold" (case-insensitive, whitespace-trimmed)

## **📋 Step-by-Step Implementation**

### **Step 1: Add New State Types (app_state.rs)**

**1.1 Add Enums**
```rust
// Add after existing enums in app_state.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentalControlStage {
    Question1,      // "Are you Mom or Dad?"
    Question2,      // "What's cooler than cool?"
    Authenticated,  // Success state
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedAction {
    DeleteTransactions,
    // Future extensions: ConfigureAllowance, ExportData, etc.
}
```

**1.2 Add State Fields to AllowanceTrackerApp**
```rust
// Add to AllowanceTrackerApp struct after existing modal states
// Parental control state
pub show_parental_control_modal: bool,
pub parental_control_stage: ParentalControlStage,
pub pending_protected_action: Option<ProtectedAction>,
pub parental_control_input: String,
pub parental_control_error: Option<String>,
pub parental_control_loading: bool,
```

**1.3 Initialize New Fields in AllowanceTrackerApp::new()**
```rust
// Add to the initialization in new() method
// Parental control state
show_parental_control_modal: false,
parental_control_stage: ParentalControlStage::Question1,
pending_protected_action: None,
parental_control_input: String::new(),
parental_control_error: None,
parental_control_loading: false,
```

### **Step 2: Create Helper Methods (app_state.rs)**

**2.1 Add Parental Control Management Methods**
```rust
impl AllowanceTrackerApp {
    /// Start parental control challenge for a specific action
    pub fn start_parental_control_challenge(&mut self, action: ProtectedAction) {
        log::info!("🔒 Starting parental control challenge for: {:?}", action);
        self.pending_protected_action = Some(action);
        self.parental_control_stage = ParentalControlStage::Question1;
        self.parental_control_input.clear();
        self.parental_control_error = None;
        self.parental_control_loading = false;
        self.show_parental_control_modal = true;
    }
    
    /// Handle "Yes" button click on first question
    pub fn parental_control_advance_to_question2(&mut self) {
        log::info!("🔒 Advancing to parental control question 2");
        self.parental_control_stage = ParentalControlStage::Question2;
        self.parental_control_input.clear();
        self.parental_control_error = None;
    }
    
    /// Cancel parental control challenge
    pub fn cancel_parental_control_challenge(&mut self) {
        log::info!("🔒 Cancelling parental control challenge");
        self.show_parental_control_modal = false;
        self.pending_protected_action = None;
        self.parental_control_stage = ParentalControlStage::Question1;
        self.parental_control_input.clear();
        self.parental_control_error = None;
        self.parental_control_loading = false;
    }
    
    /// Submit answer for validation
    pub fn submit_parental_control_answer(&mut self) {
        if self.parental_control_input.trim().is_empty() {
            self.parental_control_error = Some("Please enter an answer".to_string());
            return;
        }
        
        log::info!("🔒 Submitting parental control answer for validation");
        self.parental_control_loading = true;
        self.parental_control_error = None;
        
        // Create command for backend
        let command = crate::backend::domain::commands::parental_control::ValidateParentalControlCommand {
            answer: self.parental_control_input.clone(),
        };
        
        // Call backend service
        match self.backend.parental_control_service.validate_answer(command) {
            Ok(result) => {
                self.parental_control_loading = false;
                
                if result.success {
                    log::info!("✅ Parental control validation successful");
                    self.parental_control_stage = ParentalControlStage::Authenticated;
                    
                    // Execute the pending action
                    if let Some(action) = self.pending_protected_action {
                        self.execute_protected_action(action);
                    }
                    
                    // Close modal after brief success display
                    self.show_parental_control_modal = false;
                    self.success_message = Some("Access granted!".to_string());
                } else {
                    log::info!("❌ Parental control validation failed");
                    self.parental_control_error = Some(result.message);
                    self.parental_control_input.clear();
                }
            }
            Err(e) => {
                self.parental_control_loading = false;
                log::error!("🚨 Parental control validation error: {}", e);
                self.parental_control_error = Some("Validation failed. Please try again.".to_string());
            }
        }
    }
    
    /// Execute the action after successful authentication
    fn execute_protected_action(&mut self, action: ProtectedAction) {
        match action {
            ProtectedAction::DeleteTransactions => {
                log::info!("🗑️ Executing delete transactions action");
                // This will be implemented in Phase 2
                // For now, just enter selection mode
                self.success_message = Some("Delete mode activated! Select transactions to delete.".to_string());
                // TODO: self.enter_transaction_selection_mode();
            }
        }
        
        self.pending_protected_action = None;
    }
}
```

### **Step 3: Create Parental Control Modal Component**

**3.1 Add to modals.rs**
Find the `egui-frontend/src/ui/components/modals.rs` file and add the parental control modal rendering:

```rust
impl AllowanceTrackerApp {
    /// Render the parental control modal
    pub fn render_parental_control_modal(&mut self, ctx: &egui::Context) {
        if !self.show_parental_control_modal {
            return;
        }
        
        log::info!("🔒 Rendering parental control modal - stage: {:?}", self.parental_control_stage);
        
        // Modal window with dark background
        egui::Area::new(egui::Id::new("parental_control_modal_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Dark semi-transparent background
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    egui::Rounding::ZERO,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128)
                );
                
                // Center the modal content
                ui.allocate_ui_at_rect(screen_rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        self.render_parental_control_modal_content(ui);
                    });
                });
            });
    }
    
    /// Render the actual modal content
    fn render_parental_control_modal_content(&mut self, ui: &mut egui::Ui) {
        // Modal card background
        let modal_size = egui::vec2(400.0, 250.0);
        
        egui::Frame::window(&ui.style())
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(126, 120, 229))) // Purple border
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(20.0))
            .show(ui, |ui| {
                ui.set_min_size(modal_size);
                
                match self.parental_control_stage {
                    ParentalControlStage::Question1 => self.render_question1(ui),
                    ParentalControlStage::Question2 => self.render_question2(ui),
                    ParentalControlStage::Authenticated => self.render_success(ui),
                }
            });
    }
    
    /// Render first question: "Are you Mom or Dad?"
    fn render_question1(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            
            // Lock icon
            ui.label(egui::RichText::new("🔒")
                .font(egui::FontId::new(32.0, egui::FontFamily::Proportional)));
            
            ui.add_space(15.0);
            
            // Question
            ui.label(egui::RichText::new("Parental Control")
                .font(egui::FontId::new(20.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(10.0);
            
            ui.label(egui::RichText::new("Are you Mom or Dad?")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(80, 80, 80)));
            
            ui.add_space(20.0);
            
            // Buttons
            ui.horizontal(|ui| {
                ui.add_space(50.0);
                
                // No button
                let no_button = egui::Button::new(
                    egui::RichText::new("No")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100))
                )
                .min_size(egui::vec2(80.0, 35.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::from_rgb(240, 240, 240))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 200)));
                
                if ui.add(no_button).clicked() {
                    self.cancel_parental_control_challenge();
                }
                
                ui.add_space(20.0);
                
                // Yes button
                let yes_button = egui::Button::new(
                    egui::RichText::new("Yes")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::WHITE)
                )
                .min_size(egui::vec2(80.0, 35.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::from_rgb(126, 120, 229)) // Purple
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(126, 120, 229)));
                
                if ui.add(yes_button).clicked() {
                    self.parental_control_advance_to_question2();
                }
            });
            
            ui.add_space(10.0);
        });
    }
    
    /// Render second question: "What's cooler than cool?"
    fn render_question2(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            
            // Question mark icon
            ui.label(egui::RichText::new("❄️")
                .font(egui::FontId::new(32.0, egui::FontFamily::Proportional)));
            
            ui.add_space(15.0);
            
            // Challenge question
            ui.label(egui::RichText::new("Oh yeah?? If so...")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(60, 60, 60)));
            
            ui.add_space(5.0);
            
            ui.label(egui::RichText::new("What's cooler than cool?")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(80, 80, 80)));
            
            ui.add_space(15.0);
            
            // Text input
            let text_input = egui::TextEdit::singleline(&mut self.parental_control_input)
                .hint_text("Enter your answer...")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .desired_width(250.0);
            
            let input_response = ui.add(text_input);
            
            // Auto-focus input field
            if input_response.gained_focus() || self.parental_control_input.is_empty() {
                input_response.request_focus();
            }
            
            // Handle Enter key
            if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.submit_parental_control_answer();
            }
            
            ui.add_space(10.0);
            
            // Error message
            if let Some(error) = &self.parental_control_error {
                ui.label(egui::RichText::new(error)
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::from_rgb(220, 53, 69))); // Red
                ui.add_space(5.0);
            }
            
            // Buttons
            ui.horizontal(|ui| {
                ui.add_space(50.0);
                
                // Cancel button
                let cancel_button = egui::Button::new(
                    egui::RichText::new("Cancel")
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::from_rgb(100, 100, 100))
                )
                .min_size(egui::vec2(80.0, 35.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(egui::Color32::from_rgb(240, 240, 240))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 200)));
                
                if ui.add(cancel_button).clicked() {
                    self.cancel_parental_control_challenge();
                }
                
                ui.add_space(20.0);
                
                // Submit button
                let submit_text = if self.parental_control_loading {
                    "⏳ Checking..."
                } else {
                    "Submit"
                };
                
                let submit_button = egui::Button::new(
                    egui::RichText::new(submit_text)
                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::WHITE)
                )
                .min_size(egui::vec2(80.0, 35.0))
                .rounding(egui::Rounding::same(6.0))
                .fill(if self.parental_control_loading {
                    egui::Color32::from_rgb(150, 150, 150) // Gray when loading
                } else {
                    egui::Color32::from_rgb(126, 120, 229) // Purple
                })
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(126, 120, 229)));
                
                if ui.add(submit_button).clicked() && !self.parental_control_loading {
                    self.submit_parental_control_answer();
                }
            });
            
            ui.add_space(10.0);
        });
    }
    
    /// Render success state (brief display before closing)
    fn render_success(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            
            // Success icon
            ui.label(egui::RichText::new("✅")
                .font(egui::FontId::new(32.0, egui::FontFamily::Proportional)));
            
            ui.add_space(15.0);
            
            ui.label(egui::RichText::new("Access Granted!")
                .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                .strong()
                .color(egui::Color32::from_rgb(34, 139, 34))); // Green
            
            ui.add_space(30.0);
        });
    }
}
```

### **Step 4: Wire Settings Menu Integration**

**4.1 Modify header.rs settings menu**
Find the `render_settings_dropdown_menu()` method and update the "Delete transactions" case:

```rust
// In header.rs, update case 3 (Delete transactions)
3 => {
    // Delete transactions - trigger parental control
    log::info!("🗑️ Delete transactions menu item clicked");
    self.start_parental_control_challenge(crate::ui::app_state::ProtectedAction::DeleteTransactions);
}
```

### **Step 5: Add Modal to Main Render Loop**

**5.1 Update app_coordinator.rs**
In the `update()` method, add the parental control modal rendering:

```rust
// In app_coordinator.rs, after existing modal rendering
impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ... existing code ...
        
        // Render modals
        self.render_modals(ctx);
        
        // Add parental control modal
        self.render_parental_control_modal(ctx);
    }
}
```

### **Step 6: Import Backend Commands**

**6.1 Add necessary imports**
Ensure the parental control command types are available:

```rust
// Add to the top of app_state.rs
use crate::backend::domain::commands::parental_control::{
    ValidateParentalControlCommand, 
    ValidateParentalControlResult
};
```

## **🧪 Testing Plan**

### **Manual Testing Checklist**
1. **Settings Menu Integration**
   - [ ] Click "Delete transactions" in settings menu
   - [ ] Parental control modal opens correctly
   - [ ] Modal has proper styling and positioning

2. **Stage 1 Testing**
   - [ ] "Are you Mom or Dad?" question displays
   - [ ] "No" button cancels and closes modal
   - [ ] "Yes" button advances to stage 2

3. **Stage 2 Testing**
   - [ ] Challenge question displays correctly
   - [ ] Text input field accepts input
   - [ ] Enter key submits answer
   - [ ] Cancel button works
   - [ ] Wrong answer shows error message
   - [ ] Correct answer ("ice cold") proceeds to success

4. **Backend Integration**
   - [ ] Correct answer validates successfully
   - [ ] Incorrect answers are rejected
   - [ ] Loading state displays during validation
   - [ ] Error handling works for backend failures

5. **Edge Cases**
   - [ ] Case insensitive matching works ("ICE COLD", "Ice Cold")
   - [ ] Whitespace trimming works ("  ice cold  ")
   - [ ] Empty input validation
   - [ ] Modal dismissal with Escape key
   - [ ] Multiple rapid clicks don't cause issues

### **Backend Testing**
Test the backend service directly:
```rust
// Test correct answer
let cmd = ValidateParentalControlCommand { answer: "ice cold".to_string() };
let result = backend.parental_control_service.validate_answer(cmd);
assert!(result.is_ok() && result.unwrap().success);

// Test wrong answer
let cmd = ValidateParentalControlCommand { answer: "wrong".to_string() };
let result = backend.parental_control_service.validate_answer(cmd);
assert!(result.is_ok() && !result.unwrap().success);
```

## **📋 Implementation Timeline**

### **Session 1 (1 hour): State Setup**
- Add enums and state fields to app_state.rs
- Add helper methods for state management
- Test compilation and basic state management

### **Session 2 (1-1.5 hours): Modal UI**
- Create modal rendering functions
- Implement two-stage UI flow
- Test modal display and navigation

### **Session 3 (0.5-1 hour): Backend Integration**
- Wire backend service calls
- Implement answer validation
- Test end-to-end authentication flow

### **Session 4 (0.5 hour): Settings Integration & Polish**
- Connect settings menu to parental control
- Add final polish and error handling
- Final testing and bug fixes

**Total Estimated Time: 3-4 hours**

## **🚀 Next Steps After Completion**
Once parental control is working:
1. **Phase 2**: Transaction selection system
2. **Integration**: Wire authenticated action execution
3. **Extensions**: Apply parental control to other admin features

## **📝 Success Criteria**
- ✅ Settings menu "Delete transactions" triggers parental control
- ✅ Two-stage challenge UI functions correctly
- ✅ Backend validation works with proper answer
- ✅ Error handling covers all failure modes
- ✅ UI styling matches existing app theme
- ✅ Modal behavior is intuitive and responsive 