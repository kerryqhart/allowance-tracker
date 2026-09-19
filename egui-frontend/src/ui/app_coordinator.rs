//! # App Coordinator Module
//!
//! This module contains the main application coordination logic, handling the primary
//! update loop and overall application lifecycle.
//!
//! ## Key Functions:
//! - `eframe::App::update()` - Main application update loop (implements eframe::App trait)
//! - `render_loading_screen()` - Displays loading screen while data is being fetched
//!
//! ## Purpose:
//! This module serves as the central coordinator for the entire application, orchestrating:
//! - UI styling setup
//! - Input handling (ESC key, etc.)
//! - Data loading coordination
//! - Main content rendering
//! - Modal management
//! - Header rendering
//!
//! ## Application Flow:
//! 1. Set up kid-friendly styling
//! 2. Handle global input (ESC key)
//! 3. Load data if needed
//! 4. Render loading screen OR main content
//! 5. Render header and any active modals
//!
//! This is the main entry point that ties together all other UI modules.

use eframe::egui;
use crate::ui::app_state::AllowanceTrackerApp;
use crate::ui::components::styling::{setup_kid_friendly_style, draw_image_background};
use crate::backend::domain::{BalanceService, GoalsDivergedNotice, SyncCommand, SyncMessage, SyncStatus};
use crate::backend::storage::GitManager;
use crate::backend::sync::child_sync::{
    clear_interrupted_merge_marker, current_branch, goals_diverged, push_with_retry, working_tree_dirty,
};
use crate::ui::state::{
    FastForwardBlockedNotice, StaleHeadPollAction, SyncFailureNotice, STALE_HEAD_REFUSAL_LIMIT,
};
use shared::sync::EntityType;
use shared::ChildId;

/// Declared here rather than in `ui/mod.rs` as a top-level sibling: `apply_merge`
/// and `apply_fast_forward` below are private methods (module-private, no
/// `pub(crate)`), and Rust's default visibility is scoped to the defining
/// module AND ITS DESCENDANTS — not "anywhere in the same crate." A
/// `test_support` registered as a sibling of `app_coordinator` under `ui`
/// cannot see them, full stop; that is a hard compiler error
/// (`E0624: method ... is private`), not a style preference. Nesting the
/// module here makes it a descendant of `app_coordinator`, so it can call
/// the real private methods without widening their visibility at all — the
/// alternative (bumping them to `pub(crate)`, as Task 8 did for
/// `merge_diverged` / `working_tree_dirty`) is exactly the production-code
/// change this task was told not to make. The file still lives at the
/// specified path, `src/ui/test_support.rs`; only its module path differs
/// from the brief's sketch (`ui::app_coordinator::test_support`, not
/// `ui::test_support`).
#[cfg(test)]
#[path = "test_support.rs"]
pub mod test_support;

/// Guard for the AWS-wire chokepoint in `read_entity_for_sync`: that path
/// serializes the RAW domain `Transaction` directly (it never goes through
/// `mappers::transaction_to_dto`, so it gets none of that function's
/// `BALANCE_PENDING` -> `NaN` translation). Returns `Err` with a
/// human-readable reason if `tx` must not be put on the wire as-is.
///
/// `Money::render()` on `Transaction::BALANCE_PENDING`
/// (`Money::from_cents(i64::MIN)`) produces `"-92233720368547758.08"`, and
/// parsing that back overflows `i64` and returns `Err` (allowance-core has a
/// test pinning exactly this). A later task turns an unparseable CSV row
/// into a hard, unrecoverable read error with no fallback — so silently
/// encoding this sentinel onto the wire (or into local storage) would arm a
/// fault a later task detonates.
fn transaction_is_syncable(
    tx: &crate::backend::domain::models::transaction::Transaction,
) -> Result<(), String> {
    if tx.balance == crate::backend::domain::models::transaction::Transaction::BALANCE_PENDING {
        return Err(format!(
            "transaction {} balance is still BALANCE_PENDING (not yet calculated by BalanceService)",
            tx.id
        ));
    }
    Ok(())
}

/// Outcome of [`AllowanceTrackerApp::apply_merge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyMergeOutcome {
    /// The merge commit was created (a push was also attempted; a push
    /// failure alone does not change this outcome — see the doc comment on
    /// `apply_merge`).
    Applied,
    /// HEAD moved between `ChildSyncEngine::cycle` computing this merge and
    /// this call applying it — an ordinary local write raced the background
    /// sync. The merge was discarded rather than risk orphaning the commit
    /// that moved HEAD; nothing was written or committed. The caller must
    /// re-run the sync cycle from the new HEAD to pick this up.
    StaleHead,
    /// `transactions.csv` was dirty relative to HEAD when this merge was
    /// about to be applied — the AWS transport's non-committing apply path
    /// (`upsert_transaction_no_commit`, Task 16) had written an
    /// MCP-authored row since the tips this merge was computed against.
    /// This is this design's normal steady state, not a crash (the same
    /// class of hazard `ApplyFastForwardOutcome::CommittedLocalChangesToUnblock`
    /// already handles on the fast-forward path). The dirty tree was
    /// committed as an ordinary local commit so that row is never lost, and
    /// this merge was discarded outright rather than applied — it was
    /// computed against tips that go stale the moment that commit lands.
    /// An immediate re-poll was requested (debounced, like `StaleHead`) so
    /// the next cycle recomputes the merge from the new HEAD.
    DirtyTreeCommitted,
    /// Anything else that stopped the merge being applied (I/O, malformed
    /// input, a git failure). Logged and surfaced via `sync.status` at the
    /// point of failure.
    Failed,
}

/// Why a dirty working tree could not be resolved into a commit.
///
/// Variants carry STRUCTURE, not prose. The `Display` text below is for logs
/// and the error chain, where a developer is the reader; the user-facing
/// sentence is composed by the sync modal, which has the child's display name
/// and folder path. Prose produced down here is the mechanism by which
/// "stage", "dirty tree" and "oid" reached a parent's screen.
#[derive(Debug, thiserror::Error)]
pub enum DirtyTreeError {
    #[error("could not stage local changes")]
    Stage(#[source] git2::Error),
    #[error("could not commit local changes")]
    Commit(#[source] anyhow::Error),
    #[error("{file} could not be read as transaction data")]
    Unparseable { file: &'static str },
    /// Should never happen: `working_tree_dirty` ignores untracked files and
    /// `update_all` stages every tracked change, so any tree that reaches the
    /// guard has something to stage. Kept as an honest "something unexpected
    /// happened" rather than deleted.
    #[error("the working tree reported changes but nothing tracked was modified")]
    NothingToCommit,
}

/// Commit a dirty working tree so a merge or fast-forward can proceed —
/// the single implementation behind both paths.
///
/// # Why tracked paths, not the owned allowlist
///
/// `FILES_THIS_APP_OWNS` (`backend::sync::paths`) exists to stop
/// `add_all(["*"])` sweeping untracked strays into pushed history. A file
/// already tracked in HEAD is already in that history, so committing its
/// modification is not that hazard — and refusing to commit it is what
/// stalled a child's sync permanently and silently. `index.update_all`
/// stages modifications and deletions of tracked entries and never adds an
/// untracked path, so the anti-`add_all` invariant is preserved exactly. The
/// danger was always the `*`.
///
/// # Why parse-validate first
///
/// Atomic writes bound THIS app's corruption after the upgrade. They say
/// nothing about a `transactions.csv` already torn on disk at upgrade time,
/// damaged by a partial restore, or written by an older build. With the hard
/// reset gone nothing else validates, so committing unparseable bytes would
/// push them to the peer and break `read_rows` on BOTH machines — turning a
/// one-machine corruption into a two-machine outage.
///
/// # Why a successful parse is not sufficient on its own
///
/// `parse_transactions` runs `csv::Reader` with headers ON, so it consumes
/// the file's FIRST LINE as a header and never validates it. A file
/// truncated to a single junk line — `"garbage-from-a-crash"` — therefore
/// parses as a header with zero data rows and returns `Ok`. Committing that
/// is the exact outage this gate exists to prevent, in its worst form: the
/// peer fetches it, its own `read_rows` also reads one header and zero rows,
/// and the ledger reads as EMPTY on both machines. Nothing errors anywhere;
/// the data is simply gone from both screens.
///
/// So a parse yielding zero rows is checked further: the first line must be
/// the canonical header (`allowance_core::codec::HEADER`), which is the only
/// thing `render_transactions` ever writes there. A genuinely empty ledger
/// is header-only and passes; junk, and a zero-byte file, do not.
///
/// The check is deliberately scoped to the zero-row case, per review. A file
/// with a junk first line AND well-formed data rows still passes, and that
/// is a knowingly-accepted residual: every row survives it, identically on
/// both machines, so it is not the silent-empty-ledger class of fault.
fn resolve_dirty_tree(
    repo: &git2::Repository,
    message: &str,
) -> Result<git2::Oid, DirtyTreeError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| {
            DirtyTreeError::Commit(anyhow::anyhow!("repository has no working directory"))
        })?
        .to_path_buf();

    let tx_path = workdir.join("transactions.csv");
    if tx_path.exists() {
        let text = std::fs::read_to_string(&tx_path)
            .map_err(|e| DirtyTreeError::Commit(anyhow::Error::from(e)))?;
        match allowance_core::codec::parse_transactions(&text) {
            Err(_) => return Err(DirtyTreeError::Unparseable { file: "transactions.csv" }),
            Ok(parsed) if parsed.rows.is_empty() => {
                // `str::lines` strips a trailing `\r`, so this holds for
                // CRLF files too; a zero-byte file yields `None` and fails
                // the comparison, which is the intended fail-closed answer.
                let first_line = text.lines().next().unwrap_or_default();
                if first_line != allowance_core::codec::HEADER.join(",") {
                    return Err(DirtyTreeError::Unparseable { file: "transactions.csv" });
                }
            }
            Ok(_) => {}
        }
    }

    // `["*"]`, deliberately with no literal filename in it: `update_all`
    // only ever visits entries ALREADY IN THE INDEX, so the wildcard here
    // means "every tracked path", not "everything in the directory". That
    // is the whole difference from `add_all(["*"])`, which walks the
    // working tree and would sweep `.DS_Store` and editor swap files into a
    // pushed commit.
    let mut index = repo.index().map_err(DirtyTreeError::Stage)?;
    index.update_all(["*"].iter(), None).map_err(DirtyTreeError::Stage)?;
    index.write().map_err(DirtyTreeError::Stage)?;

    let gm = GitManager::new();
    match gm.commit_if_changed(&workdir, message) {
        Ok(Some(oid_str)) => git2::Oid::from_str(&oid_str)
            .map_err(|e| DirtyTreeError::Commit(anyhow::Error::from(e))),
        Ok(None) => Err(DirtyTreeError::NothingToCommit),
        Err(e) => Err(DirtyTreeError::Commit(e)),
    }
}

/// Outcome of [`AllowanceTrackerApp::apply_fast_forward`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyFastForwardOutcome {
    /// HEAD and the working tree were advanced to the fetched peer tip.
    Applied,
    /// Nothing to do — HEAD already matches the target (another apply, or
    /// an intervening merge, already got us there).
    AlreadyUpToDate,
    /// HEAD moved since `cycle_with_status` decided this was a
    /// fast-forward, in a way that would no longer make `to` a descendant
    /// of the current tip (an ordinary local write raced the background
    /// sync, or a local commit turned this into a genuine divergence).
    /// Refused rather than risk rewriting history backward or sideways;
    /// nothing was written. The next cycle re-classifies from the new HEAD.
    StaleHead,
    /// Review Important-1: the safe (non-force) checkout hit a genuine
    /// conflict (`GIT_ECONFLICT`) — uncommitted local content would have
    /// been overwritten, most notably the AWS transport's
    /// `upsert_transaction_from_sync` writing `transactions.csv` without
    /// committing (see `apply_remote_entity`'s doc comment). Neither
    /// forcing through it (destroying that content) nor refusing forever
    /// (this design's own AWS-dirty steady state means a machine that only
    /// ever receives MCP writes would then NEVER ingest peer data — an
    /// indefinite livelock, not a rare edge case) is acceptable. The dirty
    /// tree was instead committed as an ordinary local commit — see
    /// `AllowanceTrackerApp::commit_dirty_tree_to_unblock_fast_forward`'s
    /// doc comment for why that is bounded and safe. `sync.fast_forward_blocked`
    /// now holds a notice for this, and an immediate re-poll was requested
    /// so the resulting divergence is picked up promptly rather than
    /// waiting for the next 30s timer tick.
    CommittedLocalChangesToUnblock,
    /// Anything else that stopped the fast-forward being applied (I/O, a
    /// malformed oid, a corrupt object, a permissions problem, or any git
    /// failure that was NOT a checkout conflict). Logged and surfaced via
    /// `sync.status` at the point of failure.
    Failed,
}

impl eframe::App for AllowanceTrackerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // log::info!("APP UPDATE called - main render loop"); // Commented out - too verbose
        // Set up kid-friendly styling
        setup_kid_friendly_style(ctx);

        // Handle sync messages from the background sync thread (needs backend access)
        self.handle_sync_messages();

        // Take whatever the roster worker has reported since the last frame.
        // This is the only place child availability changes, and it is where
        // allowance issuance is triggered.
        self.drain_roster_messages();

        // Detect app focus changes and trigger an immediate sync poll on focus-gain
        let is_focused = ctx.input(|i| i.focused);
        if is_focused && !self.was_focused {
            log::info!("SYNC: app gained focus — sending PollNow");
            if let Some(ref tx) = self.sync_command_tx {
                let _ = tx.send(SyncCommand::PollNow);
            }
        }
        self.was_focused = is_focused;

        // Handle ESC key to close dropdown
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.interaction.child_dropdown.is_open = false;
        }
        
        // Load initial data on first run, and retry any load the availability
        // gate deferred — including one requested by the sync path above, which
        // runs earlier in this same frame.
        //
        // Note: Use cached current_child here to avoid infinite backend calls during loading
        if (self.ui.loading && self.core.current_child.is_none()) || self.pending_initial_load {
            self.load_initial_data_when_ready();
        }
        
        // Check for pending allowances periodically (throttled to avoid excessive calls)
        // This allows the app to issue allowances without requiring a restart
        // The refresh is throttled using Instant/Duration timing to prevent checking every frame
        self.refresh_allowances();
        
        // Clear messages after a delay
        if self.ui.error_message.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        
        // What startup could not fix by itself, above everything else. Painted
        // before the CentralPanel so it takes its own band and stays out of
        // that panel's hand-computed four-layer rect arithmetic.
        self.render_startup_banner(ctx);

        // Main UI with image background
        egui::CentralPanel::default().show(ctx, |ui| {
            // Draw image background with blue overlay first
            let full_rect = ui.available_rect_before_wrap();
            draw_image_background(ui, full_rect);
            
            if self.ui.loading {
                self.render_loading_screen(ui);
                return;
            }
            
            // STEP 2: Four-layer layout with selection controls bar and subheader for toggle buttons
            // Calculate layout areas - optimized reservations for better space utilization
            let header_height = 70.0; // Reduced from 80px
            let selection_bar_height = if self.interaction.transaction_selection_mode { 50.0 } else { 0.0 };
            let subheader_height = 50.0; // Toggle buttons area
            
            // Content area dimensions (remaining space after header, selection bar, and subheader)
            let content_height = full_rect.height() - header_height - selection_bar_height - subheader_height;
            
            // Define rectangles for each layer
            let header_rect = egui::Rect::from_min_size(
                full_rect.min,
                egui::vec2(full_rect.width(), header_height)
            );
            
            let selection_bar_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.left(), full_rect.top() + header_height),
                egui::vec2(full_rect.width(), selection_bar_height)
            );
            
            let subheader_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.left(), full_rect.top() + header_height + selection_bar_height),
                egui::vec2(full_rect.width(), subheader_height)
            );
            
            let content_rect = egui::Rect::from_min_size(
                egui::pos2(full_rect.left(), full_rect.top() + header_height + selection_bar_height + subheader_height),
                egui::vec2(full_rect.width(), content_height)
            );
            
            // DEBUG: Log parent space allocation (commented out - too verbose)
            // log::info!("🏢 WINDOW SPACE: full_rect.height={:.0}, content_height={:.0}, reserved={:.0}px", 
            //           full_rect.height(), content_height, 
            //           header_height + selection_bar_height + subheader_height);
            
            // Layer 1: Header (existing function, positioned in header area)
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                self.render_header(ui);
            });
            
            // Layer 2: Selection controls bar (only when in selection mode)
            if self.interaction.transaction_selection_mode {
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(selection_bar_rect), |ui| {
                    self.render_selection_controls_bar(ui);
                });
            }
            
            // Layer 3: Subheader (Calendar/Table toggle buttons)
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(subheader_rect), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(20.0); // Left padding
                    
                    // Tab-specific controls on the left with vertical centering
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        self.draw_tab_specific_controls(ui);
                    });
                    
                    // Tab toggle buttons on the right with vertical centering
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(20.0); // Right padding
                        self.draw_tab_toggle_buttons(ui);
                    });
                });
            });
            
            // Layer 4: Content (main content area)
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                // Error and success messages
                self.render_messages(ui);
                
                // Main content area
                self.render_main_content(ui);
            });
        });
        
        // Render modals
        self.render_modals(ctx);
    }
}

impl AllowanceTrackerApp {
    /// Check if the current goal is complete (helper function)
    pub fn is_goal_complete(&self) -> bool {
        if let Some(ref calculation) = self.goal.goal_calculation {
            calculation.amount_needed <= 0.0
        } else {
            false
        }
    }

