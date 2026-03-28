# AWS Sync Feature Design

## Overview

Add bidirectional synchronization between the local git-backed allowance tracker and a remote AWS backend (DynamoDB). This enables future integrations (e.g., an MCP-powered Claude agent) to read balances and add expenses via API, with changes flowing back to the local app and into git history.

### Scope — What We Build Now

- `RemoteStorage` trait and its integration into the local app
- A REST sync-service crate (new workspace member) backed by DynamoDB
- Background sync thread in the local app (polling, push-on-change)
- Conflict detection and resolution (including UI modal)
- All remote interactions tested against DynamoDB Local (AWS-provided mock)
- `InProcessRemoteClient` for integration testing without HTTP

### Scope — What We Defer

- Real AWS deployment (Lambda, API Gateway, IAM)
- MCP server for Claude integration
- SNS/push-based change notification (polling only for now)
- S3 storage (not needed given entity sizes)

## Architecture

Three layers, two binaries:

```
egui-frontend (existing)
  + SyncManager (new)
      - Background polling thread (std::thread + mpsc)
      - Push-on-change via SyncNotifier channel
      - Conflict detection & resolution UI modal
        |
        v
RemoteStorage trait (new, in backend/)
  + HttpRemoteClient  — calls sync-service over HTTP (reqwest blocking)
  + InProcessRemoteClient — embeds sync-service logic directly (for integration tests)
  + MockRemoteClient — in-memory HashMap/Vec (for unit tests)
        |
        v
sync-service (new crate, axum)
  REST API -> DynamoDB tables
  Exports lib.rs for InProcessRemoteClient use
```

### Data Flows

**Local change (push):**
1. Domain service writes CSV + git commit (existing behavior)
2. Repository calls `SyncNotifier::notify(event)` via mpsc channel
3. Background sync thread receives event, calls `RemoteStorage::push_events()` + `upsert_entity()`
4. On success, advances local watermark
5. On failure, event queued in `sync_retry_queue.yaml` for retry next cycle

**Remote change (pull):**
1. Background thread polls `RemoteStorage::get_events_since(local_watermark)`
2. For each event: conflict check against pending local events for same entity
3. Non-conflicting: apply via repository write methods (CSV + git commit), suppress sync notification to avoid loops
4. Conflicting: queue as `SyncConflict`, surface to UI via `SyncMessage::ConflictDetected`
5. Advance local watermark once all events applied or queued for conflict resolution

**No changes:**
Background thread polls, gets empty response, sleeps with exponential backoff (30s baseline, up to 5m, resets on activity).

## Sync Event Model

### SyncEvent

```rust
pub struct SyncEvent {
    pub event_id: String,              // UUID
    pub entity_type: EntityType,       // Transaction, Goal, Child (includes allowance config)
    pub entity_id: String,             // The entity's ID
    pub child_id: String,              // Scoping — which child's data
    pub action: SyncAction,            // Created, Updated, Deleted
    pub source: SyncSource,            // Local, Remote
    pub source_timestamp: DateTime<Utc>, // When the change happened on originating side
}

pub enum EntityType { Transaction, Goal, Child }
pub enum SyncAction { Created, Updated, Deleted }
pub enum SyncSource { Local, Remote }
```

Note: `Child` entity type includes allowance config fields (1:1 relationship, stored together).

### Sync Protocol — Event Ordering

Events are totally ordered per child via a monotonically increasing sequence number. Timestamps are stored for display/debugging but are NOT used for ordering.

**Write protocol (DynamoDB transaction):**
1. `UpdateItem` on `sync_metadata`: `SET event_sequence = event_sequence + 1` with `ConditionExpression: attribute_exists(event_sequence)` — atomically increments, returns new value
2. `PutItem` on `sync_events` with the returned sequence number, with `ConditionExpression: attribute_not_exists(sequence)` as safety belt

This guarantees: no gaps, no duplicates, total order per child.

**Deduplication:** Before allocating a sequence number, the service checks `event_id` against existing events. If an event with the same `event_id` already exists (e.g., a retried push after a timeout where the original actually succeeded), the push is treated as a no-op. This makes pushes idempotent.

### Watermarks & Event Cleanup

