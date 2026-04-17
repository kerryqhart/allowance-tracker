use egui::{Align2, Area, Color32, CornerRadius, Frame, Id, Order, Rect, RichText, UiBuilder, Vec2};
use crate::ui::app_state::AllowanceTrackerApp;

// TODO: Move to user-facing config once settings UI supports it
const DEFAULT_SYNC_SERVICE_URL: &str = "https://i99kq799kd.execute-api.us-east-2.amazonaws.com";

impl AllowanceTrackerApp {
    /// Open the backfill modal and pre-load entity counts for display
    pub fn open_backfill_modal(&mut self) {
        log::info!("Initial sync action - opening modal");
        self.settings.show_backfill_modal = true;
        self.settings.backfill_form.clear();
        self.settings.backfill_form.just_opened = true;

        // Pre-compute entity counts
        if let Ok(children_result) = self.backend().child_service.list_children() {
            let children = &children_result.children;
            self.settings.backfill_form.child_count = children.len();

            let mut tx_count = 0;
            let mut goal_count = 0;
            for child in children {
                match self.backend().transaction_service.list_all_transactions_for_child(&child.id) {
                    Ok(txs) => { tx_count += txs.len(); }
                    Err(e) => log::warn!("Failed to load transactions for child {}: {}", child.id, e),
                }
                match self.backend().goal_service.list_all_goals_for_child(&child.id) {
                    Ok(goals) => { goal_count += goals.len(); }
                    Err(e) => log::warn!("Failed to load goals for child {}: {}", child.id, e),
                }
            }
            self.settings.backfill_form.transaction_count = tx_count;
            self.settings.backfill_form.goal_count = goal_count;
            self.settings.backfill_form.total_entities =
                self.settings.backfill_form.child_count + tx_count + goal_count;
        }
    }

    /// Start a backfill operation on a background thread
    pub fn start_backfill(&mut self) {
        use std::sync::mpsc;
        use std::thread;

        // TODO: Move to user-facing config once settings UI supports it.
        // The `/internal` prefix hits the unauthenticated route group on the sync-service
        // API Gateway — matches how the MCP server accesses the same data.
        let remote_url = format!(
            "{}/internal",
            std::env::var("SYNC_SERVICE_URL")
                .unwrap_or_else(|_| DEFAULT_SYNC_SERVICE_URL.to_string())
        );

        self.settings.backfill_form.is_running = true;
        self.settings.backfill_form.result_message = None;
        self.settings.backfill_form.error_message = None;
        self.settings.backfill_form.entities_pushed = 0;

        let (progress_tx, progress_rx) = mpsc::channel();
        self.settings.backfill_form.progress_rx = Some(progress_rx);

        // Load all local data
        let children = match self.backend().child_service.list_children() {
            Ok(result) => result.children,
            Err(e) => {
                self.settings.backfill_form.is_running = false;
                self.settings.backfill_form.error_message = Some(format!("Failed to load children: {}", e));
                return;
            }
        };

        let mut transactions = std::collections::HashMap::new();
        let mut goals = std::collections::HashMap::new();

        for child in &children {
            match self.backend().transaction_service.list_all_transactions_for_child(&child.id) {
                Ok(txs) => { transactions.insert(child.id.clone(), txs); }
                Err(e) => log::warn!("Failed to load transactions for child {}: {}", child.id, e),
            }
            match self.backend().goal_service.list_all_goals_for_child(&child.id) {
                Ok(child_goals) => { goals.insert(child.id.clone(), child_goals); }
                Err(e) => log::warn!("Failed to load goals for child {}: {}", child.id, e),
            }
        }

        let remote: std::sync::Arc<dyn crate::backend::storage::remote::RemoteStorage> =
            std::sync::Arc::new(crate::backend::storage::http_remote::HttpRemoteClient::new(
                remote_url,
            ));

        thread::spawn(move || {
            let engine = crate::backend::domain::sync_manager::SyncEngine::new(remote);
            match engine.backfill(children, transactions, goals, progress_tx.clone()) {
                Ok(_) => {}
                Err(e) => {
                    let _ = progress_tx.send(
                        crate::backend::domain::sync_manager::BackfillProgress::Failed {
                            error: e.to_string(),
                            pushed_so_far: 0,
                        },
                    );
                }
            }
        });
    }

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

        let modal_size = Vec2::new(390.0, 260.0);

        Area::new(Id::new("backfill_modal_overlay"))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    CornerRadius::ZERO,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 128),
                );

                ui.allocate_new_ui(UiBuilder::new().max_rect(screen_rect), |ui| {
                    ui.centered_and_justified(|ui| {
                        Frame::window(ui.style())
                            .inner_margin(20.0)
                            .show(ui, |ui| {
                                ui.set_min_size(modal_size);
                                ui.set_max_size(modal_size);
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
                });

                // Backdrop click-to-close (only if not running) — check pointer
                // position vs modal rect. Skip on the frame the modal opened so
                // the menu-item click that triggered opening doesn't close it.
                if self.settings.backfill_form.just_opened {
                    self.settings.backfill_form.just_opened = false;
                } else if !self.settings.backfill_form.is_running
                    && ui.ctx().input(|i| i.pointer.any_click())
                {
                    if let Some(pos) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        let modal_rect =
                            Rect::from_center_size(ctx.screen_rect().center(), modal_size);
                        if !modal_rect.contains(pos) {
                            self.settings.show_backfill_modal = false;
                        }
                    }
                }
            });
    }
}
