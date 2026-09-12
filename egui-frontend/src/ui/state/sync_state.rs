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

/// UI state for sync operations
pub struct SyncUiState {
    /// Current sync status
    pub status: SyncStatus,

    /// List of detected conflicts awaiting resolution
    pub conflicts: Vec<SyncConflict>,

    /// `goals.csv` divergences a merge could not resolve, held until a
    /// future UI dismisses them. Deliberately NOT folded into `status`:
    /// `status` is last-writer-wins (see `SyncMessage::GoalsDiverged`'s doc
    /// comment), so a notice routed through it would be erased by the very
    /// next unrelated sync event.
    pub goals_diverged: Vec<GoalsDivergedNotice>,

    /// When a `SyncCommand::PollNow` was last sent in response to a
    /// stale-head merge refusal. `None` means either none has been sent
    /// yet, or the counter was just reset by a successful apply.
    last_stale_head_pollnow_at: Option<Instant>,

    /// Consecutive stale-head refusals since the last successful apply.
    /// Reset to 0 by [`SyncUiState::note_applied`].
    consecutive_stale_head_refusals: u32,

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
            last_stale_head_pollnow_at: None,
            consecutive_stale_head_refusals: 0,
            message_rx: None,
        }
    }

    /// Create a new SyncUiState with a message receiver from the sync thread
    pub fn with_receiver(rx: mpsc::Receiver<SyncMessage>) -> Self {
        Self {
            status: SyncStatus::Idle,
            conflicts: Vec::new(),
            goals_diverged: Vec::new(),
            last_stale_head_pollnow_at: None,
            consecutive_stale_head_refusals: 0,
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

    /// Record one stale-head merge refusal at `now` and decide whether the
    /// caller should send an immediate `SyncCommand::PollNow`. See
    /// `STALE_HEAD_POLL_DEBOUNCE` and `STALE_HEAD_REFUSAL_LIMIT` for the
    /// two guards. The consecutive-refusal count increments on every call
    /// regardless of the returned action; only [`Self::note_applied`]
    /// resets it.
    pub fn note_stale_head_refusal(&mut self, now: Instant) -> StaleHeadPollAction {
        self.consecutive_stale_head_refusals += 1;

        // The cap takes priority over debouncing: once tripped, stay
        // silent even if the debounce window has independently elapsed.
        if self.consecutive_stale_head_refusals > STALE_HEAD_REFUSAL_LIMIT {
            return StaleHeadPollAction::Suppressed;
        }
        if self.consecutive_stale_head_refusals == STALE_HEAD_REFUSAL_LIMIT {
            return StaleHeadPollAction::LimitReached;
        }

        let should_send = match self.last_stale_head_pollnow_at {
            None => true,
            Some(last) => now.duration_since(last) >= STALE_HEAD_POLL_DEBOUNCE,
        };
        if should_send {
            self.last_stale_head_pollnow_at = Some(now);
            StaleHeadPollAction::Send
        } else {
            StaleHeadPollAction::Debounced
        }
    }

    /// A merge was successfully applied — the stale-head streak (if any)
    /// is over. Resets both the debounce timestamp and the consecutive
    /// counter, so a fresh bout of stale-head races starts from a clean
    /// state rather than staying suppressed forever.
    pub fn note_applied(&mut self) {
        self.last_stale_head_pollnow_at = None;
        self.consecutive_stale_head_refusals = 0;
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
        assert_eq!(state.note_stale_head_refusal(t0), StaleHeadPollAction::Send);
        let t1 = t0 + Duration::from_millis(100);
        assert_eq!(
            state.note_stale_head_refusal(t1),
            StaleHeadPollAction::Debounced,
            "a refusal 100ms after the last send must be debounced, not re-sent"
        );
    }

    #[test]
    fn a_refusal_after_the_debounce_interval_sends_again() {
        let mut state = SyncUiState::new();
        let t0 = Instant::now();
        assert_eq!(state.note_stale_head_refusal(t0), StaleHeadPollAction::Send);
        let t1 = t0 + Duration::from_millis(600);
        assert_eq!(
            state.note_stale_head_refusal(t1),
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
            (0..6).map(|_| state.note_stale_head_refusal(t)).collect();

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
            state.note_stale_head_refusal(t);
        }
        assert_eq!(
            state.note_stale_head_refusal(t),
            StaleHeadPollAction::Suppressed,
            "precondition: the streak must be capped before the reset"
        );

        state.note_applied();

        assert_eq!(
            state.note_stale_head_refusal(t),
            StaleHeadPollAction::Send,
            "after a successful apply, the very next refusal must send again, not stay suppressed"
        );
    }
}