```rust
pub struct SyncCheckpoint {
    pub local_watermark: u64,    // Last sequence number local has processed
    pub remote_watermark: u64,   // Last sequence number remote has processed
}
```

Watermarks are sequence numbers, not timestamps. Events eligible for TTL cleanup only when `sequence <= min(local_watermark, remote_watermark)`. This ensures events are never cleaned up before both sides have acknowledged them — safe even if the app isn't opened for months.

## DynamoDB Table Design

Five tables, each with a clear single purpose.

### `children`

| Attribute | Type | Key |
|-----------|------|-----|
| `child_id` | String | **PK** |
| `name` | String | |
| `birthdate` | String | |
| `allowance_amount` | Number | |
| `allowance_day_of_week` | Number | |
| `allowance_is_active` | Boolean | |
| `allowance_use_age_based` | Boolean | |
| `created_at` | String | |
| `updated_at` | String | |
| `last_sequence` | Number | Sequence of most recent event touching this entity |

### `transactions`

| Attribute | Type | Key |
|-----------|------|-----|
| `child_id` | String | **PK** |
| `transaction_id` | String | **SK** |
| `date` | String | RFC3339 |
| `description` | String | |
| `amount` | Number | |
| `balance` | Number | |
| `transaction_type` | String | |
| `last_sequence` | Number | |

### `goals`

| Attribute | Type | Key |
|-----------|------|-----|
| `child_id` | String | **PK** |
| `goal_id` | String | **SK** |
| `description` | String | |
| `target_amount` | Number | |
| `state` | String | active, cancelled, completed |
| `created_at` | String | |
| `updated_at` | String | |
| `last_sequence` | Number | |

### `sync_events`

| Attribute | Type | Key |
|-----------|------|-----|
| `child_id` | String | **PK** |
| `sequence` | Number | **SK** |
| `event_id` | String | UUID for deduplication |
| `entity_type` | String | transaction, goal, child |
| `entity_id` | String | |
| `action` | String | created, updated, deleted |
| `source` | String | local, remote |
| `source_timestamp` | String | RFC3339, for display only |
| `ttl` | Number | Epoch seconds, set during cleanup |

Query pattern: `child_id = :id AND sequence > :watermark`

### `sync_metadata`

| Attribute | Type | Key |
|-----------|------|-----|
| `child_id` | String | **PK** |
| `event_sequence` | Number | Current highest sequence |
| `local_watermark` | Number | Last sequence local has processed |
| `remote_watermark` | Number | Last sequence remote has processed |

## RemoteStorage Trait

```rust
pub trait RemoteStorage: Send + Sync {
    fn push_events(&self, events: &[SyncEvent]) -> Result<()>;
    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>>;

    fn upsert_entity(&self, child_id: &str, entity_type: EntityType, entity: &str) -> Result<()>;
    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>>;
    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()>;

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint>;
    fn update_checkpoint(&self, child_id: &str, checkpoint: &SyncCheckpoint) -> Result<()>;

    fn health_check(&self) -> Result<bool>;
}
```

### Implementations

1. **`HttpRemoteClient`** — production path. Calls sync-service REST API using `reqwest` blocking client. Maps trait methods 1:1 to REST endpoints.

2. **`InProcessRemoteClient`** — integration testing. Embeds sync-service storage logic directly (same DDB code, backed by DynamoDB Local). No HTTP hop. Injected into the app via DI for end-to-end testing against real AWS mock.

3. **`MockRemoteClient`** — unit testing. In-memory `Vec`/`HashMap`. No AWS dependencies. Fast, deterministic.

## Sync-Service Crate

New workspace member: `sync-service/`

```
sync-service/
  Cargo.toml
  src/
    main.rs              # axum server startup, DI wiring
    lib.rs               # Exports storage layer for InProcessRemoteClient
    routes/
      entities.rs        # CRUD endpoints for children, transactions, goals
      sync.rs            # Event push/pull, checkpoint management
      health.rs          # Health check
    storage/
      traits.rs          # Internal storage trait (DDB abstraction)
      dynamo.rs          # DynamoDB implementation
```

### REST Endpoints

