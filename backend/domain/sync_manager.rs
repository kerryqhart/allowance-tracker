use anyhow::Result;
use shared::sync::*;
use crate::backend::storage::remote::RemoteStorage;
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::mpsc;
use crate::backend::domain::models::child::Child;
use crate::backend::domain::models::transaction::Transaction;
use crate::backend::domain::models::goal::DomainGoal;

/// Control signals from UI thread to sync thread
#[derive(Debug)]
pub enum SyncCommand {
    /// App gained focus — poll remote immediately
    PollNow,
    /// App closing — flush pending work and exit
    Shutdown,
}

/// Messages from the sync background thread to the UI.
pub enum SyncMessage {
    StatusChanged(SyncStatus),
    EntitiesUpdated { child_id: String, entity_type: EntityType, count: usize },
    ConflictDetected(SyncConflict),
    PushFailed { event_id: String, error: String },
    Error(String),

    // New — sync thread needs entity data for pushing to remote
    ReadEntityRequest {
        child_id: String,
        entity_type: EntityType,
        entity_id: String,
        response_tx: std::sync::mpsc::Sender<Option<String>>,
    },

    // New — sync thread pulled a remote entity, UI thread should apply it
    ApplyRemoteEntity {
        child_id: String,
        entity_type: EntityType,
        entity_id: String,
        entity_json: String,
        event_id: String,
    },

    // New — sync thread pulled a remote delete
    DeleteLocalEntity {
        child_id: String,
        entity_type: EntityType,
        entity_id: String,
        event_id: String,
    },
}

impl std::fmt::Debug for SyncMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncMessage::StatusChanged(s) => f.debug_tuple("StatusChanged").field(s).finish(),
            SyncMessage::EntitiesUpdated { child_id, entity_type, count } => f
                .debug_struct("EntitiesUpdated")
                .field("child_id", child_id)
                .field("entity_type", entity_type)
                .field("count", count)
                .finish(),
            SyncMessage::ConflictDetected(c) => f.debug_tuple("ConflictDetected").field(c).finish(),
            SyncMessage::PushFailed { event_id, error } => f
                .debug_struct("PushFailed")
                .field("event_id", event_id)
                .field("error", error)
                .finish(),
            SyncMessage::Error(e) => f.debug_tuple("Error").field(e).finish(),
            SyncMessage::ReadEntityRequest { child_id, entity_type, entity_id, .. } => f
                .debug_struct("ReadEntityRequest")
                .field("child_id", child_id)
                .field("entity_type", entity_type)
                .field("entity_id", entity_id)
                .finish(),
            SyncMessage::ApplyRemoteEntity { child_id, entity_type, entity_id, entity_json, event_id } => f
                .debug_struct("ApplyRemoteEntity")
                .field("child_id", child_id)
                .field("entity_type", entity_type)
                .field("entity_id", entity_id)
                .field("entity_json", entity_json)
                .field("event_id", event_id)
                .finish(),
            SyncMessage::DeleteLocalEntity { child_id, entity_type, entity_id, event_id } => f
                .debug_struct("DeleteLocalEntity")
                .field("child_id", child_id)
                .field("entity_type", entity_type)
                .field("entity_id", entity_id)
                .field("event_id", event_id)
                .finish(),
        }
    }
}

/// Progress messages from the backfill operation to the UI.
#[derive(Debug, Clone)]
pub enum BackfillProgress {
    Starting { total_entities: usize },
    ChildInitialized { child_name: String },
    EntitiesPushed { count: usize, total: usize },
    Completed { total_pushed: usize },
    Failed { error: String, pushed_so_far: usize },
}

/// Result of a completed backfill operation.
#[derive(Debug, Clone)]
pub struct BackfillResult {
    pub children_synced: usize,
    pub transactions_synced: usize,
    pub goals_synced: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Disabled,
    Idle,
    Syncing,
    Error(String),
    HasConflicts(usize),
}

/// Core sync logic. Not tied to threading — can be tested synchronously.
pub struct SyncEngine {
    remote: Arc<dyn RemoteStorage>,
    /// Outbound events that haven't been pushed yet (or failed to push).
    pending_push: Vec<SyncEvent>,
    /// Pending conflicts awaiting user resolution.
    conflicts: Vec<SyncConflict>,
    /// Per-child watermarks (local cache of what we've processed).
    watermarks: HashMap<String, u64>,
}

