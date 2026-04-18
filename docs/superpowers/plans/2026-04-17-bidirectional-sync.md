# Bidirectional Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable two-way synchronization between the local desktop app and the remote AWS sync-service, so changes flow in both directions automatically.

**Architecture:** UI thread handles all repository I/O. Sync thread handles all network I/O. Communication via three mpsc channels: SyncNotifier (local events), SyncCommand (control signals), SyncMessage (sync→UI requests/updates). Event dedup via event_id prevents sync loops. Last-write-wins for conflicts.

**Tech Stack:** Rust, egui, std::sync::mpsc, std::thread, serde_json

**Spec:** `docs/superpowers/specs/2026-04-17-bidirectional-sync-design.md`

---

### Task 1: Add SyncCommand and extend SyncMessage types

**Files:**
- Modify: `backend/domain/sync_manager.rs` (SyncMessage enum, add SyncCommand)
- Modify: `backend/domain/mod.rs` (re-export SyncCommand)

Pure type changes — no behavior modifications. Everything compiles, existing tests pass.

- [ ] **Step 1: Read existing SyncMessage and module exports**

Read `backend/domain/sync_manager.rs` and `backend/domain/mod.rs` to understand current types and exports.

- [ ] **Step 2: Add SyncCommand enum**

In `backend/domain/sync_manager.rs`, add before the `SyncMessage` enum:

```rust
/// Control signals from UI thread to sync thread
#[derive(Debug)]
pub enum SyncCommand {
    /// App gained focus — poll remote immediately
    PollNow,
    /// App closing — flush pending work and exit
    Shutdown,
}
```

- [ ] **Step 3: Extend SyncMessage with new variants**

Add new variants to the existing `SyncMessage` enum. The `ReadEntityRequest` needs a response channel — use `std::sync::mpsc::Sender<Option<String>>`:

```rust
pub enum SyncMessage {
    // Existing variants (keep as-is)
    StatusChanged(SyncStatus),
    EntitiesUpdated {
        child_id: String,
        entity_type: EntityType,
        count: usize,
    },
    ConflictDetected(SyncConflict),
    PushFailed {
        event_id: String,
        error: String,
    },
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
```

Note: `SyncMessage` cannot derive `Debug` anymore because `mpsc::Sender` doesn't implement `Debug`. Remove the derive if present, or implement `Debug` manually for the variants that need it.

- [ ] **Step 4: Update mod.rs exports**

In `backend/domain/mod.rs`, add `SyncCommand` to the re-exports:

```rust
pub use sync_manager::{SyncEngine, SyncMessage, SyncStatus, SyncCommand, PollResult};
```

- [ ] **Step 5: Verify compilation**

Run: `cargo build --package backend`
Expected: Compiles. Existing tests pass.

- [ ] **Step 6: Commit**

```
git add backend/domain/sync_manager.rs backend/domain/mod.rs
git commit -m "feat: add SyncCommand and extend SyncMessage for bidirectional sync"
```

---

### Task 2: Inject SyncNotifier into domain services

**Files:**
- Modify: `backend/domain/transaction_service.rs`
- Modify: `backend/domain/goal_service.rs`
- Modify: `backend/domain/child_service.rs`
- Modify: `backend/mod.rs` (Backend construction)
- Test: `backend/domain/transaction_service.rs` (inline tests or existing test file)

Add `Option<SyncNotifier>` to each service. Fire `SyncEvent` after each successful write. The notifier is optional so the app works without sync enabled.

- [ ] **Step 1: Read existing service constructors and write methods**

Read the three service files and `backend/mod.rs` to understand current constructor signatures and all write methods.

- [ ] **Step 2: Add SyncNotifier to TransactionService**

In `backend/domain/transaction_service.rs`:

Add field:
```rust
pub struct TransactionService {
    transaction_repository: TransactionRepository,
    child_service: ChildService,
    allowance_service: AllowanceService,
    balance_service: BalanceService,
    email_service: Option<EmailServiceWrapper>,
    sync_notifier: Option<SyncNotifier>,  // NEW
}
```

Update constructors (`new` and `with_email_service`) to accept `sync_notifier: Option<SyncNotifier>` and pass it through.

