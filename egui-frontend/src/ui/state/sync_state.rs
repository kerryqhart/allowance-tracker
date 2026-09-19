//! # Sync State Module
//!
//! This module contains UI state for sync operations and conflict management.
//!
//! ## Responsibilities:
//! - Tracking sync status (idle, syncing, conflicts, errors)
//! - Managing conflict list and pending count
//! - Polling messages from the background sync thread
//!
//! ## Purpose:
//! This separates sync-specific UI concerns from general app state,
//! making it easier to add sync UI features (status indicators, conflict modals, etc.)
//! in the future without cluttering the core app state.

use crate::backend::domain::sync_manager::{GoalsDivergedNotice, SyncMessage, SyncStatus};
use shared::sync::SyncConflict;
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Minimum gap between two `SyncCommand::PollNow` sends triggered by a
/// stale-head merge refusal. Without this, HEAD moving faster than one
/// fetch+classify+push round-trip (e.g. a user typing several transactions
/// in a row) turns into continuous ping-pong between the background and UI
/// threads — repeated fetches, `lgs status` calls, and blob comparisons,
/// many times a second, indefinitely. It is safe (a stale-head refusal
/// never writes) but shows up as a hot machine and needless daemon load
/// rather than as a test failure. Below this interval the merge is not
/// lost, only deferred to the ordinary sync timer.
pub const STALE_HEAD_POLL_DEBOUNCE: Duration = Duration::from_millis(500);

/// After this many consecutive stale-head refusals with no intervening
/// successful apply, stop sending `PollNow` entirely and let the ordinary
/// timer take over — a `PollNow`-triggered cycle can itself race a fast
/// enough writer forever, and past a handful of consecutive misses the
/// immediate re-poll is not helping.
pub const STALE_HEAD_REFUSAL_LIMIT: u32 = 5;

/// What [`SyncUiState::note_stale_head_refusal`] tells the caller to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleHeadPollAction {
    /// Send `SyncCommand::PollNow` now.
    Send,
    /// Within the debounce window since the last send — skip it. The
    /// merge is deferred to the ordinary sync timer, not lost.
    Debounced,
    /// This refusal just reached [`STALE_HEAD_REFUSAL_LIMIT`] consecutive
    /// misses. Log once here; every further refusal in this streak is
    /// [`StaleHeadPollAction::Suppressed`] until an `Applied` resets it.
    LimitReached,
    /// Already over the limit for this streak — stay silent (already
    /// logged at `LimitReached`).
    Suppressed,
}

/// A fast-forward that was blocked by uncommitted local content and
/// resolved by committing it — see `SyncUiState::fast_forward_blocked`'s
/// doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastForwardBlockedNotice {
    pub child_id: String,
    /// The fast-forward target this child was trying to reach when the
    /// block occurred.
    pub to: String,
}

/// How much the user needs to care. `FastForwardBlockedNotice` and a stalled
/// child currently render identically in red; a parent seeing two red lines
/// against the same child has no way to tell which is the emergency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoticeSeverity {
    /// Something happened and was handled. No action needed.
    Informational,
    /// This child is not syncing until a human acts.
    Blocking,
}

/// A sync failure serious enough that the user needs to find out about it,
/// which must not be silently erased by the next unrelated `SyncStatus`
/// write. Review round 4, Important-2: a genuine (non-conflict) checkout
/// failure inside `apply_fast_forward` (an I/O error, a corrupt object, a
/// permissions problem) was being written to `sync.status` — but
/// `run_child_sync_cycles` always sends `StatusChanged(Idle)` right after
/// the message whose handler wrote it, and both are drained in the SAME
/// `handle_sync_messages` batch on the UI thread before a single frame
/// renders, so that status write is provably invisible (the exact
/// reasoning already established for `FastForwardBlockedNotice`, which
/// this mirrors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFailureNotice {
    pub child_id: String,
    /// Human-readable description of what failed and why.
    pub message: String,
    /// No `Default` on this type, and no default here either — every
    /// construction site names its severity explicitly, so it is always a
    /// decision rather than an accident.
    pub severity: NoticeSeverity,
}

