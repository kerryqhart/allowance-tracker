//! # Sync with another Mac (Settings → Sync with another Mac…)
//!
//! The settings-panel half of Task 19: onboarding a second machine onto the
//! lgs (desktop-to-desktop) transport, and the first-run flow that lets a
//! machine configure lgs in the first place.
//!
//! Two states, shown in the same modal:
//! - **Not set up yet** (`sync_state.yaml` has no `cloud_root`): a button
//!   that picks a folder, checks git is present, runs `lgs init`, and
//!   adopts-or-installs the daemon. See [`AllowanceTrackerApp::start_lgs_first_run`].
//! - **Already set up**: the adoptable-projects checklist from
//!   `backend::sync::adoptable_children`, filtered to this app's own
//!   `allowance-*` projects. Archived projects are labelled, never hidden —
//!   see the module doc on `backend::sync::migration_lgs`.
//!
//! Every lgs call here is synchronous (a local subprocess talking to a
//! daemon on the same machine — see `LgsClient::run`'s bounded timeout), so
//! these run directly on click rather than through a background thread, the
//! same way `children_modal.rs`'s folder operations do.
//!
//! Deliberately does NOT try to hot-start the background sync thread after a
//! successful first run: `SyncThreadHandle` is constructed once, in
//! `AllowanceTrackerApp::new`, before this modal can even be opened. Asking
//! for a restart is a smaller, safer surface than reconstructing a running
//! thread's handle out from under whatever it may already be doing.

use eframe::egui;

use crate::backend::domain::sync_persistence::{sync_state_path, SyncState};
use crate::backend::sync::{
    adopt_child, adoptable_children, ensure_daemon, ensure_lgs_binary, run_first_run, AdoptableChild,
    ChildSyncEngine, DaemonOutcome, LgsClient, StageResult, SyncPaths,
};
use crate::backend::Backend;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::settings::shared::SettingsModalStyle;

/// State for Settings → Sync with another Mac.
#[derive(Debug, Default)]
pub struct LgsSyncFormState {
    pub success_message: Option<String>,
    pub error_message: Option<String>,
    /// The last fetched adoptable-projects list. Refetched on open and on
    /// "Refresh", never on every frame — each fetch is a real subprocess call.
    pub adoptable: Vec<AdoptableChild>,
    pub adoptable_loaded: bool,
    /// True on the frame the modal opens, so the settings-menu click that
    /// opened it isn't also read as a backdrop click that closes it, and so
    /// the adoptable list is fetched exactly once per open.
    pub just_opened: bool,
    /// The most recent "Check sync" run's per-stage results, in order.
    /// Empty until the button is clicked; cleared whenever the modal is
    /// re-opened or another action's message is set, so a stale result
    /// from a previous child/run is never shown alongside a new message.
    pub check_sync_results: Vec<StageResult>,
}

impl LgsSyncFormState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn clear_messages(&mut self) {
        self.success_message = None;
        self.error_message = None;
        self.check_sync_results.clear();
    }

    pub fn set_success(&mut self, message: String) {
        self.success_message = Some(message);
        self.error_message = None;
    }

    pub fn set_error(&mut self, message: String) {
        self.error_message = Some(message);
        self.success_message = None;
    }
}

impl AllowanceTrackerApp {
    /// Render Settings → Sync with another Mac.
    pub fn render_lgs_sync_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_lgs_sync_modal {
            return;
        }

        let data_dir = match Backend::default_data_dir() {
            Ok(d) => d,
            Err(e) => {
                self.settings.lgs_sync_form.set_error(format!("Could not resolve the data directory: {e}"));
                self.settings.show_lgs_sync_modal = false;
                return;
            }
        };
        let cloud_root_configured = SyncState::load(&sync_state_path(&data_dir))
            .ok()
            .and_then(|s| s.cloud_root)
            .is_some();

        if self.settings.lgs_sync_form.just_opened && cloud_root_configured {
            self.refresh_lgs_status();
        }

        let modal_size = egui::vec2(560.0, 460.0);
        let mut close_requested = false;