    /// Render the loading screen.
    ///
    /// Says *why* we are waiting when the roster is still pulling a folder down
    /// from iCloud — a bare "Loading..." during a multi-minute first sync is
    /// indistinguishable from a hang. Reads only in-memory roster state.
    pub fn render_loading_screen(&self, ui: &mut egui::Ui) {
        use crate::backend::domain::ChildStatus;
        let downloading = self
            .roster
            .entries()
            .iter()
            .any(|e| matches!(e.status, ChildStatus::Downloading));

        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.spinner();
            ui.label(if downloading {
                "Downloading from iCloud…"
            } else {
                "Loading..."
            });
        });
    }

    /// Draw tab-specific controls for the subheader
    fn draw_tab_specific_controls(&mut self, ui: &mut egui::Ui) {
        use crate::ui::app_state::MainTab;
        use crate::ui::components::chart_renderer::ChartPeriod;
        
        match self.current_tab() {
            MainTab::Calendar => {
                self.draw_calendar_navigation_controls(ui);
            }
            MainTab::Table => {
                // Show table title in subheader
                ui.label(egui::RichText::new("Recent Transactions")
                    .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                    .color(egui::Color32::WHITE)
                    .strong());
            }
            MainTab::Chart => {
                ui.horizontal(|ui| {
                    // Chart title on the left
                    ui.label(egui::RichText::new("Balance Chart")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::WHITE)
                        .strong());
                    
                    ui.add_space(20.0); // Space between title and buttons
                    
                    // Time period buttons
                    // 30 Days button
                    let days_30_button = egui::Button::new(
                        egui::RichText::new("30 Days")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(if self.chart.selected_period == ChartPeriod::Days30 { 
                                egui::Color32::WHITE 
                            } else { 
                                egui::Color32::from_gray(200) 
                            })
                    )
                    .min_size(egui::vec2(60.0, 28.0))
                    .corner_radius(egui::CornerRadius::same(6))
                    .fill(if self.chart.selected_period == ChartPeriod::Days30 {
                        egui::Color32::from_rgb(100, 150, 255) // Active blue
                    } else {
                        egui::Color32::from_rgb(240, 240, 240) // Light gray background for inactive
                    })
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)));
                    
                    if ui.add(days_30_button).clicked() {
                        self.chart.selected_period = ChartPeriod::Days30;
                        self.chart.chart_data.clear(); // Clear data to force reload
                        self.load_chart_data();
                    }
                    
                    ui.add_space(8.0);
                    
                    // 90 Days button
                    let days_90_button = egui::Button::new(
                        egui::RichText::new("90 Days")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(if self.chart.selected_period == ChartPeriod::Days90 { 
                                egui::Color32::WHITE 
                            } else { 
                                egui::Color32::from_gray(200) 
                            })
                    )
                    .min_size(egui::vec2(60.0, 28.0))
                    .corner_radius(egui::CornerRadius::same(6))
                    .fill(if self.chart.selected_period == ChartPeriod::Days90 {
                        egui::Color32::from_rgb(100, 150, 255) // Active blue
                    } else {
                        egui::Color32::from_rgb(240, 240, 240) // Light gray background for inactive
                    })
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)));
                    
                    if ui.add(days_90_button).clicked() {
                        self.chart.selected_period = ChartPeriod::Days90;
                        self.chart.chart_data.clear(); // Clear data to force reload
                        self.load_chart_data();
                    }
                    
                    ui.add_space(8.0);
                    
                    // All Time button
                    let all_time_button = egui::Button::new(
                        egui::RichText::new("All Time")
                            .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                            .color(if self.chart.selected_period == ChartPeriod::AllTime { 
                                egui::Color32::WHITE 
                            } else { 
                                egui::Color32::from_rgb(100, 100, 100) 
                            })
                    )
                    .min_size(egui::vec2(70.0, 28.0))
                    .corner_radius(egui::CornerRadius::same(6))
                    .fill(if self.chart.selected_period == ChartPeriod::AllTime {
                        egui::Color32::from_rgb(100, 150, 255) // Active blue
                    } else {
                        egui::Color32::from_rgb(240, 240, 240) // Light gray background for inactive
                    })
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200)));
                    
                    if ui.add(all_time_button).clicked() {
                        self.chart.selected_period = ChartPeriod::AllTime;
                        self.chart.chart_data.clear(); // Clear data to force reload
                        self.load_chart_data();
                    }
                });
            }
            MainTab::Goal => {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // Show goal title in subheader with proper vertical centering
                    ui.label(egui::RichText::new("My Goal")
                        .font(egui::FontId::new(18.0, egui::FontFamily::Proportional))
                        .color(egui::Color32::WHITE)
                        .strong());
                    
                    // Add cancel button if there's an active goal
                    if self.goal.has_active_goal() {
                        ui.add_space(20.0);
                        
                        // Change button text based on goal completion status
                        let button_text = if self.is_goal_complete() {
                            "Start new goal"
                        } else {
                            "Cancel Goal"
                        };
                        
                        // Match the styling of the inactive toggle buttons
                        let cancel_button = egui::Button::new(egui::RichText::new(button_text)
                                .font(egui::FontId::new(14.0, egui::FontFamily::Proportional))
                                .strong()
                                .color(egui::Color32::from_rgb(100, 100, 100))) // Same gray text as inactive buttons
                            .fill(egui::Color32::from_rgb(240, 240, 240)) // Same light gray background as inactive buttons
                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 200, 200))) // Same light gray border as inactive buttons
                            .corner_radius(egui::CornerRadius::same(8)) // Same rounding as toggle buttons
                            .min_size(egui::vec2(110.0, 35.0)); // Same height as toggle buttons
                        
                        if ui.add(cancel_button).clicked() {
                            self.cancel_current_goal();
                        }
                    }
                });
            }
        }
    }

    /// Draw calendar month navigation controls
    fn draw_calendar_navigation_controls(&mut self, ui: &mut egui::Ui) {
        use crate::ui::components::styling::colors;
        
        ui.horizontal(|ui| {
            // Previous month button with consistent hover styling
            let prev_button = egui::Button::new("<")
                .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100))
                .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple outline
                .corner_radius(egui::CornerRadius::same(6))
                .min_size(egui::vec2(35.0, 35.0));
            
            if ui.add(prev_button).clicked() {
                self.navigate_month(-1);
            }
            
            ui.add_space(15.0);
            
            // Calculate the maximum width needed for any month name + year
            let font_id = egui::FontId::new(16.0, egui::FontFamily::Proportional);
            let current_year = self.calendar.selected_year;
            
            // Test all month names with the current year to find the maximum width
            let month_names = [
                "January", "February", "March", "April", "May", "June",
                "July", "August", "September", "October", "November", "December"
            ];
            
            let max_width = month_names.iter()
                .map(|month| {
                    let text = format!("{} {}", month, current_year);
                    ui.fonts(|f| f.layout_no_wrap(
                        text, 
                        font_id.clone(), 
                        egui::Color32::WHITE
                    )).size().x
                })
                .fold(0.0, f32::max);
            
            // Add padding for safety
            let fixed_width = max_width + 20.0;
            
            // Current month and year display in fixed-width area
            let month_year_text = format!("{} {}", self.get_current_month_name(), self.calendar.selected_year);
            ui.allocate_ui_with_layout(
                egui::vec2(fixed_width, 35.0),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(month_year_text)
                        .font(font_id)
                        .color(egui::Color32::WHITE)
                        .strong())
                        .selectable(false)); // Disable text selection
                }
            );
            
            ui.add_space(15.0);
            
            // Next month button with consistent hover styling
            let next_button = egui::Button::new(">")
                .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100))
                .stroke(egui::Stroke::new(1.5, colors::HOVER_BORDER)) // Purple outline
                .corner_radius(egui::CornerRadius::same(6))
                .min_size(egui::vec2(35.0, 35.0));
            
            if ui.add(next_button).clicked() {
                self.navigate_to_next_month();
            }
        });
    }
    
    // ====================
    // SYNC MESSAGE HANDLING
    // ====================

    /// Drain all pending sync messages from the background thread. Called each frame.
    ///
    /// This must live in app_coordinator (not SyncUiState) because several message
    /// variants require backend access to read/write local repositories.
    fn handle_sync_messages(&mut self) {
        let mut local_state_dirty = false;
        let mut roster_dirty = false;
        while let Some(msg) = self.sync.try_recv_message() {
            match msg {
                SyncMessage::ReadEntityRequest { child_id, entity_type, entity_id, response_tx } => {
                    let json = self.read_entity_for_sync(&child_id, &entity_type, &entity_id);
                    let _ = response_tx.send(json);
                }
                SyncMessage::GetChildIdsRequest { response_tx } => {
                    // Only `Available` children are polled. Reporting a
                    // downloading or missing child would let the apply path
                    // write a fresh transactions.csv into a folder iCloud is
                    // still pulling down — a conflict generator on exactly the
                    // first-run scenario this design exists to fix.
                    let ids: Vec<String> = self
                        .roster
                        .available_ids()
                        .into_iter()
                        .map(|id| id.as_str().to_string())
                        .collect();
                    let _ = response_tx.send(ids);
                }
                // KNOWN GAP (deferred to the AWS spec, documented here so it
                // is not accidentally missing): a transaction deleted on
                // machine A, and correctly dropped by the lgs git merge on
                // machine B, can be RE-CREATED on both machines by a later
                // AWS replay of the old Created event (AWS's event log has
                // no notion that a git merge already resolved this). Once
                // re-created it has no base entry in the next git merge, so
                // that merge reads it as a fresh add on both sides and keeps
                // it. The deleted transaction comes back and stays. Nothing
                // here detects or heals this — closing it requires AWS-side
                // changes (e.g. an AWS-side tombstone or event
                // invalidation), which are out of scope for the lgs
                // coexistence work in this task.
                SyncMessage::ApplyRemoteEntity { child_id, entity_type, entity_id, entity_json, event_id } => {
                    // A remote child change can rename the child or arrive for
                    // one we have cached; re-walk so the picker and the cached
                    // label follow `child.yaml`.
                    roster_dirty |= matches!(entity_type, EntityType::Child);
                    self.apply_remote_entity(&child_id, &entity_type, &entity_id, &entity_json, &event_id);
                    local_state_dirty = true;
                }
                SyncMessage::DeleteLocalEntity { child_id, entity_type, entity_id, event_id } => {
                    roster_dirty |= matches!(entity_type, EntityType::Child);
                    self.delete_local_entity(&child_id, &entity_type, &entity_id, &event_id);
                    local_state_dirty = true;
                }
                SyncMessage::StatusChanged(status) => {
                    self.sync.status = status;
                }
                SyncMessage::Error(error) => {
                    log::error!("Sync error: {}", error);
                    self.sync.status = SyncStatus::Error(error);
                }
                SyncMessage::PushFailed { event_id, error } => {
                    self.record_push_failed(&event_id, &error);
                }
                SyncMessage::EntitiesUpdated { .. } => {
                    // Entity updates are applied inline via ApplyRemoteEntity; this is
                    // just a count notification — no additional action required.
                }
                SyncMessage::ConflictDetected(conflict) => {
                    self.sync.conflicts.push(conflict);
                    self.sync.status = SyncStatus::HasConflicts(self.sync.pending_conflict_count());
                }
                SyncMessage::ApplyMerge { child_id, rows, parents, decisions } => {
                    if self.apply_merge(&child_id, rows, &parents, &decisions) == ApplyMergeOutcome::Applied {
                        local_state_dirty = true;
                    }
                }
                SyncMessage::ApplyFastForward { child_id, to } => {
                    if self.apply_fast_forward(&child_id, &to) == ApplyFastForwardOutcome::Applied {
                        local_state_dirty = true;
                    }
                }
                SyncMessage::GoalsDiverged { child_id, ours_oid, theirs_oid } => {
                    // A NOTICE, not a status — see `GoalsDivergedNotice`'s
                    // doc comment. Held until a future UI dismisses it,
                    // never folded into `self.sync.status` (last-writer-wins,
                    // and would be erased by the very next unrelated sync
                    // event).
                    self.sync.record_goals_diverged(GoalsDivergedNotice { child_id, ours_oid, theirs_oid });
                }
                SyncMessage::ArchivedProjectSkipped { child_id } => {
                    // Review Important-1: durable, not a status — reusing
                    // `SyncFailureNotice`'s existing replace-not-accumulate
                    // storage rather than adding a fourth parallel notice
                    // list. Names the fix, not just the symptom: unlike an
                    // ordinary sync failure, this one never resolves itself
                    // by retrying.
                    self.sync.record_sync_failure(SyncFailureNotice {
                        child_id,
                        message: "This child's project is archived in lgs, so lgs refuses to \
                                  accept new changes from this machine. Local edits are saved \
                                  here, but will not sync until you run `lgs unarchive` for this \
                                  project."
                            .to_string(),
                    });
                }
            }
        }
        // Rebuild once after draining, not once per entity during a bulk sync.
        // `rebuild_roster` drains and persists pending label changes first —
        // rebuilding would otherwise throw them away.
        if roster_dirty {
            self.rebuild_roster();
        }
        // Refresh UI once after draining, rather than per-entity during bulk
        // sync. Requested rather than called directly: a `rebuild_roster` just
        // above leaves every entry `Downloading`, so this must go through the
        // availability gate in `update` — which runs later in this same frame,
        // and re-runs on the repaint the roster worker requests once the active
        // child's folder lands.
        if local_state_dirty {
            self.pending_initial_load = true;
        }
    }

    /// Read a local entity by ID and serialize it to JSON for the sync thread.
    ///
    /// Returns `Some(json)` if the entity exists, `None` if not found or on error.
    fn read_entity_for_sync(&self, child_id: &str, entity_type: &EntityType, entity_id: &str) -> Option<String> {
        match entity_type {
            EntityType::Transaction => {
                match self.core.backend.transaction_service
                    .get_transaction_by_id(child_id, entity_id)
                {
                    Ok(Some(tx)) => {
                        if let Err(reason) = transaction_is_syncable(&tx) {
                            log::error!(
                                "Refusing to sync transaction {} for child {}: {}",
                                entity_id, child_id, reason
                            );
                            return None;
                        }
                        match serde_json::to_string(&tx) {
                            Ok(json) => Some(json),
                            Err(e) => {
                                log::warn!("Failed to serialize transaction {}: {}", entity_id, e);
                                None
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("Transaction {} not found for child {}", entity_id, child_id);
                        None
                    }
                    Err(e) => {
                        log::warn!("Error reading transaction {} for child {}: {}", entity_id, child_id, e);
                        None
                    }
                }
            }
            EntityType::Goal => {
                match self.core.backend.goal_service.get_goal_by_id(child_id, entity_id) {
                    Ok(Some(goal)) => {
                        match serde_json::to_string(&goal) {
                            Ok(json) => Some(json),
                            Err(e) => {
                                log::warn!("Failed to serialize goal {}: {}", entity_id, e);
                                None
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("Goal {} not found for child {}", entity_id, child_id);
                        None
                    }
                    Err(e) => {
                        log::warn!("Error reading goal {} for child {}: {}", entity_id, child_id, e);
                        None
                    }
                }
            }
            EntityType::Child => {
                use crate::backend::domain::commands::child::GetChildCommand;
                match self.core.backend.child_service.get_child(GetChildCommand { child_id: child_id.to_string() }) {
                    Ok(result) => {
                        match result.child {
                            Some(child) => {
                                match serde_json::to_string(&child) {
                                    Ok(json) => Some(json),
                                    Err(e) => {
                                        log::warn!("Failed to serialize child {}: {}", child_id, e);
                                        None
                                    }
                                }
                            }
                            None => {
                                log::warn!("Child {} not found", child_id);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Error reading child {}: {}", child_id, e);
                        None
                    }
                }
            }
        }
    }

    /// Apply a remote entity to local storage. Called when the sync thread pulls a
    /// remote change. Does NOT fire SyncNotifier (Option A — prevents sync loops).
    ///
    /// Also does NOT create a git commit for the transaction case — see
    /// `upsert_transaction_from_sync`'s doc comment. The old AWS transport
    /// and the new lgs transport both write `transactions.csv`; if this path
    /// committed too, one MCP-server write would produce an independent
    /// commit on every machine, making divergence the steady state whenever
    /// the MCP server is active. The lgs merge produces the commit instead.
    fn apply_remote_entity(
        &mut self,
        child_id: &str,
        entity_type: &EntityType,
        entity_id: &str,
        entity_json: &str,
        _event_id: &str,
    ) {
        use crate::backend::domain::models::transaction::Transaction as DomainTransaction;
        use crate::backend::domain::models::goal::DomainGoal;
        use crate::backend::domain::models::child::Child as DomainChild;

        match entity_type {
            EntityType::Transaction => {
                match serde_json::from_str::<DomainTransaction>(entity_json) {
                    Ok(transaction) => {
                        let from_date = transaction.date.to_rfc3339();
                        if let Err(e) = self.core.backend.transaction_service.upsert_transaction_from_sync(&transaction) {
                            log::error!("Failed to apply remote transaction {}: {}", entity_id, e);
                        } else {
                            // A remote transaction can be backdated relative
                            // to rows already on this machine, which leaves
                            // their stored running balances stale — the same
                            // situation the normal local-insert path fixes
                            // with `recalculate_balances_from_date`. Reusing
                            // the shared, AWS-wired `balance_service` here
                            // would do that, but `recalculate_balances_from_date`
                            // emits one `Updated` SyncEvent per changed row:
                            // a single apply that rebalances 40 rows would
                            // push 40 events back out at AWS, from BOTH
                            // machines, after every apply — write
                            // amplification with no new information in it.
                            // A `BalanceService` built fresh here with
                            // `.with_sync_notifier(None)` recalculates the
                            // same way but can never notify, regardless of
                            // what the shared instance is wired to.
                            let merge_safe_balances =
                                BalanceService::new(self.core.backend.csv_connection.clone())
                                    .with_sync_notifier(None);
                            if let Err(e) = merge_safe_balances
                                .recalculate_balances_from_date(child_id, &from_date)
                            {
                                log::error!(
                                    "Failed to recalculate balances for child {} after applying \
                                     remote transaction {}: {}",
                                    child_id, entity_id, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to deserialize remote transaction {}: {}", entity_id, e);
                    }
                }
            }
            EntityType::Goal => {
                match serde_json::from_str::<DomainGoal>(entity_json) {
                    Ok(goal) => {
                        if let Err(e) = self.core.backend.goal_service.upsert_goal_from_sync(&goal) {
                            log::error!("Failed to apply remote goal {}: {}", entity_id, e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to deserialize remote goal {}: {}", entity_id, e);
                    }
                }
            }
            EntityType::Child => {
                match serde_json::from_str::<DomainChild>(entity_json) {
                    Ok(child) => {
                        if let Err(e) = self.core.backend.child_service.upsert_child_from_sync(&child) {
                            log::error!("Failed to apply remote child {}: {}", child_id, e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to deserialize remote child {}: {}", child_id, e);
                    }
                }
            }
        }
    }

    /// Delete a local entity that was deleted on the remote. Does NOT fire SyncNotifier.
    fn delete_local_entity(
        &mut self,
        child_id: &str,
        entity_type: &EntityType,
        entity_id: &str,
        _event_id: &str,
    ) {
        match entity_type {
            EntityType::Transaction => {
                if let Err(e) = self.core.backend.transaction_service
                    .delete_transaction_by_id(child_id, entity_id)
                {
                    log::error!("Failed to delete local transaction {}: {}", entity_id, e);
                }
            }
            EntityType::Goal => {
                if let Err(e) = self.core.backend.goal_service.delete_goal_by_id(child_id, entity_id) {
                    log::error!("Failed to delete local goal {}: {}", entity_id, e);
                }
            }
            EntityType::Child => {
                // Deregister only — never `remove_dir_all`. A sync event must
                // not delete a folder it does not own: on a second machine
                // this path would destroy the shared iCloud folder out from
                // under the first. Forgetting the child locally is the whole
                // of what a remote delete can safely mean.
                //
                // This also means no double-commit risk here (audited as
                // part of the AWS-coexistence fixes above): this arm never
                // touches `ChildRepository::delete_child` or any per-child
                // git repo at all — it only edits the top-level
                // `children.yaml` registry, which no per-child git repo
                // contains.
                let id = shared::ChildId::from(child_id);
                if self.core.backend.csv_connection.registry().path_for(&id).is_none() {
                    log::debug!("Remote delete for child {} which is not registered here", child_id);
                    return;
                }
                if let Err(e) = self
                    .core
                    .backend
                    .csv_connection
                    .update_registry(|reg| reg.deregister(&id))
                {
                    log::error!("Failed to deregister child {} after remote delete: {}", child_id, e);
                } else {
                    // Minor from Task 17 review: a deregistered child must
                    // not leave a lingering entry in the per-child
                    // stale-head debounce maps (`SyncUiState`) — harmless
                    // memory growth today, and a latent trap if the same id
                    // is ever reused later.
                    self.sync.forget_child(child_id);
                }
            }
        }
    }

    /// Apply a merge computed off-thread by `ChildSyncEngine::cycle` (see
    /// `backend/sync/child_sync.rs`). This is the ONE place `ApplyMerge`'s
    /// working-tree mutation happens — on the UI thread, per the
    /// architecture note at `sync_manager.rs:36-38` ("UI owns all repo
    /// I/O"). `rows` already carries recomputed balances (`merge` calls
    /// `recompute_running_balances` internally), so this writes them
    /// verbatim rather than re-deriving anything.
    ///
    /// `parents.0` ("ours") is the HEAD `cycle()` computed this merge
    /// against. HEAD can move between that computation (background thread)
    /// and this call (UI thread) — an ordinary local write commits via
    /// `commit_file_change` at any time in between. Applying a merge whose
    /// base has gone stale would stage the merged CSV over that commit's
    /// content and hand `commit_merge` two parents that do not include it,
    /// orphaning it — the user's transaction would vanish with no error.
    /// This is checked and refused before anything is written.
    ///
    /// Hard failures are routed through `self.sync.status`
    /// (`SyncStatus::Error`), the same path `SyncMessage::Error` already
    /// uses. Two things are deliberately NOT reported that way, because a
    /// STATUS says what the system currently IS (freely overwritten by the
    /// next status change) while these describe something that HAPPENED and
    /// must not be silently clobbered:
    /// - A stale-HEAD refusal is the safety guard working as designed, not
    ///   a fault — it is logged and answered with an immediate
    ///   `SyncCommand::PollNow`, never `SyncStatus::Error`.
    /// - A goals.csv divergence is reported via
    ///   `SyncUiState::record_goals_diverged` (backed by
    ///   `SyncMessage::GoalsDiverged`), which persists until a future UI
    ///   dismisses it — `SyncStatus` is last-writer-wins and would erase it
    ///   on the very next unrelated sync event.
    fn apply_merge(
        &mut self,
        child_id: &str,
        rows: Vec<allowance_core::row::TxRow>,
        parents: &(String, String),
        decisions: &[allowance_core::merge::Decision],
    ) -> ApplyMergeOutcome {
        let id = ChildId::from(child_id);
        let child_dir = match self.core.backend.csv_connection.child_dir(&id) {
            Ok(dir) => dir,
            Err(e) => {
                log::error!("Cannot apply merge for child {child_id}: {e}");
                self.sync.status = SyncStatus::Error(format!("Sync failed for {child_id}: {e}"));
                return ApplyMergeOutcome::Failed;
            }
        };

        let repo = match git2::Repository::open(&child_dir) {
            Ok(repo) => repo,
            Err(e) => {
                log::error!("Cannot open repo for child {child_id} at {}: {e}", child_dir.display());
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: could not open its repository"));
                return ApplyMergeOutcome::Failed;
            }
        };

        // A marker means a PREVIOUS merge for this child was interrupted. It
        // is a breadcrumb only — the dirty-tree guard below owns all content
        // handling, including for this case. See
        // `clear_interrupted_merge_marker`'s doc comment for why resetting
        // here destroyed MCP-authored rows.
        match clear_interrupted_merge_marker(&repo) {
            Ok(true) => log::warn!(
                "A previous merge for child {child_id} was interrupted; its marker has been \
                 cleared. Any uncommitted content is left exactly as it is — the dirty-tree \
                 guard below commits it."
            ),
            Ok(false) => {}
            Err(e) => log::warn!(
                "Could not clear child {child_id}'s interrupted-merge marker (continuing — the \
                 marker is diagnostic only): {e}"
            ),
        }

        let (ours_str, theirs_str) = parents;

        // CRITICAL: refuse a merge computed against a HEAD that has since
        // moved. See the doc comment above.
        let current_head = match repo.head().and_then(|h| h.peel_to_commit()) {
            Ok(c) => c.id().to_string(),
            Err(e) => {
                log::error!("Cannot read HEAD for child {child_id}: {e}");
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: could not read its current commit"));
                return ApplyMergeOutcome::Failed;
            }
        };
        if &current_head != ours_str {
            // This is the safety guard working exactly as designed —
            // expected, self-healing, and about to be retried. It is NOT a
            // fault, so it must never be reported as `SyncStatus::Error`:
            // doing so would teach the first consumer (and eventually the
            // user) that a correctly functioning safety check is a
            // problem. Log it and trigger an immediate re-poll instead —
            // without that, the refused merge would simply be dropped
            // until the next timer tick.
            //
            // The re-poll itself is debounced (`note_stale_head_refusal`):
            // if HEAD keeps moving faster than one fetch+classify+push
            // round-trip (a user typing several transactions in a row),
            // sending `PollNow` on every single refusal would ping-pong
            // continuously between this thread and the background thread —
            // safe (nothing here writes), but a hot machine and needless
            // load on the lgs daemon for no benefit, since the merge is
            // simply recomputed and reapplied once writes settle anyway.
            log::warn!(
                "Refusing to apply a merge for child {child_id}: HEAD moved from {ours_str} to \
                 {current_head} since this merge was computed (a local write raced the sync \
                 cycle). Discarding the stale merge — the local commit is untouched."
            );
            self.request_stale_head_repoll(child_id);
            return ApplyMergeOutcome::StaleHead;
        }

        let (ours_oid, theirs_oid) = match (git2::Oid::from_str(ours_str), git2::Oid::from_str(theirs_str)) {
            (Ok(o), Ok(t)) => (o, t),
            _ => {
                log::error!(
                    "Cannot apply merge for child {child_id}: malformed parent oid(s) {ours_str}/{theirs_str}"
                );
                self.sync.status = SyncStatus::Error(format!("Sync failed for {child_id}: malformed merge data"));
                return ApplyMergeOutcome::Failed;
            }
        };

        // CRITICAL: `transactions.csv` can be dirty relative to HEAD right
        // now, and that is this design's DESIGNED STEADY STATE, not a
        // fault — Task 16 made the AWS apply path non-committing
        // (`upsert_transaction_no_commit`), specifically so a machine that
        // only ever receives MCP writes does not create a local commit on
        // every single one. Writing `rows` (computed against `ours_str`,
        // `theirs_str`) over that dirty file below would silently ERASE any
        // MCP-authored row written since those tips — and the AWS
        // watermark has already advanced past it by the time it is read,
        // so it can never be re-fetched; the row is gone for good. This is
        // the same class of hazard `ApplyFastForwardOutcome::CommittedLocalChangesToUnblock`
        // already handles on the fast-forward path — this is the merge
        // path's equivalent protection, which had none.
        //
        // `clear_interrupted_merge_marker` above does not catch this: it only
        // ever clears a stale marker file — it never inspects or touches
        // working-tree content, so an ordinary dirty tree (this case) is
        // untouched by it either way.
        //
        // Rather than write over it, commit the dirty tree first — as an
        // ordinary local commit, staging only the files this app owns —
        // and discard THIS merge outright. `rows` was computed against
        // `ours_str`/`theirs_str`; the moment that commit lands, HEAD moves
        // past `ours_str`, so `rows` is stale and must not be applied
        // regardless. The next cycle recomputes the merge from the new
        // HEAD (which now includes the row that was just committed) —
        // requested immediately via the same debounced re-poll `StaleHead`
        // already uses, since this is exactly as self-healing and exactly
        // as much "working as designed" as that case.
        match working_tree_dirty(&repo) {
            Ok(false) => {}
            Ok(true) => {
                return self.commit_dirty_tree_before_merge(child_id, &repo);
            }
            Err(e) => {
                log::error!(
                    "Cannot check whether child {child_id}'s working tree is dirty before \
                     applying a merge: {e}"
                );
                self.sync.status = SyncStatus::Error(format!(
                    "Sync failed for {child_id}: could not verify its working tree before merging"
                ));
                return ApplyMergeOutcome::Failed;
            }
        }

        // `goals.csv` is out of scope for `allowance_core::merge` (it models
        // no goal row — a known, recorded gap). A diverged goals.csv must
        // never be silently resolved by picking a side: this cannot be
        // fixed by choosing better code below (some byte content ends up in
        // the merge commit's tree regardless, and it will be whatever is
        // currently checked out — "ours"), so the requirement is to SAY SO
        // rather than let the merge look like a full, clean sync. This is a
        // NOTICE (something that happened, needing to stay visible until
        // dismissed), not a status — folding it into `self.sync.status`
        // would let it be silently erased by the very next unrelated sync
        // event (a `StatusChanged`, an `Error`, even another child's
        // conflict). See `GoalsDivergedNotice`'s doc comment.
        match goals_diverged(&repo, ours_oid, theirs_oid) {
            Ok(true) => {
                log::warn!(
                    "{child_id}'s goals.csv diverged between {ours_str} and {theirs_str} and \
                     was NOT merged — it was left exactly as it is here. Any goal edits made on \
                     the other machine are not reflected and must be reconciled by hand."
                );
                self.sync.record_goals_diverged(GoalsDivergedNotice {
                    child_id: child_id.to_string(),
                    ours_oid: ours_str.clone(),
                    theirs_oid: theirs_str.clone(),
                });
            }
            Ok(false) => {}
            Err(e) => log::warn!(
                "could not determine whether goals.csv diverged for child {child_id} \
                 ({ours_str}/{theirs_str}): {e}"
            ),
        }

        // Write the interrupted-merge marker BEFORE the first working-tree
        // byte changes below — its presence is what lets a future
        // `apply_merge` call (via `clear_interrupted_merge_marker`, above)
        // know a merge here began and did not finish. It is diagnostic only
        // now: nothing branches on it. Not fatal if this fails: see
        // `write_merge_marker`'s doc comment for why losing it only costs
        // that breadcrumb, not correctness.
        if let Err(e) = crate::backend::sync::child_sync::write_merge_marker(&repo, ours_str, theirs_str) {
            log::warn!(
                "Could not write the crash-recovery marker for child {child_id} before applying \
                 this merge (continuing anyway): {e}"
            );
        }

        let csv = allowance_core::codec::render_transactions(&rows);
        if let Err(e) = crate::backend::storage::atomic::write(child_dir.join("transactions.csv"), csv) {
            log::error!("Failed to write merged transactions.csv for child {child_id}: {e}");
            self.sync.status =
                SyncStatus::Error(format!("Sync failed for {child_id}: could not write merged transactions"));
            return ApplyMergeOutcome::Failed;
        }

        // Every non-trivial merge choice is logged with the child and both
        // parent oids, so a surprising result is auditable after the fact —
        // a row disappearing or resurrecting must never be silent.
        for decision in decisions {
            log::info!(
                "[sync merge] child={child_id} parents=({ours_str}, {theirs_str}) decision={decision:?}"
            );
        }

        let branch = match current_branch(&repo) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Cannot determine checked-out branch for child {child_id}: {e}");
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: could not determine its branch"));
                return ApplyMergeOutcome::Failed;
            }
        };

        let gm = GitManager::new();
        let message = format!(
            "sync: merge {} + {} ({} decision(s))",
            &ours_str[..ours_str.len().min(7)],
            &theirs_str[..theirs_str.len().min(7)],
            decisions.len()
        );
        let merge_commit = match gm.commit_merge(&child_dir, &message, &[ours_str.as_str(), theirs_str.as_str()]) {
            Ok(oid) => oid,
            Err(e) => {
                log::error!("Failed to create merge commit for child {child_id}: {e}");
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: could not create the merge commit"));
                return ApplyMergeOutcome::Failed;
            }
        };

        // The merge commit exists now, so this marker no longer describes an
        // interruption. Clear it so a future `clear_interrupted_merge_marker`
        // call doesn't log a false-positive warning about an interruption
        // that never happened — that function never touches content either
        // way, so leaving this marker stale is a diagnostic annoyance, not a
        // correctness risk.
        if let Err(e) = crate::backend::sync::child_sync::clear_merge_marker(&repo) {
            log::warn!(
                "Could not clear the interrupted-merge marker for child {child_id} after \
                 applying this merge (harmless — the next apply's \
                 `clear_interrupted_merge_marker` call will clear it): {e}"
            );
        }

        if let Err(e) = push_with_retry(&repo, &branch, 3) {
            // No retry queue is needed here: the merge commit is already on
            // disk, durable, and the next sync cycle's push attempt carries
            // it forward. Failing to push now must not be treated as
            // failing to apply the merge, and must not touch the working
            // tree or the commit just made. Routed through the same shape
            // as every other push failure (`SyncMessage::PushFailed`)
            // rather than a bespoke log line, so one kind of failure has
            // one representation for whatever eventually consumes it.
            self.record_push_failed(
                &merge_commit,
                &format!("push to lgs failed for child {child_id} (will retry on the next cycle): {e}"),
            );
        }

        // This child's stale-head streak (if any) is over — a fresh bout of
        // races later starts its debounce and cap from a clean state rather
        // than staying suppressed forever. Scoped to `child_id` only (see
        // `SyncUiState`'s doc comments): another child's independent streak
        // must not be reset by, or ever confused with, this one.
        self.sync.note_applied(child_id);
        // If a prior fast-forward for this child was blocked and resolved
        // by committing (Review Important-1), THIS merge succeeding is what
        // actually resolves the divergence that commit deliberately
        // created — the notice can be cleared.
        self.sync.clear_fast_forward_blocked(child_id);
        self.sync.clear_sync_failure(child_id);

        ApplyMergeOutcome::Applied
    }

    /// One place where a dirty-tree failure becomes user-visible state.
    /// Replaces eight copies of "format a message, set status, record a
    /// notice, return Failed" across the two dirty-tree functions.
    ///
    /// The log line walks the `#[source]` chain by hand — that is why
    /// `DirtyTreeError` keeps `git2::Error` / `anyhow::Error` as sources
    /// rather than flattening them into a string, and the cause detail is
    /// the whole point of keeping them. It is written out explicitly because
    /// `{err:#}` does NOT do it: `thiserror`'s generated `Display` ignores
    /// the alternate flag, so `{err:#}` is byte-identical to `{err}` and
    /// every underlying libgit2 / io message is silently dropped. (That is
    /// `anyhow::Error`'s behaviour, not `std::error::Error`'s, and this is
    /// not an `anyhow::Error`.)
    ///
    /// `err.to_string()` alone — no chain — is what the notice records: the
    /// chain is developer detail for the log, not for a parent's screen.
    fn fail_sync(&mut self, child_id: &str, err: &DirtyTreeError) {
        let mut chain = err.to_string();
        let mut source = std::error::Error::source(err);
        while let Some(cause) = source {
            chain.push_str(": ");
            chain.push_str(&cause.to_string());
            source = cause.source();
        }
        log::error!("Sync failed for child {child_id}: {chain}");
        self.sync.status = SyncStatus::Error(format!("Sync failed for {child_id}: {err}"));
        self.sync.record_sync_failure(SyncFailureNotice {
            child_id: child_id.to_string(),
            message: err.to_string(),
        });
    }

    /// Ask the background sync thread to re-run a cycle straight away,
    /// debounced and capped by `note_stale_head_refusal`.
    ///
    /// Shared verbatim by `apply_merge`'s stale-head refusal and its
    /// dirty-tree refusal: both discard an already-computed merge that is
    /// now based on stale tips, and in both cases that is the safety design
    /// working — self-healing, not a fault, so neither may surface as
    /// `SyncStatus::Error`. Without the re-poll the discarded merge would
    /// simply be dropped until the next timer tick; without the debounce, a
    /// user typing several transactions in a row would ping-pong this
    /// thread and the sync thread continuously for no benefit.
    fn request_stale_head_repoll(&mut self, child_id: &str) {
        match self.sync.note_stale_head_refusal(child_id, std::time::Instant::now()) {
            StaleHeadPollAction::Send => match &self.sync_command_tx {
                Some(tx) => {
                    if let Err(e) = tx.send(SyncCommand::PollNow) {
                        log::warn!(
                            "Could not request an immediate re-poll for child {child_id} after \
                             discarding a merge computed against stale tips (sync command \
                             channel closed): {e}"
                        );
                    }
                }
                None => log::warn!(
                    "No sync command channel available to request a re-poll for child \
                     {child_id} after discarding a merge computed against stale tips"
                ),
            },
            StaleHeadPollAction::Debounced => log::debug!(
                "Skipping an immediate re-poll for child {child_id} after discarding a merge \
                 computed against stale tips (within the debounce window) — the ordinary sync \
                 timer will pick this up."
            ),
            StaleHeadPollAction::LimitReached => log::warn!(
                "Child {child_id} has had {STALE_HEAD_REFUSAL_LIMIT} consecutive stale-head/dirty \
                 merge refusals; giving up on immediate re-polling for now and letting the \
                 ordinary sync timer handle it."
            ),
            StaleHeadPollAction::Suppressed => {
                // Already logged at LimitReached above for this streak.
            }
        }
    }

    /// Commit an MCP-authored dirty `transactions.csv` before a merge is
    /// applied over it, and discard that merge outright — see the
    /// "CRITICAL" doc comment right before this function's call site in
    /// `apply_merge` for the full hazard this guards against.
    ///
    /// Shares [`resolve_dirty_tree`] with
    /// `commit_dirty_tree_to_unblock_fast_forward` — one implementation of
    /// "make this tree committable", so the staging rule and the parse gate
    /// cannot drift between the two paths. The two still differ on the
    /// SUCCESS branch and only there: this one records no
    /// `FastForwardBlockedNotice` (this is a merge, not a fast-forward) and
    /// routes its re-poll through the debounced stale-head mechanism.
    fn commit_dirty_tree_before_merge(&mut self, child_id: &str, repo: &git2::Repository) -> ApplyMergeOutcome {
        let message = "sync: commit local changes before applying a peer merge";
        match resolve_dirty_tree(repo, message) {
            Ok(oid) => {
                log::warn!(
                    "Child {child_id}'s working tree was dirty when a merge was about to be \
                     applied (an MCP write landed since the tips this merge was computed \
                     against — this design's normal steady state, not a crash); committed it as \
                     {oid} and discarded the already-computed merge, which was based on \
                     now-stale tips. The next cycle recomputes from the new HEAD."
                );
                self.request_stale_head_repoll(child_id);
                ApplyMergeOutcome::DirtyTreeCommitted
            }
            Err(e) => {
                self.fail_sync(child_id, &e);
                ApplyMergeOutcome::Failed
            }
        }
    }

    /// Apply a plain fast-forward computed off-thread by
    /// `ChildSyncEngine::cycle_with_status` (`Cycle::FastForward` — the
    /// peer is strictly ahead with no local commits to reconcile; see that
    /// enum variant's doc comment in `child_sync.rs`). This is the ONE
    /// place `ApplyFastForward`'s working-tree mutation happens — on the UI
    /// thread, per the same architecture note as `apply_merge`
    /// (`sync_manager.rs:36-40`, "UI owns all repo I/O").
    ///
    /// Unlike `apply_merge`, a fast-forward checks out the WHOLE target
    /// tree, not just `transactions.csv` — `goals.csv` and anything else
    /// tracked comes along for free, so there is no separate goals-scope
    /// concern here.
    ///
    /// No interrupted-merge marker is needed for THIS function's own crash
    /// window (unlike `apply_merge`'s): `checkout_tree` below is called
    /// with the target tree, not derived from HEAD, so a crash mid-checkout
    /// leaves HEAD still at the OLD commit (the ref move happens after,
    /// only once checkout succeeds) — the next cycle re-classifies as the
    /// same fast-forward and safely re-runs `checkout_tree` toward the same
    /// target, which only ever writes what is still missing.
    /// `clear_interrupted_merge_marker` is still called first here purely to
    /// clear a DIFFERENT crash's leftover marker (an unrelated `apply_merge`
    /// that crashed earlier) — it never touches working-tree content, so it
    /// is not a crash-recovery step for this function's own content either.
    ///
    /// Safety-checked twice before anything is written:
    /// - HEAD must still make `to` a genuine fast-forward — an ordinary
    ///   local write, or a merge applied in between, can race this exactly
    ///   the way it races `apply_merge` (see `ApplyFastForwardOutcome::StaleHead`).
    /// - The checkout itself is deliberately NON-FORCE (`git2`'s default
    ///   "safe" checkout): it refuses rather than silently discard
    ///   uncommitted local content — most notably the AWS transport's
    ///   `upsert_transaction_from_sync`, which deliberately writes
    ///   `transactions.csv` without committing (see `apply_remote_entity`'s
    ///   doc comment). A dirty tree here is this app's normal steady state,
    ///   not a crash (see `child_sync::MERGE_IN_PROGRESS_MARKER`'s doc
    ///   comment), so a force-checkout that clobbered it would be a
    ///   data-loss bug, not a convenience.
    ///
    /// # Review Important-1: refusing forever would livelock this design's
    ///   own steady state
    ///
    /// A safe-checkout refusal cannot simply be left as "retry next cycle"
    /// the way it first looks. `classify` (`child_sync.rs`) returns
    /// `FastForward` precisely because THIS machine made no local commit —
    /// which is exactly what "only ever receiving MCP writes" looks like
    /// under Task 16's fix (`upsert_transaction_from_sync` deliberately
    /// never commits). So on a machine that only receives, nothing ever
    /// changes between cycles: the checkout refuses, the tree stays dirty,
    /// `classify` returns `FastForward` again next tick, forever. That
    /// machine would never ingest another byte of peer data. The refusal
    /// itself is correct (it must never force-discard real data); the
    /// missing next step was. See
    /// `commit_dirty_tree_to_unblock_fast_forward`'s doc comment for the
    /// resolution.
    fn apply_fast_forward(&mut self, child_id: &str, to: &str) -> ApplyFastForwardOutcome {
        let id = ChildId::from(child_id);
        let child_dir = match self.core.backend.csv_connection.child_dir(&id) {
            Ok(dir) => dir,
            Err(e) => {
                log::error!("Cannot apply fast-forward for child {child_id}: {e}");
                self.sync.status = SyncStatus::Error(format!("Sync failed for {child_id}: {e}"));
                return ApplyFastForwardOutcome::Failed;
            }
        };

        let repo = match git2::Repository::open(&child_dir) {
            Ok(repo) => repo,
            Err(e) => {
                log::error!("Cannot open repo for child {child_id} at {}: {e}", child_dir.display());
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: could not open its repository"));
                return ApplyFastForwardOutcome::Failed;
            }
        };

        // A marker means a PREVIOUS merge for this child was interrupted. It
        // is a breadcrumb only — the dirty-tree guard below owns all content
        // handling, including for this case. See
        // `clear_interrupted_merge_marker`'s doc comment for why resetting
        // here destroyed MCP-authored rows.
        match clear_interrupted_merge_marker(&repo) {
            Ok(true) => log::warn!(
                "A previous merge for child {child_id} was interrupted; its marker has been \
                 cleared. Any uncommitted content is left exactly as it is before checking out \
                 {to}."
            ),
            Ok(false) => {}
            Err(e) => log::warn!(
                "Could not clear child {child_id}'s interrupted-merge marker (continuing — the \
                 marker is diagnostic only): {e}"
            ),
        }

        let to_oid = match git2::Oid::from_str(to) {
            Ok(oid) => oid,
            Err(e) => {
                log::error!(
                    "Cannot apply fast-forward for child {child_id}: malformed target oid {to}: {e}"
                );
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: malformed sync data"));
                return ApplyFastForwardOutcome::Failed;
            }
        };

        let current_head = match repo.head().and_then(|h| h.peel_to_commit()) {
            Ok(c) => c.id(),
            Err(e) => {
                log::error!("Cannot read HEAD for child {child_id}: {e}");
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: could not read its current commit"));
                return ApplyFastForwardOutcome::Failed;
            }
        };

        if current_head == to_oid {
            return ApplyFastForwardOutcome::AlreadyUpToDate;
        }

        // CRITICAL: refuse a fast-forward whose ancestry no longer holds —
        // see the doc comment above. A local commit made since
        // `cycle_with_status` classified this can turn a genuine
        // fast-forward into a real divergence, which this function must
        // never paper over by moving the ref anyway.
        match repo.graph_descendant_of(to_oid, current_head) {
            Ok(true) => {}
            Ok(false) => {
                log::warn!(
                    "Refusing to fast-forward child {child_id} to {to}: HEAD moved to \
                     {current_head} since this was classified as a fast-forward, and \
                     {current_head} is no longer an ancestor of {to} — this is no longer a \
                     genuine fast-forward. Discarding; the next cycle will re-classify from the \
                     new HEAD."
                );
                return ApplyFastForwardOutcome::StaleHead;
            }
            Err(e) => {
                log::error!("Cannot verify ancestry for child {child_id}'s fast-forward to {to}: {e}");
                self.sync.status = SyncStatus::Error(format!(
                    "Sync failed for {child_id}: could not verify fast-forward ancestry"
                ));
                return ApplyFastForwardOutcome::Failed;
            }
        }

        let target_commit = match repo.find_commit(to_oid) {
            Ok(c) => c,
            Err(e) => {
                log::error!(
                    "Cannot resolve fast-forward target commit {to} for child {child_id}: {e}"
                );
                self.sync.status = SyncStatus::Error(format!(
                    "Sync failed for {child_id}: could not resolve its sync target"
                ));
                return ApplyFastForwardOutcome::Failed;
            }
        };
        let target_tree = match target_commit.tree() {
            Ok(t) => t,
            Err(e) => {
                log::error!("Cannot read tree for fast-forward target {to} for child {child_id}: {e}");
                self.sync.status = SyncStatus::Error(format!(
                    "Sync failed for {child_id}: could not read its sync target"
                ));
                return ApplyFastForwardOutcome::Failed;
            }
        };

        // Deliberately NOT `.force()` — see this function's doc comment.
        let mut checkout_opts = git2::build::CheckoutBuilder::new();
        if let Err(e) = repo.checkout_tree(target_tree.as_object(), Some(&mut checkout_opts)) {
            // Review Minor-3: only `GIT_ECONFLICT` genuinely means "would
            // overwrite uncommitted local changes." Anything else (I/O, a
            // corrupt object, a permissions problem) is a real failure and
            // must reach `sync.status`, not be silently treated as the
            // same benign, self-resolving condition.
            if e.code() == git2::ErrorCode::Conflict {
                log::warn!(
                    "Fast-forward checkout for child {child_id} to {to} would overwrite \
                     uncommitted local changes: {e}."
                );
                return self.commit_dirty_tree_to_unblock_fast_forward(child_id, &repo, to);
            }
            let message = format!("could not check out synced data ({e})");
            log::error!("Fast-forward checkout failed for child {child_id} to {to}: {e}");
            // Review round 4, Important-2: `self.sync.status` here is
            // WRITE-AND-FORGET — `run_child_sync_cycles` always sends
            // `StatusChanged(Idle)` right after the `ApplyFastForward`
            // message being handled right now, and both are drained in the
            // SAME `handle_sync_messages` batch before a single frame ever
            // renders, so this status write is provably invisible (see
            // `SyncFailureNotice`'s doc comment). Recorded there too so a
            // genuine failure — an I/O error, a corrupt object, a
            // permissions problem — actually reaches the user.
            self.sync.status = SyncStatus::Error(format!("Sync failed for {child_id}: {message}"));
            self.sync.record_sync_failure(SyncFailureNotice {
                child_id: child_id.to_string(),
                message,
            });
            return ApplyFastForwardOutcome::Failed;
        }

        let branch = match current_branch(&repo) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Cannot determine checked-out branch for child {child_id}: {e}");
                self.sync.status =
                    SyncStatus::Error(format!("Sync failed for {child_id}: could not determine its branch"));
                return ApplyFastForwardOutcome::Failed;
            }
        };

        // Move the branch ref (and HEAD) to the new tip only AFTER the
        // checkout above succeeded — see this function's doc comment for
        // why that order is what makes a crash between them self-healing.
        let refname = format!("refs/heads/{branch}");
        if let Err(e) = repo.reference(&refname, to_oid, true, "sync: fast-forward") {
            log::error!("Cannot move {refname} to {to} for child {child_id}: {e}");
            self.sync.status =
                SyncStatus::Error(format!("Sync failed for {child_id}: could not update its branch"));
            return ApplyFastForwardOutcome::Failed;
        }
        if let Err(e) = repo.set_head(&refname) {
            log::error!("Cannot set HEAD to {refname} for child {child_id}: {e}");
            self.sync.status = SyncStatus::Error(format!("Sync failed for {child_id}: could not update HEAD"));
            return ApplyFastForwardOutcome::Failed;
        }

        log::info!("[sync fast-forward] child={child_id} {current_head} -> {to}");

        // Same as a successful merge apply: any stale-head streak for this
        // child is over.
        self.sync.note_applied(child_id);
        self.sync.clear_sync_failure(child_id);

        ApplyFastForwardOutcome::Applied
    }

    /// Resolve a fast-forward blocked by uncommitted local content (see
    /// `apply_fast_forward`'s "Review Important-1" doc section) by
    /// committing that content as an ordinary, single-parent local commit.
    ///
    /// # Why committing here is safe, bounded, and NOT a reintroduction of
    ///   what Task 16 fixed
    ///
    /// Task 16's fix was about the ROUTINE path: `apply_remote_entity`
    /// creating a commit on EVERY machine for EVERY MCP write would make
    /// divergence the permanent steady state whenever the MCP server is
    /// active. This function is not that path and does not run on every
    /// write — it runs only when a fast-forward has been SPECIFICALLY
    /// blocked, which requires uncommitted local content to already exist.
    /// It fires once per blockage, not once per write.
    ///
    /// The content being committed is not garbage: it is exactly what
    /// `apply_remote_entity`/`upsert_transaction_from_sync` already wrote
    /// to `transactions.csv` (and already applied to this app's in-memory
    /// state via the ordinary AWS entity-apply path) — this call only
    /// catches up this child's GIT HISTORY to match what the working tree
    /// (and this app's own state) already say. Once committed, the new
    /// commit and the fast-forward's `to` are both descendants of the SAME
    /// old HEAD, so the next cycle's `classify` sees a genuine divergence
    /// (never `FastForward` again against this same `to`) and routes
    /// through `ChildSyncEngine`'s ordinary merge path — whose
    /// `allowance_core::merge` already de-duplicates rows that are
    /// `intrinsic_eq` on both sides, so a row this machine and the peer
    /// both independently ended up holding is not doubled.
    fn commit_dirty_tree_to_unblock_fast_forward(
        &mut self,
        child_id: &str,
        repo: &git2::Repository,
        to: &str,
    ) -> ApplyFastForwardOutcome {
        let message = format!("sync: commit local changes blocking a fast-forward to {to}");
        match resolve_dirty_tree(repo, &message) {
            Ok(oid) => {
                log::warn!(
                    "Child {child_id}'s fast-forward to {to} was blocked by uncommitted local \
                     changes (this app's normal steady state under the AWS transport — see \
                     apply_remote_entity's doc comment, not a crash); committed them as {oid} so \
                     the next cycle reclassifies this as a genuine divergence and the merge path \
                     resolves it. Deliberate and bounded — not the routine per-write commit Task \
                     16 removed."
                );
                // The one place the two dirty-tree callers genuinely differ:
                // a blocked fast-forward is recorded as its own notice, and
                // its re-poll is sent directly rather than through the
                // stale-head debounce (this path fires once per blockage,
                // not once per racing local write).
                self.sync.record_fast_forward_blocked(FastForwardBlockedNotice {
                    child_id: child_id.to_string(),
                    to: to.to_string(),
                });
                if let Some(tx) = &self.sync_command_tx {
                    if let Err(e) = tx.send(SyncCommand::PollNow) {
                        log::warn!(
                            "Could not request an immediate re-poll for child {child_id} after \
                             committing to unblock a fast-forward: {e}"
                        );
                    }
                } else {
                    log::warn!(
                        "No sync command channel available to request a re-poll for child \
                         {child_id} after committing to unblock a fast-forward"
                    );
                }
                ApplyFastForwardOutcome::CommittedLocalChangesToUnblock
            }
            Err(e) => {
                // Includes `NothingToCommit`: the checkout conflicted but
                // nothing TRACKED differed from HEAD, so the conflicting
                // content is an untracked path colliding with the target
                // tree. There is nothing safe to commit, and a conflict that
                // never resolves needs a human — not a silently recurring
                // empty-commit attempt every tick.
                self.fail_sync(child_id, &e);
                ApplyFastForwardOutcome::Failed
            }
        }
    }

    /// Record a push failure — shared by the ordinary AWS-style
    /// `SyncMessage::PushFailed` arm and `apply_merge`'s post-merge push, so
    /// both kinds of push failure funnel through one representation. Purely
    /// a log line today (matching the pre-existing `PushFailed` handling);
    /// not a `SyncStatus` write, since a push failure is a transient,
    /// automatically-retried condition, not a durable state description.
    ///
    /// `reference` names whatever this failure is about — an AWS-style
    /// sync event id from the `PushFailed` arm, or a git merge-commit oid
    /// string from `apply_merge`. Deliberately NOT named `event_id`: that
    /// name implies one specific shape (the AWS event-sourcing id), and the
    /// git case is not that — the next reader parsing this value on the
    /// assumption it is always an event id would be wrong.
    fn record_push_failed(&mut self, reference: &str, error: &str) {
        log::warn!("Sync push failed for {}: {}", reference, error);
    }

    /// Refresh pending allowances if enough time has passed since last check
    /// 
    /// This method implements periodic allowance checking without overwhelming the system.
    /// Since egui's update() loop runs 60+ times per second, we need to throttle
    /// allowance checks to avoid excessive CPU usage and database calls.
    /// 
    /// Timing Strategy:
    /// - Use Instant::now() to track when we last checked allowances
    /// - Use Duration to define the interval (default: 5 minutes)
    /// - Only check allowances when enough time has passed
    /// - This prevents checking allowances every frame while keeping the app responsive
    /// 
    /// Why not frame counting? Frame rates vary, so timing would be inconsistent.
    /// Why not external timers? Overkill for this simple use case.
    /// Why Instant/Duration? Designed for this exact purpose - measuring time intervals.
    pub fn refresh_allowances(&mut self) {
        // Check if it's time to refresh allowances (throttled to avoid excessive calls)
        if self.ui.should_refresh_allowances() {
            log::debug!("Performing periodic allowance refresh check");

            // Issue only for an active child whose folder is materialized.
            // Issuing into a folder iCloud is still delivering would append to
            // a half-present transactions.csv. The timestamp is marked either
            // way so we don't re-check every frame; the "just became
            // available" case is covered by the roster trigger in
            // `drain_roster_messages`, not by this throttle.
            let active_available = matches!(
                self.active_child_status(),
                Some(crate::backend::domain::ChildStatus::Available(_))
            );
            if !active_available {
                log::debug!("Skipping allowance refresh: active child is not available yet");
                self.ui.mark_allowance_refresh();
                return;
            }

            // Use the existing backend method to check and issue pending allowances
            match self.core.backend.transaction_service.as_ref().check_and_issue_pending_allowances() {
                Ok(count) => {
                    if count > 0 {
                        log::info!("Periodic refresh: Issued {} pending allowances", count);

                        // Reload every transaction-derived view so the new
                        // allowance transactions show up immediately without a
                        // restart: the header balance, the calendar, the goal
                        // progress, and the chart. (Deliberately not the table —
                        // reloading it would reset the user's scroll position and
                        // pagination mid-session.)
                        log::info!("Reloading balance, calendar, goal, and chart to show new allowances");
                        self.load_balance();
                        self.load_calendar_data();
                        self.load_goal_data();
                        self.load_chart_data();

                        // Optionally show a success message to the user
                        // self.ui.set_success_message(format!("Issued {} allowances!", count));
                    } else {
                        log::debug!("Periodic refresh: No pending allowances found");
                    }
                }
                Err(e) => {
                    log::warn!("Periodic refresh failed: {}", e);
                    // Don't show error to user for background refresh - just log it
                }
            }
            
            // Mark that we just performed a refresh (updates the timestamp)
            self.ui.mark_allowance_refresh();
        }
    }
}

#[cfg(test)]
mod refresh_allowance_tests {
    use crate::ui::app_state::AllowanceTrackerApp;
    use crate::backend::Backend;
    use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
    use crate::backend::domain::commands::allowance::UpdateAllowanceConfigCommand;
    use chrono::Datelike;

    /// Regression test: when the periodic allowance refresh issues new
    /// allowance transactions, the header balance (`current_balance`) must be
    /// reloaded — not left stale until the next app restart.
    ///
    /// Reproduces the bug where `refresh_allowances` reloaded the calendar but
    /// not the balance, so the top-of-screen balance stayed stale after a
    /// background allowance was issued.
    #[test]
    fn refresh_allowances_reloads_stale_header_balance() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None)
            .expect("backend on temp dir");

        // Seed a child and make it active.
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");

        // Configure an active allowance due today, so the periodic check has
        // at least one pending allowance to issue.
        let today = chrono::Local::now().date_naive();
        backend
            .allowance_service
            .update_allowance_config(UpdateAllowanceConfigCommand {
                child_id: Some(child.id.clone()),
                amount: 10.0,
                day_of_week: today.weekday().num_days_from_sunday() as u8,
                is_active: true,
                use_age_based_amount: false,
            })
            .expect("configure allowance");

        let mut app = AllowanceTrackerApp::new_for_test(backend);

        // Simulate a stale header balance left over from before the background
        // allowance was issued.
        app.core.current_balance = 999.0;

        // Run the periodic refresh. A fresh app has never refreshed, so
        // `should_refresh_allowances()` returns true and the issuance path runs.
        app.refresh_allowances();

        // The store now reflects the issued allowance(s)...
        let store_balance = app
            .backend()
            .balance_service
            .get_current_balance(&child.id)
            .expect("store balance");
        assert!(
            store_balance > 0.0,
            "precondition: allowance issuance should have changed the store balance"
        );

        // ...and the in-memory header balance must match it, not the stale value.
        assert_eq!(
            app.current_balance(),
            store_balance,
            "header balance was not reloaded after background allowance issuance"
        );
    }

    /// Build a backend with one active child owed an allowance today, and an
    /// app whose roster has been reset to "nothing loaded yet" (every entry
    /// `Downloading`, generation 1) so availability transitions can be driven
    /// by hand. Returns the app and the child's id.
    #[cfg(test)]
    fn app_awaiting_its_active_child() -> (AllowanceTrackerApp, String, tempfile::TempDir) {
        use crate::ui::state::roster::ChildRoster;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None)
            .expect("backend on temp dir");

        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");

        let today = chrono::Local::now().date_naive();
        backend
            .allowance_service
            .update_allowance_config(UpdateAllowanceConfigCommand {
                child_id: Some(child.id.clone()),
                amount: 10.0,
                day_of_week: today.weekday().num_days_from_sunday() as u8,
                is_active: true,
                use_age_based_amount: false,
            })
            .expect("configure allowance");

        let mut app = AllowanceTrackerApp::new_for_test(backend);

        // `new_for_test` settles the roster; wind it back so the child starts
        // out unloaded and the transition can be driven message by message.
        let registry = app.backend().csv_connection.registry();
        app.roster = ChildRoster::new(registry, app.roster_generation);

        (app, child.id, temp)
    }

    fn available_status(child_id: &str) -> crate::backend::domain::ChildStatus {
        use crate::backend::domain::models::child::Child as DomainChild;
        crate::backend::domain::ChildStatus::Available(DomainChild {
            id: child_id.to_string(),
            name: "Test Kid".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2015, 1, 1).unwrap(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    /// Allowance issuance must hang off a *transition in the roster's own
    /// state*, never off the contents of a message.
    ///
    /// `ChildRoster::apply` discards a status from a superseded walk. A trigger
    /// that read the message instead would issue allowances on a report the
    /// roster itself rejected — money moving on discarded data.
    #[test]
    fn a_discarded_stale_generation_status_does_not_issue_allowances() {
        use crate::ui::state::roster::RosterMessage;
        use shared::ChildId;

        let (mut app, child_id, _temp) = app_awaiting_its_active_child();

        app.roster_tx
            .send(RosterMessage::Status {
                generation: app.roster_generation + 99, // not the current walk
                id: ChildId::from(child_id.as_str()),
                status: available_status(&child_id),
            })
            .unwrap();
        app.drain_roster_messages();

        assert!(
            !app.is_available(&ChildId::from(child_id.as_str())),
            "precondition: apply must have discarded the stale-generation status"
        );
        let balance = app
            .backend()
            .balance_service
            .get_current_balance(&child_id)
            .expect("store balance");
        assert_eq!(
            balance, 0.0,
            "no allowance may be issued off a status the roster discarded"
        );
    }

    /// The other half: a status the roster *accepts* for the active child does
    /// trigger issuance, and does so on the transition into `Available`.
    #[test]
    fn the_active_child_becoming_available_issues_its_pending_allowances() {
        use crate::ui::state::roster::RosterMessage;
        use shared::ChildId;

        let (mut app, child_id, _temp) = app_awaiting_its_active_child();

        app.roster_tx
            .send(RosterMessage::Status {
                generation: app.roster_generation,
                id: ChildId::from(child_id.as_str()),
                status: available_status(&child_id),
            })
            .unwrap();
        app.drain_roster_messages();

        assert!(app.is_available(&ChildId::from(child_id.as_str())));
        let balance = app
            .backend()
            .balance_service
            .get_current_balance(&child_id)
            .expect("store balance");
        assert!(
            balance > 0.0,
            "the transition into Available must issue the pending allowance"
        );
    }

    /// The availability gate must *defer* a load, not drop it. The sync path
    /// rebuilds the roster (leaving every entry `Downloading`) and then asks
    /// for a refresh; dropping it would leave the window on pre-sync data with
    /// nothing left to trigger a reload.
    #[test]
    fn a_load_requested_while_the_child_is_downloading_is_deferred_then_runs() {
        use crate::ui::state::roster::RosterMessage;
        use shared::ChildId;

        let (mut app, child_id, _temp) = app_awaiting_its_active_child();

        app.load_initial_data_when_ready();
        assert!(
            app.pending_initial_load,
            "a load requested while the folder is downloading must be held, not dropped"
        );
        assert!(
            app.core.current_child.is_none(),
            "nothing may be read out of a folder that is still downloading"
        );

        // The folder lands.
        app.roster_tx
            .send(RosterMessage::Status {
                generation: app.roster_generation,
                id: ChildId::from(child_id.as_str()),
                status: available_status(&child_id),
            })
            .unwrap();
        app.drain_roster_messages();

        app.load_initial_data_when_ready();
        assert!(!app.pending_initial_load, "the deferred load must be cleared once it runs");
        assert_eq!(
            app.core.current_child.as_ref().map(|c| c.id.clone()),
            Some(child_id),
            "the deferred load must actually run once the folder is available"
        );
    }

    /// A remote child delete must DEREGISTER ONLY.
    ///
    /// The folder is shared — on a second machine it is the same iCloud
    /// directory the first machine is still using. `remove_dir_all` here would
    /// destroy another machine's data in response to a sync event. Forgetting
    /// the child locally is the whole of what a remote delete can safely mean.
    #[test]
    fn a_remote_child_delete_deregisters_without_touching_the_folder() {
        use shared::sync::EntityType;
        use shared::ChildId;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None)
            .expect("backend on temp dir");

        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;

        let id = ChildId::from(child.id.as_str());
        let folder = backend
            .csv_connection
            .child_dir(&id)
            .expect("child folder resolves");
        assert!(folder.join("child.yaml").exists(), "precondition: folder is real");

        let mut app = AllowanceTrackerApp::new_for_test(backend);
        app.delete_local_entity(&child.id, &EntityType::Child, &child.id, "evt-1");

        assert!(
            folder.join("child.yaml").exists(),
            "a remote delete must not remove the shared child folder"
        );
        assert!(
            app.backend().csv_connection.registry().path_for(&id).is_none(),
            "the child must be deregistered locally"
        );
    }
}

#[cfg(test)]
mod sync_guard_tests {
    use super::transaction_is_syncable;
    use crate::backend::domain::models::transaction::{Transaction, TransactionType};
    use allowance_core::money::Money;

    fn a_transaction(balance: Money) -> Transaction {
        Transaction {
            id: "tx-1".to_string(),
            child_id: "child-1".to_string(),
            date: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap(),
            description: "Test".to_string(),
            amount: Money::from_cents(1000),
            balance,
            transaction_type: TransactionType::OneOffIncome,
        }
    }

    /// Regression test for the AWS-wire chokepoint: `read_entity_for_sync`
    /// serializes the raw domain `Transaction` (no `mappers.rs` NaN
    /// translation on this path), so a `BALANCE_PENDING` balance must be
    /// refused here rather than silently encoded as a finite garbage float.
    #[test]
    fn a_balance_pending_transaction_is_refused() {
        let tx = a_transaction(Transaction::BALANCE_PENDING);
        let result = transaction_is_syncable(&tx);
        assert!(
            result.is_err(),
            "a transaction with balance == BALANCE_PENDING must be refused, not synced"
        );
    }

    #[test]
    fn an_ordinary_balance_is_syncable() {
        let tx = a_transaction(Money::from_cents(2500));
        assert!(
            transaction_is_syncable(&tx).is_ok(),
            "an ordinary, already-calculated balance must be syncable"
        );
    }
}

/// Tests for `AllowanceTrackerApp::apply_merge` — the UI-thread half of
/// `SyncMessage::ApplyMerge`. Everything here drives real git repos in
/// tempdirs (created by the ordinary app write path, which git-initializes
/// each child directory via `commit_file_change`) — no `lgs` binary, no
/// daemon, no network, matching the same safety constraint the
/// `child_sync` tests observe.
#[cfg(test)]
mod apply_merge_tests {
    use super::test_support::{assert_resolved_or_explained, run_cycles_until_terminal, ChildRepoFixture};
    use super::{ApplyMergeOutcome, SyncStatus};
    use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
    use crate::backend::domain::commands::transactions::CreateTransactionCommand;
    use crate::backend::domain::SyncCommand;
    use crate::backend::storage::GitManager;
    use crate::backend::Backend;
    use crate::ui::app_state::AllowanceTrackerApp;
    use allowance_core::money::Money;
    use allowance_core::row::{TxRow, TxType};
    use chrono::DateTime;
    use git2::Repository;

    /// Commit directly against a repository's object database — no working
    /// tree or index touched — so this can plant a "peer's" commit that
    /// never moves any ref, exactly mirroring what `ChildSyncEngine::cycle`
    /// would have fetched into `refs/remotes/lgs-auth/main` without this
    /// test needing a real remote at all.
    ///
    /// Task 10 review fix: seeds the tree builder from the first parent's
    /// tree (when there is one) rather than an empty tree, so files this
    /// helper's callers don't mention (`child.yaml`, `goals.csv`,
    /// `allowance_config.yaml`) carry over unchanged instead of vanishing
    /// from the resulting commit's tree. Seeding from an empty tree made
    /// `a_fast_forward_checks_out_the_new_tree_onto_disk` (in
    /// `apply_fast_forward_tests`, which really does check this tree out
    /// onto disk) delete `child.yaml` from the child's directory, and made
    /// `goals_diverged` report spuriously true for every merge test here
    /// that never intended to exercise a goals divergence at all — see the
    /// same fix in `ChildRepoFixture::build` (`test_support.rs`), which
    /// this task's own fast-forward smoke test caught first.
    fn commit_with_files(
        repo: &Repository,
        message: &str,
        parents: &[&git2::Commit],
        files: &[(&str, &str)],
        timestamp: i64,
    ) -> git2::Oid {
        let sig =
            git2::Signature::new("Test", "test@example.com", &git2::Time::new(timestamp, 0)).unwrap();
        let base_tree = parents.first().map(|c| c.tree().unwrap());
        let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
        for (name, content) in files {
            let blob_id = repo.blob(content.as_bytes()).unwrap();
            builder.insert(*name, blob_id, 0o100644).unwrap();
        }
        let tree_id = builder.write().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(None, &sig, &sig, message, &tree, parents).unwrap()
    }

    /// One child, with a real committed `transactions.csv` (an ordinary
    /// transaction write initializes the git repo via `commit_file_change`,
    /// same as production). Returns the app, the child id, and the tempdir
    /// guard (must be held for the whole test).
    fn app_with_git_backed_child() -> (AllowanceTrackerApp, String, tempfile::TempDir) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None).expect("backend");
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");
        backend
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Allowance".to_string(),
                amount: 10.0,
                date: None,
            })
            .expect("create transaction");
        let app = AllowanceTrackerApp::new_for_test(backend);
        (app, child.id, temp)
    }

    fn a_row(child_id: &str, id: &str, desc: &str, cents: i64) -> TxRow {
        TxRow {
            id: id.to_string(),
            child_id: child_id.to_string(),
            date: DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00").unwrap(),
            description: desc.to_string(),
            amount: Money::from_cents(cents),
            balance: Money::from_cents(cents),
            tx_type: TxType::Allowance,
        }
    }

    /// Important-3, bullet 1: a successful apply produces a real two-parent
    /// commit whose tree holds exactly the merged CSV — not the pre-merge
    /// content, not an empty tree.
    #[test]
    fn a_successful_apply_produces_a_two_parent_commit_holding_the_merged_csv() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        // Simulates exactly what `cycle()` would have fetched as
        // `refs/remotes/lgs-auth/main` — a commit that exists in the ODB
        // but is reachable from no ref.
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        let rows = vec![a_row(&child_id, "in-1-a", "Merged Allowance", 1000)];
        let outcome = app.apply_merge(
            &child_id,
            rows.clone(),
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(outcome, ApplyMergeOutcome::Applied);

        let repo = Repository::open(&child_dir).unwrap();
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head_commit.parent_count(), 2, "must be a real two-parent merge commit");
        let parent_ids: std::collections::HashSet<git2::Oid> = head_commit.parent_ids().collect();
        assert!(parent_ids.contains(&ours_oid));
        assert!(parent_ids.contains(&theirs_oid));

        let tree = head_commit.tree().unwrap();
        let entry = tree.get_path(std::path::Path::new("transactions.csv")).unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        let content = std::str::from_utf8(blob.content()).unwrap();
        assert_eq!(
            content,
            allowance_core::codec::render_transactions(&rows),
            "the committed tree must hold exactly the merged rows, byte for byte"
        );
    }

    /// Defect 2's regression test. The crash marker proves a merge BEGAN,
    /// not that the tree's contents came from it: after the crash, the AWS
    /// transport's non-committing path can write MCP-authored rows into the
    /// same file. The previous behaviour hard-reset them away, and the AWS
    /// watermark had already advanced past them — permanent, silent loss.
    ///
    /// This test previously asserted the opposite ("the crash's garbage must
    /// be gone"), which is why the defect shipped.
    #[test]
    fn an_mcp_row_written_after_a_crash_survives_the_next_merge() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\n",
            )])
            .with_marker()
            .build();
        let peer = peer.unwrap();

        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();

        // A well-formed MCP row lands in the dirty tree after the crash.
        let mcp_csv = format!(
            "id,child_id,date,description,amount,balance,type\n\
             in-mcp-1,{child_id},2026-02-01T12:00:00+00:00,MCP gift,7.00,17.00,income\n"
        );
        std::fs::write(child_dir.join("transactions.csv"), &mcp_csv).unwrap();

        let terminal = run_cycles_until_terminal(&mut app, &child_id, peer, 5);
        assert!(terminal.is_ok(), "must reach a terminal state, got trace: {terminal:?}");

        let repo = Repository::open(&child_dir).unwrap();
        let head_csv = {
            let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
            let entry = tree.get_path(std::path::Path::new("transactions.csv")).unwrap();
            let blob = repo.find_blob(entry.id()).unwrap();
            String::from_utf8(blob.content().to_vec()).unwrap()
        };
        assert!(
            head_csv.contains("MCP gift"),
            "the MCP row written after the crash must survive into committed history — \
             the AWS watermark has already advanced past it, so discarding it loses it for good"
        );
        assert!(
            !repo.path().join(crate::backend::sync::child_sync::MERGE_IN_PROGRESS_MARKER).exists(),
            "the stale marker must be cleared"
        );
    }

    /// Defect 1's regression test, added by Task 11 (not in its brief).
    ///
    /// The brief's `an_mcp_row_written_after_a_crash_survives_the_next_merge`
    /// above is defect 2's regression test, and defect 2 was already closed
    /// by Task 9 — so that test passes both with and without THIS task's
    /// change, and pins nothing about it. This one does not: it fails
    /// against the pre-Task-11 guard.
    ///
    /// An uncommitted DELETION of a tracked file is the cleanest instance of
    /// the stall. The old guard staged with `add_file` per allowlist entry
    /// and skipped any entry `!path.exists()`, so a deletion could never be
    /// staged at all: `commit_if_changed` returned `Ok(None)`, the merge path
    /// logged a warning and returned `DirtyTreeCommitted` having committed
    /// nothing, HEAD never moved, and the next cycle re-derived the identical
    /// divergence and refused identically — until the re-poll was suppressed
    /// and that child silently stopped syncing. `update_all` stages
    /// deletions, which is exactly why it replaced the allowlist here.
    #[test]
    fn an_uncommitted_deletion_is_committed_rather_than_stalling_the_merge_forever() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        // Deliberately NOT staged — the state `add_file` cannot represent.
        std::fs::remove_file(child_dir.join("transactions.csv")).unwrap();
        app.sync.status = SyncStatus::Idle;

        let outcome = app.apply_merge(
            &child_id,
            vec![],
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(
            outcome,
            ApplyMergeOutcome::DirtyTreeCommitted,
            "a dirty tree must refuse this stale merge, not apply it"
        );

        // The invariant defect 1 violated: progress, or an explanation.
        // Never neither. Under the old guard this failed on all three
        // counts — tree still dirty, HEAD unmoved, no notice.
        let repo = Repository::open(&child_dir).unwrap();
        super::test_support::assert_resolved_or_explained(&app, &repo, &child_id, ours_oid);

        let new_head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_ne!(new_head.id(), ours_oid, "the deletion must have been committed, advancing HEAD");
        assert_eq!(new_head.parent_count(), 1, "an ordinary local commit, not a merge commit");
        assert!(
            new_head.tree().unwrap().get_path(std::path::Path::new("transactions.csv")).is_err(),
            "the commit's tree must record the deletion"
        );
        assert_eq!(
            app.sync.status,
            SyncStatus::Idle,
            "a dirty-tree refusal that resolved is not a failure and must not be reported as one"
        );
    }

    /// The parse gate, added by Task 11 (not in its brief's test list).
    ///
    /// Atomic writes bound THIS app's corruption after the upgrade. They say
    /// nothing about a `transactions.csv` already torn on disk at upgrade
    /// time, damaged by a partial restore, or written by an older build —
    /// and with Task 9's hard reset gone, nothing else validates. Committing
    /// unparseable bytes would push them to the peer and break `read_rows`
    /// on BOTH machines: a one-machine corruption becomes a two-machine
    /// outage, strictly worse than the stall being fixed. So the guard
    /// refuses, says so durably, and leaves HEAD alone.
    #[test]
    fn an_unparseable_transactions_csv_is_refused_rather_than_committed_and_pushed() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        // A torn file: a well-formed header followed by a row whose date is
        // not RFC3339. `parse_transactions` refuses rather than guessing
        // (see `codec.rs`'s module doc — a fallback would make
        // read-modify-write non-idempotent and re-merge forever).
        let torn = "id,child_id,date,description,amount,balance,type\n\
             in-torn-1,kid,NOT-A-DATE,Torn,1.00,1.00,income\n";
        std::fs::write(child_dir.join("transactions.csv"), torn).unwrap();

        let outcome = app.apply_merge(
            &child_id,
            vec![],
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(
            outcome,
            ApplyMergeOutcome::Failed,
            "corrupt bytes must never be committed into history a peer will fetch"
        );

        let repo = Repository::open(&child_dir).unwrap();
        assert_eq!(
            repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours_oid,
            "HEAD must not have moved — nothing was committed"
        );
        let notice = app
            .sync
            .sync_failures
            .iter()
            .find(|n| n.child_id == child_id)
            .expect("a refusal must be surfaced durably, not merely logged");
        assert!(
            notice.message.contains("transactions.csv"),
            "the notice must name the file that could not be read: {}",
            notice.message
        );
        // Task 12 addition: refused, not repaired, not discarded — the file
        // on disk must be left exactly as found, byte for byte, for a human
        // to look at.
        assert_eq!(
            std::fs::read(child_dir.join("transactions.csv")).unwrap(),
            torn.as_bytes(),
            "the corrupt file must be left exactly as found"
        );
    }

    /// Review Critical: the parse gate's own motivating example used to slip
    /// straight through it.
    ///
    /// `parse_transactions` runs `csv::Reader` with headers ON, so it
    /// consumes the first line as a header and never validates it. A file
    /// truncated to one junk line therefore parses as `Ok` with zero rows,
    /// and the guard used to commit and push it. The peer then fetches it
    /// and its own `read_rows` does the identical thing — one header, zero
    /// rows — so the ledger reads as EMPTY on both machines with nothing
    /// erroring anywhere. That is the two-machine outage the gate exists to
    /// prevent, in its worst form: silent, total, and symmetric.
    ///
    /// `resolve_dirty_tree` now requires a zero-row file's first line to be
    /// the canonical `codec::HEADER`, which is the only thing
    /// `render_transactions` ever writes. A genuinely empty ledger is
    /// header-only and still commits (asserted below, so the check cannot
    /// be "fixed" by refusing every empty file).
    #[test]
    fn a_transactions_csv_truncated_to_a_single_junk_line_is_refused_not_pushed() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        // The brief's own example of corruption, verbatim. It parses `Ok`.
        let junk = "garbage-from-a-crash";
        assert!(
            allowance_core::codec::parse_transactions(junk).is_ok(),
            "precondition: the codec really does accept this as a header with zero rows — \
             that is the whole reason a successful parse cannot be the gate's only check"
        );
        std::fs::write(child_dir.join("transactions.csv"), junk).unwrap();

        let outcome = app.apply_merge(
            &child_id,
            vec![],
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(
            outcome,
            ApplyMergeOutcome::Failed,
            "a file that reads as an empty ledger must never be committed and pushed"
        );

        let repo = Repository::open(&child_dir).unwrap();
        assert_eq!(
            repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours_oid,
            "HEAD must not have moved — nothing was committed"
        );
        assert_eq!(
            std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap(),
            junk,
            "the corrupt file must be left exactly as found, for a human to look at"
        );
        assert!(
            app.sync.sync_failures.iter().any(|n| n.child_id == child_id),
            "the refusal must be surfaced durably, not merely logged"
        );
    }

    /// The other half of the check above: a header-only `transactions.csv`
    /// is a legitimately EMPTY ledger (every row deleted), not corruption,
    /// and must still commit. Without this, "refuse zero-row files" would be
    /// an equally wrong over-correction — it would stall any child whose
    /// history was legitimately cleared.
    #[test]
    fn a_header_only_transactions_csv_is_a_legitimately_empty_ledger_and_still_commits() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        // Exactly what `render_transactions(&[])` produces — the canonical
        // representation of "no transactions", asserted rather than typed
        // out, so a codec change cannot silently invalidate this test.
        let empty_ledger = allowance_core::codec::render_transactions(&[]);
        assert_eq!(
            empty_ledger.lines().next().unwrap(),
            allowance_core::codec::HEADER.join(","),
            "precondition: the canonical empty ledger is exactly the canonical header"
        );
        std::fs::write(child_dir.join("transactions.csv"), &empty_ledger).unwrap();

        app.sync.status = SyncStatus::Idle;
        let outcome = app.apply_merge(
            &child_id,
            vec![],
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(
            outcome,
            ApplyMergeOutcome::DirtyTreeCommitted,
            "an empty ledger is valid data and must be committed, not refused"
        );

        let repo = Repository::open(&child_dir).unwrap();
        let new_head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_ne!(new_head.id(), ours_oid, "HEAD must have advanced");
        let entry = new_head.tree().unwrap().get_path(std::path::Path::new("transactions.csv")).unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        assert_eq!(
            std::str::from_utf8(blob.content()).unwrap(),
            empty_ledger,
            "the committed tree must hold the empty ledger byte for byte"
        );
        assert!(
            app.sync.sync_failures.iter().all(|n| n.child_id != child_id),
            "committing an empty ledger is not a failure and must raise no notice"
        );
    }

    /// Task 12 coverage: every stall route, driven through
    /// `run_cycles_until_terminal` rather than a single `apply_merge` call —
    /// defect 1 was a LIVENESS failure that only shows itself across
    /// cycles, and every merge-path test before this task's own was
    /// single-shot.
    ///
    /// Defect 1, route 2: a deleted tracked file. `index.add_path` cannot
    /// stage a deletion, and the old staging loop skipped files that do not
    /// exist — so `git status` said dirty, staging produced nothing, and the
    /// merge refused on every future cycle forever.
    ///
    /// Deliberately uses `goals.csv` rather than `transactions.csv` — the
    /// file Task 11's `an_uncommitted_deletion_is_committed_rather_than_stalling_the_merge_forever`
    /// already deletes above, in a single `apply_merge` call. That existing
    /// test is not duplicated here: this one drives the real cycle loop and
    /// deletes a file the peer commit never touches at all — and THAT is
    /// exactly what currently defeats it. See the `#[ignore]` reason below.
    ///
    /// # Currently ignored — a real gap, but NOT defect 1, and NOT
    ///   introduced by this branch
    ///
    /// Review correction: this is not a liveness/stall failure. Because
    /// `with_peer_commit` here only touches `transactions.csv`, and nothing
    /// else advances local HEAD, `classify` sees this as a pure
    /// `Cycle::FastForward` (merge_base == our own HEAD), never `Diverged`.
    /// The fast-forward SUCCEEDS: peer data is ingested, HEAD advances, the
    /// cycle reaches a terminal state, and sync stays healthy. What is lost
    /// is the OUTBOUND direction — `apply_fast_forward` has no unconditional
    /// `working_tree_dirty` check (unlike `apply_merge`; see its call a few
    /// hundred lines up in this same file), only a REACTIVE one that fires
    /// when `checkout_tree` raises `GIT_ECONFLICT` because the SAME path is
    /// dirty locally AND touched by the incoming tree. Since the peer commit
    /// never touches `goals.csv`, checkout never conflicts, and the deleted
    /// `goals.csv` is never staged, committed, or pushed — the two machines
    /// silently disagree about it. Observed directly: `head_advanced=true,
    /// tree_clean=false, notice_present=false`.
    ///
    /// This self-heals the moment either side breaks the pure-fast-forward
    /// pattern: any local write on this machine creates a real divergence,
    /// which routes through `apply_merge`'s unconditional check and resolves
    /// it; any later peer commit that happens to touch `goals.csv` also
    /// forces the checkout conflict that triggers the reactive path. It
    /// persists indefinitely only on a machine that is receive-only for as
    /// long as the peer never touches this specific file — silent
    /// non-propagation of a local change, not a stall. Severity: Important,
    /// not Critical.
    ///
    /// Also pre-existing, not a regression on this branch: on the
    /// pre-branch code, `apply_fast_forward` called the marker-gated
    /// `recover_if_dirty`, which with no marker present returned `Clean`
    /// without inspecting the tree at all — the same observable behaviour.
    /// Recorded as a named follow-up in the design spec's "Scope
    /// boundaries" section (`docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`)
    /// rather than fixed here — out of this task's scope (test code only).
    /// Re-enable once `apply_fast_forward` gets an unconditional
    /// `working_tree_dirty` check analogous to `apply_merge`'s.
    #[test]
    #[ignore = "reveals a real, pre-existing gap (NOT defect 1, NOT a regression \
                on this branch): apply_fast_forward only discovers a dirty tree \
                via a checkout conflict on the SAME path the peer also changed, \
                never proactively. On a receive-only machine, a local deletion \
                unrelated to whatever the peer's commit touches is silently \
                never staged, committed, or pushed — the fast-forward itself \
                still succeeds and sync stays healthy. See this test's doc \
                comment; tracked in the design spec's Named follow-ups."]
    fn a_deleted_tracked_file_does_not_stall_sync() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_goal()
            .with_peer_commit(&[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\n",
            )])
            .with_deleted("goals.csv")
            .build();
        assert_progress(&mut app, &child_id, peer.unwrap());
    }

    /// A tracked file this app does not manage via any allowlist. Under the
    /// tracked-path staging (`index.update_all(["*"], ..)`) this RESOLVES
    /// rather than failing — it is already in pushed history, so committing
    /// its modification is not the hazard `FILES_THIS_APP_OWNS` guards
    /// against.
    ///
    /// This test is the general case (an arbitrary tracked file this app
    /// happens to know nothing about). The next test below,
    /// `a_dirty_tracked_but_unowned_parental_control_log_resolves_rather_than_stalling`,
    /// is the SAME shape against a specific, real filename
    /// (`parental_control_attempts.csv`) that this codebase's own
    /// `ownership_contract` test names as tracked-but-exempt — kept as a
    /// separate case because it anchors to a file this app actually
    /// produces, not an arbitrary stand-in.
    #[test]
    fn a_dirty_tracked_unowned_file_resolves_rather_than_stalling() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_peer_commit(&[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\n",
            )])
            .build();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();

        // TRACKED, not untracked — `working_tree_dirty` sets
        // include_untracked(false), so an untracked file would never enter
        // the guard and this test would pass for the wrong reason.
        std::fs::write(child_dir.join("notes.txt"), "committed once").unwrap();
        let gm = GitManager::new();
        gm.add_file(&child_dir, "notes.txt").unwrap();
        gm.commit(&child_dir, "track notes.txt").unwrap();
        std::fs::write(child_dir.join("notes.txt"), "now modified").unwrap();

        let repo = Repository::open(&child_dir).unwrap();
        assert!(
            crate::backend::sync::child_sync::working_tree_dirty(&repo).unwrap(),
            "precondition: the guard must actually be reached — a fixture that stops \
             reproducing this condition must fail loudly, not pass vacuously"
        );

        assert_resolves_cleanly(&mut app, &child_id, peer.unwrap());
    }

    /// Defect 1, route 1 — CORRECTED LABEL (see Task 12's brief correction).
    /// `parental_control_attempts.csv` is NOT a reachable production stall
    /// route: `ParentalControlService` passes the literal `"global"` at all
    /// three of its call sites, and `attempts_dir` maps `"global"` to the
    /// BASE data directory, not any child's directory — so in production
    /// this file never lands inside a child's git repo at all, and can never
    /// go dirty there.
    ///
    /// What IS real: `ParentalControlRepository::record_parental_control_attempt`
    /// called directly with a real child id (exactly what `ownership_contract`'s
    /// completeness test in `backend/sync/paths.rs` does, and what
    /// `ChildRepoFixture::with_parental_control_log` now does too) commits
    /// this file into a child's repo as tracked-but-unowned — `EXEMPT`, not
    /// `FILES_THIS_APP_OWNS`. This test pins that the guard resolves that
    /// specific, real filename the same way it resolves any other
    /// tracked-but-unowned file, without claiming any production code path
    /// reaches it today.
    ///
    /// Deviation from the brief: an independent local transaction is added
    /// after the fixture builds, exactly as
    /// `a_dirty_tracked_unowned_file_resolves_rather_than_stalling` above
    /// does for `notes.txt`. Without it, `with_peer_commit` here (touching
    /// only `transactions.csv`) makes this fixture a pure
    /// `Cycle::FastForward`, and — per the finding documented on the
    /// `#[ignore]`d tests below — `apply_fast_forward` never proactively
    /// checks `working_tree_dirty`, so a dirty `parental_control_attempts.csv`
    /// the peer commit never touches would be silently never staged,
    /// committed, or pushed — the fast-forward would still succeed and sync
    /// would stay healthy, same as those two. Forcing a genuine divergence
    /// here routes this through
    /// `apply_merge`, which DOES check unconditionally, and is what actually
    /// lets this test exercise (and pass on) the tracked-but-unowned-file
    /// staging fix this task is meant to pin.
    #[test]
    fn a_dirty_tracked_but_unowned_parental_control_log_resolves_rather_than_stalling() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_parental_control_log()
            .with_peer_commit(&[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\n",
            )])
            .with_dirty("parental_control_attempts.csv", "1,1234,false\n2,5678,false\n")
            .build();

        app.backend()
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Toy".to_string(),
                amount: -2.0,
                date: None,
            })
            .expect("create local transaction to force a genuine divergence");

        assert_resolves_cleanly(&mut app, &child_id, peer.unwrap());
    }

    /// Defect 1, route 3: `delete_allowance_config` removes a tracked owned
    /// file with no commit — user-triggerable, no crash and no swallowed
    /// error required. Found in panel review. Unlike the parental-control
    /// test above, this one is deliberately left as the LITERAL,
    /// unconditioned scenario (no forced extra local commit) — that is the
    /// whole point of pinning it as a directly reachable route, not a
    /// mechanism demo.
    ///
    /// # Currently ignored — a real gap, but NOT defect 1, and NOT
    ///   introduced by this branch
    ///
    /// Review correction: this is not a liveness/stall failure. Same root
    /// cause as `a_deleted_tracked_file_does_not_stall_sync` above:
    /// `delete_allowance_config` never commits (it is a bare
    /// `std::fs::remove_file`, no `commit_file_change` call at all), and
    /// this fixture's peer commit only touches `transactions.csv`, so
    /// `classify` sees a pure `Cycle::FastForward`. The fast-forward
    /// SUCCEEDS — peer data is ingested, HEAD advances, the cycle reaches a
    /// terminal state, sync stays healthy. What is lost is the OUTBOUND
    /// direction: `apply_fast_forward` only discovers a dirty tree
    /// REACTIVELY via a checkout conflict on a path the incoming tree also
    /// touches, and never touches `allowance_config.yaml` here, so the
    /// checkout never conflicts and the deleted config is never staged,
    /// committed, or pushed — the two machines silently disagree about it.
    /// Observed directly: `head_advanced=true, tree_clean=false,
    /// notice_present=false`.
    ///
    /// This self-heals the moment either side breaks the pure-fast-forward
    /// pattern (any local write here creates a real divergence that
    /// `apply_merge`'s unconditional check resolves; any later peer commit
    /// touching `allowance_config.yaml` forces the reactive checkout-conflict
    /// path instead). It persists indefinitely only on a machine that is
    /// receive-only for as long as the peer never touches this file — silent
    /// non-propagation of a local change, not a stall. Severity: Important,
    /// not Critical. This is still the most notable of the two: route 3 was
    /// flagged in panel review as directly user-triggerable with no crash
    /// required, and is confirmed here to still silently drop the deletion
    /// on a receive-only machine.
    ///
    /// Also pre-existing, not a regression on this branch — see the
    /// parallel note on `a_deleted_tracked_file_does_not_stall_sync` above
    /// (pre-branch `apply_fast_forward` called the marker-gated
    /// `recover_if_dirty`, which returned `Clean` with no marker present,
    /// without inspecting the tree). Recorded as a named follow-up in the
    /// design spec's "Scope boundaries" section
    /// (`docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`)
    /// rather than fixed here — out of this task's scope (test code only).
    /// Re-enable once `apply_fast_forward` gets an unconditional
    /// `working_tree_dirty` check analogous to `apply_merge`'s.
    #[test]
    #[ignore = "reveals a real, pre-existing gap (NOT defect 1, NOT a regression \
                on this branch): apply_fast_forward only discovers a dirty tree \
                via a checkout conflict on the SAME path the peer also changed, \
                never proactively. On a receive-only machine, deleting the \
                allowance config is silently never staged, committed, or \
                pushed — the fast-forward itself still succeeds and sync stays \
                healthy. Route 3 is directly user-triggerable with no crash \
                required, per panel review. See this test's doc comment; \
                tracked in the design spec's Named follow-ups."]
    fn deleting_the_allowance_config_does_not_stall_sync() {
        let (mut app, child_id, _temp, peer) = ChildRepoFixture::new()
            .with_allowance_config()
            .with_peer_commit(&[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\n",
            )])
            .build();
        app.backend()
            .allowance_service
            .delete_allowance_config(&child_id)
            .expect("delete allowance config");
        assert_progress(&mut app, &child_id, peer.unwrap());
    }

    fn assert_progress(app: &mut AllowanceTrackerApp, child_id: &str, peer: git2::Oid) {
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id))
            .unwrap();
        let head_before =
            Repository::open(&child_dir).unwrap().head().unwrap().peel_to_commit().unwrap().id();

        match run_cycles_until_terminal(app, child_id, peer, 5) {
            Ok(_) => {}
            Err(trace) => panic!(
                "sync never reached a terminal state in 5 cycles — this is the stall. Trace:\n{}",
                trace.join("\n")
            ),
        }

        let repo = Repository::open(&child_dir).unwrap();
        assert_resolved_or_explained(app, &repo, child_id, head_before);
    }

    /// Stronger than `assert_progress`. `assert_resolved_or_explained`
    /// (which backs `assert_progress`) deliberately treats "refused and
    /// explained via a failure notice" as an equally acceptable outcome to
    /// "actually resolved" — correct for the stall-route tests above, where
    /// either is fine as long as the child does not silently stop syncing.
    /// It is NOT correct for a test claiming a tracked-but-unowned file
    /// "resolves rather than stalling": that claim is falsified by a clean
    /// refusal-with-notice just as much as by a silent stall, and
    /// `assert_progress` alone cannot tell the two apart.
    ///
    /// This was not a hypothetical concern — Task 12 Step 4 caught it
    /// directly: BOTH `a_dirty_tracked_unowned_file_resolves_rather_than_stalling`
    /// and `a_dirty_tracked_but_unowned_parental_control_log_resolves_rather_than_stalling`
    /// still reported `Ok` under `assert_progress` against the OLD
    /// owned-allowlist staging, because the old code correctly refuses to
    /// stage a file outside `FILES_THIS_APP_OWNS`, that refusal is reported
    /// as an ordinary `ApplyMergeOutcome::Failed` -> a sync-failure notice,
    /// and a notice satisfies "explained" — passing for the wrong reason,
    /// exactly what this task's own brief warns about. This helper closes
    /// that hole by additionally requiring a genuinely clean tree AND no
    /// failure notice for this child.
    ///
    /// Review Important: a stricter helper must be STRICTLY STRONGER than
    /// the weaker one it replaces, not merely different. `assert_resolved_or_explained`
    /// treats "HEAD advanced" as load-bearing (see its own doc comment: the
    /// weaker "tree is clean or a notice exists" form "holds vacuously if
    /// the guard commits something unrelated and leaves the real problem
    /// for the next cycle") — this helper carries that same check forward
    /// rather than silently dropping it. It was harmless to omit today only
    /// because "tree clean" already implies progress happened in every
    /// scenario this helper is actually used for; that is not a reason for
    /// the helper itself to be weaker than its sibling on paper.
    fn assert_resolves_cleanly(app: &mut AllowanceTrackerApp, child_id: &str, peer: git2::Oid) {
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id))
            .unwrap();
        let head_before =
            Repository::open(&child_dir).unwrap().head().unwrap().peel_to_commit().unwrap().id();

        match run_cycles_until_terminal(app, child_id, peer, 5) {
            Ok(_) => {}
            Err(trace) => panic!(
                "sync never reached a terminal state in 5 cycles — this is the stall. Trace:\n{}",
                trace.join("\n")
            ),
        }

        let repo = Repository::open(&child_dir).unwrap();
        let head_now = repo.head().unwrap().peel_to_commit().unwrap().id();
        assert_ne!(
            head_now, head_before,
            "HEAD must actually advance — a no-op cycle that leaves the dirty content \
             uncommitted but coincidentally reports a clean tree is not a resolution"
        );
        assert!(
            !crate::backend::sync::child_sync::working_tree_dirty(&repo).unwrap(),
            "the tree must actually become clean — a refusal explained via a failure notice is \
             not what \"resolves rather than stalling\" claims"
        );
        assert!(
            app.sync.sync_failures.iter().all(|n| n.child_id != child_id),
            "this must resolve outright, not be refused and reported as a failure"
        );
    }

    /// Every combination of file state the guard can meet ON THE MERGE
    /// PATH — see the name and the "Review correction" section below for
    /// why that scope qualifier is load-bearing. Deterministic and
    /// exhaustive: four owned files plus a tracked-unowned representative
    /// (`parental_control_attempts.csv` — real EXEMPT filename, not a
    /// production-reachable route; see the correction on
    /// `a_dirty_tracked_but_unowned_parental_control_log_resolves_rather_than_stalling`
    /// above), each unchanged / modified / deleted. A proptest/shrinker was
    /// deliberately not used: this state space is small and fully
    /// enumerable, and a shrinker would hand back a misleadingly minimal
    /// case instead of the full table.
    ///
    /// Only three states, not the spec's four: `Created` (a new, untracked
    /// file) is not reachable through this guard at all.
    /// `working_tree_dirty` sets `include_untracked(false)`, so a brand-new
    /// untracked file never makes the tree "dirty" in the sense this guard
    /// checks — there is nothing to stage or resolve, by design, not an
    /// oversight in this table.
    ///
    /// # Deviation from the brief: every case forces a genuine divergence,
    ///   so this table covers the MERGE path only
    ///
    /// After each fixture builds, an unrelated local commit is grafted on
    /// directly (via `commit_with_files`, bypassing any domain service so it
    /// cannot disturb whichever file/state is under test) to move local HEAD
    /// to a sibling of the peer tip. Without this, `with_peer_commit` here
    /// only touches `transactions.csv`, so every OTHER file's dirty/deleted
    /// state would make this a pure `Cycle::FastForward` — and, per the
    /// finding documented on `a_deleted_tracked_file_does_not_stall_sync`
    /// and `deleting_the_allowance_config_does_not_stall_sync` above,
    /// `apply_fast_forward` has no unconditional dirty-tree check, only a
    /// reactive one that fires solely when the peer's commit touches the
    /// SAME path. 8 of these 15 cases (every file but `transactions.csv`, in
    /// its `Modified`/`Deleted` states) would silently fail to propagate
    /// that way — not because the guard (`resolve_dirty_tree`, invoked from
    /// `apply_merge`'s unconditional `working_tree_dirty` check) is broken,
    /// but because the fast-forward path never reaches it. Forcing
    /// divergence here routes every case through the path that DOES call
    /// the guard unconditionally, so this table exhaustively proves what
    /// Tasks 9-11 actually built on the merge path — but, precisely because
    /// every case is forced onto that path, it can NEVER catch a regression
    /// on the fast-forward path. That gap is carried by the two
    /// `#[ignore]`d tests above (and the design spec's Named follow-ups)
    /// instead. Renamed (review Important-2) from
    /// `..._resolves_or_explains_...` to `..._resolves_..._on_the_merge_path`
    /// so the name cannot be read as covering more than it does — it also
    /// now agrees with the body, which asserts the strict
    /// `assert_resolves_cleanly`, not the "or explains" invariant.
    ///
    /// # A silently-elevated invariant, made explicit
    ///
    /// This table asserts as a SUCCESS case committing and pushing an
    /// unparseable `child.yaml` / `goals.csv` / `allowance_config.yaml`
    /// (`State::Modified` writes `"modified: true\n"` into each, which is
    /// not valid YAML for the first two and not meaningful CSV for
    /// `goals.csv`). That is correct, specified behaviour, not a gap this
    /// table is failing to catch: per the design spec §3, only
    /// `transactions.csv` is parse-validated by `resolve_dirty_tree` — the
    /// other owned files are staged and committed as opaque bytes. Calling
    /// that out here so a future reader does not mistake "this table
    /// accepts it" for "nobody thought about it."
    #[test]
    fn the_guard_resolves_every_dirty_tree_shape_on_the_merge_path() {
        #[derive(Debug, Clone, Copy)]
        enum State {
            Unchanged,
            Modified,
            Deleted,
        }

        const FILES: &[&str] = &[
            "transactions.csv",
            "goals.csv",
            "child.yaml",
            "allowance_config.yaml",
            "parental_control_attempts.csv",
        ];

        for file in FILES {
            for state in [State::Unchanged, State::Modified, State::Deleted] {
                // `child.yaml` deleted is excluded, not skipped quietly: per
                // `ChildRepository::delete_child`'s doc comment,
                // `child_dir()` DELIBERATELY refuses to resolve once
                // `child.yaml` is missing — "a folder that has lost its
                // child.yaml is precisely the damaged state a user reaches
                // for delete to clean up." That is not this guard's job:
                // `run_cycles_until_terminal` itself calls `child_dir()` on
                // every cycle and would panic on `.unwrap()` before ever
                // reaching `classify`, confirmed by actually running this
                // case. This is a real, by-design boundary (self-healing a
                // vanished `child.yaml` is explicitly a delete-and-recreate
                // flow, not a sync concern), not a gap in the dirty-tree
                // guard, so it is excluded rather than folded into the
                // strict "resolves cleanly" assertion the other 14 cells share.
                if *file == "child.yaml" && matches!(state, State::Deleted) {
                    continue;
                }

                let mut fixture = ChildRepoFixture::new()
                    .with_goal()
                    .with_allowance_config()
                    .with_parental_control_log()
                    .with_peer_commit(&[(
                        "transactions.csv",
                        "id,child_id,date,description,amount,balance,type\n",
                    )]);
                fixture = match state {
                    State::Unchanged => fixture,
                    // Content that still parses — the unparseable case has
                    // its own test with its own expected outcome.
                    State::Modified if *file == "transactions.csv" => fixture.with_dirty(
                        file,
                        "id,child_id,date,description,amount,balance,type\n",
                    ),
                    State::Modified => fixture.with_dirty(file, "modified: true\n"),
                    State::Deleted => fixture.with_deleted(file),
                };

                let (mut app, child_id, _temp, peer) = fixture.build();
                let child_dir = app
                    .backend()
                    .csv_connection
                    .child_dir(&shared::ChildId::from(child_id.as_str()))
                    .unwrap();

                // Force a genuine divergence unrelated to `file` — see this
                // test's doc comment for why.
                let repo = Repository::open(&child_dir).unwrap();
                let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
                let ours_commit = repo.find_commit(ours_oid).unwrap();
                let diverged_oid = commit_with_files(
                    &repo,
                    "local unrelated change",
                    &[&ours_commit],
                    &[("local_marker.txt", "x")],
                    1_700_000_600,
                );
                let branch = super::current_branch(&repo).unwrap();
                repo.reference(
                    &format!("refs/heads/{branch}"),
                    diverged_oid,
                    true,
                    "force divergence for cross-product test",
                )
                .unwrap();

                // Deliberately the STRICT check (`assert_resolves_cleanly`),
                // not the weaker `assert_resolved_or_explained` the named
                // stall-route tests use. A "refused and explained via a
                // failure notice" outcome would satisfy the weak invariant
                // for every file/state combination here too (confirmed by
                // Task 12 Step 4: with the OLD owned-allowlist staging
                // restored, a deleted owned file or a dirty
                // `parental_control_attempts.csv` is refused as
                // `ApplyMergeOutcome::Failed` with a notice — "explained,"
                // but not what a table named "resolves" should accept as
                // success). The strict check is what actually distinguishes
                // the tracked-path staging fix from the old allowlist, and
                // it is achievable for every one of these 14 cases because
                // the forced divergence above always routes through
                // `apply_merge`'s unconditional `working_tree_dirty` check.
                assert_resolves_cleanly(&mut app, &child_id, peer.unwrap());
            }
        }
    }

    /// CRITICAL-1 regression: Task 16 made the AWS apply path
    /// (`upsert_transaction_no_commit`, reached here via
    /// `apply_remote_entity`) deliberately non-committing, so an ordinary
    /// dirty `transactions.csv` with NO interrupted-merge marker present is
    /// this design's STEADY STATE, not a crash. `clear_interrupted_merge_marker`
    /// never even looks at the tree in this no-marker case — but the merge
    /// write used to then bulldoze it unconditionally anyway, silently erasing any MCP-
    /// authored row written since the tips this merge was computed
    /// against. Once the AWS watermark advances past that row it can never
    /// be re-fetched, so that used to be permanent, silent data loss.
    ///
    /// This is the fix: the dirty row must survive (committed as an
    /// ordinary local commit, never overwritten by the stale merge's
    /// rows), and the merge itself must be refused and rescheduled rather
    /// than applied — exactly the shape `ApplyMergeOutcome::StaleHead`
    /// already uses for the same reason (HEAD moving out from under an
    /// already-computed merge is not a fault).
    #[test]
    fn an_mcp_authored_dirty_row_survives_a_merge_which_is_discarded_and_rescheduled() {
        use crate::backend::domain::models::transaction::{Transaction as DomainTransaction, TransactionType};
        use shared::sync::EntityType;

        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        // Simulate the AWS transport's non-committing apply path writing an
        // MCP-authored row AFTER `ours_oid` was computed as this merge's
        // base. No crash-recovery marker is written — this is NOT a crash,
        // just the ordinary uncommitted write Task 16 made deliberate.
        assert!(
            !repo.path().join(crate::backend::sync::child_sync::MERGE_IN_PROGRESS_MARKER).exists(),
            "precondition: no crash-recovery marker present"
        );
        let mcp_row = DomainTransaction {
            id: DomainTransaction::generate_id(Money::from_cents(700), 1),
            child_id: child_id.clone(),
            date: chrono::DateTime::parse_from_rfc3339("2026-02-01T12:00:00Z").unwrap(),
            description: "MCP gift".to_string(),
            amount: Money::from_cents(700),
            balance: Money::from_cents(1700),
            transaction_type: TransactionType::OneOffIncome,
        };
        let json = serde_json::to_string(&mcp_row).unwrap();
        app.apply_remote_entity(&child_id, &EntityType::Transaction, &mcp_row.id, &json, "evt-mcp-1");

        let repo = Repository::open(&child_dir).unwrap();
        assert_eq!(
            repo.head().unwrap().peel_to_commit().unwrap().id(),
            ours_oid,
            "precondition: the MCP write must not have moved HEAD (non-committing path)"
        );
        let dirty_on_disk = std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();
        assert!(dirty_on_disk.contains("MCP gift"), "precondition: the MCP row is on disk uncommitted");

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SyncCommand>();
        app.sync_command_tx = Some(cmd_tx);
        app.sync.status = SyncStatus::Idle;

        // This merge's `rows` were computed against `ours_oid`/`theirs_oid`
        // — before the MCP row landed. Applying them unconditionally would
        // overwrite the dirty file above and erase the MCP row for good.
        let stale_rows = vec![a_row(&child_id, "in-1-a", "Merged Allowance", 1000)];
        let outcome = app.apply_merge(
            &child_id,
            stale_rows,
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(
            outcome,
            ApplyMergeOutcome::DirtyTreeCommitted,
            "a dirty transactions.csv must refuse this stale merge, not apply it"
        );

        // Never surfaced as an error — this is the safety guard working as
        // designed, same as a stale-head refusal.
        assert_eq!(
            app.sync.status,
            SyncStatus::Idle,
            "a dirty-tree refusal must never be reported as SyncStatus::Error"
        );

        // Rescheduled: the caller must re-run the cycle from the new HEAD.
        assert!(
            matches!(cmd_rx.try_recv(), Ok(SyncCommand::PollNow)),
            "discarding a merge for a dirty tree must trigger an immediate re-poll"
        );

        // The MCP row must still be on disk in the working tree...
        let repo = Repository::open(&child_dir).unwrap();
        let on_disk = std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();
        assert!(
            on_disk.contains("MCP gift"),
            "the MCP-authored row must survive, not be erased by the stale merge's write: {on_disk}"
        );
        assert!(
            !on_disk.contains("Merged Allowance"),
            "the stale merge's own rows must never have been written at all"
        );

        // ...AND committed: HEAD must have moved to a new, single-parent
        // commit whose tree holds it — the dirty tree was durably
        // committed, not merely left dirty for the next crash to find.
        let new_head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_ne!(new_head.id(), ours_oid, "the dirty tree must have been committed, advancing HEAD");
        assert_eq!(new_head.parent_count(), 1, "an ordinary local commit, not a merge commit");
        assert_eq!(new_head.parent_id(0).unwrap(), ours_oid);
        let tree = new_head.tree().unwrap();
        let entry = tree.get_path(std::path::Path::new("transactions.csv")).unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        let content = std::str::from_utf8(blob.content()).unwrap();
        assert!(
            content.contains("MCP gift"),
            "the new commit's tree must hold the MCP row, not just the working tree: {content}"
        );
    }

    /// Important-3, bullet 3 (folded into the same scenario as above): there
    /// is no `lgs` remote configured for this repo, so `push_lgs` inside
    /// `apply_merge` necessarily fails. The merge commit above was still
    /// created correctly and the working tree was not corrupted by the
    /// failed push — asserted here as its own test so a future change
    /// cannot silently make push failure block or roll back the commit.
    #[test]
    fn a_push_failure_does_not_corrupt_the_working_tree_or_block_the_commit() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        assert!(repo.find_remote("lgs").is_err(), "precondition: no lgs remote configured");
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        let rows = vec![a_row(&child_id, "in-1-a", "Merged Allowance", 1000)];
        let outcome =
            app.apply_merge(&child_id, rows.clone(), &(ours_oid.to_string(), theirs_oid.to_string()), &[]);

        // Push had nothing to push to and must have failed silently from the
        // caller's point of view (logged, not fatal) — the apply itself
        // still succeeded and the tree still holds the merged content.
        assert_eq!(outcome, ApplyMergeOutcome::Applied);
        let on_disk = std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();
        assert_eq!(on_disk, allowance_core::codec::render_transactions(&rows));
    }

    /// Goals.csv handling: the divergence must reach the UI as a durable
    /// NOTICE (`sync.goals_diverged`), never as `sync.status` — a status is
    /// last-writer-wins and would be erased by the very next unrelated sync
    /// event, which is exactly wrong for something the user still needs to
    /// see after their goals failed to merge.
    #[test]
    fn goals_csv_divergence_is_recorded_as_a_persistent_notice_not_a_status() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        // `theirs` carries a goals.csv that `ours` does not have at all --
        // present on one side, absent on the other counts as diverged.
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[
                ("transactions.csv", "id,child_id,date,description,amount,balance,type\n"),
                ("goals.csv", "id,child_id,description,target\ng1,x,Bike,100.00\n"),
            ],
            1_700_000_500,
        );

        // Status starts as something a goals notice must not disturb.
        app.sync.status = SyncStatus::Idle;

        let outcome =
            app.apply_merge(&child_id, vec![], &(ours_oid.to_string(), theirs_oid.to_string()), &[]);
        assert_eq!(outcome, ApplyMergeOutcome::Applied, "the transactions merge still succeeds");

        assert_eq!(
            app.sync.status,
            SyncStatus::Idle,
            "a goals notice must never be reported through sync.status"
        );

        let notice = app
            .sync
            .goals_diverged
            .iter()
            .find(|n| n.child_id == child_id)
            .expect("the goals divergence must be recorded as a persistent notice");
        assert_eq!(notice.ours_oid, ours_oid.to_string());
        assert_eq!(notice.theirs_oid, theirs_oid.to_string());
    }

    /// A second, later divergence for the SAME child replaces its notice
    /// rather than piling up duplicates while the condition remains
    /// unresolved across repeated sync cycles.
    #[test]
    fn a_repeated_goals_divergence_for_the_same_child_replaces_its_notice() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[
                ("transactions.csv", "id,child_id,date,description,amount,balance,type\n"),
                ("goals.csv", "id,child_id,description,target\ng1,x,Bike,100.00\n"),
            ],
            1_700_000_500,
        );
        app.apply_merge(&child_id, vec![], &(ours_oid.to_string(), theirs_oid.to_string()), &[]);
        assert_eq!(app.sync.goals_diverged.iter().filter(|n| n.child_id == child_id).count(), 1);

        // A later cycle re-detects the (still unresolved) divergence at a
        // new pair of tips.
        let repo2 = Repository::open(&child_dir).unwrap();
        let head_oid = repo2.head().unwrap().peel_to_commit().unwrap().id();
        let head_commit = repo2.find_commit(head_oid).unwrap();
        let theirs2_oid = commit_with_files(
            &repo2,
            "their second edit",
            &[&head_commit],
            &[
                ("transactions.csv", "id,child_id,date,description,amount,balance,type\n"),
                ("goals.csv", "id,child_id,description,target\ng1,x,Skateboard,60.00\n"),
            ],
            1_700_000_600,
        );
        app.apply_merge(&child_id, vec![], &(head_oid.to_string(), theirs2_oid.to_string()), &[]);

        let matches: Vec<_> =
            app.sync.goals_diverged.iter().filter(|n| n.child_id == child_id).collect();
        assert_eq!(matches.len(), 1, "must replace, not accumulate, duplicate notices");
        assert_eq!(matches[0].theirs_oid, theirs2_oid.to_string());
    }

    /// CRITICAL-2 regression: HEAD moved (an ordinary local write raced the
    /// background sync) between when this merge's `parents.0` was computed
    /// and this call applying it. Applying anyway would stage the merged
    /// CSV over the interloping commit's content and hand `commit_merge`
    /// two parents that do not include it — orphaning a real user
    /// transaction with no error. This must be refused, not applied.
    #[test]
    fn refuses_a_merge_when_head_moved_since_it_was_computed() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        // This is the HEAD `cycle()` would have seen and computed the merge
        // against.
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        // An ordinary local write races the sync cycle: HEAD advances past
        // `ours_oid` before the already-computed merge is applied.
        app.backend()
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Interloper".to_string(),
                amount: 5.0,
                date: None,
            })
            .expect("interloping transaction");

        let repo_after = Repository::open(&child_dir).unwrap();
        let interloper_head = repo_after.head().unwrap().peel_to_commit().unwrap().id();
        assert_ne!(interloper_head, ours_oid, "precondition: HEAD must have moved");

        // A status a stale-head refusal must not disturb -- it is not an
        // error, so it must not clobber whatever status already holds.
        app.sync.status = SyncStatus::Idle;

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SyncCommand>();
        app.sync_command_tx = Some(cmd_tx);

        let outcome = app.apply_merge(
            &child_id,
            vec![],
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(outcome, ApplyMergeOutcome::StaleHead, "a stale-base merge must be refused");

        // Item 1: the refusal must trigger an immediate re-poll, or the
        // refused merge is simply dropped until the next timer tick.
        assert!(
            matches!(cmd_rx.try_recv(), Ok(SyncCommand::PollNow)),
            "a stale-head refusal must send SyncCommand::PollNow to re-run the cycle"
        );

        // Item 2: this is the safety guard working as designed, not a
        // fault -- it must NOT be reported as SyncStatus::Error.
        assert_eq!(
            app.sync.status,
            SyncStatus::Idle,
            "a stale-head refusal must never be reported as SyncStatus::Error"
        );

        // HEAD must be untouched: the interloping commit survives, not
        // silently overwritten or orphaned by a merge commit that does not
        // have it as a parent.
        let repo_final = Repository::open(&child_dir).unwrap();
        let final_head = repo_final.head().unwrap().peel_to_commit().unwrap().id();
        assert_eq!(
            final_head, interloper_head,
            "the interloping commit must survive untouched — refusing must not rewrite HEAD"
        );

        // The interloper's transaction must still be present on disk — it
        // was never overwritten by the stale merge's CSV.
        let on_disk = std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();
        assert!(
            on_disk.contains("Interloper"),
            "the interloping transaction must not have been overwritten: {on_disk}"
        );
    }

    /// If no `sync_command_tx` is wired (e.g. sync disabled), a stale-head
    /// refusal must still refuse the merge safely rather than panicking on
    /// the missing channel.
    #[test]
    fn stale_head_refusal_is_safe_with_no_command_channel_wired() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();
        let theirs_oid = commit_with_files(
            &repo,
            "their edit",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );
        app.backend()
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Interloper".to_string(),
                amount: 5.0,
                date: None,
            })
            .expect("interloping transaction");

        assert!(app.sync_command_tx.is_none(), "precondition: no channel wired");
        let outcome = app.apply_merge(
            &child_id,
            vec![],
            &(ours_oid.to_string(), theirs_oid.to_string()),
            &[],
        );
        assert_eq!(outcome, ApplyMergeOutcome::StaleHead);
    }
}

