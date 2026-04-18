# Bidirectional Sync Design

## Goal

Enable two-way synchronization between the local desktop app (CSV/git storage) and the remote AWS sync-service (DynamoDB), so that changes made locally appear remotely and changes made remotely (e.g., via MCP/Claude) appear locally.

## Architecture

Three threads with clear responsibilities and no shared mutable state:

- **UI thread**: all repository reads/writes, renders UI, responds to data requests from the sync thread
- **Sync thread**: network I/O only — pushes events/entities to remote, polls remote for new events, coordinates via message channels
- **Existing infrastructure reused**: `RemoteStorage` trait, `HttpRemoteClient`, `SyncEvent`/`SyncCheckpoint` types, `SyncNotifier`, `SyncPersistence`

## Thread Communication

Three channels connect the threads:

### 1. SyncNotifier (existing) — UI → Sync

`mpsc::Sender<SyncEvent>` — fire-and-forget. The UI thread sends a `SyncEvent` after every local repository write. The event carries entity type, entity ID, child ID, and action (Created/Updated/Deleted) but NOT the entity data. The sync thread reads the entity from local storage (via a round-trip message to the UI thread) at push time.

**Why no entity data in the event:** If the entity is modified again before the push lands, reading at push time ensures we always push the latest version. Rapid edits are naturally coalesced.

### 2. SyncCommand (new) — UI → Sync

`mpsc::Sender<SyncCommand>` — control signals:

```rust
pub enum SyncCommand {
    PollNow,   // App gained focus — poll remote immediately
    Shutdown,  // App closing — flush and exit
}
```

Separate from SyncNotifier because these are control signals, not data events.

### 3. SyncMessage (existing, extended) — Sync → UI

`mpsc::Sender<SyncMessage>` — the sync thread sends messages to the UI thread for any operation that requires repository access:

```rust
pub enum SyncMessage {
    // Existing
    StatusChanged(SyncStatus),
    Error(String),

    // New — sync thread needs entity data for push
    ReadEntityRequest {
        child_id: String,
        entity_type: EntityType,
        entity_id: String,
        response_tx: oneshot::Sender<Option<String>>,
    },

    // New — sync thread pulled a remote entity, apply locally
    ApplyRemoteEntity {
        child_id: String,
        entity_type: EntityType,
        entity_id: String,
        entity_json: String,
        event_id: String,  // Preserved for idempotency
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

## Data Flows

### Local Write → Remote Push

```
1. User action (e.g., add expense)
2. UI thread: repository.write() → CSV + git commit
3. UI thread: SyncNotifier.notify(SyncEvent { entity_type, entity_id, child_id, action })
4. Sync thread: receives SyncEvent
5. Sync thread: sends ReadEntityRequest to UI thread via SyncMessage
6. UI thread: reads entity from repository, responds via oneshot channel
7. Sync thread: calls remote.upsert_entity() + remote.push_events()
8. On success: advance local_watermark, persist to disk
9. On failure: add to RetryQueue, persist to disk
```

### Remote Pull → Local Apply (App Focus)

```
1. App gains focus (egui focus detection)
2. UI thread: sends PollNow via SyncCommand channel
3. Sync thread: for each child_id (from ChildRepository via ReadEntityRequest):
   a. remote.get_events_since(child_id, remote_watermark)
   b. For each event with action Created/Updated:
      - remote.get_entity(child_id, entity_type, entity_id)
      - Send ApplyRemoteEntity message to UI thread
   c. For each event with action Deleted:
      - Send DeleteLocalEntity message to UI thread
4. UI thread: receives ApplyRemoteEntity
   a. Deserializes entity JSON into domain type
   b. Writes via repository (upsert to CSV)
   c. SyncNotifier fires with same event_id (preserved from remote)
5. Sync thread: receives the re-notified event, pushes to remote
6. Remote: deduplicates by event_id → no-op, returns existing sequence
7. Sync thread: advances remote_watermark, persists to disk
```

### Retry on Connectivity Loss

```
1. Local write → push fails (network error)
2. Sync thread: event added to RetryQueue
3. RetryQueue persisted to sync_retry_queue.yaml
4. On next successful network operation (or app focus):
   a. Drain retry queue
   b. For each: read latest entity from local (via UI thread), push to remote
   c. On success: remove from queue, persist
   d. On failure: keep in queue
