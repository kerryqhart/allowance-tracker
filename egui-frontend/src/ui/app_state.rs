//! # App State Module
//!
//! This module defines the central application state structure and initialization logic
//! for the allowance tracker app.
//!
//! ## Key Types:
//! - `MainTab` - Enum defining available tabs (Calendar, Table)
//! - `AllowanceTrackerApp` - Main application state struct
//!
//! ## Key Functions:
//! - `new()` - Initialize new app instance with backend connection
//! - `clear_messages()` - Clear success/error messages
//!
//! ## Purpose:
//! This module serves as the central state management for the entire application,
//! using a modular architecture with separate state modules for different concerns.
//!
//! ## State Management:
//! The AllowanceTrackerApp struct composes multiple focused state modules:
//! - CoreAppState: Backend, child, balance, tab
//! - UIState: Loading, messages
//! - CalendarState: Calendar navigation, transactions
//! - ModalState: Modal visibility and flow
//! - FormState: Form inputs and validation
//! - InteractionState: User selections, dropdowns

use log::{info, warn};
use chrono::{Datelike, TimeZone};
use shared::*;
use crate::backend::Backend;
use crate::backend::domain::{ChildStatus, RealFolderSource, SyncCommand, SyncThreadHandle, WakeUi};
use crate::backend::domain::sync_notifier::sync_channel;
use crate::backend::domain::sync_persistence::{SyncState, RetryQueue, sync_state_path, retry_queue_path};
use crate::backend::storage::csv::GlobalConfigRepository;
use crate::backend::storage::HttpRemoteClient;
use std::sync::Arc;

// Import all state modules
use crate::ui::state::*;
use crate::ui::state::roster::{spawn_loader, ChildRoster, RosterMessage};

// Re-export types from state modules to avoid duplication
pub use crate::ui::state::{MainTab, OverlayType, ParentalControlStage, ProtectedAction, TransactionType};
pub use crate::ui::state::form_state::{MoneyTransactionModalConfig, MoneyTransactionFormState};

/// Main application struct for the egui allowance tracker
///
/// This uses a modular architecture with focused state modules for maintainability.
pub struct AllowanceTrackerApp {
    // Modular state architecture
    pub core: CoreAppState,           // Backend, child, balance, tab
    pub ui: UIState,                  // Loading, messages
    pub calendar: CalendarState,      // Calendar navigation, overlays
    pub modal: ModalState,            // All modal states
    pub form: FormState,              // Form validation, inputs
    pub interaction: InteractionState, // User selections, dropdowns
    pub table: TableState,            // Transaction table pagination
    pub chart: ChartState,            // Chart visualization and time periods
    pub goal: GoalUiState,            // Goal management and progress tracking
    pub settings: crate::ui::components::settings::SettingsState, // Settings modals and forms
    pub sync: SyncUiState,            // Sync status and conflict management

    // Sync thread handles — None until Task 6 wires them at startup
    /// Sender to control the sync thread (PollNow, Shutdown). None when sync is disabled.
    pub sync_command_tx: Option<std::sync::mpsc::Sender<SyncCommand>>,
    /// Handle to the sync background thread. None when sync is disabled.
    pub sync_thread: Option<SyncThreadHandle>,
    /// Tracks whether the window was focused on the previous frame, for edge-detection.
    /// Initialised to `true` so the first frame doesn't spuriously trigger a `PollNow`
    /// — Task 6 can do an explicit startup poll if one is desired.
    pub was_focused: bool,

    // ── Child roster ─────────────────────────────────────────────────────
    /// What every render path reads instead of scanning the filesystem.
    pub roster: ChildRoster,
    /// Statuses from the worker walk, drained once per frame.
    pub roster_rx: std::sync::mpsc::Receiver<RosterMessage>,
    /// Kept so a rebuild can hand a fresh loader the same channel; stale
    /// messages from the superseded walk are discarded by generation.
    pub roster_tx: std::sync::mpsc::Sender<RosterMessage>,
    /// Repaint hook, the same `WakeUi` pattern the sync thread uses.
    pub roster_wake: WakeUi,
    /// Monotonic walk counter. Bumped by `next_generation` on every rebuild so
    /// a slow walk cannot overwrite a newer one's results.
    pub roster_generation: u64,
    /// What startup found and could not fix by itself: a `children.yaml` that
    /// would not parse, a migration that skipped a folder, a migration that
    /// found an orphan. Painted across the top of the window until dismissed,
    /// because all three end in a child missing from the picker, and a
    /// silently empty picker is indistinguishable from the bug this branch
    /// exists to remove.
    pub startup_banner: crate::ui::components::startup_banner::StartupBanner,
    /// A `load_initial_data` that could not run yet because the active child's
    /// folder was still coming down. Retried every frame until it can. It is
    /// deferred rather than dropped: the sync path uses it to refresh the UI
    /// after applying remote entities, and silently skipping that would leave
    /// the window showing pre-sync data with nothing to trigger a reload.
    pub pending_initial_load: bool,
}

