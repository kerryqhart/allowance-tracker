//! # Children Modal (Settings → Children)
//!
//! The machine-local child registry, made editable.
//!
//! This replaces the old "Data directory" modal, which could only ever move an
//! *already-active* child's folder — useless on a second machine, where no
//! child is registered at all and the folder to adopt is sitting in iCloud
//! Drive. The four operations here are the whole lifecycle of a registry entry:
//!
//! - **Add existing child…** — point at a folder that already holds a child.
//!   Identity comes from its `child.yaml`, never the folder name.
//! - **Create new child…** — the existing create-child flow, which mints a
//!   folder under the base directory and registers it.
//! - **Move data…** — relocate a folder and repoint the entry. Refuses a
//!   non-empty target; adopting a populated folder is Add-existing, and that is
//!   non-destructive.
//! - **Remove from this machine** — deregister only. The data is never touched,
//!   and the confirmation names the path being left behind.
//!
//! Every registry mutation here goes through the *shared* `Arc<CsvConnection>`
//! on `Backend`. Two `CsvConnection`s over one directory hold independent
//! registry snapshots, so constructing one locally would make a mutation
//! invisible to the rest of the app.

use anyhow::{Context, Result};
use eframe::egui;
use shared::ChildId;
use std::path::{Path, PathBuf};

use crate::backend::domain::{ChildStatus, RealFolderSource, UnavailableReason};
use crate::backend::storage::csv::{tree_checksum, ChildRegistry, CsvConnection, RegistryEntry};
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::settings::shared::SettingsModalStyle;
use crate::ui::state::roster::spawn_loader;

/// Validate a folder the user picked and produce the entry to register.
/// Identity comes from `child.yaml`; the folder's own name is irrelevant.
pub fn validate_child_folder(path: &Path) -> Result<RegistryEntry> {
    let yaml_path = path.join("child.yaml");
    if !yaml_path.exists() {
        anyhow::bail!("{} does not contain a child.yaml", path.display());
    }
    #[derive(serde::Deserialize)]
    struct Y {
        id: String,
        name: String,
    }
    let parsed: Y = serde_yaml::from_str(&std::fs::read_to_string(&yaml_path)?)
        .with_context(|| format!("parsing {}", yaml_path.display()))?;

    Ok(RegistryEntry {
        id: ChildId::new(parsed.id),
        path: path.to_path_buf(),
        label: parsed.name,
    })
}

/// Move a child's folder, then repoint the registry.
///
/// Refuses a non-empty target — adopting a folder that already holds data is
/// Add-existing, which is non-destructive. Refuses a target *inside* the
/// source, which `copy_dir_recursive` would otherwise descend into forever.
/// Verifies the copy by checksum before deleting the source, so a partial copy
/// never costs data.
pub fn move_child_data(conn: &CsvConnection, id: &ChildId, target: &Path) -> Result<()> {
    let source = conn.child_dir(id)?;

    // A target nested inside the source (or equal to it) makes
    // `copy_dir_recursive` copy into the tree it is walking. It never
    // terminates, and it does so while the source is the only copy of the
    // data. Cheap to refuse, catastrophic to attempt.
    if target.starts_with(&source) {
        anyhow::bail!(
            "{} is inside {} — a child's folder cannot be moved into itself",
            target.display(),
            source.display()
        );
    }

    let target_existed = target.exists();
    if target_existed {
        let occupied = std::fs::read_dir(target)?.next().is_some();
        if occupied {
            anyhow::bail!(
                "{} is not empty — use Add existing child… to adopt a folder that already holds data",
                target.display()
            );
        }
    }

    copy_dir_recursive(&source, target)?;

    if tree_checksum(&source)? != tree_checksum(target)? {
        if let Err(cleanup) = undo_copy(target, target_existed) {
            anyhow::bail!(
                "copy verification failed and the partial copy under {} could not be cleaned \
                 up ({cleanup}); nothing was moved — {} is untouched",
                target.display(),
                source.display()
            );
        }
        anyhow::bail!("copy verification failed; nothing was moved");
    }

    conn.update_registry(|reg| reg.repoint(id, target.to_path_buf()))?;
    std::fs::remove_dir_all(&source)?;
    Ok(())
}