impl SyncEngine {
    pub fn new(remote: Arc<dyn RemoteStorage>) -> Self {
        Self {
            remote,
            pending_push: Vec::new(),
            conflicts: Vec::new(),
            watermarks: HashMap::new(),
        }
    }

    /// Enqueue a local event for pushing to remote.
    pub fn enqueue_event(&mut self, event: SyncEvent) {
        self.pending_push.push(event);
    }

    /// Push all pending events to remote. Returns events that failed.
    pub fn push_pending(&mut self) -> Vec<(SyncEvent, String)> {
        let events = std::mem::take(&mut self.pending_push);
        if events.is_empty() {
            return Vec::new();
        }

        match self.remote.push_events(&events) {
            Ok(_sequences) => Vec::new(),
            Err(e) => {
                let error_msg = e.to_string();
                // All events failed — put them back for retry
                let failures: Vec<(SyncEvent, String)> = events
                    .into_iter()
                    .map(|ev| (ev, error_msg.clone()))
                    .collect();
                failures
            }
        }
    }

    /// Poll remote for new events for a child. Returns events to apply locally
    /// and any conflicts detected.
    pub fn poll_child(&mut self, child_id: &str) -> Result<PollResult> {
        // Don't poll if there are pending conflicts for this child
        if self.conflicts.iter().any(|c| c.child_id == child_id && c.status == ConflictStatus::Pending) {
            return Ok(PollResult { events_to_apply: Vec::new(), new_conflicts: Vec::new() });
        }

        let watermark = *self.watermarks.get(child_id).unwrap_or(&0);
        let remote_events = self.remote.get_events_since(child_id, watermark)?;

        if remote_events.is_empty() {
            return Ok(PollResult { events_to_apply: Vec::new(), new_conflicts: Vec::new() });
        }

        let mut to_apply = Vec::new();
        let mut new_conflicts = Vec::new();

        for remote_event in remote_events {
            if remote_event.source == SyncSource::Local {
                // This event originated from us — skip, we already have it locally
                if let Some(seq) = remote_event.sequence {
                    self.watermarks.insert(child_id.to_string(), seq);
                }
                continue;
            }

            // Check for conflict: do we have a pending outbound event for the same entity?
            let conflicting_local = self.pending_push.iter().find(|local| {
                local.entity_type == remote_event.entity_type
                    && local.entity_id == remote_event.entity_id
                    && local.child_id == child_id
            });

            if let Some(local_event) = conflicting_local {
                // Both sides deleted? Auto-resolve.
                if local_event.action == SyncAction::Deleted && remote_event.action == SyncAction::Deleted {
                    if let Some(seq) = remote_event.sequence {
                        self.watermarks.insert(child_id.to_string(), seq);
                    }
                    continue;
                }

                let conflict = SyncConflict {
                    id: uuid::Uuid::new_v4().to_string(),
                    entity_type: remote_event.entity_type.clone(),
                    entity_id: remote_event.entity_id.clone(),
                    child_id: child_id.to_string(),
                    local_event: local_event.clone(),
                    remote_event: remote_event.clone(),
                    status: ConflictStatus::Pending,
                };
                new_conflicts.push(conflict.clone());
                self.conflicts.push(conflict);
            } else {
                to_apply.push(remote_event.clone());
                if let Some(seq) = remote_event.sequence {
                    self.watermarks.insert(child_id.to_string(), seq);
                }
            }
        }

        Ok(PollResult { events_to_apply: to_apply, new_conflicts })
    }

    /// Get all pending conflicts.
    pub fn pending_conflicts(&self) -> &[SyncConflict] {
        &self.conflicts
    }