impl AllowanceTrackerApp {
    /// Create a new AllowanceTrackerApp with modular architecture
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, anyhow::Error> {
        info!("Initializing AllowanceTrackerApp with modular architecture");
        
        // Setup custom fonts including Chalkboard
        crate::ui::setup_custom_fonts(&cc.egui_ctx);
        
        // Install image loaders for background support
        egui_extras::install_image_loaders(&cc.egui_ctx);
        
        // ── Sync startup wiring ───────────────────────────────────────────────
        //
        // Load persisted sync state BEFORE constructing Backend so we can decide
        // whether to wire a live SyncNotifier. If sync is disabled or misconfigured,
        // we pass `None` to Backend so domain writes don't spam warn-logs trying to
        // send events to a dropped receiver.
        //
        // To enable sync manually (until a settings UI exists), create the
        // file `sync_state.yaml` in ~/Documents/Allowance Tracker:
        //
        //   enabled: true
        //   remote_url: "https://<your-api-gateway>.execute-api.<region>.amazonaws.com/internal"
        //   watermarks: {}
        let data_dir = crate::backend::Backend::default_data_dir()?;
        // `load` returns Ok(default) when the file is absent, so an Err here means
        // the file exists but failed to parse — surface it as a warning rather than
        // silently resetting, so a typo'd manual config is diagnosable.
        let mut sync_state = SyncState::load(&sync_state_path(&data_dir)).unwrap_or_else(|e| {
            warn!("Failed to parse sync_state.yaml ({}); starting with sync disabled", e);
            SyncState::default()
        });
        let retry_queue = RetryQueue::load(&retry_queue_path(&data_dir)).unwrap_or_else(|e| {
            warn!("Failed to parse retry_queue.yaml ({}); starting with empty retry queue", e);
            RetryQueue::default()
        });

        let will_spawn_aws = sync_state.enabled
            && sync_state.remote_url.as_ref().map_or(false, |u| is_valid_http_url(u));
        // The lgs (desktop-to-desktop) transport is configured once first
        // run has completed on this machine — `cloud_root` is `None` until
        // then. Either transport being configured is reason enough to run
        // the background sync thread; see the combined gate below.
        let lgs_configured = sync_state.cloud_root.is_some();
        let will_spawn_sync = will_spawn_aws || lgs_configured;

        // The event channel is always created so `SyncThreadHandle::spawn`
        // (which takes a plain `Receiver`, not an `Option`) has one to take
        // when the thread spawns for an lgs-only installation with no AWS
        // transport. When AWS is off, `sync_notifier` is simply never handed
        // to `Backend`, so nothing ever sends into it — an idle, empty
        // channel is indistinguishable from none existing at all.
        let (notifier, event_rx) = sync_channel();
        let sync_notifier = if will_spawn_aws { Some(notifier) } else { None };

        let mut backend = crate::backend::Backend::new(sync_notifier)?;

        // ── lgs (desktop-to-desktop) bootstrap ─────────────────────────────
        //
        // Best-effort and non-fatal: a failure here must never keep the app
        // from launching — it degrades to "lgs sync did not start this run"
        // plus a StartupNotice, not a crash. Runs before `startup_banner` is
        // built below so a bootstrap failure lands in the same banner as
        // every other startup notice, rather than being lost.
        let mut child_sync: Option<crate::backend::sync::ChildSyncEngine> = None;
        if lgs_configured {
            match bootstrap_lgs_child_sync(
                data_dir.clone(),
                sync_state.cloud_root.clone(),
                &sync_state.daemon_ownership,
                backend.csv_connection.clone(),
            ) {
                Ok((engine, ownership)) => {
                    child_sync = Some(engine);
                    if ownership.installed_by_app != sync_state.daemon_ownership.installed_by_app {
                        sync_state.daemon_ownership = ownership;
                        if let Err(e) = sync_state.save(&sync_state_path(&data_dir)) {
                            warn!("Could not persist daemon ownership after installing lgs: {e}");
                        }
                    }
                }
                Err(e) => {
                    warn!("lgs sync could not start this run: {e}");
                    backend.startup_notices.push(crate::backend::StartupNotice {
                        severity: crate::backend::NoticeSeverity::Warning,
                        title: "Sync with another Mac did not start".to_string(),
                        details: vec![e.to_string()],
                    });
                }
            }
        }

        // Take what startup wants to tell the user before `backend` is moved
        // into `CoreAppState`. Migration orphans, migration skips, a
        // registry file that would not parse, and the lgs bootstrap outcome
        // above were all log-only until now.
        let startup_banner = crate::ui::components::startup_banner::StartupBanner::new(
            std::mem::take(&mut backend.startup_notices),
        );

        // ── Child roster ─────────────────────────────────────────────────
        //
        // Registry load policy lives here, mirroring the sync_state.yaml
        // precedent above: a malformed hand-editable file must be
        // diagnosable, never silently reset to empty. (`CsvConnection::new`
        // has already surfaced a parse failure as a startup error.)
        //
        // There is deliberately NO eager `check_and_issue_pending_allowances`
        // here any more. It ran on the main thread before the first frame, so
        // a child folder that iCloud had not yet materialized turned app
        // launch into a bouncing Dock icon with no window at all — strictly
        // worse than the mid-frame freeze this design removes, because there
        // is not even a label to read. Issuance is now a consumer of roster
        // completion; see `drain_roster_messages`.
        let registry = backend.csv_connection.registry();
        if registry.entries().is_empty() {
            info!("No children registered — Settings → Children → Add existing child…");
        }

        let (roster_tx, roster_rx) = std::sync::mpsc::channel();
        let roster_generation = 1;
        let roster = ChildRoster::new(registry.clone(), roster_generation);
        let ctx_for_roster = cc.egui_ctx.clone();
        let roster_wake: WakeUi = Arc::new(move || ctx_for_roster.request_repaint());
        spawn_loader(
            registry,
            Arc::new(RealFolderSource),
            roster_generation,
            roster_tx.clone(),
            roster_wake.clone(),
        );

        let now = chrono::Local::now();
        let _current_month = now.month();
        let _current_year = now.year();

        // Review Important-3: this diagnostic is decided by a pure function
        // (`diagnose_aws_transport`, tested below) on `will_spawn_aws`
        // alone, independent of `will_spawn_sync`'s branch further down.
        // It used to live in that branch's `else`, which is only reached
        // when NEITHER transport is configured — so once lgs being
        // configured could make `will_spawn_sync` true on its own, an
        // invalid/missing AWS `remote_url` fell into the `if` branch
        // instead (silently substituting `NullRemoteStorage`) and this
        // warning became unreachable. AWS misconfiguration must be
        // diagnosable whether or not lgs happens to be picking up the slack.
        match diagnose_aws_transport(sync_state.enabled, &sync_state.remote_url, will_spawn_aws, lgs_configured) {
            AwsTransportDiagnostic::InvalidUrl { remote_url: Some(url), lgs_configured } => warn!(
                "Sync enabled but remote_url {:?} is not a valid http(s) URL — AWS sync disabled{}",
                url,
                if lgs_configured { " (lgs sync continues)" } else { "" }
            ),
            AwsTransportDiagnostic::InvalidUrl { remote_url: None, lgs_configured } => warn!(
                "Sync enabled but remote_url is missing — AWS sync disabled{}",
                if lgs_configured { " (lgs sync continues)" } else { "" }
            ),
            AwsTransportDiagnostic::NothingConfigured => {
                info!("Sync disabled — no AWS remote_url configured and lgs has not completed first run");
            }
            AwsTransportDiagnostic::Silent => {}
        }

        // Spawn the sync thread if (and only if) EITHER transport is
        // configured: AWS (enabled with a valid URL) or lgs (a cloud root
        // persisted from a completed first run). Before Task 19 this was
        // gated on AWS alone, which meant an installation that only ever
        // configured lgs — the ordinary case, since AWS is a legacy/optional
        // transport — never spawned this thread at all, and so never ran
        // `child_sync` regardless of whether it was wired below. `remote`
        // is `NullRemoteStorage` in the lgs-only case: a safe no-op stand-in
        // for the AWS transport this installation never configured, needed
        // only because `SyncThreadHandle::spawn` requires the type.
        let (sync, sync_command_tx, sync_thread) = if will_spawn_sync {
            let remote: Arc<dyn crate::backend::storage::RemoteStorage> = if will_spawn_aws {
                let url = sync_state.remote_url.as_ref().expect("checked by will_spawn_aws");
                info!("Sync enabled — connecting to remote: {}", url);
                Arc::new(HttpRemoteClient::new(url.clone()))
            } else {
                info!("AWS sync not configured — lgs (desktop-to-desktop) sync only");
                Arc::new(crate::backend::storage::NullRemoteStorage)
            };
            let (message_tx, message_rx) = std::sync::mpsc::channel();
            let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<SyncCommand>();

            // Wake the UI (request a repaint) whenever the sync thread sends
            // a message. Without this the egui UI only draws on input events,
            // so request/response messages like ReadEntityRequest and
            // GetChildIdsRequest can time out while the app is unfocused.
            let ctx_for_wake = cc.egui_ctx.clone();
            let wake_ui: crate::backend::domain::WakeUi =
                std::sync::Arc::new(move || ctx_for_wake.request_repaint());

            let handle = SyncThreadHandle::spawn(
                remote,
                event_rx,
                cmd_rx,
                message_tx,
                wake_ui,
                sync_state.clone(),
                retry_queue.events.clone(),
                data_dir.clone(),
                child_sync,
            );

            (SyncUiState::with_receiver(message_rx), Some(cmd_tx), Some(handle))
        } else {
            // Neither transport is configured. The diagnostic for WHY
            // (invalid/missing remote_url, or nothing configured at all)
            // was already logged above, independent of this branch — see
            // the Important-3 comment there for why that independence
            // matters.
            (SyncUiState::new(), None, None)
        };
        // ─────────────────────────────────────────────────────────────────────

        // Initialize modular state components
        let core = CoreAppState::new(backend);
        let ui = UIState::new();
        let calendar = CalendarState::new(); // Uses current date
        let modal = ModalState::new();
        let form = FormState::new();
        let interaction = InteractionState::new();
        let table = TableState::new();
        let chart = ChartState::new();
        let goal = GoalUiState::new();
        let settings = crate::ui::components::settings::SettingsState::new();

        Ok(Self {
            // Modular state
            core,
            ui,
            calendar,
            modal,
            form,
            interaction,
            table,
            chart,
            goal,
            settings,
            sync,
            sync_command_tx,
            sync_thread,
            was_focused: true,
            roster,
            roster_rx,
            roster_tx,
            roster_wake,
            roster_generation,
            startup_banner,
            pending_initial_load: false,
        })
    }