Add a helper method:
```rust
fn notify_sync(&self, entity_type: EntityType, entity_id: &str, child_id: &str, action: SyncAction) {
    if let Some(ref notifier) = self.sync_notifier {
        notifier.notify(SyncEvent::new(
            entity_type,
            entity_id.to_string(),
            child_id.to_string(),
            action,
            SyncSource::Local,
        ));
    }
}
```

Add `notify_sync` calls after successful writes in:
- `create_transaction_domain` — after `transaction_repository.store_transaction` succeeds, call `self.notify_sync(EntityType::Transaction, &transaction.id, &command.child_id, SyncAction::Created)`
- `delete_transactions_domain` — after successful delete, call `self.notify_sync(EntityType::Transaction, &id, &cmd.child_id, SyncAction::Deleted)` for each deleted transaction ID
- `create_allowance_transaction` — after `store_transaction` succeeds, same as create

Required imports: `use shared::sync::{SyncEvent, SyncAction, SyncSource, EntityType};` and `use crate::domain::SyncNotifier;`

- [ ] **Step 3: Add SyncNotifier to GoalService**

Same pattern. Add `sync_notifier: Option<SyncNotifier>` field. Update constructor. Add `notify_sync` helper.

Fire notifications in:
- `create_goal` — after `goal_repository.store_goal`, notify Created
- `update_goal` — after `goal_repository.update_goal`, notify Updated
- `cancel_goal` — after `goal_repository.cancel_current_goal`, notify Updated (state change, not delete)
- `check_and_complete_goals` — after `goal_repository.complete_current_goal`, notify Updated

- [ ] **Step 4: Add SyncNotifier to ChildService**

Same pattern. Add `sync_notifier: Option<SyncNotifier>` field. Update constructor.

Fire notifications in:
- `create_child` — after store, notify Created
- `update_child` — after update, notify Updated
- `delete_child` — after delete, notify Deleted

Note: `ChildService` derives `Clone`. Adding `Option<SyncNotifier>` is fine — `SyncNotifier` is `Clone` (wraps `mpsc::Sender` which is `Clone`).

- [ ] **Step 5: Update Backend::new to accept and pass SyncNotifier**

In `backend/mod.rs`, update `Backend::new` to accept `sync_notifier: Option<SyncNotifier>`:

```rust
pub fn new(sync_notifier: Option<SyncNotifier>) -> Result<Self>
```

Pass `sync_notifier.clone()` to each service constructor. `SyncNotifier` is `Clone`, so each service gets its own clone of the sender.

- [ ] **Step 6: Update all callers of Backend::new**

Search for `Backend::new()` calls. The primary caller is in `egui-frontend/src/ui/state/app_state.rs` or `app_coordinator.rs`. Update to pass `None` for now — wiring the real notifier happens in Task 6.

```rust
let backend = Backend::new(None)?;
```

- [ ] **Step 7: Verify compilation and tests**

Run: `cargo build`
Run: `cargo test --package backend`
Expected: All pass. No behavioral change yet — sync_notifier is `None` everywhere.

- [ ] **Step 8: Commit**

```
git add backend/domain/transaction_service.rs backend/domain/goal_service.rs backend/domain/child_service.rs backend/mod.rs
```
Also add any frontend files that call `Backend::new`.
```
git commit -m "feat: inject SyncNotifier into domain services for sync event firing"
```

---

### Task 3: Simplify SyncEngine for last-write-wins

**Files:**
- Modify: `backend/domain/sync_manager.rs` (SyncEngine)
- Modify: tests in same file or test module

Remove conflict detection/resolution from SyncEngine. `poll_child` returns only `events_to_apply` (no `new_conflicts`). Remove `pending_conflicts`, `resolve_conflict` methods. Keep `enqueue_event`, `push_pending`, `poll_child`, watermark management, and `backfill`.

- [ ] **Step 1: Read current SyncEngine implementation and tests**

Read `backend/domain/sync_manager.rs` fully — understand the conflict detection logic in `poll_child` and what tests exist.

- [ ] **Step 2: Simplify PollResult**

Change `PollResult` to remove conflicts:

```rust
pub struct PollResult {
    pub events_to_apply: Vec<SyncEvent>,
}
```

- [ ] **Step 3: Remove conflict fields and methods from SyncEngine**

Remove from struct:
```rust
// Remove: conflicts: Vec<SyncConflict>,
```

Remove methods:
```rust
// Remove: pub fn pending_conflicts()
// Remove: pub fn resolve_conflict()
```