/// Roll back a failed copy, removing only what the copy itself created.
///
/// The target directory may be one the *user* made — this flow only requires
/// it to be empty, not absent — so `remove_dir_all(target)` would delete a
/// directory we were merely lent. When the target pre-existed we clear its
/// contents (all of which we just wrote, since it was empty) and leave the
/// directory itself standing; only a target we created ourselves is removed
/// outright.
fn undo_copy(target: &Path, target_existed: bool) -> Result<()> {
    if !target_existed {
        return Ok(std::fs::remove_dir_all(target)?);
    }
    for entry in std::fs::read_dir(target)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// The user-facing status chip for one roster entry, and its colour.
///
/// Deliberately more specific than [`crate::ui::state::roster::status_label`],
/// which is written for the child picker: this screen is where a user *fixes*
/// a broken entry, so it names what is actually wrong — including the id found
/// in a folder that belongs to somebody else.
fn status_chip(status: &ChildStatus) -> (String, egui::Color32) {
    const READY: egui::Color32 = egui::Color32::from_rgb(40, 140, 70);
    const WAITING: egui::Color32 = egui::Color32::from_rgb(190, 130, 30);
    const BROKEN: egui::Color32 = egui::Color32::from_rgb(190, 60, 60);

    match status {
        ChildStatus::Available(_) => ("Ready".to_string(), READY),
        ChildStatus::Downloading => ("Downloading from iCloud…".to_string(), WAITING),
        ChildStatus::Unavailable(reason) => {
            let text = match reason {
                UnavailableReason::PathMissing => "Folder not found".to_string(),
                UnavailableReason::NotAChildFolder => "Not a child folder".to_string(),
                UnavailableReason::IdMismatch { found } => format!("ID mismatch: found `{found}`"),
                UnavailableReason::ReadFailed(e) => format!("Folder could not be read: {e}"),
                UnavailableReason::ParseFailed(e) => format!("child.yaml is unreadable: {e}"),
            };
            (text, BROKEN)
        }
    }
}

/// One roster entry, flattened for rendering.
///
/// Snapshotted before the `Area` closure so the closure can hold `&mut self`
/// without also borrowing `self.roster`.
struct Row {
    id: ChildId,
    name: String,
    path: PathBuf,
    status_text: String,
    status_color: egui::Color32,
    available: bool,
}

/// Which per-row buttons a row offers.
#[derive(Debug, PartialEq, Eq)]
struct RowActions {
    /// Re-walk this one entry. Only means anything for a row that failed.
    retry: bool,
    /// Point the entry at a different folder. Only for a row that failed.
    locate: bool,
    /// Deregister. Offered for **every** row, whatever its status.
    remove: bool,
}

/// Decide a row's actions from its status.
///
/// Retry and Locate… are repairs, so they appear only where there is something
/// to repair. **Remove from this machine is unconditional.** It was previously
/// gated on `!available`, which meant a perfectly healthy child could never be
/// deregistered through any UI — `delete_child` is a different operation (it
/// deletes data) and had no caller at all. That left "stop tracking this child
/// on this laptop, leave the iCloud folder alone" unreachable, which is the
/// normal way to unwind a machine.
fn row_actions(available: bool) -> RowActions {
    RowActions { retry: !available, locate: !available, remove: true }
}

/// What a click asked for. Collected during rendering and executed afterwards,
/// so no handler runs while the `Area` closure holds `&mut self`.
enum Action {
    AddExisting,
    CreateNew,
    MoveData(ChildId),
    Retry(ChildId),
    Locate(ChildId),
    AskRemove(ChildId),
    ConfirmRemove,
    CancelRemove,
    Close,
}

impl AllowanceTrackerApp {
    /// Render Settings → Children.
    pub fn render_children_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_children_modal {
            return;
        }

        let rows: Vec<Row> = self
            .roster
            .entries()
            .iter()
            .map(|e| {
                let (status_text, status_color) = status_chip(&e.status);
                Row {
                    id: e.entry.id.clone(),
                    name: e.display_name().to_string(),
                    path: e.entry.path.clone(),
                    status_text,
                    status_color,
                    available: e.is_available(),
                }
            })
            .collect();

        let modal_size = egui::vec2(620.0, 480.0);
        let mut action: Option<Action> = None;

        egui::Area::new(egui::Id::new("children_modal_overlay"))
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
                                        egui::RichText::new("👶 Children")
                                            .font(egui::FontId::new(
                                                style.title_font_size,
                                                egui::FontFamily::Proportional,
                                            ))
                                            .strong()
                                            .color(style.title_color),
                                    );
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "Children registered on this machine. \
                                             Their data stays wherever it lives.",
                                        )
                                        .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                        .color(egui::Color32::from_rgb(100, 100, 100)),
                                    );
                                });

                                ui.add_space(12.0);

                                if let Some(pending) = self.settings.children_form.pending_removal.clone() {
                                    render_removal_confirmation(ui, &pending, &mut action);
                                } else {
                                    self.render_children_rows(ui, &rows, &mut action);
                                    ui.add_space(8.0);
                                    self.render_children_footer(ui, &rows, &mut action);
                                }
                            });
                        });
                    });
                });

                // Backdrop click-to-close, skipped on the frame the modal opened
                // so the settings-menu click that opened it doesn't close it.
                if self.settings.children_form.just_opened {
                    self.settings.children_form.just_opened = false;
                } else if ui.ctx().input(|i| i.pointer.any_click()) {
                    if let Some(pos) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        let modal_rect =
                            egui::Rect::from_center_size(ctx.screen_rect().center(), modal_size);
                        if !modal_rect.contains(pos) {
                            action = Some(Action::Close);
                        }
                    }
                }
            });

        if let Some(action) = action {
            self.handle_children_action(action);
        }
    }

    /// The scrolling list: one row per registry entry.
    fn render_children_rows(&mut self, ui: &mut egui::Ui, rows: &[Row], action: &mut Option<Action>) {
        if rows.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No children are registered on this machine yet. \
                     Use “Add existing child…” to adopt a folder that already holds a child, \
                     or “Create new child…” to start a fresh one.",
                )
                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                .color(egui::Color32::from_rgb(120, 120, 120)),
            );
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(270.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for row in rows {
                    let selected = self.settings.children_form.selected.as_ref() == Some(&row.id);

                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                selected,
                                egui::RichText::new(&row.name)
                                    .font(egui::FontId::new(16.0, egui::FontFamily::Proportional))
                                    .strong(),
                            )
                            .clicked()
                        {
                            self.settings.children_form.selected = Some(row.id.clone());
                        }
                        ui.label(
                            egui::RichText::new(&row.status_text)
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(row.status_color),
                        );
                    });

                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(row.path.display().to_string())
                                .font(egui::FontId::new(11.0, egui::FontFamily::Monospace))
                                .color(egui::Color32::from_rgb(140, 140, 140)),
                        )
                        .wrap(),
                    );

                    // Repairs where there is something to repair; deregister
                    // always. See `row_actions`.
                    let actions = row_actions(row.available);
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        if actions.retry && ui.small_button("Retry").clicked() {
                            *action = Some(Action::Retry(row.id.clone()));
                        }
                        if actions.locate && ui.small_button("Locate…").clicked() {
                            *action = Some(Action::Locate(row.id.clone()));
                        }
                        if actions.remove
                            && ui.small_button("Remove from this machine").clicked()
                        {
                            *action = Some(Action::AskRemove(row.id.clone()));
                        }
                    });

                    ui.add_space(6.0);
                    ui.separator();
                }
            });
    }

    /// Footer: the three additive/relocating operations, plus Close.
    fn render_children_footer(&mut self, ui: &mut egui::Ui, rows: &[Row], action: &mut Option<Action>) {
        // "Move data…" acts on the selected child, and only a materialized
        // folder can be copied and verified — a half-downloaded one would
        // checksum-mismatch at best and lose data at worst.
        let movable = self
            .settings
            .children_form
            .selected
            .as_ref()
            .and_then(|id| rows.iter().find(|r| &r.id == id))
            .filter(|r| r.available)
            .map(|r| r.id.clone());

        ui.horizontal_wrapped(|ui| {
            if ui.button("Add existing child…").clicked() {
                *action = Some(Action::AddExisting);
            }
            if ui.button("Create new child…").clicked() {
                *action = Some(Action::CreateNew);
            }
            let move_button = ui.add_enabled(movable.is_some(), egui::Button::new("Move data…"));
            if move_button.clicked() {
                if let Some(id) = movable.clone() {
                    *action = Some(Action::MoveData(id));
                }
            }
            if movable.is_none() {
                move_button.on_hover_text("Select a child that is Ready to move its data.");
            }
            if ui.button("Close").clicked() {
                *action = Some(Action::Close);
            }
        });

        if let Some(msg) = &self.settings.children_form.success_message {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(msg).color(egui::Color32::from_rgb(0, 140, 0)));
        }
        if let Some(msg) = &self.settings.children_form.error_message {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(msg).color(egui::Color32::from_rgb(190, 60, 60)),
                )
                .wrap(),
            );
        }
    }

    fn handle_children_action(&mut self, action: Action) {
        match action {
            Action::AddExisting => self.add_existing_child(),
            Action::CreateNew => {
                self.settings.show_children_modal = false;
                self.settings.children_form.clear();
                self.settings.create_child_form.clear();
                self.settings.show_create_child_modal = true;
            }
            Action::MoveData(id) => self.move_child_folder(&id),
            Action::Retry(id) => self.retry_child(&id),
            Action::Locate(id) => self.locate_child_folder(&id),
            Action::AskRemove(id) => {
                // The confirmation's entire job is naming the folder being
                // left behind, so there is no confirmation to show without a
                // path. `unwrap_or_default()` here produced a dialogue that
                // said data would be left at "" — worse than no dialogue.
                // An id with no registry entry means the row is stale (the
                // registry changed under a roster the modal snapshotted), so
                // say so and resync rather than confirming a phantom.
                let Some(path) = self
                    .backend()
                    .csv_connection
                    .registry()
                    .path_for(&id)
                    .map(Path::to_path_buf)
                else {
                    self.settings.children_form.set_error(format!(
                        "'{id}' is no longer registered on this machine — the list has been \
                         refreshed."
                    ));
                    self.rebuild_roster();
                    return;
                };
                let label = self
                    .roster
                    .entries()
                    .iter()
                    .find(|e| e.entry.id == id)
                    .map(|e| e.display_name().to_string())
                    .unwrap_or_else(|| id.to_string());
                self.settings.children_form.clear_messages();
                self.settings.children_form.pending_removal =
                    Some(PendingRemoval { id, label, path });
            }
            Action::ConfirmRemove => self.remove_child_from_machine(),
            Action::CancelRemove => self.settings.children_form.pending_removal = None,
            Action::Close => {
                self.settings.show_children_modal = false;
                self.settings.children_form.clear();
            }
        }
    }

    /// Adopt a folder that already holds a child.
    ///
    /// The fresh-machine case this whole feature exists for: the folder is in
    /// iCloud Drive, nothing is registered yet, and the old modal could only
    /// repoint a child that was already active.
    fn add_existing_child(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Select a child's folder")
            .pick_folder()
        else {
            return;
        };

        self.settings.children_form.clear_messages();

        let entry = match validate_child_folder(&path) {
            Ok(entry) => entry,
            Err(e) => {
                self.settings.children_form.set_error(format!(
                    "That folder can't be added: {e}"
                ));
                return;
            }
        };

        let label = entry.label.clone();
        // `register` refuses a duplicate id and a duplicate path, naming the
        // incumbent in both cases — exactly the two collisions this flow can hit.
        let conn = self.backend().csv_connection.clone();
        match conn.update_registry(|reg| reg.register(entry)) {
            Ok(()) => {
                self.settings
                    .children_form
                    .set_success(format!("Added {label}."));
                self.rebuild_roster();
            }
            Err(e) => self.settings.children_form.set_error(e.to_string()),
        }
    }

    /// Re-walk a single entry, without disturbing the rest of the roster.
    ///
    /// Reuses the *current* generation so the results are accepted by the
    /// roster the UI is already showing. A full `rebuild_roster` would also
    /// work but would reset every other row to `Downloading`.
    fn retry_child(&mut self, id: &ChildId) {
        let registry = self.backend().csv_connection.registry();
        let Some(entry) = registry.entries().iter().find(|e| &e.id == id).cloned() else {
            return;
        };
        let mut one = ChildRegistry::default();
        if one.register(entry).is_err() {
            return;
        }
        self.settings.children_form.clear_messages();
        spawn_loader(
            std::sync::Arc::new(one),
            std::sync::Arc::new(RealFolderSource),
            self.roster_generation,
            self.roster_tx.clone(),
            self.roster_wake.clone(),
        );
    }

    /// Repoint an entry at the folder the user picks, keeping its id.
    ///
    /// The picked folder is validated the same way Add-existing validates, and
    /// its `child.yaml` must carry *this* child's id: a repoint that silently
    /// accepted another child's folder would make two entries fight over one
    /// directory.
    fn locate_child_folder(&mut self, id: &ChildId) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Locate this child's folder")
            .pick_folder()
        else {
            return;
        };

        self.settings.children_form.clear_messages();

        let entry = match validate_child_folder(&path) {
            Ok(entry) => entry,
            Err(e) => {
                self.settings
                    .children_form
                    .set_error(format!("That folder can't be used: {e}"));
                return;
            }
        };

        if &entry.id != id {
            self.settings.children_form.set_error(format!(
                "{} holds child '{}', not '{}'.",
                path.display(),
                entry.id,
                id
            ));
            return;
        }

        let conn = self.backend().csv_connection.clone();
        match conn.update_registry(|reg| reg.repoint(id, path.clone())) {
            Ok(()) => {
                self.settings
                    .children_form
                    .set_success(format!("{id} now points at {}.", path.display()));
                self.rebuild_roster();
            }
            Err(e) => self.settings.children_form.set_error(e.to_string()),
        }
    }

    /// Copy a child's folder to a new location, verify it, then repoint.
    fn move_child_folder(&mut self, id: &ChildId) {
        let Some(target) = rfd::FileDialog::new()
            .set_title("Choose an empty folder to move this child's data into")
            .pick_folder()
        else {
            return;
        };

        self.settings.children_form.clear_messages();

        let conn = self.backend().csv_connection.clone();
        match move_child_data(&conn, id, &target) {
            Ok(()) => {
                self.settings
                    .children_form
                    .set_success(format!("Moved to {}.", target.display()));
                self.rebuild_roster();
            }
            Err(e) => self.settings.children_form.set_error(e.to_string()),
        }
    }

    /// Deregister, and only deregister. The folder is left exactly as it is.
    fn remove_child_from_machine(&mut self) {
        let Some(pending) = self.settings.children_form.pending_removal.take() else {
            return;
        };

        let conn = self.backend().csv_connection.clone();
        match conn.update_registry(|reg| reg.deregister(&pending.id)) {
            Ok(()) => {
                // A dangling "active child" pointing at an id this machine no
                // longer knows would leave the app waiting for a child that can
                // never resolve.
                if self.active_child_id().as_ref() == Some(&pending.id) {
                    self.clear_active_child();
                }
                self.settings.children_form.set_success(format!(
                    "{} is no longer registered here. Their data is untouched at {}.",
                    pending.label,
                    pending.path.display()
                ));
                self.rebuild_roster();
            }
            Err(e) => self.settings.children_form.set_error(e.to_string()),
        }
    }

    /// Forget the active child, in `global_config.yaml` and in memory.
    fn clear_active_child(&mut self) {
        use crate::backend::storage::csv::{GlobalConfigRepository, GlobalConfigStorage};
        let repo = GlobalConfigRepository::new((*self.backend().csv_connection).clone());
        if let Err(e) = repo.set_active_child_directory(None) {
            log::warn!("Could not clear the active child after deregistering it: {e}");
        }
        self.core.current_child = None;
    }
}