    /// Construct an app instance directly from a `Backend`, bypassing the
    /// eframe `CreationContext` (fonts, image loaders, sync-thread wiring)
    /// that `new` requires. For tests that exercise app-level logic without a
    /// live egui context. Sync is disabled.
    ///
    /// The roster is walked *synchronously* here: the loader still runs on its
    /// own thread, but we block until it reports `Finished` so a test sees a
    /// settled roster rather than racing the walk. Every availability gate in
    /// the app then behaves deterministically under test.
    #[cfg(test)]
    pub fn new_for_test(mut backend: Backend) -> Self {
        let startup_banner = crate::ui::components::startup_banner::StartupBanner::new(
            std::mem::take(&mut backend.startup_notices),
        );
        let registry = backend.csv_connection.registry();
        let (roster_tx, roster_rx) = std::sync::mpsc::channel();
        let roster_generation = 1;
        let mut roster = ChildRoster::new(registry.clone(), roster_generation);
        let roster_wake: WakeUi = Arc::new(|| {});
        spawn_loader(
            registry,
            Arc::new(RealFolderSource),
            roster_generation,
            roster_tx.clone(),
            roster_wake.clone(),
        );
        while let Ok(msg) = roster_rx.recv() {
            let finished = matches!(msg, RosterMessage::Finished { .. });
            roster.apply(msg);
            if finished {
                break;
            }
        }

        Self {
            core: CoreAppState::new(backend),
            ui: UIState::new(),
            calendar: CalendarState::new(),
            modal: ModalState::new(),
            form: FormState::new(),
            interaction: InteractionState::new(),
            table: TableState::new(),
            chart: ChartState::new(),
            goal: GoalUiState::new(),
            settings: crate::ui::components::settings::SettingsState::new(),
            sync: SyncUiState::new(),
            sync_command_tx: None,
            sync_thread: None,
            was_focused: true,
            roster,
            roster_rx,
            roster_tx,
            roster_wake,
            roster_generation,
            startup_banner,
            pending_initial_load: false,
        }
    }

    // TEMPORARY: Getter methods for backward compatibility
    pub fn backend(&self) -> &Backend {
        &self.core.backend
    }

    // ====================
    // CHILD ROSTER
    // ====================

    /// The active child's id, read from `global_config.yaml` and nothing else.
    ///
    /// Deliberately *not* `child_service.get_active_child()`: that resolves the
    /// child by reading its `child.yaml`, which blocks on a folder iCloud has
    /// not materialized. This is the one question the app must be able to
    /// answer before any child folder is readable — "which child are we waiting
    /// for?" — so it goes to the machine-local config file directly.
    ///
    /// `GlobalConfigRepository` is constructed from a *clone* of the shared
    /// `CsvConnection`, which shares its registry handle; it is not a second,
    /// independent snapshot.
    pub fn active_child_id(&self) -> Option<ChildId> {
        let repo = GlobalConfigRepository::new((*self.core.backend.csv_connection).clone());
        match repo.active_child_id() {
            Ok(id) => id,
            Err(e) => {
                warn!("Could not read the active child from global_config.yaml: {}", e);
                None
            }
        }
    }

    /// The active child's roster status, or `None` when no child is active or
    /// the active id is not registered on this machine.
    pub fn active_child_status(&self) -> Option<ChildStatus> {
        let id = self.active_child_id()?;
        self.roster.status_of(&id).cloned()
    }

    /// Whether the roster currently reports this child as materialized.
    pub fn is_available(&self, id: &ChildId) -> bool {
        matches!(self.roster.status_of(id), Some(ChildStatus::Available(_)))
    }