    /// Resolve a conflict. Returns the resolution event to push to remote (if Keep Local or Merged).
    pub fn resolve_conflict(&mut self, conflict_id: &str, resolution: ConflictStatus) -> Option<SyncEvent> {
        if let Some(conflict) = self.conflicts.iter_mut().find(|c| c.id == conflict_id) {
            conflict.status = resolution.clone();

            match resolution {
                ConflictStatus::ResolvedKeepLocal => {
                    // Advance watermark past the remote event we're discarding
                    if let Some(seq) = conflict.remote_event.sequence {
                        self.watermarks.insert(conflict.child_id.clone(), seq);
                    }
                    // Push local state as the winner
                    Some(SyncEvent::new(
                        conflict.entity_type.clone(),
                        conflict.entity_id.clone(),
                        conflict.child_id.clone(),
                        conflict.local_event.action.clone(),
                        SyncSource::Local,
                    ))
                }
                ConflictStatus::ResolvedKeepRemote => {
                    // Advance watermark
                    if let Some(seq) = conflict.remote_event.sequence {
                        self.watermarks.insert(conflict.child_id.clone(), seq);
                    }
                    // Remove the local pending event for this entity
                    self.pending_push.retain(|e| {
                        !(e.entity_type == conflict.entity_type
                            && e.entity_id == conflict.entity_id
                            && e.child_id == conflict.child_id)
                    });
                    None // No event to push — remote already has the right state
                }
                ConflictStatus::ResolvedMerged => {
                    if let Some(seq) = conflict.remote_event.sequence {
                        self.watermarks.insert(conflict.child_id.clone(), seq);
                    }
                    // Merged version will be pushed as a new Updated event
                    Some(SyncEvent::new(
                        conflict.entity_type.clone(),
                        conflict.entity_id.clone(),
                        conflict.child_id.clone(),
                        SyncAction::Updated,
                        SyncSource::Local,
                    ))
                }
                _ => None,
            }
        } else {
            None
        }
    }

    /// Load watermarks from persisted state.
    pub fn set_watermark(&mut self, child_id: &str, watermark: u64) {
        self.watermarks.insert(child_id.to_string(), watermark);
    }

    /// Get the current watermark for a child.
    pub fn get_watermark(&self, child_id: &str) -> u64 {
        *self.watermarks.get(child_id).unwrap_or(&0)
    }

    /// Get the number of pending outbound events.
    pub fn pending_push_count(&self) -> usize {
        self.pending_push.len()
    }

    /// Push all local entities to remote. Reports progress via the channel.
    /// Safe to retry: entity upserts overwrite, sync event dedup prevents duplicate sequences.
    pub fn backfill(
        &self,
        children: Vec<Child>,
        transactions: HashMap<String, Vec<Transaction>>,
        goals: HashMap<String, Vec<DomainGoal>>,
        progress_tx: mpsc::Sender<BackfillProgress>,
    ) -> Result<BackfillResult> {
        let total = children.len()
            + transactions.values().map(|v| v.len()).sum::<usize>()
            + goals.values().map(|v| v.len()).sum::<usize>();

        let _ = progress_tx.send(BackfillProgress::Starting { total_entities: total });

        let mut pushed = 0usize;
        let mut children_synced = 0usize;
        let mut transactions_synced = 0usize;
        let mut goals_synced = 0usize;
        let batch_size = 25;

        for child in &children {
            if let Err(e) = self.remote.initialize_child(&child.id) {
                let _ = progress_tx.send(BackfillProgress::Failed {
                    error: format!("Failed to initialize child {}: {}", child.name, e),
                    pushed_so_far: pushed,
                });
                return Err(e);
            }
            let _ = progress_tx.send(BackfillProgress::ChildInitialized {
                child_name: child.name.clone(),
            });

            let child_json = serde_json::to_string(&child)
                .map_err(|e| anyhow::anyhow!("Failed to serialize child: {}", e))?;
            self.remote.upsert_entity(&child.id, EntityType::Child, &child.id, &child_json)?;

            let mut events = vec![SyncEvent::new(
                EntityType::Child,
                child.id.clone(),
                child.id.clone(),
                SyncAction::Created,
                SyncSource::Local,
            )];
            pushed += 1;
            children_synced += 1;

            if let Some(txns) = transactions.get(&child.id) {
                for tx in txns {
                    let tx_json = serde_json::to_string(&tx)
                        .map_err(|e| anyhow::anyhow!("Failed to serialize transaction: {}", e))?;
                    self.remote.upsert_entity(&child.id, EntityType::Transaction, &tx.id, &tx_json)?;

                    events.push(SyncEvent::new(
                        EntityType::Transaction,
                        tx.id.clone(),
                        child.id.clone(),
                        SyncAction::Created,
                        SyncSource::Local,
                    ));
                    pushed += 1;
                    transactions_synced += 1;

                    if events.len() >= batch_size {
                        self.remote.push_events(&events)?;
                        let _ = progress_tx.send(BackfillProgress::EntitiesPushed { count: pushed, total });
                        events.clear();
                    }
                }
            }

            if let Some(child_goals) = goals.get(&child.id) {
                for goal in child_goals {
                    let goal_json = serde_json::to_string(&goal)
                        .map_err(|e| anyhow::anyhow!("Failed to serialize goal: {}", e))?;
                    self.remote.upsert_entity(&child.id, EntityType::Goal, &goal.id, &goal_json)?;

                    events.push(SyncEvent::new(
                        EntityType::Goal,
                        goal.id.clone(),
                        child.id.clone(),
                        SyncAction::Created,
                        SyncSource::Local,
                    ));
                    pushed += 1;
                    goals_synced += 1;

                    if events.len() >= batch_size {
                        self.remote.push_events(&events)?;
                        let _ = progress_tx.send(BackfillProgress::EntitiesPushed { count: pushed, total });
                        events.clear();
                    }
                }
            }

            if !events.is_empty() {
                self.remote.push_events(&events)?;
                let _ = progress_tx.send(BackfillProgress::EntitiesPushed { count: pushed, total });
            }
        }

        let _ = progress_tx.send(BackfillProgress::Completed { total_pushed: pushed });

        Ok(BackfillResult {
            children_synced,
            transactions_synced,
            goals_synced,
        })
    }
}