| Method | Endpoint | Maps to |
|--------|----------|---------|
| POST | `/sync/events` | `push_events` |
| GET | `/sync/events?child_id={id}&since={seq}` | `get_events_since` |
| PUT | `/entities/{type}/{id}` | `upsert_entity` |
| GET | `/entities/{type}/{id}` | `get_entity` |
| DELETE | `/entities/{type}/{id}` | `delete_entity` |
| GET | `/sync/checkpoint/{child_id}` | `get_checkpoint` |
| PUT | `/sync/checkpoint/{child_id}` | `update_checkpoint` |
| GET | `/health` | `health_check` |

### Dependencies

- `axum` — HTTP framework
- `aws-sdk-dynamodb` — DynamoDB client
- `tokio` — async runtime (sync-service only)
- `serde`, `serde_json` — serialization
- Shared types from `shared` crate

## SyncManager (Local App Integration)

### Structure

```rust
pub struct SyncManager {
    remote: Arc<dyn RemoteStorage>,
    event_queue: Vec<SyncEvent>,       // Outbound events pending push
    conflicts: Vec<SyncConflict>,
    checkpoint: SyncCheckpoint,
    status: SyncStatus,
}

pub enum SyncStatus {
    Disabled,
    Idle,
    Syncing,
    Error(String),
    HasConflicts(usize),
}
```

### SyncNotifier

Injected into repositories as `Option<SyncNotifier>` — `None` when sync is disabled.

```rust
pub struct SyncNotifier {
    tx: std::sync::mpsc::Sender<SyncEvent>,
}
```

Repositories call `self.sync_notifier.notify(event)` after successful CSV write + git commit. Non-blocking send; failures logged but do not affect local write success.

### SyncMessage (background thread to UI)

```rust
pub enum SyncMessage {
    StatusChanged(SyncStatus),
    EntitiesUpdated { child_id: String, entity_type: EntityType, count: usize },
    ConflictDetected(SyncConflict),
    PushFailed { event: SyncEvent, error: String },
    Error(String),
}
```

UI drains messages via `try_recv()` each frame. Status bar shows sync state. Conflict badge triggers resolution modal.

### Background Thread Lifecycle

1. App startup: spawn thread with `Arc<AtomicBool>` shutdown flag
2. Loop:
   - Load retry queue from `sync_retry_queue.yaml`
   - Drain retry queue (push failed events from prior cycles)
   - Push new local events from mpsc channel
   - Poll for remote events since `local_watermark`
   - Conflict check, apply clean changes, queue conflicts
   - Send `SyncMessage` updates to UI
   - Sleep with backoff (30s baseline, up to 5m, resets on activity)
3. App shutdown: set shutdown flag, thread exits after current cycle

### Suppressing Notification Loops

When the sync thread applies a remote change locally (writes CSV + git commit via repository), it must NOT trigger a sync notification back to itself. Two mechanisms:

- The sync thread calls repositories directly, passing `SyncNotifier: None` (or a dedicated write path that skips notification)
- Alternatively, the `SyncNotifier` checks `event.source == Remote` and drops it

### Local Persistence

```
~/Documents/Allowance Tracker/
  sync_retry_queue.yaml    # Outbound events that failed to push
  sync_state.yaml          # Local copy of watermarks + sync config
```

Loaded on startup, updated after each sync cycle.

## Conflict Detection & Resolution

### Conflict Definition

A conflict exists when both sides have a `SyncEvent` for the same `(entity_type, entity_id)` since the last sync checkpoint.

### SyncConflict

```rust
pub struct SyncConflict {
    pub id: String,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub child_id: String,
    pub local_event: SyncEvent,
    pub remote_event: SyncEvent,
    pub status: ConflictStatus,
}

pub enum ConflictStatus {
    Pending,
    ResolvedKeepLocal,
    ResolvedKeepRemote,
    ResolvedMerged,
}
```

### Auto-Resolution (no user intervention)

- Different `entity_id` values: no conflict, both apply
- Same entity, both `Deleted`: auto-resolve, already agree

### Manual Resolution (conflict modal)

Conflicts presented one at a time. Modal displays:

- Entity type and human-readable description (e.g., "Alice — Transaction: Toy Store, -$5.00")
- Side-by-side: local version (fields + "changed X ago") vs remote version
- Three actions: **Keep Local**, **Keep Remote**, **Edit & Merge**
- **Edit & Merge** opens the existing entity edit form pre-populated with remote values
- **Skip for now** moves to next conflict; this one stays pending