    /// Run `load_initial_data`, or defer it if the active child's folder is
    /// still coming down from iCloud.
    ///
    /// `load_initial_data` reads `child.yaml` and then the child's transactions
    /// synchronously; on a dataless folder that read blocks the frame, and on
    /// frame 1 it blocks before anything is drawn at all. **Every** caller goes
    /// through here so no path can reintroduce that freeze — including the sync
    /// path, which calls `rebuild_roster` first and so is guaranteed to find
    /// every entry `Downloading` at that instant.
    ///
    /// A deferred load is retried from `update` on the repaint the roster
    /// worker requests when a status lands.
    pub fn load_initial_data_when_ready(&mut self) {
        if matches!(self.active_child_status(), Some(ChildStatus::Downloading)) {
            self.pending_initial_load = true;
            return;
        }
        // No active child, or one that is Available (warm) or Unavailable —
        // `load_initial_data` resolves the latter two to a child or to `None`
        // and clears the loading flag rather than hanging.
        self.pending_initial_load = false;
        self.load_initial_data();
    }

    /// Pre-increment and return the roster generation. Every rebuild takes a
    /// fresh one so results from the walk it supersedes are discarded.
    pub fn next_generation(&mut self) -> u64 {
        self.roster_generation += 1;
        self.roster_generation
    }

    /// Persist the display names this walk observed — all of them, in ONE
    /// registry write.
    ///
    /// Must be called before `self.roster` is replaced: `ChildRoster::new`
    /// starts with an empty `changed_labels`, so a rebuild silently discards
    /// anything not yet drained. Losing them means a child whose folder is
    /// still downloading has no cached name to show after a restart — exactly
    /// the case the cache exists for.
    fn persist_roster_labels(&mut self) {
        let changed = self.roster.drain_changed_labels();
        if changed.is_empty() {
            return;
        }
        if let Err(e) = self.core.backend.csv_connection.update_registry(|reg| {
            for (id, label) in &changed {
                reg.set_label(id, label);
            }
            Ok(())
        }) {
            // Display cache only — the children themselves are fine — but it
            // must not vanish silently: `children.yaml` can sit on a read-only
            // or full volume.
            warn!(
                "Could not persist {} refreshed child display name(s); the picker may \
                 show stale names until this succeeds: {}",
                changed.len(),
                e
            );
        }
    }

    /// Rebuild the roster from the current registry and start a fresh walk.
    ///
    /// Call this after anything that changes *which* children are registered
    /// (a create, a deregistration, a sync-applied child change). Pending
    /// label changes are drained and persisted first — see
    /// `persist_roster_labels`.
    ///
    /// Rebuilding without respawning the loader would leave every entry stuck
    /// at `Downloading` forever: sync would stop polling, and the active child
    /// would never come back. So the two always happen together.
    pub fn rebuild_roster(&mut self) {
        self.persist_roster_labels();

        let generation = self.next_generation();
        let registry = self.core.backend.csv_connection.registry();
        self.roster = ChildRoster::new(registry.clone(), generation);
        spawn_loader(
            registry,
            Arc::new(RealFolderSource),
            generation,
            self.roster_tx.clone(),
            self.roster_wake.clone(),
        );
    }

    /// Drain this frame's roster messages and react to them.
    ///
    /// Allowance issuance hangs off this rather than off app start: the moment
    /// the active child's folder is readable is the earliest moment issuance
    /// can succeed, and it is reached on a worker thread with the window
    /// already drawn.
    pub fn drain_roster_messages(&mut self) {
        let mut messages = Vec::new();
        while let Ok(msg) = self.roster_rx.try_recv() {
            messages.push(msg);
        }
        if messages.is_empty() {
            return;
        }

        // One read of global_config.yaml per frame that has messages, rather
        // than one per message.
        let active = self.active_child_id();
        let mut issue_allowances = false;
        let mut walk_finished = false;

        for msg in messages {
            let msg_id = match &msg {
                RosterMessage::Status { id, .. } => Some(id.clone()),
                RosterMessage::Finished { .. } => None,
            };
            if matches!(msg, RosterMessage::Finished { generation } if generation == self.roster_generation) {
                walk_finished = true;
            }

            // The trigger is a *transition* in the roster's own state, read
            // either side of `apply` — never the message's contents.
            //
            // Reading the message would mean a status `apply` discarded (one
            // from a superseded walk) could still issue allowances: money
            // moving on a report the roster itself rejected. It would also
            // re-fire on every re-report of an already-`Available` child,
            // paying for a full transaction read to discover nothing changed.
            let was_available = msg_id
                .as_ref()
                .map(|id| self.is_available(id))
                .unwrap_or(false);
            self.roster.apply(msg);

            if let Some(id) = msg_id {
                if !was_available && self.is_available(&id) && Some(&id) == active.as_ref() {
                    issue_allowances = true;
                }
            }
        }

        if walk_finished {
            // One registry write per walk, not one per renamed child.
            self.persist_roster_labels();
        }

        if issue_allowances {
            self.issue_pending_allowances_for_active_child();
        }
    }

    /// Issue any allowances the active child is owed, and refresh the views
    /// derived from transactions if any were issued.
    fn issue_pending_allowances_for_active_child(&mut self) {
        let issued = self
            .backend()
            .transaction_service
            .check_and_issue_pending_allowances();
        match issued {
            Ok(0) => info!("No pending allowances for the active child"),
            Ok(count) => {
                info!("Issued {} pending allowances for the active child", count);
                // Same reload set as the periodic path: header balance,
                // calendar, goal and chart — deliberately not the table, which
                // would reset the user's scroll position mid-session.
                self.load_balance();
                self.load_calendar_data();
                self.load_goal_data();
                self.load_chart_data();
            }
            Err(e) => warn!("Failed to check pending allowances: {}", e),
        }
    }
    
    /// Get current child directly from backend service (the source of truth)
    /// This replaces the cached current_child() method to avoid inconsistencies
    pub fn get_current_child_from_backend(&self) -> Option<shared::Child> {
        match self.backend().child_service.get_active_child() {
            Ok(result) => {
                // Only log once per actual change, not every frame (commented out to reduce noise)
                // log::info!("GET_CURRENT_CHILD_BACKEND: Raw result: {:?}", 
                //     result.active_child.child.as_ref().map(|c| (&c.id, &c.name)));
                
                result.active_child.child.map(|domain_child| {
                    crate::ui::mappers::to_dto(domain_child)
                })
            },
            Err(e) => {
                log::warn!("GET_CURRENT_CHILD_BACKEND: Failed to get current child from backend: {}", e);
                None
            }
        }
    }
    
    /// DEPRECATED: Get current child from cached state 
    /// Use get_current_child_from_backend() instead to ensure consistency
    #[deprecated(note = "Use get_current_child_from_backend() instead")]
    pub fn current_child(&self) -> &Option<Child> {
        &self.core.current_child
    }
    