pub struct PollResult {
    pub events_to_apply: Vec<SyncEvent>,
    pub new_conflicts: Vec<SyncConflict>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::mock_remote::MockRemoteClient;

    fn make_engine() -> SyncEngine {
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();
        SyncEngine::new(mock)
    }

    fn make_engine_with_mock() -> (SyncEngine, Arc<MockRemoteClient>) {
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();
        let engine = SyncEngine::new(mock.clone());
        (engine, mock)
    }

    #[test]
    fn test_enqueue_and_push() {
        let mut engine = make_engine();

        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        engine.enqueue_event(event);
        assert_eq!(engine.pending_push_count(), 1);

        let failures = engine.push_pending();
        assert!(failures.is_empty());
        assert_eq!(engine.pending_push_count(), 0);
    }

    #[test]
    fn test_push_failure_returns_events() {
        let mock = Arc::new(MockRemoteClient::new());
        mock.force_error("network error");
        let mut engine = SyncEngine::new(mock);

        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        engine.enqueue_event(event);

        let failures = engine.push_pending();
        assert_eq!(failures.len(), 1);
        assert!(failures[0].1.contains("network error"));
    }

    #[test]
    fn test_poll_applies_remote_events() {
        let (mut engine, mock) = make_engine_with_mock();

        // Simulate a remote event
        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx_remote".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let result = engine.poll_child("child1").unwrap();
        assert_eq!(result.events_to_apply.len(), 1);
        assert_eq!(result.events_to_apply[0].entity_id, "tx_remote");
        assert!(result.new_conflicts.is_empty());
        assert_eq!(engine.get_watermark("child1"), 1);
    }

    #[test]
    fn test_poll_skips_local_source_events() {
        let (mut engine, mock) = make_engine_with_mock();

        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx_local".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        mock.push_events(&[local_event]).unwrap();

        let result = engine.poll_child("child1").unwrap();
        assert!(result.events_to_apply.is_empty());
        assert_eq!(engine.get_watermark("child1"), 1);
    }

    #[test]
    fn test_conflict_detected() {
        let (mut engine, mock) = make_engine_with_mock();

        // Local has a pending event for tx1
        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        // Remote also modified tx1
        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let result = engine.poll_child("child1").unwrap();
        assert!(result.events_to_apply.is_empty());
        assert_eq!(result.new_conflicts.len(), 1);
        assert_eq!(engine.pending_conflicts().len(), 1);
    }