        egui::Area::new(egui::Id::new("lgs_sync_modal_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen_rect,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128),
                );

                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(screen_rect), |ui| {
                    ui.centered_and_justified(|ui| {
                        let style = SettingsModalStyle::default_style();
                        style.apply_frame_styling().show(ui, |ui| {
                            ui.set_min_size(modal_size);
                            ui.set_max_size(modal_size);

                            ui.vertical(|ui| {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(5.0);
                                    ui.label(
                                        egui::RichText::new("🔄 Sync with another Mac")
                                            .font(egui::FontId::new(
                                                style.title_font_size,
                                                egui::FontFamily::Proportional,
                                            ))
                                            .strong()
                                            .color(style.title_color),
                                    );
                                });
                                ui.add_space(12.0);

                                if cloud_root_configured {
                                    self.render_adopt_checklist(ui);
                                } else {
                                    render_first_run_pitch(ui);
                                }

                                ui.add_space(10.0);
                                if let Some(msg) = &self.settings.lgs_sync_form.success_message {
                                    ui.label(egui::RichText::new(msg).color(egui::Color32::from_rgb(0, 140, 0)));
                                }
                                if let Some(msg) = &self.settings.lgs_sync_form.error_message {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(msg)
                                                .color(egui::Color32::from_rgb(190, 60, 60)),
                                        )
                                        .wrap(),
                                    );
                                }
                                render_check_sync_results(ui, &self.settings.lgs_sync_form.check_sync_results);

                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if !cloud_root_configured && ui.button("Set up sync with another Mac…").clicked() {
                                        self.start_lgs_first_run();
                                    }
                                    if cloud_root_configured && ui.button("Refresh").clicked() {
                                        self.refresh_lgs_status();
                                    }
                                    if cloud_root_configured {
                                        let can_check = self.core.current_child.is_some();
                                        if ui
                                            .add_enabled(can_check, egui::Button::new("Check sync"))
                                            .on_disabled_hover_text("Select a child first.")
                                            .clicked()
                                        {
                                            self.check_lgs_sync();
                                        }
                                    }
                                    if ui.button("Close").clicked() {
                                        close_requested = true;
                                    }
                                });
                            });
                        });
                    });
                });

                if self.settings.lgs_sync_form.just_opened {
                    self.settings.lgs_sync_form.just_opened = false;
                } else if ui.ctx().input(|i| i.pointer.any_click()) {
                    if let Some(pos) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        let modal_rect = egui::Rect::from_center_size(ctx.screen_rect().center(), modal_size);
                        if !modal_rect.contains(pos) {
                            close_requested = true;
                        }
                    }
                }
            });

        if close_requested {
            self.settings.show_lgs_sync_modal = false;
            self.settings.lgs_sync_form.clear();
        }
    }

    fn render_adopt_checklist(&mut self, ui: &mut egui::Ui) {
        if !self.settings.lgs_sync_form.adoptable_loaded {
            ui.label("Loading…");
            return;
        }
        if self.settings.lgs_sync_form.adoptable.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No children from another Mac are waiting to be adopted right now.",
                )
                .color(egui::Color32::from_rgb(120, 120, 120)),
            );
            return;
        }

        let rows = self.settings.lgs_sync_form.adoptable.clone();
        let mut to_adopt: Option<String> = None;

        egui::ScrollArea::vertical()
            .max_height(260.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for row in &rows {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&row.child_id).strong());
                        // Review Important-2: an unreadable archive record
                        // must be labelled the same cautious way as a
                        // confirmed-archived one — `archived` alone
                        // defaults to `false` when lgs couldn't read the
                        // record, which must never present as "safe,
                        // ordinary project." See
                        // `AdoptableChild::should_be_labelled_archived`.
                        if row.archived {
                            ui.label(
                                egui::RichText::new("archived — read-only")
                                    .color(egui::Color32::from_rgb(190, 130, 30)),
                            );
                        } else if row.archive_status_unknown.is_some() {
                            ui.label(
                                egui::RichText::new("archived status unknown")
                                    .color(egui::Color32::from_rgb(190, 130, 30)),
                            );
                        }
                        if ui.small_button("Adopt").clicked() {
                            to_adopt = Some(row.project_name.clone());
                        }
                    });
                    if let Some(note) = &row.note {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(note)
                                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                    .color(egui::Color32::from_rgb(120, 120, 120)),
                            )
                            .wrap(),
                        );
                    }
                    if let Some(reason) = &row.archive_status_unknown {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("Could not confirm archive status: {reason}"))
                                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                    .color(egui::Color32::from_rgb(190, 130, 30)),
                            )
                            .wrap(),
                        );
                    }
                    ui.add_space(4.0);
                    ui.separator();
                }
            });

        if let Some(project_name) = to_adopt {
            self.adopt_lgs_child(&project_name);
        }
    }

    /// Folder picker → git check → `lgs init` → daemon adopt-or-install →
    /// persist `cloud_root` (canonicalized — see `SyncPaths::for_production`)
    /// and any resulting `DaemonOwnership` change.
    fn start_lgs_first_run(&mut self) {
        let Some(picked) = rfd::FileDialog::new()
            .set_title("Choose (or create) a folder for lgs's cloud data")
            .pick_folder()
        else {
            return;
        };

        self.settings.lgs_sync_form.clear_messages();

        let data_dir = match Backend::default_data_dir() {
            Ok(d) => d,
            Err(e) => {
                self.settings.lgs_sync_form.set_error(e.to_string());
                return;
            }
        };
        let state_path = sync_state_path(&data_dir);

        // Review round 2: read ONLY to decide `ensure_daemon`'s action from
        // the current ownership below — never saved back verbatim. The
        // calls between here and the save site at the end of this function
        // (`ensure_lgs_binary`, `run_first_run`, `ensure_daemon`) can block
        // for up to ~50s combined. The background sync thread's own
        // periodic save (`persist_watermarks` in `sync_thread.rs`) can
        // advance `watermarks` on disk during that whole window. Saving a
        // `SyncState` read from BEFORE that window — the bug this comment
        // replaces — would silently erase whatever the loop wrote: the
        // exact stale-snapshot clobber `persist_watermarks` exists to
        // prevent, pointing in the opposite direction. The fix is the same
        // shape: re-read immediately before writing, at the save site below.
        let starting_ownership = SyncState::load(&state_path).unwrap_or_default().daemon_ownership;

        let paths = match SyncPaths::for_production(data_dir.clone(), Some(picked)) {
            Ok(p) => p,
            Err(e) => {
                self.settings.lgs_sync_form.set_error(format!("That folder can't be used: {e}"));
                return;
            }
        };

        if let Err(e) = ensure_lgs_binary(&paths) {
            self.settings
                .lgs_sync_form
                .set_error(format!("Could not install the lgs binary: {e}"));
            return;
        }
        let lgs = LgsClient::new(paths.lgs_binary.clone());

        let Some(cloud_root) = paths.cloud_root.clone() else {
            self.settings.lgs_sync_form.set_error("No cloud root was resolved.".to_string());
            return;
        };

        if let Err(e) = run_first_run(&lgs, &cloud_root, crate::backend::sync::git_is_available()) {
            self.settings.lgs_sync_form.set_error(e.to_string());
            return;
        }

        let mut updated_ownership = starting_ownership.clone();
        match ensure_daemon(&lgs, &starting_ownership) {
            Ok(DaemonOutcome::InstalledAndOwned) => {
                updated_ownership.installed_by_app = true;
            }
            Ok(DaemonOutcome::Skewed(message)) => {
                self.settings
                    .lgs_sync_form
                    .set_error(format!("lgs is set up, but its daemon needs attention: {message}"));
            }
            Ok(DaemonOutcome::Healthy) | Ok(DaemonOutcome::Restarted) => {}
            Err(e) => {
                self.settings
                    .lgs_sync_form
                    .set_error(format!("lgs is set up, but the daemon could not be reached: {e}"));
            }
        }

        // Re-read immediately before writing — see the comment at the
        // first `load` above. Only `cloud_root` and `daemon_ownership` are
        // ours to set here; everything else (in particular `watermarks`)
        // comes from whatever the background thread most recently wrote,
        // not from a snapshot taken before the blocking calls above.
        let mut sync_state = SyncState::load(&state_path).unwrap_or_default();
        sync_state.cloud_root = Some(cloud_root);
        sync_state.daemon_ownership = updated_ownership;
        if let Err(e) = sync_state.save(&state_path) {
            self.settings.lgs_sync_form.set_error(format!("Could not save sync settings: {e}"));
            return;
        }

        if self.settings.lgs_sync_form.error_message.is_none() {
            self.settings.lgs_sync_form.set_success(
                "Sync with another Mac is set up. Restart the app to finish enabling it.".to_string(),
            );
        }
    }

    /// Exercise the whole lgs loop for the currently selected child, on
    /// demand, and show each stage's pass/fail by name — Task 20's "Check
    /// sync": health *reporting* (`refresh_lgs_status`) proves nothing about
    /// whether a child's own commits can actually get out and back, and the
    /// no-terminal goal means the user needs a button, not a command. Never
    /// touches the child's branch — see `check_sync`'s module doc — so no
    /// `GitManager` is needed here either.
    fn check_lgs_sync(&mut self) {
        self.settings.lgs_sync_form.clear_messages();

        let Some(child) = self.core.current_child.clone() else {
            self.settings.lgs_sync_form.set_error("Select a child first.".to_string());
            return;
        };
        let Some((lgs, _paths)) = self.lgs_client_for_settings() else { return };

        let engine = ChildSyncEngine::new(lgs, self.core.backend.csv_connection.clone());
        let child_id = shared::ChildId::from(child.id.clone());
        let results = engine.check_sync(&child_id);

        let all_ok = !results.is_empty() && results.iter().all(|r| r.ok);
        self.settings.lgs_sync_form.check_sync_results = results;
        if all_ok {
            self.settings.lgs_sync_form.set_success(format!("Sync check passed for {}.", child.name));
        } else {
            self.settings
                .lgs_sync_form
                .set_error(format!("Sync check failed for {} — see details below.", child.name));
        }
    }

    fn refresh_lgs_status(&mut self) {
        self.settings.lgs_sync_form.clear_messages();
        let Some((lgs, _paths)) = self.lgs_client_for_settings() else { return };

        match lgs.status() {
            Ok(status) => {
                if !status.durability_data_is_fresh() {
                    self.settings.lgs_sync_form.set_error(format!(
                        "lgs daemon is reporting {:?}, not Ok — durability info below may be stale.",
                        status.daemon.state
                    ));
                }
                self.settings.lgs_sync_form.adoptable = adoptable_children(&status);
                self.settings.lgs_sync_form.adoptable_loaded = true;
            }
            Err(e) => self.settings.lgs_sync_form.set_error(format!("Could not reach the lgs daemon: {e}")),
        }
    }

    fn adopt_lgs_child(&mut self, project_name: &str) {
        self.settings.lgs_sync_form.clear_messages();
        let Some((lgs, paths)) = self.lgs_client_for_settings() else { return };

        match adopt_child(&lgs, project_name, &paths) {
            Ok(()) => {
                self.settings.lgs_sync_form.set_success(format!(
                    "Adopted {project_name}. It will start syncing on the next cycle."
                ));
                self.rebuild_roster();
                self.refresh_lgs_status();
            }
            Err(e) => self.settings.lgs_sync_form.set_error(e.to_string()),
        }
    }

    /// Resolve a live `LgsClient` + `SyncPaths` for the settings panel, or
    /// set an error message and return `None` when lgs has not been set up
    /// yet (should not happen given the callers all gate on
    /// `cloud_root_configured`, but a stale read is possible if the file
    /// changed between the modal opening and the button click).
    fn lgs_client_for_settings(&mut self) -> Option<(LgsClient, SyncPaths)> {
        let data_dir = match Backend::default_data_dir() {
            Ok(d) => d,
            Err(e) => {
                self.settings.lgs_sync_form.set_error(e.to_string());
                return None;
            }
        };
        let sync_state = SyncState::load(&sync_state_path(&data_dir)).unwrap_or_default();
        let Some(cloud_root) = sync_state.cloud_root else {
            self.settings
                .lgs_sync_form
                .set_error("Sync with another Mac has not been set up on this machine yet.".to_string());
            return None;
        };
        let paths = match SyncPaths::for_production(data_dir, Some(cloud_root)) {
            Ok(p) => p,
            Err(e) => {
                self.settings.lgs_sync_form.set_error(e.to_string());
                return None;
            }
        };
        let lgs = LgsClient::new(paths.lgs_binary.clone());
        Some((lgs, paths))
    }
}