- [ ] **Step 4: Simplify poll_child — remove conflict detection**

The current `poll_child` checks if a remote event conflicts with a pending local event. For last-write-wins, skip this check — just return all remote events as `events_to_apply`. The only filtering to keep: skip events with `source == Local` (these are our own events echoed back).

```rust
pub fn poll_child(&mut self, child_id: &str) -> Result<PollResult> {
    let watermark = self.get_watermark(child_id);
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
        self.set_watermark(child_id, max_sequence);
    }

    Ok(PollResult { events_to_apply })
}
```

- [ ] **Step 5: Update or remove conflict-related tests**

Remove tests that test conflict detection/resolution. Keep tests for push_pending, poll_child (simplified), watermark management, and backfill.

- [ ] **Step 6: Remove ConflictDetected from SyncMessage if unused**

Check if `ConflictDetected` variant is still referenced anywhere. If only used in code being removed, remove it from `SyncMessage`. Keep it if there are references outside of SyncEngine (e.g., SyncUiState) — those can be cleaned up later.

- [ ] **Step 7: Verify compilation and tests**

Run: `cargo build --package backend`
Run: `cargo test --package backend`
Expected: All remaining tests pass.

- [ ] **Step 8: Commit**

```
git add backend/domain/sync_manager.rs
git commit -m "refactor: simplify SyncEngine to last-write-wins, remove conflict resolution"
```

---

### Task 4: Rewrite SyncThreadHandle loop

**Files:**
- Modify: `backend/domain/sync_thread.rs`
- Modify: `backend/domain/mod.rs` (if exports change)

Rewrite the sync loop to use the new channel protocol: receive local events from SyncNotifier, request entity data from UI thread via `ReadEntityRequest`, push to remote, handle `PollNow` commands, persist watermarks and retry queue.

- [ ] **Step 1: Read current SyncThreadHandle and sync loop**

Read `backend/domain/sync_thread.rs` fully.

- [ ] **Step 2: Update SyncThreadHandle::spawn signature**

```rust
pub fn spawn(
    remote: Arc<dyn RemoteStorage>,
    event_rx: mpsc::Receiver<SyncEvent>,        // from SyncNotifier
    command_rx: mpsc::Receiver<SyncCommand>,     // from UI thread
    message_tx: mpsc::Sender<SyncMessage>,       // to UI thread
    initial_watermarks: HashMap<String, u64>,    // loaded from persistence
    initial_retry_queue: Vec<SyncEvent>,          // loaded from persistence
    data_dir: PathBuf,                           // for persisting state
) -> Self
```

Remove the `child_ids: Vec<String>` parameter — the sync thread will request child IDs from the UI thread when it needs to poll.

- [ ] **Step 3: Rewrite the sync loop**

The new loop:

```rust
fn sync_loop(
    remote: Arc<dyn RemoteStorage>,
    event_rx: mpsc::Receiver<SyncEvent>,
    command_rx: mpsc::Receiver<SyncCommand>,
    message_tx: mpsc::Sender<SyncMessage>,
    mut engine: SyncEngine,
    mut retry_queue: RetryQueue,
    data_dir: PathBuf,
    shutdown: Arc<AtomicBool>,
) {
    loop {
        if shutdown.load(Ordering::Relaxed) { break; }

        // 1. Drain retry queue
        let mut remaining_retries = Vec::new();
        for event in retry_queue.events.drain(..) {
            match push_event(&remote, &event, &message_tx) {
                Ok(()) => {}
                Err(_) => remaining_retries.push(event),
            }
        }
        retry_queue.events = remaining_retries;

        // 2. Drain local events from SyncNotifier
        while let Ok(event) = event_rx.try_recv() {
            match push_event(&remote, &event, &message_tx) {
                Ok(()) => {}
                Err(_) => retry_queue.events.push(event),
            }
        }

        // 3. Check for PollNow command
        let mut should_poll = false;
        while let Ok(cmd) = command_rx.try_recv() {
            match cmd {
                SyncCommand::PollNow => should_poll = true,
                SyncCommand::Shutdown => {
                    shutdown.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }

        if should_poll {
            let _ = message_tx.send(SyncMessage::StatusChanged(SyncStatus::Syncing));
            poll_remote(&remote, &mut engine, &message_tx);
            let _ = message_tx.send(SyncMessage::StatusChanged(SyncStatus::Idle));
        }

        // 4. Persist state if changed
        let sync_state = SyncState {
            watermarks: engine.watermarks_snapshot(),
            enabled: true,
            remote_url: None, // not tracked here
        };
        let _ = sync_state.save(&sync_persistence::sync_state_path(&data_dir));
        let _ = retry_queue.save(&sync_persistence::retry_queue_path(&data_dir));

        // 5. Sleep in short increments to stay responsive
        for _ in 0..60 {  // 30 seconds total (500ms * 60)
            if shutdown.load(Ordering::Relaxed) { break; }
            std::thread::sleep(std::time::Duration::from_millis(500));
            // Also check for incoming events/commands during sleep
            if event_rx.try_recv().is_ok() || command_rx.try_recv().is_ok() {
                // TODO: handle these — break out of sleep and process
                break;
            }
        }
    }
}
```