    pub fn current_balance(&self) -> f64 {
        self.core.current_balance
    }
    
    pub fn current_tab(&self) -> MainTab {
        self.core.current_tab
    }
    
    // TEMPORARY: Setter methods for state synchronization
    pub fn set_current_tab(&mut self, tab: MainTab) {
        self.core.current_tab = tab;
    }
    
    pub fn set_loading(&mut self, loading: bool) {
        self.ui.loading = loading;
    }

    /// Start parental control challenge for a specific action
    pub fn start_parental_control_challenge(&mut self, action: crate::ui::state::modal_state::ProtectedAction) {
        use crate::ui::state::modal_state::ParentalControlStage;
        
        info!("Starting parental control challenge for: {:?}", action);
        self.modal.pending_protected_action = Some(action);
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
        self.modal.show_parental_control_modal = true;
    }

    /// Cancel parental control challenge
    pub fn cancel_parental_control_challenge(&mut self) {
        info!("Cancelling parental control challenge");
        self.modal.show_parental_control_modal = false;
        self.modal.pending_protected_action = None;
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
    }

    /// Reset parental control state to question 1
    pub fn reset_parental_control(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
    }

    /// Advance to question 2
    pub fn advance_to_question_2(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question2;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
    }

    /// Mark parental control as authenticated and proceed to question stage
    pub fn mark_parental_control_authenticated(&mut self) {
        self.modal.parental_control_stage = ParentalControlStage::Question1;
        self.modal.parental_control_input.clear();
        self.modal.parental_control_error = None;
        self.modal.parental_control_loading = false;
        
        // Sync compatibility fields
        // self.selected_month = self.calendar.selected_month;
        // self.selected_year = self.calendar.selected_year;
        // self.calendar_loading = self.calendar.calendar_loading;
        
        // Reload calendar data for the new month
        self.load_calendar_data();
        info!("Navigated to previous month: {}/{}", self.calendar.selected_month, self.calendar.selected_year);
    }

    /// Navigate to the next month
    pub fn navigate_to_next_month(&mut self) {
        self.calendar.navigate_to_next_month();
        
        // Sync compatibility fields
        // self.selected_month = self.calendar.selected_month;
        // self.selected_year = self.calendar.selected_year;
        // self.calendar_loading = self.calendar.calendar_loading;
        
        // Reload calendar data for the new month
        self.load_calendar_data();
        info!("Navigated to next month: {}/{}", self.calendar.selected_month, self.calendar.selected_year);
    }

    /// Get the current month name as a string
    pub fn get_current_month_name(&self) -> String {
        self.calendar.get_current_month_name()
    }

    /// Clear any error or success messages
    pub fn clear_messages(&mut self) {
        self.ui.clear_messages();
        
        // Sync compatibility fields
        // self.error_message = self.ui.error_message.clone(); // Removed
        // self.success_message = self.ui.success_message.clone(); // Removed
    }

    /// Submit answer for parental control validation
    pub fn submit_parental_control_answer(&mut self) {
        // Validate input
        if self.modal.parental_control_input.trim().is_empty() {
            self.modal.parental_control_error = Some("Please enter an answer".to_string());
            return;
        }
        
        // Set loading state and clear errors
        self.modal.parental_control_loading = true;
        self.modal.parental_control_error = None;
        
        // Create command for backend validation
        let command = crate::backend::domain::commands::parental_control::ValidateParentalControlCommand {
            answer: self.modal.parental_control_input.clone(),
        };
        
        // Call backend service
        match self.backend().parental_control_service.validate_answer(command) {
            Ok(result) => {
                self.modal.parental_control_loading = false;
                
                if result.success {
                    info!("Parental control authentication successful");
                    self.modal.parental_control_stage = ParentalControlStage::Authenticated;
                    
                    // Execute the pending action
                    info!("PARENTAL_CONTROL_SUCCESS: Checking for pending actions...");
                    info!("pending_protected_action = {:?}", self.modal.pending_protected_action);
                    info!("pending_settings_action = {:?}", self.modal.pending_settings_action);
                    
                    if let Some(action) = self.modal.pending_protected_action {
                        info!("EXECUTING protected action: {:?}", action);
                        self.execute_protected_action(action);
                    } else {
                        log::warn!("WARNING: No pending protected action found after successful parental control!");
                    }
                    
                    // Close modal after brief success display
                    self.modal.show_parental_control_modal = false;
                    // Access granted feedback removed
                } else {
                    info!("Parental control validation failed");
                    self.modal.parental_control_error = Some(result.message);
                    self.modal.parental_control_input.clear();
                }
            }
            Err(e) => {
                self.modal.parental_control_loading = false;
                log::error!("🚨 Parental control validation error: {}", e);
                self.modal.parental_control_error = Some("Validation failed. Please try again.".to_string());
            }
        }
    }
    
    /// Execute the action after successful authentication
    fn execute_protected_action(&mut self, action: crate::ui::state::modal_state::ProtectedAction) {
        use crate::ui::state::modal_state::ProtectedAction;
        
        info!("EXECUTE_PROTECTED_ACTION called with: {:?}", action);
        
        match action {
            ProtectedAction::DeleteTransactions => {
                info!("Executing delete transactions action");
                self.enter_transaction_selection_mode();
            }
            ProtectedAction::AccessSettings => {
                info!("EXECUTING SETTINGS ACCESS ACTION!");
                info!("Checking for pending_settings_action...");
                if let Some(settings_action) = self.modal.pending_settings_action {
                    info!("Found pending settings action: {:?}", settings_action);
                    info!("CALLING execute_settings_action...");
                    self.execute_settings_action(settings_action);
                } else {
                    log::warn!("🚨 AccessSettings action triggered but no pending settings action found");
                }
            }
        }
        
        self.modal.pending_protected_action = None;
        self.modal.pending_settings_action = None; // Clear both actions
    }
    