```

## Idempotency

The system relies on server-side event deduplication to prevent sync loops:

1. Remote event arrives with `event_id: "abc123"`
2. Sync thread applies it locally via UI thread
3. UI thread's repository write fires SyncNotifier with same `event_id: "abc123"`
4. Sync thread pushes event to remote
5. Server sees `event_id: "abc123"` already exists → returns existing sequence, no-op
6. Loop ends

**Critical invariant:** When applying a remote entity locally, the resulting SyncEvent MUST preserve the original `event_id`. Generating a new event_id would break dedup and create an infinite loop.

## Conflict Handling

Last-write-wins based on server-assigned sequence numbers. No conflict resolution UI needed.

If a local event and a remote event modify the same entity, whichever is pushed to the server last gets the highest sequence number and "wins." The next pull will overwrite the other side's version.

For a family allowance app with one primary user and occasional MCP writes, true conflicts are extremely unlikely.

## Persistence

### SyncState (existing struct)

```rust
pub struct SyncState {
    pub watermarks: HashMap<String, u64>,  // child_id → remote_watermark
    pub enabled: bool,
    pub remote_url: Option<String>,
}
```

Persisted to `sync_state.yaml` in the data directory. Loaded on startup, saved after each watermark advancement.

### RetryQueue (existing struct)

```rust
pub struct RetryQueue {
    pub events: Vec<SyncEvent>,
}
```

Persisted to `sync_retry_queue.yaml`. Loaded on startup, saved on modification.

## App Startup

When sync is enabled (`SyncState.enabled == true` and `remote_url` is set):

1. Load `SyncState` from disk → restore watermarks
2. Load `RetryQueue` from disk → restore pending pushes
3. Create channels: SyncNotifier, SyncCommand, SyncMessage
4. Spawn sync thread with: `Arc<dyn RemoteStorage>`, channel endpoints, watermarks, retry queue
5. Wire SyncNotifier into domain services (transaction service, goal service, child service)
6. UI thread begins polling SyncMessage channel each frame

## Sync Thread Loop

```
loop {
    // 1. Check for shutdown
    if shutdown_requested: break

    // 2. Drain retry queue (if any pending)
    for event in retry_queue:
        request entity from UI thread
        push to remote
        on success: remove from queue

    // 3. Drain local events from SyncNotifier channel
    while let Ok(event) = event_rx.try_recv():
        request entity from UI thread
        push event + entity to remote
        on failure: add to retry queue

    // 4. Check for PollNow command
    if command_rx.try_recv() == PollNow:
        for child_id in children:
            poll remote for events since watermark
            send ApplyRemoteEntity / DeleteLocalEntity messages to UI
            advance watermark

    // 5. Persist state if changed
    save watermarks + retry queue to disk

    // 6. Sleep (short intervals to stay responsive to commands)
    sleep 500ms
}
```

## UI Thread Integration

The UI thread's `update()` method (called each frame by egui) adds:

```
fn update(&mut self, ctx: &egui::Context, ...) {
    // Poll sync messages (non-blocking)
    while let Ok(msg) = self.sync_message_rx.try_recv() {
        match msg {
            ReadEntityRequest { ... } => {
                // Read from repository, respond via oneshot
            }
            ApplyRemoteEntity { ... } => {
                // Deserialize, write to repository
                // SyncNotifier fires with preserved event_id
                // Trigger UI refresh for affected child
            }
            DeleteLocalEntity { ... } => {
                // Delete from repository
                // Trigger UI refresh
            }
            StatusChanged(status) => {
                // Update status display
            }
            Error(msg) => {
                // Show error
            }
        }
    }

    // Detect focus changes
    if ctx.input(|i| i.focused) && !self.was_focused {
        self.sync_command_tx.send(SyncCommand::PollNow);
    }
    self.was_focused = ctx.input(|i| i.focused);

    // ... rest of existing update logic
}
```

## Wiring SyncNotifier into Services

Each domain service that writes entities needs a `SyncNotifier` to fire events after successful writes. The notifier is injected at construction time:

- `TransactionService` — fires on add/update/delete transaction
- `GoalService` — fires on add/update/delete goal
- `ChildService` — fires on add/update child

The notifier call happens AFTER the repository write succeeds, not before. A failed write must not produce a sync event.

The `SyncEvent` is constructed with:
- `event_id`: new UUID (for local-originated events)
- `entity_type`: matches the entity being written
- `entity_id`: the entity's ID
- `child_id`: the child this entity belongs to
- `action`: Created, Updated, or Deleted
- `source`: `SyncSource::Local`
- `source_timestamp`: current time

## SyncNotifier Injection

The SyncNotifier is optional — the app works without sync enabled. Services accept `Option<SyncNotifier>`:

```rust
pub struct TransactionService {
    repository: TransactionRepository,
    sync_notifier: Option<SyncNotifier>,
}
```

When sync is disabled, `sync_notifier` is `None` and no events are fired.

## What Changes vs What's New

### Existing code — keep as-is
- `RemoteStorage` trait + `HttpRemoteClient`
- `SyncEvent`, `SyncCheckpoint`, `EntityType`, `SyncAction`, `SyncSource` types
- `SyncNotifier` (fire-and-forget channel wrapper)
- `SyncPersistence` (`SyncState` + `RetryQueue` load/save)

### Existing code — modify
- `SyncEngine` — simplify to remove conflict detection/resolution, keep push and poll mechanics
- `SyncThreadHandle` — update loop to use new channel protocol, load/save persistence, handle retry queue
- `SyncMessage` — add `ReadEntityRequest`, `ApplyRemoteEntity`, `DeleteLocalEntity` variants
- `SyncUiState` — handle new message types, detect app focus
- Domain services — inject optional `SyncNotifier`, fire events after writes
- `CoreAppState` / app startup — spawn sync thread when enabled, wire channels

### New code
- `SyncCommand` enum and channel
- UI thread message handler (poll sync messages each frame)
- Apply-remote-entity logic (deserialize JSON → domain type → repository write)
- App focus detection → PollNow signal

## Out of Scope

- Selective sync (always syncs all entities for all children)
- Real-time push notifications from server (polling only)
- Conflict resolution UI (last-write-wins)
- Sync settings UI beyond enable/disable (already exists as backfill modal)
- Multi-device sync (designed for one local app + MCP, though the architecture supports multiple clients)