/// Review Critical-1: `CycleOutcome::FastForward` used to have no consumer
/// at all (a `log::warn!` and nothing else) even though it is the ORDINARY
/// case — `classify` returns it whenever the peer advanced and we made no
/// local commits, which is what happens on every machine that is not the
/// one editing right now. These tests exercise `apply_fast_forward`
/// directly (mirroring `apply_merge_tests`'s own style — real git repos in
/// tempdirs, no `lgs` binary, no daemon, no network) and check the ACTUAL
/// working-tree file content on disk, not merely that a message shape was
/// produced.
#[cfg(test)]
mod apply_fast_forward_tests {
    use super::{ApplyFastForwardOutcome, SyncStatus};
    use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
    /// Stand-in for "content the AWS transport wrote to `transactions.csv`
    /// and deliberately did not commit" — the ordinary steady state these
    /// tests exercise staging and exclusion against.
    ///
    /// Review fix: these tests used to use a single junk line
    /// (`"uncommitted-aws-row"`) as this stand-in, which was fine when the
    /// guard only staged files, and is not now that it parse-validates them.
    /// Junk of exactly that shape is what `resolve_dirty_tree`'s
    /// zero-row/header check is there to REFUSE (see its doc comment), so
    /// using it here would have quietly turned these into tests of the
    /// refusal path instead of the staging path they are named for. They
    /// test staging and exclusion, not parsing, so they get well-formed
    /// content — the gate is not weakened to accommodate them.
    const UNCOMMITTED_AWS_ROW: &str = "id,child_id,date,description,amount,balance,type\n\
         in-aws-1,test-kid,2026-03-01T09:00:00+00:00,Uncommitted AWS row,4.00,14.00,income\n";
    use crate::backend::domain::commands::transactions::CreateTransactionCommand;
    use crate::backend::domain::SyncCommand;
    use crate::backend::Backend;
    use crate::ui::app_state::AllowanceTrackerApp;
    use git2::Repository;

