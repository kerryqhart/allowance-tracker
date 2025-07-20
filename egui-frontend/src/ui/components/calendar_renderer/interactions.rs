use eframe::egui;
use chrono::NaiveDate;
use crate::ui::app_state::AllowanceTrackerApp;
use super::types::DayMenuGlyph;
use super::styling::action_icons;

impl AllowanceTrackerApp {
    /// Handle clicks on calendar days
    pub fn handle_calendar_day_click(&mut self, clicked_date: chrono::NaiveDate) {
        if let Some(selected_date) = self.calendar.selected_day {
            if selected_date == clicked_date {
                // Clicking the same day again deselects it
                self.calendar.selected_day = None;
                
                // TEMPORARY: Sync compatibility field
                return;
            }
        }
        
        // Select new day
        self.calendar.selected_day = Some(clicked_date);
        
        // TEMPORARY: Sync compatibility field  
        
        log::info!("📅 Selected day: {}", clicked_date);
    }

    /// Render action icons above the selected day
    pub fn render_day_action_icons(&mut self, ui: &mut egui::Ui, day_cell_rect: egui::Rect, selected_date: NaiveDate) {
        // Get glyphs that should be shown for this specific date
        let glyphs = DayMenuGlyph::for_date(selected_date);
        
        // If no glyphs should be shown for this date, return early
        if glyphs.is_empty() {
            return;
        }
        
        // Shared styling for all glyphs
        let glyph_size = action_icons::GLYPH_SIZE;
        let glyph_spacing = action_icons::GLYPH_SPACING;
        
        // Shared colors
        let outline_color = action_icons::outline_color();
        let background_color = action_icons::background_color();
        let text_color = action_icons::text_color();
        
        // Calculate the actual width of the glyphs by measuring them
        let total_glyph_width = glyph_size.x * glyphs.len() as f32;
        let total_spacing = glyph_spacing * (glyphs.len() - 1) as f32;
        let total_width = total_glyph_width + total_spacing;
        
        // Position each glyph individually for precise control
        let center_x = day_cell_rect.center().x;
        let start_x = center_x - (total_width / 2.0);
        let glyphs_y = day_cell_rect.top() - glyph_size.y - action_icons::VERTICAL_OFFSET;
        
        // Render each glyph as a separate Area for precise positioning
        for (i, glyph) in glyphs.iter().enumerate() {
            let glyph_x = start_x + (i as f32 * (glyph_size.x + glyph_spacing));
            let glyph_pos = egui::pos2(glyph_x, glyphs_y);
            
            egui::Area::new(egui::Id::new(format!("day_menu_glyph_{}", i)))
                .fixed_pos(glyph_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    let glyph_text = glyph.text();
                    
                    // Create a button with consistent styling
                    let button = egui::Button::new(egui::RichText::new(glyph_text).color(text_color))
                        .fill(background_color)
                        .stroke(egui::Stroke::new(2.0, outline_color))
                        .rounding(egui::Rounding::same(4.0));
                    
                    if ui.add_sized(glyph_size, button).clicked() {
                        self.calendar.active_overlay = Some(glyph.overlay_type());
                        self.calendar.modal_just_opened = true; // Prevent backdrop click detection this frame
                        println!("🎯 Day menu glyph '{}' clicked for date: {}", glyph_text, selected_date);
                    }
                });
        }
    }

    /// Handle transaction deletion (placeholder for now)
    pub fn handle_transaction_deletion(&mut self) -> bool {
        if self.interaction.selected_transaction_ids.is_empty() {
            println!("⚠️ No transactions selected for deletion");
            return false;
        }

        println!("🗑️ Would delete {} transactions: {:?}", 
                 self.interaction.selected_transaction_ids.len(), 
                 self.interaction.selected_transaction_ids);

        // TODO: Implement actual deletion logic
        // For now, just clear the selection
        self.interaction.selected_transaction_ids.clear();
        self.interaction.transaction_selection_mode = false;
        
        true
    }

    /// Handle click on day action glyph (money buttons)
    pub fn handle_action_glyph_click(&mut self, glyph: &DayMenuGlyph) -> bool {
        // Create action overlay for the clicked glyph type
        match glyph {
            DayMenuGlyph::AddMoney | DayMenuGlyph::SpendMoney => {
                log::info!("💰 Action glyph clicked: {:?}", glyph);
                self.calendar.active_overlay = Some(glyph.overlay_type());
                self.calendar.modal_just_opened = true; // Prevent backdrop click detection this frame
                true
            }
            _ => false,
        }
    }

    /// Exit transaction selection mode
    pub fn exit_transaction_selection(&mut self) {
        if self.interaction.selected_transaction_ids.is_empty() {
            log::info!("🔄 Exiting transaction selection mode (no transactions selected)");
        } else {
            log::info!("🔄 Exiting transaction selection mode. {} transaction(s) were selected: {:?}", 
                      self.interaction.selected_transaction_ids.len(), 
                      self.interaction.selected_transaction_ids);
        }
        
        self.interaction.selected_transaction_ids.clear();
        self.interaction.transaction_selection_mode = false;
        self.clear_messages();
    }

    /// Toggle expanded state for a calendar day
    pub fn toggle_day_expanded(&mut self, date: chrono::NaiveDate) {
        if self.calendar.expanded_day == Some(date) {
            self.calendar.expanded_day = None;
            log::info!("📅 Collapsed day: {}", date);
        } else {
            self.calendar.expanded_day = Some(date);
            log::info!("📅 Expanded day: {}", date);
        }
    }

    /// Handle clicks outside of the calendar area to clear selection
    pub fn handle_calendar_background_click(&mut self) {
        if self.calendar.selected_day.is_some() {
            self.calendar.selected_day = None;
            
            // TEMPORARY: Sync compatibility field
            
            log::info!("📅 Cleared day selection");
        }
    }
} 