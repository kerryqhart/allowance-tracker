# Phase 2b: calendar_renderer/rendering.rs Refactoring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Reduce nesting and eliminate code duplication in `rendering.rs` by extracting helper methods and separating concerns.

**Architecture:** Three-part refactoring: (A) Extract visual helpers from `render_with_config()`, (B) DRY up `render_calendar_chip()` duplicate branches, (C) Separate layout calculation from rendering in `draw_calendar_section_with_toggle()`.

**Tech Stack:** Rust, egui 0.31

---

## Task 1: Extract draw_today_shadow() Helper

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Add the helper method**

Add this method inside `impl CalendarDay` block (after line 68, before `render_with_config`):

```rust
    /// Draw subtle shadow behind today's cell
    fn draw_today_shadow(&self, ui: &egui::Ui, cell_rect: egui::Rect) {
        let shadow_rect = egui::Rect::from_min_size(
            cell_rect.min + egui::vec2(2.0, 2.0),
            cell_rect.size()
        );
        ui.painter().rect_filled(
            shadow_rect,
            egui::CornerRadius::same(2),
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 30)
        );
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "^error" | head -5`

Expected: No errors

**Step 3: Replace inline shadow code with helper call**

In `render_with_config()`, find lines 80-91 (the shadow drawing code) and replace with:

```rust
        // Draw shadow first (behind everything else) for today's date
        if self.is_today {
            self.draw_today_shadow(ui, cell_rect);
        }
```

**Step 4: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -5`

Expected: Build succeeds, tests pass

**Step 5: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: extract draw_today_shadow helper

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Extract draw_day_cell_background() Helper

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Add the helper method**

Add after `draw_today_shadow()`:

```rust
    /// Calculate and draw background fill for day cell with hover/selection effects
    fn draw_day_cell_background(
        &self,
        ui: &egui::Ui,
        cell_rect: egui::Rect,
        is_hovered: bool,
        config: &RenderConfig,
    ) {
        let base_bg_color = self.day_type.background_color(self.is_today);
        let bg_color = if config.is_selected {
            egui::Color32::from_rgba_unmultiplied(230, 190, 235, 140) // Purple-pink for selection
        } else if is_hovered {
            if self.is_today {
                egui::Color32::from_rgba_unmultiplied(255, 248, 220, 180) // More opaque yellow
            } else {
                match self.day_type {
                    CalendarDayType::CurrentMonth => {
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 120) // More opaque white
                    }
                    CalendarDayType::FillerDay => {
                        egui::Color32::from_rgba_unmultiplied(120, 120, 120, 160) // More opaque gray
                    }
                }
            }
        } else {
            base_bg_color
        };

        ui.painter().rect_filled(
            cell_rect,
            egui::CornerRadius::same(2),
            bg_color
        );
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "^error" | head -5`

Expected: No errors

**Step 3: Replace inline background code with helper call**

In `render_with_config()`, find lines 93-123 (background calculation and drawing) and replace with:

```rust
        // Draw background for the day cell
        self.draw_day_cell_background(ui, cell_rect, is_hovered, config);
```

**Step 4: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -5`

Expected: Build succeeds, tests pass

**Step 5: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: extract draw_day_cell_background helper

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Extract draw_day_cell_border() Helper

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Add the helper method**

Add after `draw_day_cell_background()`:

```rust
    /// Draw border around day cell (handles today's double-outline, selection, normal)
    fn draw_day_cell_border(&self, ui: &egui::Ui, cell_rect: egui::Rect, config: &RenderConfig) {
        if config.is_selected {
            // Selected day gets a purple-pink border
            ui.painter().rect_stroke(
                cell_rect,
                egui::CornerRadius::same(2),
                egui::Stroke::new(2.0, egui::Color32::from_rgb(199, 112, 221)),
                egui::StrokeKind::Outside
            );
        } else if self.is_today {
            // Double outline for today: white inner + dark outer
            ui.painter().rect_stroke(
                cell_rect,
                egui::CornerRadius::same(2),
                egui::Stroke::new(2.0, egui::Color32::WHITE),
                egui::StrokeKind::Outside
            );

            let outer_rect = egui::Rect::from_min_size(
                cell_rect.min - egui::vec2(1.0, 1.0),
                cell_rect.size() + egui::vec2(2.0, 2.0)
            );
            ui.painter().rect_stroke(
                outer_rect,
                egui::CornerRadius::same(2),
                egui::Stroke::new(2.0, self.day_type.border_color(self.is_today)),
                egui::StrokeKind::Outside
            );
        } else {
            // Normal single outline
            let border_color = self.day_type.border_color(self.is_today);
            ui.painter().rect_stroke(
                cell_rect,
                egui::CornerRadius::same(2),
                egui::Stroke::new(0.5, border_color),
                egui::StrokeKind::Outside
            );
        }
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "^error" | head -5`