Each resolution generates a `SyncEvent` with `source: Local` pushed to remote.

### Conflict Blocking

While conflicts are pending for a child, pulls for that child pause. Pushes of unrelated entities for that child continue. This prevents cascading conflicts.

### Parental Controls

Conflict resolution requires the same permission level as editing the underlying entity. If parental controls are active and unauthenticated, the conflict badge is visible but the modal requires the parental control challenge.

## Automated Testing Strategy

### Testing Pyramid

```
                  /  E2E  \           Few: full app + DynamoDB Local
                 /----------\
                / Integration \       Many: sync-service + DynamoDB Local
               /--------------\
              /   Unit Tests    \     Most: pure logic, in-memory mocks
             /------------------\
```

### Layer 1: Unit Tests (MockRemoteClient, no AWS)

Fast, deterministic tests using the in-memory `MockRemoteClient`.

**SyncManager logic:**

- `test_local_change_produces_sync_event` — domain write triggers SyncNotifier, event appears in outbound queue
- `test_remote_event_applied_locally` — mock returns events, verify CSV + git updated
- `test_remote_event_does_not_retrigger_sync` — applying a remote change must not enqueue a new outbound event (no infinite loop)
- `test_retry_queue_persisted_on_push_failure` — mock returns error, verify event written to `sync_retry_queue.yaml`
- `test_retry_queue_loaded_on_startup` — pre-populate yaml, start SyncManager, verify events are retried
- `test_retry_queue_drained_before_new_events` — ensure ordering: retries pushed before fresh events
- `test_backoff_increases_on_empty_polls` — verify sleep interval grows when no remote changes found
- `test_backoff_resets_on_activity` — verify sleep interval resets when events are found or pushed

**Conflict detection:**

- `test_no_conflict_different_entities` — local changes entity A, remote changes entity B, both apply cleanly
- `test_no_conflict_different_entity_types` — local changes a transaction, remote changes a goal for same child, both apply
- `test_conflict_detected_same_entity` — local and remote both changed same transaction since last sync, conflict created
- `test_conflict_both_deleted_auto_resolves` — both sides deleted same entity, auto-resolved
- `test_conflict_blocks_pulls_for_child` — pending conflict pauses pull for that child
- `test_conflict_does_not_block_other_children` — child A has conflict, child B syncs normally
- `test_conflict_does_not_block_pushes` — local changes to unrelated entities for same child still push while conflict pending
- `test_resolve_keep_local_pushes_event` — resolving with Keep Local generates outbound SyncEvent
- `test_resolve_keep_remote_writes_locally` — resolving with Keep Remote writes to CSV + git
- `test_resolve_merged_pushes_merged_state` — Edit & Merge produces new entity state pushed to remote

**Watermark management:**

- `test_watermark_advances_after_successful_pull` — local watermark updated after applying remote events
- `test_watermark_advances_after_successful_push` — remote watermark updated after push acknowledged
- `test_watermark_not_advanced_on_failure` — push/pull failure leaves watermark unchanged
- `test_watermark_not_advanced_with_pending_conflicts` — watermark advances up to the conflict, not past it

**SyncNotifier:**

- `test_sync_notifier_sends_event_on_write` — repository write with active notifier sends event
- `test_sync_notifier_none_does_nothing` — repository write with `None` notifier works normally (sync disabled path)
- `test_sync_notifier_disconnected_channel_does_not_fail_write` — if channel is dropped, local write still succeeds

### Layer 2: Integration Tests (DynamoDB Local)

These tests run against DynamoDB Local (AWS-provided mock) to validate real DynamoDB behavior: conditional writes, transactions, query semantics, and edge cases that in-memory mocks cannot catch.

**Prerequisites:**
- DynamoDB Local running (Docker: `amazon/dynamodb-local` or JAR)
- Each test creates tables with unique prefixes (e.g., `test_abc123_transactions`) to allow parallel test execution
- Teardown deletes tables after each test

**Sync-service storage layer (no HTTP, direct DDB calls):**