    /// Commit directly against a repository's object database (no working
    /// tree or index touched) — plants a "peer's" further commit without
    /// ever checking it out, exactly mirroring what `fetch_lgs` would have
    /// landed at `refs/remotes/lgs-auth/main` in production.
    ///
    /// Task 10 review fix: this tree IS checked out for real by
    /// `apply_fast_forward` in these tests, so seeding the builder from an
    /// empty tree (dropping `child.yaml`/`goals.csv`/`allowance_config.yaml`
    /// from the peer commit) meant `a_fast_forward_checks_out_the_new_tree_onto_disk`
    /// was deleting `child.yaml` from the child's directory on every run and
    /// passing anyway, because nothing downstream re-resolves `child_dir`.
    /// Seeding from the first parent's tree instead carries those files over
    /// unchanged, matching what a real peer commit looks like.
    fn commit_with_files(
        repo: &Repository,
        message: &str,
        parents: &[&git2::Commit],
        files: &[(&str, &str)],
        timestamp: i64,
    ) -> git2::Oid {
        let sig =
            git2::Signature::new("Test", "test@example.com", &git2::Time::new(timestamp, 0)).unwrap();
        let base_tree = parents.first().map(|c| c.tree().unwrap());
        let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
        for (name, content) in files {
            let blob_id = repo.blob(content.as_bytes()).unwrap();
            builder.insert(*name, blob_id, 0o100644).unwrap();
        }
        let tree_id = builder.write().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(None, &sig, &sig, message, &tree, parents).unwrap()
    }