- [ ] **Step 4: Implement push_event helper**

This function requests entity data from the UI thread via a round-trip message, then pushes to remote:

```rust
fn push_event(
    remote: &Arc<dyn RemoteStorage>,
    event: &SyncEvent,
    message_tx: &mpsc::Sender<SyncMessage>,
) -> Result<(), String> {
    // For deletes, no need to read entity data
    if event.action == SyncAction::Deleted {
        remote.push_events(&[event.clone()])
            .map_err(|e| format!("push failed: {e}"))?;
        remote.delete_entity(&event.child_id, event.entity_type.clone(), &event.entity_id)
            .map_err(|e| format!("delete failed: {e}"))?;
        return Ok(());
    }

    // Request entity data from UI thread
    let (response_tx, response_rx) = mpsc::channel();
    let _ = message_tx.send(SyncMessage::ReadEntityRequest {
        child_id: event.child_id.clone(),
        entity_type: event.entity_type.clone(),
        entity_id: event.entity_id.clone(),
        response_tx,
    });

    // Wait for response (with timeout)
    let entity_json = response_rx.recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| format!("timeout waiting for entity read: {e}"))?
        .ok_or_else(|| "entity not found locally".to_string())?;

    // Push entity + event to remote
    remote.upsert_entity(&event.child_id, event.entity_type.clone(), &event.entity_id, &entity_json)
        .map_err(|e| format!("upsert failed: {e}"))?;
    remote.push_events(&[event.clone()])
        .map_err(|e| format!("push failed: {e}"))?;

    Ok(())
}
```

- [ ] **Step 5: Implement poll_remote helper**

Polls remote for each child, sends ApplyRemoteEntity/DeleteLocalEntity messages to UI thread:

```rust
fn poll_remote(
    remote: &Arc<dyn RemoteStorage>,
    engine: &mut SyncEngine,
    message_tx: &mpsc::Sender<SyncMessage>,
) {
    // Request child list from UI thread
    let (response_tx, response_rx) = mpsc::channel();
    let _ = message_tx.send(SyncMessage::ReadEntityRequest {
        child_id: String::new(),  // empty = list children request
        entity_type: EntityType::Child,
        entity_id: String::new(),
        response_tx,
    });

    // This is a bit awkward — ReadEntityRequest is for single entities.
    // Alternative: add a ListChildren variant to SyncMessage.
    // For now, we'll get child IDs from the engine's watermarks
    // (every child that was ever synced has a watermark entry).
    // On first run after backfill, watermarks are loaded from SyncState.
    let child_ids: Vec<String> = engine.watermarks_snapshot().keys().cloned().collect();

    for child_id in &child_ids {
        match engine.poll_child(child_id) {
            Ok(poll_result) => {
                for event in poll_result.events_to_apply {
                    match event.action {
                        SyncAction::Deleted => {
                            let _ = message_tx.send(SyncMessage::DeleteLocalEntity {
                                child_id: event.child_id.clone(),
                                entity_type: event.entity_type.clone(),
                                entity_id: event.entity_id.clone(),
                                event_id: event.event_id.clone(),
                            });
                        }
                        SyncAction::Created | SyncAction::Updated => {
                            // Fetch entity data from remote
                            match remote.get_entity(&event.child_id, event.entity_type.clone(), &event.entity_id) {
                                Ok(Some(json)) => {
                                    let _ = message_tx.send(SyncMessage::ApplyRemoteEntity {
                                        child_id: event.child_id.clone(),
                                        entity_type: event.entity_type.clone(),
                                        entity_id: event.entity_id.clone(),
                                        entity_json: json,
                                        event_id: event.event_id.clone(),
                                    });
                                }
                                Ok(None) => {
                                    // Entity was deleted between event and fetch — skip
                                }
                                Err(e) => {
                                    let _ = message_tx.send(SyncMessage::Error(
                                        format!("Failed to fetch entity: {e}")
                                    ));
                                }
                            }
                        }
                    }
                }
                // Update remote watermark on server
                let new_watermark = engine.get_watermark(child_id);
                let _ = remote.update_watermark(child_id, "remote", new_watermark);
            }
            Err(e) => {
                let _ = message_tx.send(SyncMessage::Error(format!("Poll failed for {child_id}: {e}")));
            }
        }
    }
}
```