    /// Execute specific settings menu action after parental control authentication
    fn execute_settings_action(&mut self, action: crate::ui::state::modal_state::SettingsAction) {
        use crate::ui::state::modal_state::SettingsAction;
        
        info!("⚙️ EXECUTE_SETTINGS_ACTION CALLED!");
        info!("⚙️ Settings action received: {:?}", action);
        info!("⚙️ About to enter match statement...");
        
        match action {
            SettingsAction::ShowProfile => {
                // Extract child data before mutations to avoid borrow conflicts
                let child_data = if let Some(child) = self.get_current_child_from_backend() {
                    Some((
                        child.id.clone(),
                        child.name.clone(),
                        child.birthdate,
                        child.created_at,
                        child.updated_at,
                    ))
                } else {
                    None
                };
                
                if let Some((id, name, birthdate, created_at, updated_at)) = child_data {
                    let domain_child = crate::backend::domain::models::child::Child {
                        id: id.clone(),
                        name: name.clone(),
                        birthdate,
                        created_at,
                        updated_at,
                    };
                    self.settings.profile_form.populate_from_child(&domain_child);
                    self.settings.show_profile_modal = true;
                    info!("👤 Profile modal opened for child: {}", name);
                } else {
                    log::warn!("🚨 No active child found for profile action");
                    self.ui.error_message = Some("No child selected. Please select a child first.".to_string());
                }
            }
            SettingsAction::CreateChild => {
                info!("👶 Create child action - opening modal");
                self.settings.show_create_child_modal = true;
                self.settings.create_child_form.clear(); // Reset form state
            }
            SettingsAction::ConfigureAllowance => {
                info!("🚨 CONFIGURE_ALLOWANCE_ACTION_TRIGGERED! Opening modal...");
                if self.get_current_child_from_backend().is_some() {
                    info!("🚨 Setting show_allowance_config_modal = true");
                    self.settings.show_allowance_config_modal = true;
                    info!("🚨 Modal flag set, now loading config...");
                    self.load_allowance_config_for_modal(); // Load existing config
                    info!("🚨 Config loaded, modal should be visible");
                } else {
                    info!("🚨 ERROR: No child selected for allowance config");
                    self.ui.error_message = Some("No child selected. Please select a child first.".to_string());
                }
            }
            SettingsAction::DeleteTransactions => {
                info!("Delete transactions action - entering selection mode");
                self.enter_transaction_selection_mode();
            }
            SettingsAction::ExportData => {
                info!("📤 Export data action - opening modal");
                self.settings.show_export_modal = true;
                self.settings.export_form.clear(); // Reset form state
                
                // Update preview immediately
                let child_name = self.get_current_child_from_backend().as_ref().map(|c| c.name.clone());
                let child_name_ref = child_name.as_deref();
                self.settings.export_form.update_preview(child_name_ref);
            }
            SettingsAction::Children => {
                info!("👶 Children action - opening modal");
                self.settings.show_children_modal = true;
                self.settings.children_form.clear();
                self.settings.children_form.just_opened = true;
            }
            SettingsAction::InitialSync => {
                self.open_backfill_modal();
            }
        }
    }
    
    // ====================
    // TRANSACTION SELECTION METHODS
    // ====================
    
    /// Enter transaction selection mode for deletion
    pub fn enter_transaction_selection_mode(&mut self) {
        info!("Entering transaction selection mode");
        self.interaction.transaction_selection_mode = true;
        self.interaction.selected_transaction_ids.clear();
        
        // TEMPORARY: Sync compatibility fields
        // self.transaction_selection_mode = true;
        // self.selected_transaction_ids.clear();
        
        // Transaction selection mode feedback removed
    }
    
    /// Exit transaction selection mode without deleting
    pub fn exit_transaction_selection_mode(&mut self) {
        info!("🚫 Exiting transaction selection mode");
        self.interaction.transaction_selection_mode = false;
        self.interaction.selected_transaction_ids.clear();
        
        // TEMPORARY: Sync compatibility fields
        // self.transaction_selection_mode = false;
        // self.selected_transaction_ids.clear();
        
        self.clear_messages();
    }
    
    /// Toggle selection of a transaction
    pub fn toggle_transaction_selection(&mut self, transaction_id: &str) {
        if self.interaction.selected_transaction_ids.contains(transaction_id) {
            info!("➖ Deselecting transaction: {}", transaction_id);
            self.interaction.selected_transaction_ids.remove(transaction_id);
            // self.selected_transaction_ids.remove(transaction_id); // Sync compatibility field
        } else {
            info!("Selecting transaction: {}", transaction_id);
            self.interaction.selected_transaction_ids.insert(transaction_id.to_string());
            // self.selected_transaction_ids.insert(transaction_id.to_string()); // Sync compatibility field
        }
    }
    
    /// Check if a transaction is selected
    pub fn is_transaction_selected(&self, transaction_id: &str) -> bool {
        self.interaction.selected_transaction_ids.contains(transaction_id)
    }
    
    /// Get count of selected transactions
    pub fn selected_transaction_count(&self) -> usize {
        self.interaction.selected_transaction_ids.len()
    }
    
    /// Clear all selected transactions
    pub fn clear_transaction_selection(&mut self) {
        info!("🧹 Clearing all transaction selections");
        self.interaction.selected_transaction_ids.clear();
        // self.selected_transaction_ids.clear(); // Sync compatibility field
    }
    
    /// Check if any transactions are selected
    pub fn has_selected_transactions(&self) -> bool {
        !self.interaction.selected_transaction_ids.is_empty()
    }
    
    // ====================
    // ADD MONEY FORM VALIDATION METHODS
    // ====================

    /// Validate the add money form and update validation state
    /// Delegates to MoneyManagementService for consistent validation logic
    pub fn validate_add_money_form(&mut self) {
        use crate::backend::domain::money_management::MoneyManagementService;

        self.form.add_money_description_error = None;
        self.form.add_money_amount_error = None;

        let service = MoneyManagementService::new();
        let description = &self.form.add_money_description;
        let amount_input = &self.form.add_money_amount;

        // Use domain service validation
        let validation = service.validate_add_money_form(description, amount_input);

        // Map validation errors to UI state
        for error in &validation.errors {
            use shared::ValidationError;
            match error {
                ValidationError::EmptyDescription |
                ValidationError::DescriptionTooLong(_) => {
                    self.form.add_money_description_error = Some(service.get_error_message(error));
                }
                ValidationError::EmptyAmount => {
                    // Don't show "Amount is required" error immediately - let the grayed button be sufficient
                }
                _ => {
                    self.form.add_money_amount_error = Some(service.get_error_message(error));
                }
            }
        }

        // Update overall validation state
        self.form.add_money_is_valid = self.form.add_money_description_error.is_none()
            && self.form.add_money_amount_error.is_none()
            && !description.trim().is_empty()
            && !amount_input.trim().is_empty();
    }

    /// Clean and parse amount input string
    /// Delegates to MoneyManagementService for consistent parsing logic
    fn clean_and_parse_amount(&self, amount_input: &str) -> Result<f64, String> {
        use crate::backend::domain::money_management::MoneyManagementService;
        MoneyManagementService::new().clean_and_parse_amount(amount_input)
    }
    