    fn app_with_git_backed_child() -> (AllowanceTrackerApp, String, tempfile::TempDir) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), None).expect("backend");
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");
        backend
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Allowance".to_string(),
                amount: 10.0,
                date: None,
            })
            .expect("create transaction");
        let app = AllowanceTrackerApp::new_for_test(backend);
        (app, child.id, temp)
    }

    /// The coordinator's explicit ask: prove a fast-forward genuinely
    /// updates the working tree, not merely that a message was handled.
    /// The peer's commit content must land on disk byte for byte, and HEAD
    /// must move to it.
    #[test]
    fn a_fast_forward_checks_out_the_new_tree_onto_disk() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        let peer_content =
            "id,child_id,date,description,amount,balance,type\nin-peer-a,x,2026-01-02T00:00:00+00:00,Peer Allowance,5.00,5.00,allowance\n";
        let ahead_oid = commit_with_files(
            &repo,
            "peer advanced",
            &[&ours_commit],
            &[("transactions.csv", peer_content)],
            1_700_000_500,
        );

        let outcome = app.apply_fast_forward(&child_id, &ahead_oid.to_string());
        assert_eq!(outcome, ApplyFastForwardOutcome::Applied);

        let repo = Repository::open(&child_dir).unwrap();
        let head_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        assert_eq!(head_oid, ahead_oid, "HEAD must move to the fast-forward target");

        let on_disk = std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();
        assert_eq!(
            on_disk, peer_content,
            "the peer's committed content must actually be checked out onto disk"
        );
    }

    #[test]
    fn fast_forwarding_to_the_current_head_is_a_no_op() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();

        let outcome = app.apply_fast_forward(&child_id, &ours_oid.to_string());
        assert_eq!(outcome, ApplyFastForwardOutcome::AlreadyUpToDate);
    }

    /// Same race `apply_merge` guards against: HEAD moved (a local commit
    /// was made) between `cycle_with_status` classifying this as a
    /// fast-forward and this call applying it, so `to` is no longer a
    /// descendant of the current tip. Must be refused, not applied blindly
    /// (which would silently discard the interloping local commit).
    #[test]
    fn a_fast_forward_is_refused_when_head_moved_since_it_was_classified() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        // The "peer" commit is a sibling of an INTERLOPING local commit,
        // not a descendant of it — once the interloper lands, the peer's
        // commit is no longer reachable by fast-forwarding from HEAD.
        let ahead_oid = commit_with_files(
            &repo,
            "peer advanced",
            &[&ours_commit],
            &[("transactions.csv", "id,child_id,date,description,amount,balance,type\n")],
            1_700_000_500,
        );

        app.backend()
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Interloper".to_string(),
                amount: 5.0,
                date: None,
            })
            .expect("interloping transaction");
        let repo_after = Repository::open(&child_dir).unwrap();
        let interloper_head = repo_after.head().unwrap().peel_to_commit().unwrap().id();
        assert_ne!(interloper_head, ours_oid, "precondition: HEAD must have moved");

        app.sync.status = SyncStatus::Idle;
        let outcome = app.apply_fast_forward(&child_id, &ahead_oid.to_string());
        assert_eq!(outcome, ApplyFastForwardOutcome::StaleHead);

        let repo_final = Repository::open(&child_dir).unwrap();
        assert_eq!(
            repo_final.head().unwrap().peel_to_commit().unwrap().id(),
            interloper_head,
            "the interloping commit must survive untouched"
        );
        assert!(
            std::fs::read_to_string(child_dir.join("transactions.csv"))
                .unwrap()
                .contains("Interloper"),
            "the interloping transaction must not have been overwritten"
        );
    }

    /// Task 17 Important-3's theme applied to fast-forward: an uncommitted
    /// local write (the AWS transport's ordinary steady state — see
    /// `apply_remote_entity`'s doc comment) that would conflict with the
    /// incoming tree must be left alone, not silently discarded by a forced
    /// checkout.
    #[test]
    fn a_fast_forward_blocked_by_uncommitted_local_changes_commits_them_rather_than_discarding_or_forcing() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        let ahead_oid = commit_with_files(
            &repo,
            "peer advanced",
            &[&ours_commit],
            &[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\nin-peer-a,x,2026-01-02T00:00:00+00:00,Peer,5.00,5.00,allowance\n",
            )],
            1_700_000_500,
        );

        // Simulate the AWS transport's ordinary uncommitted write:
        // transactions.csv modified on disk, HEAD untouched, no
        // crash-recovery marker (this is not a crash).
        std::fs::write(child_dir.join("transactions.csv"), UNCOMMITTED_AWS_ROW).unwrap();

        let outcome = app.apply_fast_forward(&child_id, &ahead_oid.to_string());
        assert_eq!(outcome, ApplyFastForwardOutcome::CommittedLocalChangesToUnblock);

        // Nothing was silently discarded, and nothing was force-checked-out
        // over: the uncommitted content is still exactly what it was, now
        // safely committed on top of the old HEAD.
        let repo_final = Repository::open(&child_dir).unwrap();
        let new_head = repo_final.head().unwrap().peel_to_commit().unwrap();
        assert_ne!(new_head.id(), ours_oid, "a new commit must have been created");
        assert_ne!(new_head.id(), ahead_oid, "must not have jumped straight to the fast-forward target");
        assert_eq!(
            new_head.parent_id(0).unwrap(),
            ours_oid,
            "the new commit's parent must be the old HEAD"
        );
        assert_eq!(
            std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap(),
            UNCOMMITTED_AWS_ROW,
            "the uncommitted local content must land in the commit byte for byte"
        );
    }

    /// Review round 4, Important-1: an untracked stray file (the kind macOS
    /// or an editor drops into a data directory) sitting alongside a
    /// legitimate uncommitted AWS write must NOT be swept into the unblock
    /// commit. `add_all(["*"])` would have picked this up and pushed it to
    /// the other machine permanently.
    ///
    /// Task 11 moved the staging rule from the `FILES_THIS_APP_OWNS`
    /// allowlist to `index.update_all`, so this assertion now rests on a
    /// different mechanism and matters more, not less: `update_all` only
    /// visits entries already in the index, so an untracked path is
    /// invisible to it. This test is what proves that is true of the real
    /// libgit2 call rather than merely assumed of it.
    #[test]
    fn an_untracked_stray_file_is_not_included_in_the_unblock_commit() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        let ahead_oid = commit_with_files(
            &repo,
            "peer advanced",
            &[&ours_commit],
            &[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\nin-peer-a,x,2026-01-02T00:00:00+00:00,Peer,5.00,5.00,allowance\n",
            )],
            1_700_000_500,
        );

        // The legitimate uncommitted AWS write that actually blocks the
        // fast-forward...
        std::fs::write(child_dir.join("transactions.csv"), UNCOMMITTED_AWS_ROW).unwrap();
        // ...alongside a stray file this app never wrote and does not own.
        std::fs::write(child_dir.join(".DS_Store"), b"not this app's business").unwrap();

        let outcome = app.apply_fast_forward(&child_id, &ahead_oid.to_string());
        assert_eq!(outcome, ApplyFastForwardOutcome::CommittedLocalChangesToUnblock);

        let repo_final = Repository::open(&child_dir).unwrap();
        let new_head = repo_final.head().unwrap().peel_to_commit().unwrap();
        let tree = new_head.tree().unwrap();
        assert!(
            tree.get_path(std::path::Path::new(".DS_Store")).is_err(),
            ".DS_Store must NOT be present in the unblock commit's tree"
        );
        // The file itself is untouched on disk — still untracked, not
        // deleted, not modified — this is purely a staging exclusion.
        assert_eq!(
            std::fs::read(child_dir.join(".DS_Store")).unwrap(),
            b"not this app's business"
        );

        // The legitimate content still landed correctly.
        let entry = tree.get_path(std::path::Path::new("transactions.csv")).unwrap();
        let blob = repo_final.find_blob(entry.id()).unwrap();
        assert_eq!(blob.content(), UNCOMMITTED_AWS_ROW.as_bytes());
    }

    /// Review round 4, Minor-3 + Important-2, combined: a checkout conflict
    /// caused ENTIRELY by an UNTRACKED file (`notes.txt`, which exists in
    /// the target tree but was never committed here) leaves nothing for
    /// `commit_dirty_tree_to_unblock_fast_forward` to legitimately stage —
    /// every tracked path is already byte-identical to HEAD. This must not
    /// produce a content-free commit (Minor-3's `commit_if_changed` guard),
    /// and the resulting failure must be surfaced durably (Important-2's
    /// `SyncFailureNotice`), not just written to `sync.status` where it
    /// would be silently clobbered by the very next `StatusChanged(Idle)`.
    ///
    /// This is the one case that still reaches `DirtyTreeError::NothingToCommit`
    /// — reachable only because `checkout_tree`'s conflict check considers
    /// untracked collisions, which `working_tree_dirty` (untracked-blind)
    /// does not. On the merge path, whose guard is gated by
    /// `working_tree_dirty`, that variant really is unreachable.
    #[test]
    fn a_conflict_from_an_untracked_file_is_not_committed_empty_and_is_surfaced_durably() {
        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        // Preserve transactions.csv's real (randomly-id'd) content so the
        // "ahead" commit's copy is byte-identical to what is already on
        // disk and in HEAD — nothing about it is actually changing.
        let unchanged_transactions =
            std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();

        let ahead_oid = commit_with_files(
            &repo,
            "peer advanced",
            &[&ours_commit],
            &[
                ("transactions.csv", &unchanged_transactions),
                ("notes.txt", "peer's notes"),
            ],
            1_700_000_500,
        );

        // Dirty ONLY a file this app does not own — an untracked path that
        // collides with what the target tree would introduce at
        // notes.txt. transactions.csv is left completely untouched.
        std::fs::write(child_dir.join("notes.txt"), "local notes, different from the peer's").unwrap();

        let outcome = app.apply_fast_forward(&child_id, &ahead_oid.to_string());
        assert_eq!(
            outcome,
            ApplyFastForwardOutcome::Failed,
            "a conflict with nothing legitimately stage-able must fail cleanly, not fabricate an \
             empty commit"
        );

        // No commit was created: HEAD must be exactly where it was.
        let repo_final = Repository::open(&child_dir).unwrap();
        assert_eq!(
            repo_final.head().unwrap().peel_to_commit().unwrap().id(),
            ours_oid,
            "HEAD must not have moved — nothing was legitimately committable"
        );

        // Surfaced durably, not just via sync.status (which would be
        // clobbered by the very next StatusChanged(Idle) in production).
        assert!(
            app.sync.sync_failures.iter().any(|n| n.child_id == child_id),
            "a genuine, unresolved checkout failure must be recorded as a durable notice"
        );
    }

    /// Review Important-1's explicit ask: prove this resolves rather than
    /// looping forever. The AWS transport's dirty-without-committing write
    /// is this design's own STEADY STATE (Task 16), so a machine that only
    /// ever receives MCP writes would hit `Cycle::FastForward` and a
    /// checkout refusal on every single tick if nothing changed the
    /// classification — an indefinite livelock. This asserts three things
    /// together: the blockage is surfaced (a notice, not silence), an
    /// immediate re-poll is requested (not left to the 30s timer), and —
    /// the actual proof of "does not loop forever" — the very next
    /// `classify` call for the same target now says `Diverged`, never
    /// `FastForward` again.
    #[test]
    fn a_dirty_tree_blocking_a_fast_forward_resolves_instead_of_refusing_forever() {
        use crate::backend::sync::child_sync::{classify, Cycle};

        let (mut app, child_id, _temp) = app_with_git_backed_child();
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&shared::ChildId::from(child_id.as_str()))
            .unwrap();
        let repo = Repository::open(&child_dir).unwrap();
        let ours_oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        let ours_commit = repo.find_commit(ours_oid).unwrap();

        let ahead_oid = commit_with_files(
            &repo,
            "peer advanced",
            &[&ours_commit],
            &[(
                "transactions.csv",
                "id,child_id,date,description,amount,balance,type\nin-peer-a,x,2026-01-02T00:00:00+00:00,Peer,5.00,5.00,allowance\n",
            )],
            1_700_000_500,
        );
        std::fs::write(child_dir.join("transactions.csv"), UNCOMMITTED_AWS_ROW).unwrap();

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SyncCommand>();
        app.sync_command_tx = Some(cmd_tx);

        let outcome = app.apply_fast_forward(&child_id, &ahead_oid.to_string());
        assert_eq!(outcome, ApplyFastForwardOutcome::CommittedLocalChangesToUnblock);

        // Surfaced: a visible notice, not just a log line.
        assert!(
            app.sync
                .fast_forward_blocked
                .iter()
                .any(|n| n.child_id == child_id && n.to == ahead_oid.to_string()),
            "a blocked-and-resolved fast-forward must be recorded as a visible notice"
        );

        // Prompt, not left to the 30s timer.
        assert!(
            matches!(cmd_rx.try_recv(), Ok(SyncCommand::PollNow)),
            "committing to unblock a fast-forward must trigger an immediate re-poll"
        );

        // The actual "does not loop forever" proof: the next cycle's
        // classification against this SAME target must be Diverged, never
        // FastForward again — this is what routes to the merge path
        // instead of hitting the identical refusal on every future tick.
        let repo_final = Repository::open(&child_dir).unwrap();
        let new_head_oid = repo_final.head().unwrap().peel_to_commit().unwrap().id();
        let base = repo_final.merge_base(new_head_oid, ahead_oid).unwrap();
        assert_eq!(base, ours_oid, "precondition: both tips still share the original commit as their base");
        let next = classify(
            Some(&new_head_oid.to_string()),
            Some(&ahead_oid.to_string()),
            Some(&base.to_string()),
        );
        assert_eq!(
            next,
            Cycle::Diverged,
            "the next cycle must classify as Diverged (merge path), not FastForward again — \
             otherwise this livelocks exactly as Review Important-1 described"
        );
    }
}

