# Phase 2: money_transaction.rs Refactoring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract helper methods from `render_money_transaction_modal()` to reduce nesting and improve readability.

**Architecture:** Extract two helper methods (`render_money_transaction_form_content` and `render_money_transaction_buttons`) from the main render method, mirroring the pattern already established in `allowance_config_modal.rs`.

**Tech Stack:** Rust, egui 0.31

---

## Task 1: Extract Form Content Helper

**Files:**
- Modify: `egui-frontend/src/ui/components/modals/money_transaction.rs`

**Step 1: Add the new helper method signature**

Add this method after the closing brace of `render_money_transaction_modal()` (after line 257), still inside the `impl AllowanceTrackerApp` block:

```rust
    /// Render the form fields (description + amount) for money transaction modal
    /// Returns the description_response for validation triggering
    fn render_money_transaction_form_content(
        &mut self,
        ui: &mut egui::Ui,
        config: &crate::ui::app_state::MoneyTransactionModalConfig,
        form_state: &mut crate::ui::app_state::MoneyTransactionFormState,
    ) {
        // Description field with validation
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Description:")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(60, 60, 60)));

            // Character count
            let char_count = form_state.description.len();
            let count_color = if char_count > config.max_description_length {
                egui::Color32::from_rgb(220, 50, 50) // Red if over limit
            } else if char_count > (config.max_description_length * 4 / 5) {
                egui::Color32::from_rgb(255, 140, 0) // Orange if approaching limit (80%)
            } else {
                egui::Color32::from_rgb(120, 120, 120) // Gray for normal
            };

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{}/{}", char_count, config.max_description_length))
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(count_color));
            });
        });
        ui.add_space(5.0);

        // Description field
        let description_response = ui.add(
            egui::TextEdit::singleline(&mut form_state.description)
                .hint_text(config.description_placeholder)
                .desired_width(400.0)
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
        );

        // Show description error message
        if let Some(error) = &form_state.description_error {
            ui.add_space(3.0);
            ui.label(egui::RichText::new(error)
                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(220, 50, 50)));
        }

        ui.add_space(15.0);

        // Amount field with validation
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Amount:")
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(60, 60, 60)));
        });
        ui.add_space(5.0);

        // Amount input with static dollar sign
        ui.horizontal(|ui| {
            // Static dollar sign
            ui.label(egui::RichText::new("$")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(60, 60, 60)));

            ui.add_space(2.0);

            // Amount field
            let amount_response = ui.add(
                egui::TextEdit::singleline(&mut form_state.amount)
                    .hint_text(config.amount_placeholder)
                    .desired_width(120.0)
                    .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
            );

            // Validate form whenever fields change
            if description_response.changed() || amount_response.changed() {
                self.validate_money_transaction_form(form_state, config);
            }
        });

        // Show amount error message
        if let Some(error) = &form_state.amount_error {
            ui.add_space(3.0);
            ui.label(egui::RichText::new(error)
                .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(220, 50, 50)));
        }
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "(error|warning:.*money_transaction)" | head -20`

Expected: No errors (warnings OK)

**Step 3: Replace inline form code with helper call**

In `render_money_transaction_modal()`, replace lines 79-158 (the form fields section) with:

```rust
                                    // Form fields
                                    self.render_money_transaction_form_content(ui, config, form_state);

                                    ui.add_space(30.0);
```

**Step 4: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -10`

Expected: Build succeeds, tests pass

**Step 5: Commit**

```bash
git add egui-frontend/src/ui/components/modals/money_transaction.rs
git commit -m "refactor: extract render_money_transaction_form_content helper

Reduces nesting in render_money_transaction_modal() by extracting
form field rendering (description + amount) into dedicated method.
Mirrors pattern from allowance_config_modal.rs.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Extract Buttons Helper

**Files:**
- Modify: `egui-frontend/src/ui/components/modals/money_transaction.rs`

**Step 1: Add the buttons helper method**

Add this method after `render_money_transaction_form_content()`:

```rust
    /// Render action buttons for money transaction modal
    /// Returns true if submit was clicked with valid form
    fn render_money_transaction_buttons(
        &mut self,
        ui: &mut egui::Ui,
        config: &crate::ui::app_state::MoneyTransactionModalConfig,
        form_state: &mut crate::ui::app_state::MoneyTransactionFormState,
    ) -> bool {
        let mut form_submitted = false;

        ui.horizontal(|ui| {
            ui.add_space(50.0);

            // Submit button
            let button_enabled = form_state.is_valid &&
                !form_state.description.trim().is_empty() &&
                !form_state.amount.trim().is_empty();

            let button_color = if button_enabled {
                config.color
            } else {
                egui::Color32::from_rgb(180, 180, 180) // Gray when disabled
            };

            let submit_button = egui::Button::new(egui::RichText::new(config.button_text)
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::WHITE))
                .fill(button_color)
                .stroke(egui::Stroke::new(2.0, button_color))
                .corner_radius(egui::CornerRadius::same(10))
                .min_size(egui::vec2(150.0, 40.0));

            let submit_response = ui.add(submit_button);

            if submit_response.clicked() && button_enabled {
                form_submitted = true;
            }

            // Show tooltip for disabled button
            if !button_enabled && submit_response.hovered() {
                submit_response.on_hover_text("Please fix the errors above to continue");
            }

            ui.add_space(30.0);

            // Cancel button
            let cancel_button = egui::Button::new(egui::RichText::new("Cancel")
                .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                .color(egui::Color32::WHITE))
                .fill(egui::Color32::from_rgb(120, 120, 120))
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(120, 120, 120)))
                .corner_radius(egui::CornerRadius::same(10))
                .min_size(egui::vec2(100.0, 40.0));

            if ui.add(cancel_button).clicked() {
                // Clear form and close modal
                form_state.clear();
                self.calendar.selected_day = None;
                self.calendar.active_overlay = None;
            }
        });

        form_submitted
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "(error|warning:.*money_transaction)" | head -20`

Expected: No errors (may have unused warning - that's OK)

**Step 3: Replace inline buttons code with helper call**

In `render_money_transaction_modal()`, find the buttons section (the `ui.horizontal` starting with `ui.add_space(50.0)`) and replace it with:

```rust
                                    // Action buttons
                                    if self.render_money_transaction_buttons(ui, config, form_state) {
                                        form_submitted = true;
                                    }
```

**Step 4: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -10`

Expected: Build succeeds, tests pass

**Step 5: Commit**

```bash
git add egui-frontend/src/ui/components/modals/money_transaction.rs
git commit -m "refactor: extract render_money_transaction_buttons helper

Extracts button rendering (submit + cancel) into dedicated method.
Completes Phase 2 nesting reduction for money_transaction.rs.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Clean Up and Verify

**Files:**
- Modify: `egui-frontend/src/ui/components/modals/money_transaction.rs`

**Step 1: Remove unused form_submitted declaration if redundant**

After the refactoring, check if `let mut form_submitted = false;` at line 33 is still needed. It should be, since the helper returns the value.

**Step 2: Run full test suite**

Run: `cargo test 2>&1 | tail -20`

Expected: All 191 tests pass

**Step 3: Run clippy for lint check**

Run: `cargo clippy 2>&1 | grep -E "(error|warning)" | head -20`

Expected: No new warnings related to money_transaction.rs

**Step 4: Verify line count reduction**

Run: `wc -l egui-frontend/src/ui/components/modals/money_transaction.rs`

Expected: Around 180-200 lines (down from 258)

**Step 5: Final commit if any cleanup was needed**

If changes were made:
```bash
git add egui-frontend/src/ui/components/modals/money_transaction.rs
git commit -m "chore: cleanup money_transaction.rs after refactoring

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Verification Checklist

After completing all tasks:

- [ ] `cargo build` succeeds
- [ ] `cargo test` passes (191 tests)
- [ ] `render_money_transaction_modal()` is under 120 lines
- [ ] Two new helper methods exist: `render_money_transaction_form_content()` and `render_money_transaction_buttons()`
- [ ] Modal still functions correctly (manual test: open app, click on calendar day, try add income/expense)