/// UI state for sync operations
pub struct SyncUiState {
    /// Current sync status.
    ///
    /// **Currently read by no UI component.** Written in ~30 places in
    /// `app_coordinator.rs` and rendered nowhere — a status write alone does
    /// NOT tell the user anything. Anything the user must see goes through
    /// `sync_failures` with `NoticeSeverity::Blocking`, which the
    /// child-picker badge (`header.rs`) surfaces.
    pub status: SyncStatus,

    /// List of detected conflicts awaiting resolution
    pub conflicts: Vec<SyncConflict>,

    /// `goals.csv` divergences a merge could not resolve, held until a
    /// future UI dismisses them. Deliberately NOT folded into `status`:
    /// `status` is last-writer-wins (see `SyncMessage::GoalsDiverged`'s doc
    /// comment), so a notice routed through it would be erased by the very
    /// next unrelated sync event.
    pub goals_diverged: Vec<GoalsDivergedNotice>,

    /// Fast-forwards that were blocked by uncommitted local content and
    /// resolved by committing it (Review Important-1 on Task 17 — see
    /// `AllowanceTrackerApp::commit_dirty_tree_to_unblock_fast_forward`'s
    /// doc comment for why committing there is safe and bounded). Held
    /// until a future UI dismisses it or a subsequent successful merge for
    /// the same child resolves the resulting divergence (see
    /// [`SyncUiState::clear_fast_forward_blocked`]). Deliberately NOT
    /// folded into `status`, for the exact reason `goals_diverged` isn't:
    /// `status` is last-writer-wins, and the `StatusChanged(Idle)` that
    /// `run_child_sync_cycles` sends right after this event — drained in
    /// the SAME batch on the UI thread — would erase a status write before
    /// a single frame ever rendered it.
    pub fast_forward_blocked: Vec<FastForwardBlockedNotice>,

    /// Genuine, non-self-resolving sync failures — see
    /// [`SyncFailureNotice`]'s doc comment for why these cannot be routed
    /// through `status`.
    pub sync_failures: Vec<SyncFailureNotice>,

    /// When a `SyncCommand::PollNow` was last sent in response to a
    /// stale-head merge refusal, keyed by child id. Missing means either
    /// none has been sent yet for that child, or its entry was just reset by
    /// a successful apply for that same child.
    ///
    /// Per-child, not global: once `run_child_sync_cycles`
    /// (`sync_thread.rs`) loops over every registered child on each tick
    /// (Task 17), a GLOBAL debounce/cap would let one child's noise (rapid
    /// local edits racing its own merges) suppress the immediate re-poll a
    /// completely unrelated child legitimately needed — `PollNow` re-runs
    /// the cycle for every child, so that suppressed re-poll is not about
    /// the noisy child at all, it collaterally delays a different child's
    /// otherwise-uncapped refusal to the next 30s timer tick. Keying by
    /// child id keeps the guard doing what it was built for (stop ONE
    /// child's fast-moving HEAD from ping-ponging PollNow) without letting
    /// that child's streak spend down a budget that belongs to every other
    /// child too.
    last_stale_head_pollnow_at: HashMap<String, Instant>,

    /// Consecutive stale-head refusals since the last successful apply, per
    /// child. An entry is reset to absent (equivalent to 0) by
    /// [`SyncUiState::note_applied`] for that same child id only — see the
    /// doc comment on `last_stale_head_pollnow_at` for why this is keyed
    /// per-child rather than global.
    consecutive_stale_head_refusals: HashMap<String, u32>,

    /// Receiver for messages from the background sync thread
    pub message_rx: Option<mpsc::Receiver<SyncMessage>>,
}

impl SyncUiState {
    /// Create a new SyncUiState with no receiver (sync disabled)
    pub fn new() -> Self {
        Self {
            status: SyncStatus::Disabled,
            conflicts: Vec::new(),
            goals_diverged: Vec::new(),
            fast_forward_blocked: Vec::new(),
            sync_failures: Vec::new(),
            last_stale_head_pollnow_at: HashMap::new(),
            consecutive_stale_head_refusals: HashMap::new(),
            message_rx: None,
        }
    }

    /// Create a new SyncUiState with a message receiver from the sync thread
    pub fn with_receiver(rx: mpsc::Receiver<SyncMessage>) -> Self {
        Self {
            status: SyncStatus::Idle,
            conflicts: Vec::new(),
            goals_diverged: Vec::new(),
            fast_forward_blocked: Vec::new(),
            sync_failures: Vec::new(),
            last_stale_head_pollnow_at: HashMap::new(),
            consecutive_stale_head_refusals: HashMap::new(),
            message_rx: Some(rx),
        }
    }