/// Task 16 (AWS coexistence): the OLD AWS event-sourcing transport and the
/// NEW lgs (local git sync) transport now both write the same CSV files.
/// Without these two fixes, ONE MCP-server write produces a SEPARATE git
/// commit on EACH machine for the same logical change (divergence becomes
/// the steady state whenever the MCP server is active), and a downstream
/// balance recalculation after applying it would echo events back out at
/// AWS from both machines. See `apply_remote_entity`'s doc comment.
#[cfg(test)]
mod coexist_tests {
    use super::AllowanceTrackerApp;
    use crate::backend::domain::commands::child::{CreateChildCommand, SetActiveChildCommand};
    use crate::backend::domain::commands::transactions::CreateTransactionCommand;
    use crate::backend::domain::models::child::Child as DomainChild;
    use crate::backend::domain::models::goal::{DomainGoal, DomainGoalState};
    use crate::backend::domain::models::transaction::{Transaction as DomainTransaction, TransactionType};
    use crate::backend::domain::{sync_channel, SyncNotifier};
    use crate::backend::Backend;
    use allowance_core::money::Money;
    use git2::Repository;
    use shared::sync::EntityType;
    use shared::ChildId;

    /// One child with a real, git-initialized folder (an ordinary local
    /// transaction write initializes the repo via `commit_file_change`,
    /// same as production) and one existing transaction dated 2026-01-10,
    /// so a backdated remote transaction has a real downstream row to
    /// disturb. `sync_notifier` is wired to a real channel when the test
    /// needs to observe what does or does not get notified.
    fn app_with_git_backed_child(
        sync_notifier: Option<SyncNotifier>,
    ) -> (AllowanceTrackerApp, String, tempfile::TempDir) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let backend = Backend::with_data_dir(temp.path().to_path_buf(), sync_notifier)
            .expect("backend");
        let child = backend
            .child_service
            .create_child(CreateChildCommand {
                name: "Test Kid".to_string(),
                birthdate: "2015-01-01".to_string(),
            })
            .expect("create child")
            .child;
        backend
            .child_service
            .set_active_child(SetActiveChildCommand { child_id: child.id.clone() })
            .expect("set active child");
        backend
            .transaction_service
            .create_transaction(CreateTransactionCommand {
                description: "Allowance".to_string(),
                amount: 10.0,
                date: Some(chrono::DateTime::parse_from_rfc3339("2026-01-10T12:00:00Z").unwrap()),
            })
            .expect("create transaction");
        let app = AllowanceTrackerApp::new_for_test(backend);
        (app, child.id, temp)
    }

    /// A transaction as it would arrive over the wire from the other
    /// machine — already carrying the balance computed there.
    fn sample_transaction(
        child_id: &str,
        date: &str,
        description: &str,
        amount_cents: i64,
        balance_cents: i64,
    ) -> DomainTransaction {
        let amount = Money::from_cents(amount_cents);
        DomainTransaction {
            id: DomainTransaction::generate_id(amount, 1),
            child_id: child_id.to_string(),
            date: chrono::DateTime::parse_from_rfc3339(date).unwrap(),
            description: description.to_string(),
            amount,
            balance: Money::from_cents(balance_cents),
            transaction_type: if amount_cents >= 0 { TransactionType::OneOffIncome } else { TransactionType::Expense },
        }
    }

    /// Fix 1: one MCP-server write otherwise produces an independent commit
    /// on EACH machine for the same logical change — divergence becomes the
    /// steady state whenever the MCP server is active, defeating the whole
    /// purpose of the lgs merge. `ApplyRemoteEntity` must write through the
    /// non-committing path; the lgs merge produces the commit, not this call.
    ///
    /// Dated after the existing local transaction so no downstream balance
    /// recalculation is triggered either — this test isolates fix 1 only.
    #[test]
    fn applying_a_remote_entity_does_not_create_a_git_commit() {
        let (mut app, child_id, _temp) = app_with_git_backed_child(None);
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&ChildId::from(child_id.as_str()))
            .unwrap();
        let before = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        let remote_tx = sample_transaction(&child_id, "2026-02-01T12:00:00Z", "Remote gift", 500, 1500);
        let json = serde_json::to_string(&remote_tx).unwrap();
        app.apply_remote_entity(&child_id, &EntityType::Transaction, &remote_tx.id, &json, "evt-1");

        let after = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(before, after, "the lgs merge produces the commit, not the apply path");

        // The write itself must still have landed on disk — only the commit
        // is suppressed, not the data.
        let on_disk = std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();
        assert!(
            on_disk.contains("Remote gift"),
            "the remote transaction must still be written to disk: {on_disk}"
        );
    }

    /// Fix 2: a remote transaction can be backdated relative to rows already
    /// on this machine, which requires recalculating their stored running
    /// balances downstream — but `recalculate_balances_from_date` emits one
    /// `Updated` SyncEvent per changed row. Reusing the shared, AWS-wired
    /// balance service for that recalculation would push those events back
    /// out at AWS, from both machines, after every apply. It must go
    /// through a `BalanceService` built with `.with_sync_notifier(None)` so
    /// it can never notify, regardless of what the shared instance is
    /// wired to.
    #[test]
    fn applying_a_backdated_remote_entity_recalculates_balances_without_notifying() {
        let (tx, rx) = sync_channel();
        let (mut app, child_id, _temp) = app_with_git_backed_child(Some(tx));

        // Drain the Created event from the local setup transaction above —
        // this test is only about what the *apply* path itself notifies.
        while rx.try_recv().is_ok() {}

        // Backdated relative to the existing "Allowance" transaction
        // (2026-01-10), so applying it forces a downstream recalculation of
        // that later row's stored balance: 2.00 + 10.00 = 12.00.
        let remote_tx = sample_transaction(&child_id, "2026-01-05T12:00:00Z", "Backdated remote gift", 200, 200);
        let json = serde_json::to_string(&remote_tx).unwrap();
        app.apply_remote_entity(&child_id, &EntityType::Transaction, &remote_tx.id, &json, "evt-2");

        // Precondition: the recalculation actually ran and changed the
        // downstream row's balance — otherwise "no events" would hold for
        // the trivial reason that nothing happened.
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&ChildId::from(child_id.as_str()))
            .unwrap();
        let on_disk = std::fs::read_to_string(child_dir.join("transactions.csv")).unwrap();
        assert!(
            on_disk.contains("12.00"),
            "precondition: the downstream row's balance must have been recalculated to 12.00: {on_disk}"
        );

        assert!(
            rx.try_recv().is_err(),
            "merge-safe recalculation after an AWS apply must not notify AWS"
        );
    }

    /// Same bug, same fix, different entity type: `GoalRepository::store_goal`
    /// and `update_goal` both commit unconditionally, exactly as
    /// `store_transaction` used to. `ApplyRemoteEntity`'s Goal arm must use
    /// the non-committing path too, or one MCP-server goal write still
    /// produces an independent commit on every machine.
    #[test]
    fn applying_a_remote_goal_does_not_create_a_git_commit() {
        let (mut app, child_id, _temp) = app_with_git_backed_child(None);
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&ChildId::from(child_id.as_str()))
            .unwrap();
        let before = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        let goal = DomainGoal {
            id: "goal-1".to_string(),
            child_id: child_id.clone(),
            description: "New bike".to_string(),
            target_amount: 150.0,
            state: DomainGoalState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&goal).unwrap();
        app.apply_remote_entity(&child_id, &EntityType::Goal, &goal.id, &json, "evt-goal-1");

        let after = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(before, after, "the lgs merge produces the commit, not the apply path");

        let on_disk = std::fs::read_to_string(child_dir.join("goals.csv")).unwrap();
        assert!(
            on_disk.contains("New bike"),
            "the remote goal must still be written to disk: {on_disk}"
        );
    }

    /// Same bug, same fix, different entity type again: `ChildRepository::store_child`
    /// and `update_child` both commit unconditionally against `child.yaml`
    /// itself — the child's own record. `ApplyRemoteEntity`'s Child arm must
    /// use the non-committing path too. The write must still be correct:
    /// only the commit is suppressed, never the write.
    #[test]
    fn applying_a_remote_child_does_not_create_a_git_commit() {
        let (mut app, child_id, _temp) = app_with_git_backed_child(None);
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&ChildId::from(child_id.as_str()))
            .unwrap();
        let before = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        // A rename arriving from the other machine.
        let renamed = DomainChild {
            id: child_id.clone(),
            name: "Renamed Kid".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2015, 1, 1).unwrap(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&renamed).unwrap();
        app.apply_remote_entity(&child_id, &EntityType::Child, &child_id, &json, "evt-child-1");

        let after = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(before, after, "the lgs merge produces the commit, not the apply path");

        // Only the commit is suppressed — child.yaml itself must still be
        // written correctly, since it is the child's own record.
        let on_disk = std::fs::read_to_string(child_dir.join("child.yaml")).unwrap();
        assert!(
            on_disk.contains("Renamed Kid"),
            "the remote child rename must still be written to disk: {on_disk}"
        );
    }

    // ------------------------------------------------------------------
    // DeleteLocalEntity: the sibling of ApplyRemoteEntity in the same sync
    // match. Same bug shape as the applies above — the committing
    // `delete_transaction` / `delete_goal_by_id` produced an independent
    // commit on every machine for the same logical delete.
    // ------------------------------------------------------------------

    /// `delete_transaction` (the committing variant) was reachable from
    /// `DeleteLocalEntity` via `TransactionService::delete_transaction_by_id`
    /// — same defect as the transaction apply, third instance overall.
    #[test]
    fn deleting_a_local_transaction_via_remote_delete_does_not_create_a_git_commit() {
        let (mut app, child_id, _temp) = app_with_git_backed_child(None);
        let existing = app
            .backend()
            .transaction_service
            .list_all_transactions_for_child(&child_id)
            .expect("list transactions");
        assert_eq!(existing.len(), 1, "precondition: one seed transaction");
        let tx_id = existing[0].id.clone();

        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&ChildId::from(child_id.as_str()))
            .unwrap();
        let before = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        app.delete_local_entity(&child_id, &EntityType::Transaction, &tx_id, "evt-del-tx");

        let after = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(before, after, "the lgs merge produces the commit, not the delete path");

        // Only the commit is suppressed — the row itself must actually be
        // gone, never just the commit.
        let remaining = app
            .backend()
            .transaction_service
            .list_all_transactions_for_child(&child_id)
            .expect("list transactions");
        assert!(remaining.is_empty(), "the deleted transaction must actually be removed from disk");
    }

    /// `write_goals` (the committing variant, via `delete_goal_by_id`) was
    /// reachable from `DeleteLocalEntity` via `GoalService::delete_goal_by_id`
    /// — same defect, fourth instance overall (transaction apply, goal
    /// apply, child apply, now this).
    #[test]
    fn deleting_a_local_goal_via_remote_delete_does_not_create_a_git_commit() {
        use crate::backend::domain::commands::goal::CreateGoalCommand;

        let (mut app, child_id, _temp) = app_with_git_backed_child(None);
        let created = app
            .backend()
            .goal_service
            .create_goal(CreateGoalCommand {
                child_id: None,
                description: "Bike".to_string(),
                target_amount: 100.0,
            })
            .expect("create goal")
            .goal;

        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&ChildId::from(child_id.as_str()))
            .unwrap();
        let before = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        app.delete_local_entity(&child_id, &EntityType::Goal, &created.id, "evt-del-goal");

        let after = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(before, after, "the lgs merge produces the commit, not the delete path");

        let on_disk = std::fs::read_to_string(child_dir.join("goals.csv")).unwrap();
        assert!(
            !on_disk.contains("Bike"),
            "the deleted goal must actually be removed from disk: {on_disk}"
        );
    }

    /// `EntityType::Child`'s delete arm never touches `ChildRepository::delete_child`
    /// or any per-child git repo — it only deregisters in the top-level
    /// `children.yaml` registry (see the comment in `delete_local_entity`).
    /// This proves that definitively via real git inspection rather than by
    /// reading the code, and doubles as confirmation that a remote child
    /// delete never touches the shared folder (already covered by
    /// `a_remote_child_delete_deregisters_without_touching_the_folder` in
    /// `refresh_allowance_tests`).
    #[test]
    fn deleting_a_local_child_via_remote_delete_does_not_create_a_git_commit() {
        let (mut app, child_id, _temp) = app_with_git_backed_child(None);
        let child_dir = app
            .backend()
            .csv_connection
            .child_dir(&ChildId::from(child_id.as_str()))
            .unwrap();
        let before = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();

        app.delete_local_entity(&child_id, &EntityType::Child, &child_id, "evt-del-child");

        let after = Repository::open(&child_dir)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id();
        assert_eq!(before, after, "a remote child delete must never touch the child's own git repo");

        // Deregistered, but the folder (and its git repo) must survive —
        // it may still be in use on another machine.
        assert!(child_dir.join("child.yaml").exists(), "the shared folder must not be removed");
        assert!(
            app.backend().csv_connection.registry().path_for(&ChildId::from(child_id.as_str())).is_none(),
            "the child must be deregistered locally"
        );
    }
}