- `test_push_event_increments_sequence` — push event, verify sequence in `sync_metadata` incremented by 1
- `test_push_event_stores_correct_attributes` — push event, read back from DDB, verify all fields
- `test_push_multiple_events_sequential_sequences` — push 3 events rapidly, verify sequences are 1, 2, 3 with no gaps
- `test_get_events_since_returns_correct_range` — push 10 events, query since sequence 5, verify events 6-10 returned in order
- `test_get_events_since_empty` — query with watermark at latest sequence, verify empty result
- `test_upsert_entity_creates_new` — upsert a transaction that doesn't exist, verify stored
- `test_upsert_entity_updates_existing` — upsert over existing transaction, verify latest state
- `test_delete_entity_removes_from_table` — delete entity, verify get returns None
- `test_checkpoint_round_trip` — write checkpoint, read back, verify match
- `test_entity_last_sequence_updated` — push event + upsert entity, verify `last_sequence` on entity matches event sequence

**Conditional write correctness (critical for ordering guarantees):**

- `test_concurrent_sequence_increment_no_duplicates` — spawn 10 threads, each pushing 10 events for the same child concurrently. Verify: all 100 events stored, sequences are 1-100 with no gaps and no duplicates. This is THE critical test for the ordering guarantee.
- `test_concurrent_push_different_children_independent` — spawn threads pushing events for different children concurrently. Verify each child's sequences are independent and gapless.
- `test_sequence_condition_fails_on_conflict` — manually write a sequence number, then attempt to push an event with the same sequence. Verify the conditional write fails (not silently overwrites).
- `test_transaction_atomicity_event_and_metadata` — verify that the DDB transaction (increment counter + write event) either both succeed or both fail. Simulate failure (e.g., malformed event) and verify metadata sequence was not incremented.

**Watermark and TTL:**

- `test_ttl_not_set_before_both_watermarks_pass` — push events, advance only local watermark. Verify no TTL set on events.
- `test_ttl_set_after_both_watermarks_pass` — advance both watermarks past event. Verify TTL attribute set.
- `test_events_survive_when_one_watermark_stale` — push 50 events, advance remote watermark to 50 but leave local at 0 (simulating app not opened). Verify all 50 events still queryable (no TTL expiration).

**Sync-service REST layer (HTTP, axum, against DynamoDB Local):**

- `test_push_events_endpoint_201` — POST valid events, verify 201 response
- `test_push_events_endpoint_400_invalid` — POST malformed events, verify 400 response with error detail
- `test_get_events_endpoint_returns_ordered` — push events via POST, query via GET, verify order matches sequence
- `test_get_events_endpoint_pagination` — push 100 events, query with since=0, verify all returned in correct order
- `test_entity_crud_round_trip` — PUT, GET, DELETE cycle through HTTP endpoints
- `test_checkpoint_endpoints_round_trip` — PUT + GET checkpoint via HTTP
- `test_health_endpoint` — GET /health returns 200 when DDB is reachable

### Layer 3: Concurrency & Race Condition Tests (DynamoDB Local)

These tests specifically target race conditions and concurrent access patterns. They are the most important tests for correctness.

**Simulated bidirectional sync (the "two clients" test pattern):**

Create two `InProcessRemoteClient` instances pointed at the same DynamoDB Local, simulating local app + MCP/remote actor operating concurrently.

- `test_two_clients_push_interleaved_events` — client A pushes event, client B pushes event, client A pushes event. Verify all three events have sequential, gapless sequence numbers. Verify both clients see all events when polling.
- `test_two_clients_push_simultaneously` — spawn two threads, each pushing 50 events for the same child as fast as possible. Verify: 100 events total, sequences 1-100, no gaps, no duplicates. This is the stress test version of the conditional write test.
- `test_client_a_pushes_while_client_b_polls` — client A pushes events in a loop. Client B polls repeatedly. Verify B never sees events out of order and never misses events (no gaps in the sequence B observes across multiple polls).
- `test_watermark_update_race` — client A and B both try to update the same watermark concurrently. Verify the watermark only moves forward, never backwards. (The update should use a conditional: `SET watermark = :new IF watermark < :new`.)
- `test_push_and_watermark_advance_interleaved` — client A pushes event 1, client B advances watermark to 1, client A pushes event 2. Event 1 becomes TTL-eligible; event 2 does not. Verify event 2 is still queryable.

**Failure injection:**