- [ ] **Step 6: Add watermarks_snapshot to SyncEngine**

In `backend/domain/sync_manager.rs`, add a method to SyncEngine:

```rust
pub fn watermarks_snapshot(&self) -> HashMap<String, u64> {
    self.watermarks.clone()
}
```

- [ ] **Step 7: Update shutdown to use SyncCommand**

The `shutdown` method should also send `SyncCommand::Shutdown` through the command channel. Store the command sender in `SyncThreadHandle`:

```rust
pub struct SyncThreadHandle {
    shutdown: Arc<AtomicBool>,
    command_tx: Option<mpsc::Sender<SyncCommand>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

pub fn shutdown(&mut self) {
    if let Some(tx) = self.command_tx.take() {
        let _ = tx.send(SyncCommand::Shutdown);
    }
    self.shutdown.store(true, Ordering::Relaxed);
    if let Some(thread) = self.thread.take() {
        let _ = thread.join();
    }
}
```

- [ ] **Step 8: Verify compilation**

Run: `cargo build --package backend`
Expected: Compiles. Note: the sync thread is not spawned anywhere yet, so this is just ensuring the code is valid.

- [ ] **Step 9: Commit**

```
git add backend/domain/sync_thread.rs backend/domain/sync_manager.rs
git commit -m "feat: rewrite sync thread loop with bidirectional channel protocol"
```

---

### Task 5: UI thread sync message handler and focus detection

**Files:**
- Modify: `egui-frontend/src/ui/state/sync_state.rs` (handle new message variants)
- Modify: `egui-frontend/src/ui/app_coordinator.rs` (focus detection, message handling)
- Modify: `egui-frontend/src/ui/state/app_state.rs` (add sync channel fields)

The UI thread handles `ReadEntityRequest` (reads entity from repository, responds via channel), `ApplyRemoteEntity` (deserializes and writes to repository), `DeleteLocalEntity` (deletes from repository), and sends `PollNow` on app focus.

- [ ] **Step 1: Read current SyncUiState and app_coordinator**

Read `egui-frontend/src/ui/state/sync_state.rs`, `egui-frontend/src/ui/app_coordinator.rs`, and `egui-frontend/src/ui/state/app_state.rs` fully.

- [ ] **Step 2: Add sync channel fields to AllowanceTrackerApp**

In `AllowanceTrackerApp` (likely in `app_state.rs` or wherever the struct is defined), add:

```rust
pub struct AllowanceTrackerApp {
    // ... existing fields ...
    pub sync: SyncUiState,
    pub sync_command_tx: Option<mpsc::Sender<SyncCommand>>,
    sync_thread: Option<SyncThreadHandle>,
    was_focused: bool,
}
```

- [ ] **Step 3: Extend SyncUiState.poll_messages to handle new variants**

In `egui-frontend/src/ui/state/sync_state.rs`, update `poll_messages` to handle the new message types. However, `SyncUiState` doesn't have access to repositories — it's a pure state struct. The actual handling of `ReadEntityRequest`, `ApplyRemoteEntity`, and `DeleteLocalEntity` needs to happen in `app_coordinator.rs` where the `Backend` is available.

Approach: `SyncUiState.poll_messages()` returns a `Vec<SyncMessage>` of unhandled messages that need backend access, instead of handling everything inline. Or: move the message polling to `app_coordinator.rs` entirely.