/// Render "Check sync"'s per-stage results, each named and marked pass/fail
/// — the whole point of Task 20 is that a failure says which stage broke,
/// in plain language, rather than "something went wrong." A no-op when
/// `results` is empty (nothing has been checked yet this session).
fn render_check_sync_results(ui: &mut egui::Ui, results: &[StageResult]) {
    if results.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .max_height(140.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for stage in results {
                let (icon, color) = if stage.ok {
                    ("✅", egui::Color32::from_rgb(0, 140, 0))
                } else {
                    ("❌", egui::Color32::from_rgb(190, 60, 60))
                };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).color(color));
                    ui.label(egui::RichText::new(format!("{:?}", stage.stage)).strong());
                });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&stage.detail)
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(egui::Color32::from_rgb(120, 120, 120)),
                    )
                    .wrap(),
                );
                ui.add_space(4.0);
            }
        });
}

fn render_first_run_pitch(ui: &mut egui::Ui) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "Set up sync so this Mac and another Mac can share the same children over lgs \
                 (a cloud-drive-backed git remote — no AWS account needed). You'll be asked to \
                 choose a folder for lgs's own cloud data; the folder you pick should already be \
                 covered by whatever cloud drive you use (iCloud Drive, Dropbox, etc.).",
            )
            .color(egui::Color32::from_rgb(100, 100, 100)),
        )
        .wrap(),
    );
}
