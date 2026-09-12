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
            message_rx: None,
        }
    }

    /// Create a new SyncUiState with a message receiver from the sync thread
    pub fn with_receiver(rx: mpsc::Receiver<SyncMessage>) -> Self {
        Self {
            status: SyncStatus::Idle,
            conflicts: Vec::new(),
            goals_diverged: Vec::new(),
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
