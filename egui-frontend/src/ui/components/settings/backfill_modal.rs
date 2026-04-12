use egui::{Align2, Area, Color32, Frame, Id, Order, RichText, Vec2};
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    pub fn render_backfill_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_backfill_modal {
            return;
        }

        // Poll for progress updates
        self.settings.backfill_form.poll_progress();

        // If running, request repaint to keep polling
        if self.settings.backfill_form.is_running {
            ctx.request_repaint();
        }

        // Dark backdrop
        let screen_rect = ctx.screen_rect();
        Area::new(Id::new("backfill_backdrop"))
            .fixed_pos(screen_rect.min)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen_rect, egui::Sense::click());
                ui.painter().rect_filled(
                    screen_rect,
                    0.0,
                    Color32::from_black_alpha(128),
                );
                // Click backdrop to close (only if not running)
                if response.clicked() && !self.settings.backfill_form.is_running {
                    self.settings.show_backfill_modal = false;
                }
            });

        // Modal content
        Area::new(Id::new("backfill_modal"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::window(ui.style())
                    .inner_margin(20.0)
                    .show(ui, |ui| {
                        ui.set_width(350.0);
                        ui.heading("Initial Sync");
                        ui.add_space(10.0);

                        let form = &self.settings.backfill_form;

                        if let Some(ref result) = form.result_message.clone() {
                            // Completed state
                            ui.label(RichText::new(result.clone()).color(Color32::GREEN));
                            ui.add_space(10.0);
                            if ui.button("Close").clicked() {
                                self.settings.show_backfill_modal = false;
                            }
                        } else if let Some(ref error) = form.error_message.clone() {
                            // Error state
                            ui.label(RichText::new(format!("Error: {}", error)).color(Color32::RED));
                            let pushed = form.entities_pushed;
                            if pushed > 0 {
                                ui.label(format!("({} entities synced before failure)", pushed));
                            }
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Retry").clicked() {
                                    self.start_backfill();
                                }
                                if ui.button("Close").clicked() {
                                    self.settings.show_backfill_modal = false;
                                }
                            });
                        } else if form.is_running {
                            // Running state
                            let pushed = form.entities_pushed;
                            let total = form.total_entities;
                            ui.label(format!("Syncing... {}/{}", pushed, total));
                            let progress = if total > 0 {
                                pushed as f32 / total as f32
                            } else {
                                0.0
                            };
                            ui.add(egui::ProgressBar::new(progress));
                        } else {
                            // Ready state
                            let total = form.total_entities;
                            let child_count = form.child_count;
                            let transaction_count = form.transaction_count;
                            let goal_count = form.goal_count;
                            ui.label(format!("Ready to sync {} entities to remote:", total));
                            ui.add_space(5.0);
                            ui.label(format!("  {} children", child_count));
                            ui.label(format!("  {} transactions", transaction_count));
                            ui.label(format!("  {} goals", goal_count));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Start Sync").clicked() {
                                    self.start_backfill();
                                }
                                if ui.button("Cancel").clicked() {
                                    self.settings.show_backfill_modal = false;
                                }
                            });
                        }
                    });
            });
    }
}