    /// Format amount for currency display ($XX.XX)
    pub fn format_currency_amount(&self, amount: f64) -> String {
        format!("${:.2}", amount)
    }
    
    /// Clear add money form and validation state
    pub fn clear_add_money_form(&mut self) {
        self.form.add_money_description.clear();
        self.form.add_money_amount.clear();
        self.form.add_money_description_error = None;
        self.form.add_money_amount_error = None;
        self.form.add_money_is_valid = true;
    }
    
    /// Auto-format amount field as user types (adds $ and proper decimal formatting)
    pub fn auto_format_amount_field(&mut self) {
        let input = self.form.add_money_amount.clone();

        // Only auto-format if the input looks like a valid number
        if let Ok(amount) = self.clean_and_parse_amount(&input) {
            // Only format if the amount is reasonable
            if amount > 0.0 && amount < 1_000_000.0 {
                // Format as $XX.XX but only if user isn't currently typing
                if !input.ends_with('.') && !input.ends_with('0') {
                    self.form.add_money_amount = format!("{:.2}", amount);
                }
            }
        }
    }
    
    // ====================
    // GENERIC MONEY TRANSACTION FORM VALIDATION METHODS
    // ====================

    /// Validate a generic money transaction form and update its validation state
    /// Delegates amount validation to MoneyManagementService for consistency
    pub fn validate_money_transaction_form(&self, form_state: &mut MoneyTransactionFormState, config: &MoneyTransactionModalConfig) {
        use crate::backend::domain::money_management::MoneyManagementService;

        form_state.description_error = None;
        form_state.amount_error = None;

        let service = MoneyManagementService::new();

        // Validate description (use UI config for max length)
        let description = form_state.description.trim();
        if description.is_empty() {
            form_state.description_error = Some("Description is required".to_string());
        } else if description.len() > config.max_description_length {
            form_state.description_error = Some(format!(
                "Description too long ({}/{} characters)",
                description.len(),
                config.max_description_length
            ));
        }

        // Validate amount using domain service
        let amount_input = form_state.amount.trim();
        if !amount_input.is_empty() {
            let validation = service.validate_add_money_form(description, amount_input);
            for error in &validation.errors {
                use shared::ValidationError;
                match error {
                    ValidationError::EmptyDescription |
                    ValidationError::DescriptionTooLong(_) => {
                        // Skip - we handle description validation above with UI config
                    }
                    ValidationError::EmptyAmount => {
                        // Don't show error for empty - let grayed button be sufficient
                    }
                    _ => {
                        form_state.amount_error = Some(service.get_error_message(error));
                    }
                }
            }
        }

        // Update overall validation state
        form_state.is_valid = form_state.description_error.is_none()
            && form_state.amount_error.is_none()
            && !description.is_empty()
            && !amount_input.is_empty();
    }
    
    // ====================
    // BACKEND INTEGRATION METHODS
    // ====================
    
    /// Return the true current balance after a transaction. `money_management`
    /// reports the just-inserted row's balance column, which is wrong for
    /// backdated transactions (it's the historical running total at that
    /// earlier date). Fall back to that value only if the live lookup fails.
    fn current_balance_after_transaction(&self, fallback: f64) -> f64 {
        match self.backend().child_service.get_active_child() {
            Ok(resp) => match resp.active_child.child {
                Some(child) => self
                    .backend()
                    .balance_service
                    .get_current_balance(&child.id)
                    .unwrap_or(fallback),
                None => fallback,
            },
            Err(_) => fallback,
        }
    }

