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

    // New — sync thread needs the current list of local child IDs to know
    // which children to poll for remote events. Populated from ChildService
    // on the UI thread (architecture: UI owns all repo I/O).
    GetChildIdsRequest {
        response_tx: std::sync::mpsc::Sender<Vec<String>>,
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
            SyncMessage::GetChildIdsRequest { .. } => f
                .debug_struct("GetChildIdsRequest")
                .finish(),
            SyncMessage::ApplyRemoteEntity { child_id, entity_type, entity_id, entity_json, event_id } => f
                .debug_struct("ApplyRemoteEntity")
                .field("child_id", child_id)
                .field("entity_type", entity_type)
                .field("entity_id", entity_id)
                .field("entity_json", &format_args!("<{} bytes>", entity_json.len()))
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

/// Callback invoked whenever the sync thread sends a message to the UI.
/// Used to wake the egui UI thread (via `ctx.request_repaint()`) so that
/// pending `SyncMessage`s are drained promptly even while the window is
/// unfocused — without this the UI only repaints on input events and
/// request/response messages (`ReadEntityRequest`, `GetChildIdsRequest`)
/// time out while the app is backgrounded.
pub type WakeUi = Arc<dyn Fn() + Send + Sync>;

/// Wraps the `SyncMessage` sender with a wake-UI callback so every send
/// is paired with a repaint request.
pub struct UiMessenger {
    tx: mpsc::Sender<SyncMessage>,
    wake: WakeUi,
}

impl UiMessenger {
    pub fn new(tx: mpsc::Sender<SyncMessage>, wake: WakeUi) -> Self {
        Self { tx, wake }
    }

    pub fn send(&self, msg: SyncMessage) -> Result<(), mpsc::SendError<SyncMessage>> {
        let result = self.tx.send(msg);
        (self.wake)();
        result
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
    /// Per-child watermarks (local cache of what we've processed).
    watermarks: HashMap<String, u64>,
}

impl SyncEngine {
    pub fn new(remote: Arc<dyn RemoteStorage>) -> Self {
        Self {
            remote,
            pending_push: Vec::new(),
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

    /// Poll remote for new events for a child. Returns events to apply locally.
    /// Last-write-wins: no conflict detection — remote events are always applied.
    pub fn poll_child(&mut self, child_id: &str) -> Result<PollResult> {
        let watermark = *self.watermarks.get(child_id).unwrap_or(&0);
        let remote_events = self.remote.get_events_since(child_id, watermark)?;

        let mut events_to_apply = Vec::new();
        let mut max_sequence = watermark;

        for event in remote_events {
            let seq = event.sequence.unwrap_or(0);
            if seq > max_sequence {
                max_sequence = seq;
            }
            // Skip our own events echoed back
            if event.source == SyncSource::Local {
                continue;
            }
            events_to_apply.push(event);
        }

        if max_sequence > watermark {
            self.watermarks.insert(child_id.to_string(), max_sequence);
        }

        Ok(PollResult { events_to_apply })
    }

    /// Returns a snapshot of all current watermarks.
    pub fn watermarks_snapshot(&self) -> HashMap<String, u64> {
        self.watermarks.clone()
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
        mock.seed_event(remote_event);

        let result = engine.poll_child("child1").unwrap();
        assert_eq!(result.events_to_apply.len(), 1);
        assert_eq!(result.events_to_apply[0].entity_id, "tx_remote");
        assert_eq!(engine.get_watermark("child1"), 1);
    }

    #[test]
    fn test_poll_skips_local_source_events() {
        let (mut engine, mock) = make_engine_with_mock();

        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx_local".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        mock.seed_event(local_event);

        let result = engine.poll_child("child1").unwrap();
        assert!(result.events_to_apply.is_empty());
        assert_eq!(engine.get_watermark("child1"), 1);
    }

    #[test]
    fn test_poll_returns_all_non_local_events_different_entities() {
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
        mock.seed_event(remote_event);

        let result = engine.poll_child("child1").unwrap();
        assert_eq!(result.events_to_apply.len(), 1);
        assert_eq!(result.events_to_apply[0].entity_id, "tx_b");
    }

    #[test]
    fn test_poll_returns_all_non_local_events_different_entity_types() {
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
        mock.seed_event(remote_event);

        let result = engine.poll_child("child1").unwrap();
        assert_eq!(result.events_to_apply.len(), 1);
    }

    #[test]
    fn test_last_write_wins_no_conflict_detection() {
        let (mut engine, mock) = make_engine_with_mock();

        // Local has a pending event for tx1
        let local_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Local,
        );
        engine.enqueue_event(local_event);

        // Remote also modified tx1 — under last-write-wins, this should just be applied
        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Updated, SyncSource::Remote,
        );
        mock.seed_event(remote_event);

        let result = engine.poll_child("child1").unwrap();
        // Remote event is returned to apply — no conflict
        assert_eq!(result.events_to_apply.len(), 1);
        // Local pending push is unchanged — it'll still get pushed next
        assert_eq!(engine.pending_push_count(), 1);
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