Expected: No errors

**Step 3: Replace inline border code with helper call**

In `render_with_config()`, find lines 125-164 (border drawing code) and replace with:

```rust
        // Draw border around the day cell
        self.draw_day_cell_border(ui, cell_rect, config);
```

**Step 4: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -5`

Expected: Build succeeds, tests pass

**Step 5: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: extract draw_day_cell_border helper

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Extract render_collapse_button() Helper

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Add the helper method**

Add after `draw_day_cell_border()`:

```rust
    /// Render collapse button at bottom of expanded day cell
    /// Returns true if clicked
    fn render_collapse_button(&self, ui: &mut egui::Ui, cell_rect: egui::Rect) -> bool {
        let collapse_height = 22.0;
        let collapse_rect = egui::Rect::from_min_size(
            egui::pos2(cell_rect.left(), cell_rect.bottom() - collapse_height),
            egui::vec2(cell_rect.width(), collapse_height)
        );

        let collapse_response = ui.allocate_rect(collapse_rect, egui::Sense::hover().union(egui::Sense::click()));

        let collapse_bg_color = if collapse_response.hovered() {
            egui::Color32::from_rgba_unmultiplied(245, 245, 245, 255)
        } else {
            egui::Color32::WHITE
        };

        ui.painter().rect_filled(
            collapse_rect,
            egui::CornerRadius::ZERO,
            collapse_bg_color
        );

        // Draw triangle symbol
        let triangle_size = 8.0;
        let center = collapse_rect.center();
        let triangle_points = [
            egui::pos2(center.x, center.y - triangle_size / 2.0),
            egui::pos2(center.x - triangle_size / 2.0, center.y + triangle_size / 2.0),
            egui::pos2(center.x + triangle_size / 2.0, center.y + triangle_size / 2.0),
        ];

        ui.painter().add(egui::Shape::convex_polygon(
            triangle_points.to_vec(),
            egui::Color32::from_rgb(120, 120, 120),
            egui::Stroke::NONE,
        ));

        collapse_response.clicked()
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "^error" | head -5`

Expected: No errors

**Step 3: Replace inline collapse button code with helper call**

In `render_with_config()`, find lines 284-331 (collapse button rendering) and replace with:

```rust
        // Render collapse button OUTSIDE content flow if day is expanded
        if config.expanded_day == Some(self.date) {
            if self.render_collapse_button(ui, cell_rect) {
                clicked_transaction_ids.push("COLLAPSE_CLICKED".to_string());
            }
        }
```

**Step 4: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -5`

Expected: Build succeeds, tests pass

**Step 5: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: extract render_collapse_button helper

Completes Part A of Phase 2b - render_with_config() extraction.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Extract draw_chip_visual() Helper

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Add the helper method**

Add after `render_collapse_button()`, before `render_calendar_chip()`:

```rust
    /// Draw the visual elements of a chip (background, border, text)
    fn draw_chip_visual(
        &self,
        ui: &mut egui::Ui,
        chip: &CalendarChip,
        rect: egui::Rect,
        is_hovered: bool,
        font_family: egui::FontFamily,
        chip_font_size: f32,
    ) {
        let chip_color = chip.chip_type.primary_color();
        let text_color = chip.chip_type.text_color();
        let uses_dotted_border = chip.chip_type.uses_dotted_border();
        let chip_background = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255);

        // Background color - slightly darker when hovered
        let background_color = if is_hovered {
            egui::Color32::from_rgba_unmultiplied(245, 245, 245, 255)
        } else {
            chip_background
        };

        // Draw background
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(4),
            background_color
        );

        // Draw border - solid or dotted based on chip type
        if uses_dotted_border {
            self.draw_dotted_border(ui, rect, chip_color);
        } else {
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(4),
                egui::Stroke::new(1.0, chip_color),
                egui::StrokeKind::Outside
            );
        }

        // Draw text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &chip.display_amount,
            egui::FontId::new(chip_font_size, font_family),
            text_color,
        );
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "^error" | head -5`

Expected: No errors

**Step 3: Commit intermediate progress**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: add draw_chip_visual helper method

Preparation for Part B - DRYing up render_calendar_chip().

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Refactor render_calendar_chip() to Use draw_chip_visual()

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Rewrite render_calendar_chip() to use the helper**

Replace the entire `render_calendar_chip()` method with:

```rust
    /// Render a single calendar chip with unified styling and hover effects
    /// Returns the transaction ID if the checkbox was clicked (for selection toggle)
    /// Returns "ELLIPSIS_CLICKED" if the ellipsis chip was clicked (for expansion toggle)
    fn render_calendar_chip(&self, ui: &mut egui::Ui, chip: &CalendarChip, width: f32, _height: f32, config: &RenderConfig) -> Option<String> {
        let font_family = get_calendar_font_family(ui.ctx());
        let (chip_width, chip_height, chip_font_size) = calculate_chip_dimensions(config.is_grid_layout, width);

        // Check if we should show checkbox (only for deletable transactions in selection mode)
        let show_checkbox = config.transaction_selection_mode &&
                            !matches!(chip.chip_type, CalendarChipType::FutureAllowance | CalendarChipType::Goal);
        let checkbox_width = if show_checkbox { 16.0 } else { 0.0 };
        let checkbox_spacing = if show_checkbox { 4.0 } else { 0.0 };
        let adjusted_chip_width = if show_checkbox {
            chip_width - checkbox_width - checkbox_spacing
        } else {
            chip_width
        };

        let mut result = None;

        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            if show_checkbox {
                ui.horizontal(|ui| {
                    // Checkbox on the left
                    let is_selected = config.selected_transaction_ids.contains(&chip.transaction.id);
                    let checkbox_response = ui.add_sized(
                        [checkbox_width, checkbox_width],
                        egui::Checkbox::new(&mut is_selected.clone(), "")
                    );

                    if checkbox_response.clicked() {
                        result = Some(chip.transaction.id.clone());
                    }

                    ui.add_space(checkbox_spacing);

                    // Chip on the right
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(adjusted_chip_width, chip_height), egui::Sense::hover());
                    self.draw_chip_visual(ui, chip, rect, response.hovered(), font_family.clone(), chip_font_size);

                    if response.hovered() && !chip.transaction.description.is_empty() {
                        self.show_transaction_tooltip(ui, &chip.transaction.description, rect);
                    }
                });
            } else {
                // Determine sense based on chip type
                let sense = if matches!(chip.chip_type, CalendarChipType::Ellipsis) {
                    egui::Sense::hover().union(egui::Sense::click())
                } else {
                    egui::Sense::hover()
                };

                let (rect, response) = ui.allocate_exact_size(egui::vec2(chip_width, chip_height), sense);
                self.draw_chip_visual(ui, chip, rect, response.hovered(), font_family.clone(), chip_font_size);

                // Check for ellipsis click
                if response.clicked() && matches!(chip.chip_type, CalendarChipType::Ellipsis) {
                    result = Some("ELLIPSIS_CLICKED".to_string());
                }

                if response.hovered() && !chip.transaction.description.is_empty() {
                    self.show_transaction_tooltip(ui, &chip.transaction.description, rect);
                }
            }
        });

        result
    }
```

**Step 2: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -5`

Expected: Build succeeds, tests pass

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: DRY up render_calendar_chip using draw_chip_visual

Completes Part B of Phase 2b - eliminates duplicated chip rendering code.
Method reduced from 156 to ~60 lines.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 7: Create CalendarLayout Struct

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Add the CalendarLayout struct**

Add at the top of the file, after the imports (around line 38):

```rust
/// Calculated layout dimensions for calendar rendering
struct CalendarLayout {
    cell_width: f32,
    cell_height: f32,
    calendar_width: f32,
    card_height: f32,
    card_rect: egui::Rect,
    header_height: f32,
}
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "^error" | head -5`

Expected: No errors (may have unused warning)

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: add CalendarLayout struct for layout dimensions

Preparation for Part C - separating layout from rendering.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 8: Extract calculate_calendar_layout() Method

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Add the calculate_calendar_layout method**

Add inside `impl AllowanceTrackerApp`, before `draw_calendar_section_with_toggle()`:

```rust
    /// Calculate all layout dimensions for calendar rendering
    fn calculate_calendar_layout(&self, ui: &egui::Ui, available_rect: egui::Rect) -> CalendarLayout {
        let content_width = available_rect.width() - 40.0;
        let calendar_width = content_width;
        let total_spacing = CALENDAR_CARD_SPACING * 6.0;
        let cell_width = (calendar_width - total_spacing) / 7.0;

        let actual_available_rect = ui.available_rect_before_wrap();
        let card_height = actual_available_rect.height() - 40.0;

        let header_height = header::HEADER_HEIGHT;
        let calendar_container_padding = 20.0;

        // Get calendar data to determine row count
        let calendar_days_count = if let Some(ref calendar_month) = self.calendar.calendar_month {
            calendar_month.days.len()
        } else {
            35
        };

        let rows_needed = (calendar_days_count as f32 / 7.0).ceil();
        let vertical_spacing = CALENDAR_CARD_SPACING * (rows_needed - 1.0);
        let available_height_for_cells = card_height - calendar_container_padding - header_height - vertical_spacing;
        let cell_height = (available_height_for_cells / rows_needed).max(40.0).min(200.0);

        let card_rect = egui::Rect::from_min_size(
            egui::pos2(actual_available_rect.left() + 20.0, actual_available_rect.top() + 20.0),
            egui::vec2(content_width, card_height)
        );

        CalendarLayout {
            cell_width,
            cell_height,
            calendar_width,
            card_height,
            card_rect,
            header_height,
        }
    }
```

**Step 2: Build to verify syntax**

Run: `cargo build 2>&1 | grep -E "^error" | head -5`

Expected: No errors

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: add calculate_calendar_layout method

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 9: Refactor draw_calendar_section_with_toggle() to Use CalendarLayout

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Rewrite draw_calendar_section_with_toggle()**

Replace the method with:

```rust
    /// Draw calendar section with toggle header integrated
    pub fn draw_calendar_section_with_toggle(&mut self, ui: &mut egui::Ui, available_rect: egui::Rect, transactions: &[Transaction]) {
        let font_family = get_calendar_font_family(ui.ctx());

        ui.add_space(15.0);

        let layout = self.calculate_calendar_layout(ui, available_rect);

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(layout.card_rect), |ui| {
            ui.vertical(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(layout.calendar_width, layout.card_height),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                // Day headers
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = CALENDAR_CARD_SPACING;
                                    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
                                    for day_name in day_names.iter() {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(layout.cell_width, layout.header_height),
                                            egui::Layout::centered_and_justified(egui::Direction::TopDown),
                                            |ui| {
                                                let header_rect = ui.available_rect_before_wrap();

                                                ui.painter().rect_filled(
                                                    header_rect,
                                                    egui::CornerRadius::same(2),
                                                    header::background_color()
                                                );

                                                ui.painter().rect_stroke(
                                                    header_rect,
                                                    egui::CornerRadius::same(2),
                                                    egui::Stroke::new(1.0, header::border_color()),
                                                    egui::StrokeKind::Outside
                                                );

                                                ui.add(egui::Label::new(egui::RichText::new(*day_name)
                                                    .font(egui::FontId::new(header::HEADER_FONT_SIZE, font_family.clone()))
                                                    .strong()
                                                    .color(egui::Color32::DARK_GRAY))
                                                    .selectable(false));
                                            },
                                        );
                                    }
                                });

                                ui.add_space(5.0);

                                self.draw_calendar_days_responsive(ui, transactions, layout.cell_width, layout.cell_height);
                            }
                        );
                    }
                );
            });
        });
    }