Better approach: add a new method to the app coordinator that handles sync messages with backend access:

```rust
// In app_coordinator.rs
fn handle_sync_messages(&mut self) {
    let Some(rx) = &self.sync.message_rx else { return };

    while let Ok(msg) = rx.try_recv() {
        match msg {
            SyncMessage::ReadEntityRequest { child_id, entity_type, entity_id, response_tx } => {
                let json = self.read_entity_for_sync(&child_id, &entity_type, &entity_id);
                let _ = response_tx.send(json);
            }
            SyncMessage::ApplyRemoteEntity { child_id, entity_type, entity_id, entity_json, event_id } => {
                self.apply_remote_entity(&child_id, &entity_type, &entity_id, &entity_json, &event_id);
            }
            SyncMessage::DeleteLocalEntity { child_id, entity_type, entity_id, event_id } => {
                self.delete_local_entity(&child_id, &entity_type, &entity_id, &event_id);
            }
            SyncMessage::StatusChanged(status) => {
                self.sync.status = status;
            }
            SyncMessage::Error(msg) => {
                eprintln!("Sync error: {msg}");
                self.sync.status = SyncStatus::Error(msg);
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 4: Implement read_entity_for_sync**

Reads an entity from local storage and serializes to JSON:

```rust
fn read_entity_for_sync(&self, child_id: &str, entity_type: &EntityType, entity_id: &str) -> Option<String> {
    match entity_type {
        EntityType::Transaction => {
            // Read transaction by ID from repository
            // TransactionRepository needs a get_by_id method, or search through all transactions
            // If no direct lookup exists, read all transactions for child and find by ID
            let repo = &self.core.backend.transaction_service;
            // ... depends on available repository methods
            // Serialize to JSON with serde_json::to_string()
        }
        EntityType::Goal => {
            // Similar — read goal by ID
        }
        EntityType::Child => {
            // Read child by ID
            match self.core.backend.child_service.get_child(child_id) {
                Ok(Some(child)) => serde_json::to_string(&child).ok(),
                _ => None,
            }
        }
    }
}
```

Note: The exact implementation depends on available repository methods. `ChildService.get_child(id)` likely exists. For transactions, there may need to be a lookup by ID method. Check the repository APIs during implementation. If a `get_transaction_by_id` doesn't exist, the implementer should add one to `TransactionRepository` — it's a simple CSV scan with an ID filter.

- [ ] **Step 5: Implement apply_remote_entity**

Deserializes remote entity JSON and writes to local repository:

```rust
fn apply_remote_entity(
    &mut self,
    child_id: &str,
    entity_type: &EntityType,
    entity_id: &str,
    entity_json: &str,
    event_id: &str,
) {
    match entity_type {
        EntityType::Transaction => {
            match serde_json::from_str::<backend::domain::models::transaction::Transaction>(entity_json) {
                Ok(transaction) => {
                    // Write to repository — use a store/upsert method
                    // The repository write will fire SyncNotifier with the SAME event_id
                    // which gets deduped by the server
                    if let Err(e) = self.core.backend.transaction_service
                        .upsert_transaction_from_sync(transaction) {
                        eprintln!("Failed to apply remote transaction: {e}");
                    }
                }
                Err(e) => eprintln!("Failed to deserialize remote transaction: {e}"),
            }
        }
        EntityType::Goal => {
            match serde_json::from_str::<backend::domain::models::goal::DomainGoal>(entity_json) {
                Ok(goal) => {
                    if let Err(e) = self.core.backend.goal_service
                        .upsert_goal_from_sync(goal) {
                        eprintln!("Failed to apply remote goal: {e}");
                    }
                }
                Err(e) => eprintln!("Failed to deserialize remote goal: {e}"),
            }
        }
        EntityType::Child => {
            match serde_json::from_str::<backend::domain::models::child::Child>(entity_json) {
                Ok(child) => {
                    if let Err(e) = self.core.backend.child_service
                        .upsert_child_from_sync(child) {
                        eprintln!("Failed to apply remote child: {e}");
                    }
                }
                Err(e) => eprintln!("Failed to deserialize remote child: {e}"),
            }
        }
    }

    // Trigger UI refresh
    self.load_initial_data();
}
```

Note: `upsert_transaction_from_sync`, `upsert_goal_from_sync`, and `upsert_child_from_sync` are NEW methods that need to be added to the services. They differ from normal create/update in that:
- They accept a fully-formed domain object (not a command)
- They should fire the SyncNotifier with the **preserved event_id** (not a new UUID)
- They write directly to the repository (upsert semantics — create if not exists, update if exists)

These methods should be added in this task or as a sub-step. They're thin wrappers around repository writes.

- [ ] **Step 6: Add upsert_from_sync methods to services**

Add to `TransactionService`:
```rust
pub fn upsert_transaction_from_sync(&self, transaction: Transaction) -> Result<()> {
    self.transaction_repository.upsert_transaction(&transaction)?;
    // Don't fire sync notifier here — the event_id preservation
    // is handled by the caller or we accept the dedup approach
    Ok(())
}
```

Wait — the design says the SyncNotifier fires naturally (repository write triggers it) and the event_id dedup handles the loop. But the SyncNotifier we added in Task 2 fires with a NEW event_id (via `SyncEvent::new` which generates a UUID). For idempotency to work, we need to either:

**Option A:** Don't fire SyncNotifier for sync-originated writes. The `upsert_from_sync` methods bypass the notifier.
**Option B:** Fire with the original event_id. This requires passing the event_id through to the notifier.

Option A is simpler and more reliable. The `upsert_from_sync` methods write directly to the repository without calling `notify_sync`. Since these methods exist only for applying remote data, there's no reason to re-notify.

Add thin upsert methods to each service that write to the repository directly, without triggering sync notifications. These methods need corresponding repository methods if they don't exist (e.g., `TransactionRepository.upsert_transaction`).

- [ ] **Step 7: Implement delete_local_entity**

```rust
fn delete_local_entity(
    &mut self,
    child_id: &str,
    entity_type: &EntityType,
    entity_id: &str,
    event_id: &str,
) {
    match entity_type {
        EntityType::Transaction => {
            // Delete transaction by ID — may need a new repository method
            let _ = self.core.backend.transaction_service.delete_transaction_by_id(child_id, entity_id);
        }
        EntityType::Goal => {
            let _ = self.core.backend.goal_service.delete_goal_by_id(child_id, entity_id);
        }
        EntityType::Child => {
            let _ = self.core.backend.child_service.delete_child_by_id(child_id);
        }
    }
    self.load_initial_data();
}
```

- [ ] **Step 8: Add focus detection**

In `app_coordinator.rs`, in the `update` method (called each frame):

```rust
// Detect focus changes — send PollNow when app gains focus
let is_focused = ctx.input(|i| i.focused);
if is_focused && !self.was_focused {
    if let Some(ref tx) = self.sync_command_tx {
        let _ = tx.send(SyncCommand::PollNow);
    }
}
self.was_focused = is_focused;
```

- [ ] **Step 9: Replace sync.poll_messages() with handle_sync_messages()**

In `app_coordinator.rs`, replace the existing `self.sync.poll_messages()` call at the top of `update()` with `self.handle_sync_messages()`.

- [ ] **Step 10: Verify compilation**

Run: `cargo build`
Expected: Compiles. The sync thread is still not spawned, but all the handling code is in place.

- [ ] **Step 11: Commit**

```
git add egui-frontend/src/ui/state/sync_state.rs egui-frontend/src/ui/app_coordinator.rs egui-frontend/src/ui/state/app_state.rs
```
Also add any service files modified for upsert_from_sync methods.
```
git commit -m "feat: add UI thread sync message handling and focus detection"
```

---

### Task 6: App startup wiring — connect everything

**Files:**
- Modify: `egui-frontend/src/ui/state/app_state.rs` (or wherever AllowanceTrackerApp::new lives)
- Modify: `backend/mod.rs` (if Backend needs additional fields)

Wire all the pieces together at app startup: create channels, load persistence, spawn sync thread, pass notifier to Backend.

- [ ] **Step 1: Read current app initialization code**

Read `AllowanceTrackerApp::new` and `Backend::new` to understand the current startup flow.

- [ ] **Step 2: Add sync startup logic**

In `AllowanceTrackerApp::new` (or a helper method), after Backend construction:

```rust
// Sync setup
let (sync_notifier, event_rx) = sync_channel();
let backend = Backend::new(Some(sync_notifier))?;