/// The confirmation for **Remove from this machine**.
///
/// Names the path being left behind, because the whole point of this operation
/// is that it is not a delete.
fn render_removal_confirmation(
    ui: &mut egui::Ui,
    pending: &PendingRemoval,
    action: &mut Option<Action>,
) {
    ui.label(
        egui::RichText::new(format!("Remove {} from this machine?", pending.label))
            .font(egui::FontId::new(17.0, egui::FontFamily::Proportional))
            .strong(),
    );
    ui.add_space(10.0);
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "This only removes the entry from this machine's child registry. \
                 Nothing is deleted — the folder and everything in it is left exactly where it is:",
            )
            .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
            .color(egui::Color32::from_rgb(100, 100, 100)),
        )
        .wrap(),
    );
    ui.add_space(6.0);
    ui.add(
        egui::Label::new(
            egui::RichText::new(pending.path.display().to_string())
                .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                .color(egui::Color32::from_rgb(70, 130, 180)),
        )
        .wrap(),
    );
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            *action = Some(Action::CancelRemove);
        }
        ui.add_space(8.0);
        if ui.button("Remove from this machine").clicked() {
            *action = Some(Action::ConfirmRemove);
        }
    });
}

/// A deregistration awaiting confirmation.
#[derive(Debug, Clone)]
pub struct PendingRemoval {
    pub id: ChildId,
    pub label: String,
    pub path: PathBuf,
}