```

**Step 2: Build and test**

Run: `cargo build && cargo test 2>&1 | tail -5`

Expected: Build succeeds, tests pass

**Step 3: Commit**

```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "refactor: use CalendarLayout in draw_calendar_section_with_toggle

Completes Part C of Phase 2b - separates layout calculation from rendering.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Task 10: Final Cleanup and Verification

**Files:**
- Modify: `egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

**Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -20`

Expected: All 191 tests pass

**Step 2: Run clippy for lint check**

Run: `cargo clippy 2>&1 | grep -E "(error|warning.*rendering)" | head -10`

Expected: No new warnings in rendering.rs

**Step 3: Check line count**

Run: `wc -l egui-frontend/src/ui/components/calendar_renderer/rendering.rs`

Expected: ~750-800 lines (down from 979)

**Step 4: Final commit if cleanup needed**

If any cleanup was done:
```bash
git add egui-frontend/src/ui/components/calendar_renderer/rendering.rs
git commit -m "chore: cleanup rendering.rs after Phase 2b refactoring

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Verification Checklist

After completing all tasks:

- [ ] `cargo build` succeeds
- [ ] `cargo test` passes (191 tests)
- [ ] `render_with_config()` reduced from 266 to ~80 lines
- [ ] `render_calendar_chip()` reduced from 156 to ~60 lines
- [ ] `draw_calendar_section_with_toggle()` uses CalendarLayout struct
- [ ] No duplicated chip rendering code
- [ ] Calendar still displays correctly (manual test: run app, navigate months, click days)