// Load persisted sync state
let data_dir = backend.data_directory_service.get_data_directory();
let sync_state = SyncState::load(&sync_persistence::sync_state_path(&data_dir))
    .unwrap_or_default();
let retry_queue = RetryQueue::load(&sync_persistence::retry_queue_path(&data_dir))
    .unwrap_or_else(|_| RetryQueue { events: vec![] });

let (sync_command_tx, sync_command_rx, sync_thread) = if sync_state.enabled {
    if let Some(ref url) = sync_state.remote_url {
        let remote: Arc<dyn RemoteStorage> = Arc::new(HttpRemoteClient::new(url.clone()));
        let (message_tx, message_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();

        let thread = SyncThreadHandle::spawn(
            remote,
            event_rx,
            cmd_rx,
            message_tx,
            sync_state.watermarks.clone(),
            retry_queue.events.clone(),
            data_dir.clone(),
        );

        let sync_ui = SyncUiState::with_receiver(message_rx);

        (Some(cmd_tx), Some(sync_ui), Some(thread))
    } else {
        (None, None, None)
    }
} else {
    (None, None, None)
};
```

- [ ] **Step 3: Store sync handles in AllowanceTrackerApp**

```rust
AllowanceTrackerApp {
    // ... existing fields ...
    sync: sync_ui.unwrap_or_else(SyncUiState::new),
    sync_command_tx,
    sync_thread,
    was_focused: false,
}
```

- [ ] **Step 4: Add shutdown on app close**

Implement `Drop` for `AllowanceTrackerApp` or add cleanup in the egui `on_close` handler:

```rust
impl Drop for AllowanceTrackerApp {
    fn drop(&mut self) {
        if let Some(ref mut thread) = self.sync_thread {
            thread.shutdown();
        }
    }
}
```

- [ ] **Step 5: Add sync enable/disable UI**

If there isn't already a settings UI for entering the remote URL and enabling sync, add a minimal one. Check if the existing backfill modal / settings already has this. If it does, wire it to:
1. Save `SyncState { enabled: true, remote_url: Some(url), watermarks }` to disk
2. Show a message that sync will start on next app launch (or spawn the thread immediately)

If settings UI already exists and just needs wiring, do that. If it doesn't exist, add a minimal text field + toggle in settings.

- [ ] **Step 6: Verify full app builds and runs**

Run: `cargo build`
Expected: Compiles.

Run the app manually if possible, verify it starts without crashing. If sync is not configured (no `sync_state.yaml`), it should start normally with sync disabled.

- [ ] **Step 7: Commit**

```
git add egui-frontend/ backend/
git commit -m "feat: wire bidirectional sync at app startup"
```

---

### Task 7: End-to-end testing and deploy

**Files:** Various — depends on what needs fixing

- [ ] **Step 1: Test local → remote sync**

1. Ensure sync is enabled (sync_state.yaml with enabled=true and remote_url set)
2. Launch app
3. Add a transaction locally
4. Check DynamoDB — the transaction should appear within seconds
5. Verify via: `curl https://i99kq799kd.execute-api.us-east-2.amazonaws.com/internal/entities/transaction/keiko_hart?sort=desc&limit=1`

- [ ] **Step 2: Test remote → local sync**

1. Add an expense via MCP in Claude.ai
2. Switch focus to the desktop app (triggers PollNow)
3. Verify the expense appears in the transaction list
4. Verify the balance updated

- [ ] **Step 3: Test idempotency**

1. Add a transaction locally
2. Verify it appears remotely (once, not duplicated)
3. Verify it doesn't create a sync loop (check sync_events count is stable)

- [ ] **Step 4: Test persistence**

1. Add a transaction locally, verify it syncs
2. Close the app
3. Reopen the app
4. Verify watermarks loaded correctly (no re-sync of already-synced data)

- [ ] **Step 5: Test offline resilience**

1. Disconnect network
2. Add a transaction locally
3. Reconnect network
4. Verify the transaction syncs on next push cycle or app focus

- [ ] **Step 6: Fix any issues found**

Address bugs discovered during testing.

- [ ] **Step 7: Deploy sync-service if changes needed**

If any sync-service changes were required during testing:
```
cd infrastructure
sam build --beta-features
sam deploy --no-confirm-changeset
```

- [ ] **Step 8: Commit final fixes**

```
git add -A
git commit -m "fix: end-to-end sync testing fixes"
```