/// State for Settings → Children.
#[derive(Debug, Default)]
pub struct ChildrenFormState {
    /// The row the footer actions apply to.
    pub selected: Option<ChildId>,
    /// Set while a "Remove from this machine" confirmation is on screen.
    pub pending_removal: Option<PendingRemoval>,
    pub success_message: Option<String>,
    pub error_message: Option<String>,
    /// True on the frame the modal opens, so the settings-menu click that
    /// opened it isn't also read as a backdrop click that closes it.
    pub just_opened: bool,
}

impl ChildrenFormState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn clear_messages(&mut self) {
        self.success_message = None;
        self.error_message = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::csv::{tree_checksum, CsvConnection, RegistryEntry};
    use shared::ChildId;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn child_folder(root: &Path, name: &str, id: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("child.yaml"),
            format!(
                "id: {id}\nname: Keiko Hart\nbirthdate: '2010-01-01'\n\
                 created_at: '2024-01-01T00:00:00Z'\nupdated_at: '2024-01-01T00:00:00Z'\n"
            ),
        )
        .unwrap();
        dir
    }

    #[test]
    fn validate_accepts_a_real_child_folder_and_reads_the_id_from_yaml() {
        let root = TempDir::new().unwrap();
        let dir = child_folder(root.path(), "any_folder_name", "keiko_hart");

        let entry = validate_child_folder(&dir).unwrap();
        assert_eq!(entry.id, ChildId::from("keiko_hart"));
        assert_eq!(entry.label, "Keiko Hart");
        assert_eq!(entry.path, dir);
    }

    #[test]
    fn validate_rejects_a_folder_without_child_yaml() {
        let root = TempDir::new().unwrap();
        let dir = root.path().join("empty");
        std::fs::create_dir_all(&dir).unwrap();

        let err = validate_child_folder(&dir).unwrap_err();
        assert!(err.to_string().contains("child.yaml"));
    }

    #[test]
    fn move_refuses_a_non_empty_target_and_changes_nothing() {
        let base = TempDir::new().unwrap();
        let source = child_folder(base.path(), "kid", "kid");
        let target = base.path().join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("something.txt"), "occupied").unwrap();

        let conn = CsvConnection::new(base.path()).unwrap();
        conn.update_registry(|reg| {
            reg.register(RegistryEntry {
                id: ChildId::from("kid"),
                path: source.clone(),
                label: "Kid".into(),
            })
        })
        .unwrap();

        let before = tree_checksum(base.path()).unwrap();
        assert!(move_child_data(&conn, &ChildId::from("kid"), &target).is_err());
        assert_eq!(
            before,
            tree_checksum(base.path()).unwrap(),
            "a refused move must change nothing"
        );
    }

    /// `copy_dir_recursive` would descend into its own output. Refused before
    /// a single byte is copied, because the only copy of the data is the
    /// source it would be churning.
    #[test]
    fn move_refuses_a_target_nested_inside_the_source() {
        let base = TempDir::new().unwrap();
        let source = child_folder(base.path(), "kid", "kid");

        let conn = CsvConnection::new(base.path()).unwrap();
        conn.update_registry(|reg| {
            reg.register(RegistryEntry {
                id: ChildId::from("kid"),
                path: source.clone(),
                label: "Kid".into(),
            })
        })
        .unwrap();

        let before = tree_checksum(base.path()).unwrap();
        for nested in [source.join("inside"), source.clone()] {
            let err = move_child_data(&conn, &ChildId::from("kid"), &nested).unwrap_err();
            assert!(err.to_string().contains("into itself"), "got: {err}");
        }
        assert_eq!(
            before,
            tree_checksum(base.path()).unwrap(),
            "a refused move must change nothing"
        );
    }

    /// A failed verification must not take a directory the user created with
    /// it. `undo_copy` removes only what the copy wrote.
    #[test]
    fn undo_copy_spares_a_target_directory_the_user_created() {
        let base = TempDir::new().unwrap();
        let user_made = base.path().join("their_folder");
        std::fs::create_dir_all(user_made.join("copied_subdir")).unwrap();
        std::fs::write(user_made.join("copied.txt"), "from the copy").unwrap();

        undo_copy(&user_made, true).unwrap();

        assert!(user_made.exists(), "the user's own directory must survive");
        assert_eq!(
            std::fs::read_dir(&user_made).unwrap().count(),
            0,
            "everything the copy wrote must be gone"
        );
    }

    /// The other branch: a target we created ourselves is ours to remove.
    #[test]
    fn undo_copy_removes_a_target_the_copy_created() {
        let base = TempDir::new().unwrap();
        let ours = base.path().join("we_made_this");
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::write(ours.join("copied.txt"), "from the copy").unwrap();

        undo_copy(&ours, false).unwrap();
        assert!(!ours.exists());
    }

    /// Important 4: **Remove from this machine** must be reachable for a
    /// healthy child. It was gated behind `!available`, which — with
    /// `delete_child` having no caller — left no way at all to deregister a
    /// child that was working fine.
    #[test]
    fn remove_is_offered_for_an_available_row() {
        let ready = row_actions(true);
        assert!(ready.remove, "a Ready child must still be removable");
        assert!(!ready.retry, "a Ready child has nothing to retry");
        assert!(!ready.locate, "a Ready child has nothing to locate");

        let broken = row_actions(false);
        assert_eq!(broken, RowActions { retry: true, locate: true, remove: true });
    }

    /// The recovery path the relaxed migration guard depends on: migration
    /// persisted a registry that omits a child whose iCloud folder had not
    /// arrived, and the user adds it by hand once it does. Exercised through
    /// the exact pair **Add existing child…** calls.
    #[test]
    fn a_child_migration_could_not_resolve_is_addable_afterwards() {
        use crate::backend::storage::csv::run_migration;

        let base = TempDir::new().unwrap();
        let icloud = TempDir::new().unwrap();
        let late_arrival = icloud.path().join("keiko_hart");

        let stub = base.path().join("keiko_hart");
        std::fs::create_dir_all(&stub).unwrap();
        std::fs::write(stub.join(".allowance_redirect"), late_arrival.to_string_lossy().as_bytes())
            .unwrap();

        // Migration runs while the folder is still in the cloud.
        let report = run_migration(base.path()).unwrap().unwrap();
        assert!(report.registered.is_empty());

        // iCloud delivers it, and the user picks it in Settings → Children.
        child_folder(icloud.path(), "keiko_hart", "keiko_hart");
        let conn = CsvConnection::new(base.path()).unwrap();
        let entry = validate_child_folder(&late_arrival).unwrap();
        conn.update_registry(|reg| reg.register(entry)).unwrap();

        assert_eq!(
            conn.registry().path_for(&ChildId::from("keiko_hart")),
            Some(late_arrival.as_path())
        );
        assert!(
            run_migration(base.path()).unwrap().is_none(),
            "migration stays idempotent — it cannot undo what the user added"
        );
    }

    #[test]
    fn move_relocates_and_repoints_the_registry() {
        let base = TempDir::new().unwrap();
        let source = child_folder(base.path(), "kid", "kid");
        let target = base.path().join("moved");

        let conn = CsvConnection::new(base.path()).unwrap();
        conn.update_registry(|reg| {
            reg.register(RegistryEntry {
                id: ChildId::from("kid"),
                path: source.clone(),
                label: "Kid".into(),
            })
        })
        .unwrap();

        move_child_data(&conn, &ChildId::from("kid"), &target).unwrap();

        assert!(target.join("child.yaml").exists());
        assert!(!source.exists(), "source must be removed after a verified copy");
        assert_eq!(
            conn.registry().path_for(&ChildId::from("kid")),
            Some(target.as_path())
        );
    }
}