- `test_push_event_succeeds_but_entity_upsert_fails` — inject a failure on the entity upsert after the event is written. Verify: the event exists in sync_events, entity table is stale. Then retry the operation — verify it succeeds and entity is now consistent. (This tests the "event is source of truth" property.)
- `test_network_failure_mid_push_retries_correctly` — push 3 events. Inject failure on event 2. Verify: event 1 is in DDB, events 2-3 are in the retry queue. On next cycle (no failure), all 3 are in DDB with correct sequences.
- `test_duplicate_event_push_idempotent` — push the same event twice (same event_id). Verify only one copy exists and sequence was only consumed once. (This requires the service to check event_id for deduplication before allocating a sequence number.)

**Stale client recovery:**

- `test_client_offline_30_days_then_syncs` — push 500 events via client B while client A's watermark stays at 0. Then client A polls. Verify: A receives all 500 events in correct order, A's watermark advances to 500, events are NOT cleaned up during the "offline" period.
- `test_client_recovers_from_partial_apply` — client A polls and gets 10 events. Apply 5 successfully, then simulate crash (watermark at 5). On recovery, client A polls again, gets events 6-10, applies them. Verify no data loss or duplication.

**Entity-level conflict timing:**

- `test_conflict_detection_when_both_clients_modify_same_entity` — client A modifies transaction X, pushes event. Client B modifies same transaction X, pushes event. Client A polls. Verify: conflict detected for transaction X with both events available.
- `test_no_false_conflict_when_events_are_sequential` — client A modifies transaction X, pushes, client B polls and applies. Then client B modifies transaction X, pushes. Client A polls. Verify: no conflict — B's change happened after A's was acknowledged.
- `test_conflict_resolution_event_visible_to_both` — after conflict resolved (Keep Local), verify the resolution event is visible to both clients and the entity state converges.

### Layer 4: End-to-End Tests (Full App + DynamoDB Local)

These test the complete flow from domain service through SyncManager to DynamoDB Local and back.

- `test_e2e_add_transaction_syncs_to_remote` — use TransactionService to add a transaction. Wait for sync cycle. Query DDB directly and verify the transaction and sync event are present.
- `test_e2e_remote_transaction_appears_locally` — write a transaction directly to DDB (simulating MCP). Trigger sync cycle. Verify transaction appears in local CSV and a git commit was created with appropriate message.
- `test_e2e_bidirectional_sync_no_conflicts` — add different transactions on each side. Sync. Verify both sides have all transactions.
- `test_e2e_conflict_surfaces_to_ui` — modify same transaction on both sides. Sync. Verify SyncMessage::ConflictDetected sent to UI channel.
- `test_e2e_app_restart_resumes_sync` — add a transaction, kill sync thread before it pushes (or inject failure). Restart SyncManager. Verify the event is loaded from `sync_retry_queue.yaml` and pushed successfully.

### Test Infrastructure

**DynamoDB Local management:**

- Tests that need DynamoDB Local use a shared test utility module.
- A `DynamoTestContext` struct handles: starting DDB Local (if not already running), creating tables with unique prefixes, providing a configured DDB client, and teardown.
- Tests are annotated with `#[ignore]` if DynamoDB Local is not available, with a clear skip message. CI runs them with DDB Local in Docker.

**Table isolation:**

```rust
pub struct DynamoTestContext {
    pub client: aws_sdk_dynamodb::Client,
    pub table_prefix: String,  // e.g., "test_a1b2c3_"
}

impl DynamoTestContext {
    pub async fn new() -> Self { /* create tables with unique prefix */ }
    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }
}

impl Drop for DynamoTestContext {
    fn drop(&mut self) { /* delete prefixed tables */ }
}
```

**Concurrency test helpers:**

```rust
/// Run N closures in parallel, collect results, assert all Ok
fn run_concurrent<F, T>(n: usize, f: F) -> Vec<T>
where F: Fn(usize) -> T + Send + Sync, T: Send;

/// Assert a sequence of numbers is gapless: 1, 2, 3, ..., n
fn assert_gapless_sequence(sequences: &[u64]);
```

**Timing considerations:**
- Concurrency tests should use barriers (`std::sync::Barrier`) to maximize overlap, not rely on sleep-based timing
- Poll-based tests should have bounded retry loops with clear timeout assertions, not unbounded waits