    /// Record (or refresh) a goals-divergence notice for a child. Replaces
    /// any existing notice for the same child rather than accumulating a
    /// duplicate every cycle the divergence remains unresolved.
    pub fn record_goals_diverged(&mut self, notice: GoalsDivergedNotice) {
        self.goals_diverged.retain(|n| n.child_id != notice.child_id);
        self.goals_diverged.push(notice);
    }

    /// Record (or refresh) a fast-forward-blocked notice for a child —
    /// same replace-not-accumulate behavior as [`Self::record_goals_diverged`].
    pub fn record_fast_forward_blocked(&mut self, notice: FastForwardBlockedNotice) {
        self.fast_forward_blocked.retain(|n| n.child_id != notice.child_id);
        self.fast_forward_blocked.push(notice);
    }

    /// Clear a child's fast-forward-blocked notice. Call once a subsequent
    /// merge for that child succeeds — that is what actually resolves the
    /// divergence the blocking commit deliberately created.
    pub fn clear_fast_forward_blocked(&mut self, child_id: &str) {
        self.fast_forward_blocked.retain(|n| n.child_id != child_id);
    }

    /// Record (or refresh) a durable sync-failure notice for a child — same
    /// replace-not-accumulate behavior as [`Self::record_goals_diverged`].
    pub fn record_sync_failure(&mut self, notice: SyncFailureNotice) {
        self.sync_failures.retain(|n| n.child_id != notice.child_id);
        self.sync_failures.push(notice);
    }

    /// Clear a child's sync-failure notice — call once a subsequent apply
    /// (merge or fast-forward) for that child succeeds.
    pub fn clear_sync_failure(&mut self, child_id: &str) {
        self.sync_failures.retain(|n| n.child_id != child_id);
    }

    /// Blocking first, so the one that matters is never the one scrolled out
    /// of a 120px box.
    pub fn notices_blocking_first(&self) -> Vec<&SyncFailureNotice> {
        let mut all: Vec<&SyncFailureNotice> = self.sync_failures.iter().collect();
        all.sort_by(|a, b| b.severity.cmp(&a.severity));
        all
    }

    /// Drives the child-picker badge.
    pub fn has_blocking_notice(&self) -> bool {
        self.sync_failures.iter().any(|n| n.severity == NoticeSeverity::Blocking)
    }

    /// Record one stale-head merge refusal for `child_id` at `now` and
    /// decide whether the caller should send an immediate
    /// `SyncCommand::PollNow`. See `STALE_HEAD_POLL_DEBOUNCE` and
    /// `STALE_HEAD_REFUSAL_LIMIT` for the two guards, and the doc comment on
    /// `consecutive_stale_head_refusals` for why this is keyed by
    /// `child_id` rather than tracked once globally. The consecutive-refusal
    /// count for this child increments on every call regardless of the
    /// returned action; only [`Self::note_applied`] for the same child
    /// resets it.
    pub fn note_stale_head_refusal(&mut self, child_id: &str, now: Instant) -> StaleHeadPollAction {
        let count = self.consecutive_stale_head_refusals.entry(child_id.to_string()).or_insert(0);
        *count += 1;
        let count = *count;

        // The cap takes priority over debouncing: once tripped, stay
        // silent even if the debounce window has independently elapsed.
        if count > STALE_HEAD_REFUSAL_LIMIT {
            return StaleHeadPollAction::Suppressed;
        }
        if count == STALE_HEAD_REFUSAL_LIMIT {
            return StaleHeadPollAction::LimitReached;
        }

        let should_send = match self.last_stale_head_pollnow_at.get(child_id) {
            None => true,
            Some(last) => now.duration_since(*last) >= STALE_HEAD_POLL_DEBOUNCE,
        };
        if should_send {
            self.last_stale_head_pollnow_at.insert(child_id.to_string(), now);
            StaleHeadPollAction::Send
        } else {
            StaleHeadPollAction::Debounced
        }
    }

