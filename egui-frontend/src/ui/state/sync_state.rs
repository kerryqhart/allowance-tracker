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

use crate::backend::domain::sync_manager::{SyncMessage, SyncStatus};
use shared::sync::SyncConflict;
use std::sync::mpsc;

/// UI state for sync operations
pub struct SyncUiState {
    /// Current sync status
    pub status: SyncStatus,

    /// List of detected conflicts awaiting resolution
    pub conflicts: Vec<SyncConflict>,

    /// Receiver for messages from the background sync thread
    pub message_rx: Option<mpsc::Receiver<SyncMessage>>,
}

impl SyncUiState {
    /// Create a new SyncUiState with no receiver (sync disabled)
    pub fn new() -> Self {
        Self {
            status: SyncStatus::Disabled,
            conflicts: Vec::new(),
            message_rx: None,
        }
    }

    /// Create a new SyncUiState with a message receiver from the sync thread
    pub fn with_receiver(rx: mpsc::Receiver<SyncMessage>) -> Self {
        Self {
            status: SyncStatus::Idle,
            conflicts: Vec::new(),
            message_rx: Some(rx),
        }
    }

    /// Drain sync messages from the background thread. Call each frame.
    pub fn poll_messages(&mut self) {
        let Some(rx) = &self.message_rx else { return };

        while let Ok(msg) = rx.try_recv() {
            match msg {
                SyncMessage::StatusChanged(status) => {
                    self.status = status;
                }
                SyncMessage::ConflictDetected(conflict) => {
                    self.conflicts.push(conflict);
                    self.status = SyncStatus::HasConflicts(self.pending_conflict_count());
                }
                SyncMessage::EntitiesUpdated { .. } => {
                    // TODO: trigger data reload for the affected child
                }
                SyncMessage::PushFailed { event_id, error } => {
                    log::warn!("Sync push failed for event {}: {}", event_id, error);
                }
                SyncMessage::Error(error) => {
                    log::error!("Sync error: {}", error);
                    self.status = SyncStatus::Error(error);
                }
                // New variants handled in later tasks (Task 5: UI thread sync handler)
                SyncMessage::ReadEntityRequest { .. } => {}
                SyncMessage::ApplyRemoteEntity { .. } => {}
                SyncMessage::DeleteLocalEntity { .. } => {}
            }
        }
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