    pub fn submit_income_transaction(&mut self) -> bool {
        use crate::backend::domain::money_management::MoneyManagementService;
        use chrono::Timelike;
        info!("Submitting income transaction - Description: '{}', Amount: '{}'", 
                  self.form.income_form_state.description, self.form.income_form_state.amount);
        let amount = match self.clean_and_parse_amount(&self.form.income_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("Failed to parse amount: {}", error);
                self.ui.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        let date_time = self.calendar.selected_day.map(|date| {
            let now = chrono::Local::now();
            let naive_datetime = date.and_hms_opt(now.hour(), now.minute(), now.second()).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::AddMoneyRequest {
            description: self.form.income_form_state.description.trim().to_string(),
            amount,
            date: date_time,
        };
        let money_service = MoneyManagementService::new();
        match money_service.add_money_complete(
            request,
            &self.backend().child_service,
            &self.backend().transaction_service,
            &self.backend().goal_service,
        ) {
            Ok(response) => {
                info!("Income transaction successful: {}", response.success_message);
                self.core.current_balance = self.current_balance_after_transaction(response.new_balance);
                self.load_calendar_data();
                true
            }
            Err(error) => {
                log::error!("Income transaction failed: {}", error);
                self.ui.error_message = Some(format!("Failed to add money: {}", error));
                false
            }
        }
    }

    pub fn submit_expense_transaction(&mut self) -> bool {
        use crate::backend::domain::money_management::MoneyManagementService;
        use chrono::Timelike;
        info!("💸 Submitting expense transaction - Description: '{}', Amount: '{}'", 
                  self.form.expense_form_state.description, self.form.expense_form_state.amount);
        let amount = match self.clean_and_parse_amount(&self.form.expense_form_state.amount) {
            Ok(amount) => amount,
            Err(error) => {
                log::error!("Failed to parse amount: {}", error);
                self.ui.error_message = Some(format!("Invalid amount: {}", error));
                return false;
            }
        };
        let date_time = self.calendar.selected_day.map(|date| {
            let now = chrono::Local::now();
            let naive_datetime = date.and_hms_opt(now.hour(), now.minute(), now.second()).unwrap();
            let eastern_offset = chrono::FixedOffset::west_opt(5 * 3600).unwrap();
            eastern_offset.from_local_datetime(&naive_datetime).single().unwrap()
        });
        let request = shared::SpendMoneyRequest {
            description: self.form.expense_form_state.description.trim().to_string(),
            amount,
            date: date_time,
        };
        let money_service = MoneyManagementService::new();
        match money_service.spend_money_complete(
            request,
            &self.backend().child_service,
            &self.backend().transaction_service,
            &self.backend().goal_service,
        ) {
            Ok(response) => {
                info!("Expense transaction successful: {}", response.success_message);
                self.core.current_balance = self.current_balance_after_transaction(response.new_balance);
                self.load_calendar_data();
                true
            }
            Err(error) => {
                log::error!("Expense transaction failed: {}", error);
                self.ui.error_message = Some(format!("Failed to spend money: {}", error));
                false
            }
        }
    }
}

/// Quick sanity check on the remote URL so an obvious misconfiguration (wrong
/// scheme, empty string) is caught at startup rather than producing obscure
/// HTTP errors later.
fn is_valid_http_url(url: &str) -> bool {
    let trimmed = url.trim();
    (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && trimmed.len() > "https://".len()
}

/// What `AllowanceTrackerApp::new` should log about the AWS transport's
/// configuration.
///
/// Review Important-3: deliberately decided from `will_spawn_aws` alone,
/// NOT from whether the sync thread ends up spawning at all (`will_spawn_aws
/// || lgs_configured`). Gating this on the combined flag is exactly the bug
/// that made an invalid/missing `remote_url` silently unreportable once lgs
/// alone was enough to spawn the thread — the diagnostic must fire whenever
/// AWS specifically is misconfigured, whether or not lgs is separately
/// picking up the slack.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AwsTransportDiagnostic {
    /// `enabled` but `remote_url` fails validation (or is absent).
    /// `lgs_configured` is carried through so the message can say whether
    /// lgs sync continues regardless.
    InvalidUrl { remote_url: Option<String>, lgs_configured: bool },
    /// Neither transport is configured — nothing to warn about, but worth
    /// one `info!` so a blank first run doesn't read as silence.
    NothingConfigured,
    /// AWS is healthy, or lgs is configured and AWS was never asked for.
    /// Nothing to say either way.
    Silent,
}

fn diagnose_aws_transport(
    enabled: bool,
    remote_url: &Option<String>,
    will_spawn_aws: bool,
    lgs_configured: bool,
) -> AwsTransportDiagnostic {
    if enabled && !will_spawn_aws {
        AwsTransportDiagnostic::InvalidUrl { remote_url: remote_url.clone(), lgs_configured }
    } else if !enabled && !lgs_configured {
        AwsTransportDiagnostic::NothingConfigured
    } else {
        AwsTransportDiagnostic::Silent
    }
}

#[cfg(test)]
mod aws_transport_diagnostic_tests {
    use super::*;

    /// The exact regression: lgs being configured must not swallow the
    /// warning that AWS's own `remote_url` is garbage.
    #[test]
    fn an_invalid_url_is_reported_even_when_lgs_is_configured() {
        let diagnostic = diagnose_aws_transport(true, &Some("not-a-url".to_string()), false, true);
        assert_eq!(
            diagnostic,
            AwsTransportDiagnostic::InvalidUrl {
                remote_url: Some("not-a-url".to_string()),
                lgs_configured: true
            }
        );
    }

    #[test]
    fn an_invalid_url_is_reported_when_lgs_is_not_configured_either() {
        let diagnostic = diagnose_aws_transport(true, &Some("not-a-url".to_string()), false, false);
        assert_eq!(
            diagnostic,
            AwsTransportDiagnostic::InvalidUrl {
                remote_url: Some("not-a-url".to_string()),
                lgs_configured: false
            }
        );
    }

    #[test]
    fn a_missing_url_is_reported_even_when_lgs_is_configured() {
        let diagnostic = diagnose_aws_transport(true, &None, false, true);
        assert_eq!(
            diagnostic,
            AwsTransportDiagnostic::InvalidUrl { remote_url: None, lgs_configured: true }
        );
    }

    #[test]
    fn silent_when_aws_is_healthy() {
        let diagnostic = diagnose_aws_transport(true, &Some("https://example.com".to_string()), true, false);
        assert_eq!(diagnostic, AwsTransportDiagnostic::Silent);
    }

    #[test]
    fn silent_when_only_lgs_is_configured_and_aws_was_never_enabled() {
        let diagnostic = diagnose_aws_transport(false, &None, false, true);
        assert_eq!(diagnostic, AwsTransportDiagnostic::Silent);
    }

    #[test]
    fn nothing_configured_is_reported_once_on_a_blank_install() {
        let diagnostic = diagnose_aws_transport(false, &None, false, false);
        assert_eq!(diagnostic, AwsTransportDiagnostic::NothingConfigured);
    }
}

/// Build the real `ChildSyncEngine` for a machine that has already completed
/// lgs first run (`cloud_root` is `Some`).
///
/// This is the composition point that makes the lgs desktop-to-desktop
/// transport actually reachable from the shipped app: it resolves
/// production `SyncPaths` (canonicalizing `home`/`cloud_root` — see
/// `SyncPaths::for_production`'s doc comment), refreshes the bundled `lgs`
/// binary at its stable path, and adopts-or-installs the daemon via
/// `ensure_daemon` — never reinstalling or repointing one this app did not
/// install.
///
/// Returns the engine plus the (possibly updated) `DaemonOwnership` so the
/// caller can persist it — this function has no persistence of its own.
/// Errors here are always non-fatal to the caller: see the call site in
/// [`AllowanceTrackerApp::new`].
fn bootstrap_lgs_child_sync(
    data_dir: std::path::PathBuf,
    cloud_root: Option<std::path::PathBuf>,
    ownership: &crate::backend::sync::DaemonOwnership,
    csv_connection: Arc<crate::backend::CsvConnection>,
) -> anyhow::Result<(crate::backend::sync::ChildSyncEngine, crate::backend::sync::DaemonOwnership)> {
    use crate::backend::sync::{ensure_daemon, ensure_lgs_binary, ChildSyncEngine, DaemonOutcome, LgsClient, SyncPaths};

    let paths = SyncPaths::for_production(data_dir, cloud_root)?;
    ensure_lgs_binary(&paths)?;
    let lgs = LgsClient::new(paths.lgs_binary.clone());

    let mut ownership = ownership.clone();
    match ensure_daemon(&lgs, &ownership)? {
        DaemonOutcome::InstalledAndOwned => ownership.installed_by_app = true,
        DaemonOutcome::Skewed(message) => {
            warn!("lgs daemon is outdated and not owned by this app: {message}");
        }
        DaemonOutcome::Healthy | DaemonOutcome::Restarted => {}
    }

    let engine = ChildSyncEngine::new(lgs, csv_connection);
    Ok((engine, ownership))
}

impl Drop for AllowanceTrackerApp {
    fn drop(&mut self) {
        // Shut down the sync thread cleanly when the app closes.
        // SyncThreadHandle's own Drop is a no-op after this because
        // shutdown() uses Option::take on its internal join handle.
        if let Some(ref mut thread) = self.sync_thread {
            thread.shutdown();
        }
    }
}