    #[test]
    fn test_conflict_both_deleted_auto_resolves() {
        let (mut engine, mock) = make_engine_with_mock();

        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Deleted, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Deleted, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let result = engine.poll_child("child1").unwrap();
        assert!(result.events_to_apply.is_empty());
        assert!(result.new_conflicts.is_empty());
    }

    #[test]
    fn test_conflict_blocks_further_polls() {
        let (mut engine, mock) = make_engine_with_mock();

        // Create a conflict
        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();
        engine.poll_child("child1").unwrap();

        // Push another remote event
        let remote_event2 = SyncEvent::new(
            EntityType::Transaction, "tx2".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Remote,
        );
        mock.push_events(&[remote_event2]).unwrap();

        // Poll should return empty because of pending conflict
        let result = engine.poll_child("child1").unwrap();
        assert!(result.events_to_apply.is_empty());
    }

    #[test]
    fn test_resolve_conflict_keep_local() {
        let (mut engine, mock) = make_engine_with_mock();

        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        engine.poll_child("child1").unwrap();

        let conflict_id = engine.pending_conflicts()[0].id.clone();
        let resolution_event = engine.resolve_conflict(&conflict_id, ConflictStatus::ResolvedKeepLocal);

        assert!(resolution_event.is_some());
        assert_eq!(resolution_event.unwrap().source, SyncSource::Local);
    }

    #[test]
    fn test_resolve_conflict_keep_remote() {
        let (mut engine, mock) = make_engine_with_mock();

        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        engine.poll_child("child1").unwrap();

        let conflict_id = engine.pending_conflicts()[0].id.clone();
        let resolution_event = engine.resolve_conflict(&conflict_id, ConflictStatus::ResolvedKeepRemote);

        assert!(resolution_event.is_none()); // No event to push
        // The local pending event for tx1 should be removed
        assert_eq!(engine.pending_push_count(), 0);
    }

    #[test]
    fn test_no_conflict_different_entities() {
        let (mut engine, mock) = make_engine_with_mock();

        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx_a".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx_b".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let result = engine.poll_child("child1").unwrap();
        assert_eq!(result.events_to_apply.len(), 1);
        assert!(result.new_conflicts.is_empty());
    }

    #[test]
    fn test_no_conflict_different_entity_types() {
        let (mut engine, mock) = make_engine_with_mock();

        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        let remote_event = SyncEvent::new(
            EntityType::Goal, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let result = engine.poll_child("child1").unwrap();
        assert_eq!(result.events_to_apply.len(), 1);
        assert!(result.new_conflicts.is_empty());
    }

    #[test]
    fn test_backfill_pushes_all_entities() {
        let (engine, _mock) = make_engine_with_mock();
        let (tx, rx) = mpsc::channel();

        let child = super::super::models::child::Child {
            id: "child1".to_string(),
            name: "Alice".to_string(),
            birthdate: chrono::NaiveDate::from_ymd_opt(2018, 1, 1).unwrap(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let transaction = super::super::models::transaction::Transaction {
            id: "tx-1".to_string(),
            child_id: "child1".to_string(),
            date: chrono::Utc::now().fixed_offset(),
            description: "Allowance".to_string(),
            amount: 10.0,
            balance: 10.0,
            transaction_type: super::super::models::transaction::TransactionType::Allowance,
        };

        let goal = super::super::models::goal::DomainGoal {
            id: "goal-1".to_string(),
            child_id: "child1".to_string(),
            description: "Bicycle".to_string(),
            target_amount: 100.0,
            state: super::super::models::goal::DomainGoalState::Active,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let mut transactions_map = std::collections::HashMap::new();
        transactions_map.insert("child1".to_string(), vec![transaction]);
        let mut goals_map = std::collections::HashMap::new();
        goals_map.insert("child1".to_string(), vec![goal]);

        let result = engine.backfill(vec![child], transactions_map, goals_map, tx).unwrap();

        assert_eq!(result.children_synced, 1);
        assert_eq!(result.transactions_synced, 1);
        assert_eq!(result.goals_synced, 1);

        // Verify progress messages
        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }
        assert!(messages.len() >= 3);
    }
}