    /// A merge was successfully applied for `child_id` — that child's
    /// stale-head streak (if any) is over. Resets both its debounce
    /// timestamp and its consecutive counter, so a fresh bout of stale-head
    /// races for THIS child starts from a clean state rather than staying
    /// suppressed forever. Other children's streaks are untouched — see the
    /// doc comment on `consecutive_stale_head_refusals` for why this must
    /// not reset (or be reset by) any other child's state.
    pub fn note_applied(&mut self, child_id: &str) {
        self.last_stale_head_pollnow_at.remove(child_id);
        self.consecutive_stale_head_refusals.remove(child_id);
    }

    /// Forget everything this per-child debounce state remembers about
    /// `child_id`. Call when a child is deregistered (removed from
    /// `children.yaml`, on this machine or via a remote delete) — without
    /// this, a deregistered child's entries linger in these maps forever:
    /// harmless memory growth on their own, but a latent trap if the same
    /// id is ever reused (e.g. the child is re-registered later) and
    /// inherits a stale debounce/cap state that has nothing to do with its
    /// new registration.
    pub fn forget_child(&mut self, child_id: &str) {
        self.last_stale_head_pollnow_at.remove(child_id);
        self.consecutive_stale_head_refusals.remove(child_id);
    }

    /// Try to receive the next sync message from the background thread.
    /// Returns None if there are no pending messages or no receiver is set.
    /// Used by `app_coordinator::handle_sync_messages` to drain the channel.
    pub fn try_recv_message(&self) -> Option<SyncMessage> {
        self.message_rx.as_ref()?.try_recv().ok()
    }

    /// Count the number of pending conflicts (not yet resolved)
    pub fn pending_conflict_count(&self) -> usize {
        self.conflicts
            .iter()
            .filter(|c| c.status == shared::sync::ConflictStatus::Pending)
            .count()
    }
}

impl Default for SyncUiState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod stale_head_debounce_tests {
    use super::*;

    #[test]
    fn a_second_refusal_within_the_debounce_window_does_not_send() {
        let mut state = SyncUiState::new();
        let t0 = Instant::now();
        assert_eq!(state.note_stale_head_refusal("child1", t0), StaleHeadPollAction::Send);
        let t1 = t0 + Duration::from_millis(100);
        assert_eq!(
            state.note_stale_head_refusal("child1", t1),
            StaleHeadPollAction::Debounced,
            "a refusal 100ms after the last send must be debounced, not re-sent"
        );
    }

    #[test]
    fn a_refusal_after_the_debounce_interval_sends_again() {
        let mut state = SyncUiState::new();
        let t0 = Instant::now();
        assert_eq!(state.note_stale_head_refusal("child1", t0), StaleHeadPollAction::Send);
        let t1 = t0 + Duration::from_millis(600);
        assert_eq!(
            state.note_stale_head_refusal("child1", t1),
            StaleHeadPollAction::Send,
            "600ms is past the 500ms debounce window, so this refusal must send again"
        );
    }

    #[test]
    fn the_fifth_consecutive_refusal_trips_the_limit_then_further_refusals_are_suppressed() {
        let mut state = SyncUiState::new();
        // Fixed instant for every call: the cap must fire regardless of
        // debounce timing once the consecutive count reaches the limit.
        let t = Instant::now();
        let actions: Vec<StaleHeadPollAction> =
            (0..6).map(|_| state.note_stale_head_refusal("child1", t)).collect();

        assert_eq!(actions[4], StaleHeadPollAction::LimitReached, "the 5th refusal must trip the limit");
        assert_eq!(
            actions[5],
            StaleHeadPollAction::Suppressed,
            "the 6th consecutive refusal must stay suppressed, not re-trip or resume sending"
        );
        // The first four are Send/Debounced depending on the (here,
        // identical) timestamps — not the focus of this test, but none of
        // them may be LimitReached or Suppressed this early.
        for (i, action) in actions.iter().take(4).enumerate() {
            assert!(
                matches!(action, StaleHeadPollAction::Send | StaleHeadPollAction::Debounced),
                "refusal {} should not yet be capped, got {:?}", i + 1, action
            );
        }
    }

    #[test]
    fn a_successful_apply_resets_the_counter_and_the_debounce_timestamp() {
        let mut state = SyncUiState::new();
        let t = Instant::now();
        for _ in 0..6 {
            state.note_stale_head_refusal("child1", t);
        }
        assert_eq!(
            state.note_stale_head_refusal("child1", t),
            StaleHeadPollAction::Suppressed,
            "precondition: the streak must be capped before the reset"
        );

        state.note_applied("child1");

        assert_eq!(
            state.note_stale_head_refusal("child1", t),
            StaleHeadPollAction::Send,
            "after a successful apply, the very next refusal must send again, not stay suppressed"
        );
    }

    /// Task 17, Part C: once `run_child_sync_cycles` loops over every
    /// registered child on a tick, the debounce/cap MUST be per-child — a
    /// noisy child tripping the cap must not suppress or delay a
    /// completely different child's own, independent, uncapped refusal.
    #[test]
    fn one_childs_capped_streak_does_not_suppress_a_different_childs_refusal() {
        let mut state = SyncUiState::new();
        let t = Instant::now();

        // Child "noisy" races its own merges hard enough to blow through
        // the cap.
        for _ in 0..6 {
            state.note_stale_head_refusal("noisy", t);
        }
        assert_eq!(
            state.note_stale_head_refusal("noisy", t),
            StaleHeadPollAction::Suppressed,
            "precondition: the noisy child's own streak must be capped"
        );

        // Child "quiet" has never refused before — a global counter would
        // already be past the cap at this point and would wrongly suppress
        // this too.
        assert_eq!(
            state.note_stale_head_refusal("quiet", t),
            StaleHeadPollAction::Send,
            "an unrelated child's first refusal must send, unaffected by another child's capped streak"
        );
    }

    /// The debounce timestamp is the other half of the same guard: a recent
    /// `PollNow` sent for one child must not debounce a different child's
    /// refusal that happens moments later.
    #[test]
    fn one_childs_recent_pollnow_does_not_debounce_a_different_childs_refusal() {
        let mut state = SyncUiState::new();
        let t0 = Instant::now();
        assert_eq!(state.note_stale_head_refusal("a", t0), StaleHeadPollAction::Send);

        let t1 = t0 + Duration::from_millis(50);
        assert_eq!(
            state.note_stale_head_refusal("b", t1),
            StaleHeadPollAction::Send,
            "child b's first-ever refusal must send even though child a just sent 50ms ago"
        );
    }

    /// Minor from Task 17 review: deregistering a child must not leave its
    /// debounce/cap state lingering forever — a later refusal for the SAME
    /// id (e.g. after re-registration) must behave exactly like a brand new
    /// child, not inherit a capped streak from before.
    #[test]
    fn forget_child_clears_both_the_debounce_timestamp_and_the_counter() {
        let mut state = SyncUiState::new();
        let t = Instant::now();
        for _ in 0..6 {
            state.note_stale_head_refusal("gone", t);
        }
        assert_eq!(
            state.note_stale_head_refusal("gone", t),
            StaleHeadPollAction::Suppressed,
            "precondition: the streak must be capped before forgetting"
        );

        state.forget_child("gone");

        assert_eq!(
            state.note_stale_head_refusal("gone", t),
            StaleHeadPollAction::Send,
            "after forget_child, the next refusal for the same id must send again, not stay \
             suppressed by state that should have been cleared"
        );
    }
}

#[cfg(test)]
mod notice_severity_tests {
    use super::*;

    #[test]
    fn blocking_notices_sort_ahead_of_informational_ones() {
        let mut state = SyncUiState::new();
        state.record_sync_failure(SyncFailureNotice {
            child_id: "child-a".to_string(),
            message: "informational".to_string(),
            severity: NoticeSeverity::Informational,
        });
        state.record_sync_failure(SyncFailureNotice {
            child_id: "child-b".to_string(),
            message: "blocking".to_string(),
            severity: NoticeSeverity::Blocking,
        });

        let ordered = state.notices_blocking_first();
        assert_eq!(ordered[0].child_id, "child-b", "a blocking notice must never be scrolled below an informational one");
    }

    #[test]
    fn a_child_with_no_blocking_notice_is_not_badged() {
        let mut state = SyncUiState::new();
        state.record_sync_failure(SyncFailureNotice {
            child_id: "child-a".to_string(),
            message: "informational".to_string(),
            severity: NoticeSeverity::Informational,
        });
        assert!(!state.has_blocking_notice());
    }
}
