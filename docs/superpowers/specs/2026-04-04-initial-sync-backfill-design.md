# Initial Sync Backfill Design

## Goal

Populate the remote DynamoDB tables with existing local data so that the MCP server and future sync clients can see all children, transactions, and goals. Provide a UI for triggering the initial sync and retrying if interrupted.

## Architecture

The backfill reuses existing sync infrastructure. No new server endpoints (aside from query optimization). The local app reads all entities from git-based storage, pushes entity data via `upsert_entity()`, and records sync events via `push_events()` — all through the existing `RemoteStorage` trait on `SyncEngine`.

```
Local App (egui)
  → SyncEngine.backfill()
    → RemoteStorage.initialize_child()     (per child)
    → RemoteStorage.upsert_entity()        (per entity)
    → RemoteStorage.push_events()          (batches of 25)
      → sync-service API Gateway
        → SyncFunction Lambda
          → DynamoDB (entity tables + sync_events)
```

## Backfill Data Flow

1. UI reads all local entities: children (via `ChildRepository`), transactions (via `TransactionRepository`), goals (via `GoalRepository`)
2. UI passes pre-loaded entities to `SyncEngine.backfill()`
3. For each child:
   a. `remote.initialize_child(child_id)` — sets up sync metadata (idempotent)
   b. For each entity (child, transactions, goals): serialize to JSON via `serde_json::to_string()`, call `remote.upsert_entity()`
   c. Create `SyncEvent(Created, Local)` for each entity
   d. Push events in batches of 25 via `remote.push_events()`
4. Progress reported per batch via `mpsc::Sender<BackfillProgress>`

## Idempotency

Safe to retry at any point:

- **Entity upserts**: DynamoDB `PutItem` without conditions — overwrites with identical data
- **Sync events**: Server checks `event_id` before writing. Same `event_id` returns existing sequence number (tested: `test_duplicate_event_push_idempotent`). On full retry, new `event_id`s are generated — creates extra events in the log but entity data is correct via upsert
- **Watermarks**: Monotonically increasing, condition-guarded. Setting to a lower value is silently ignored
- **initialize_child**: Creates metadata entry only if not present

## SyncEngine API

```rust
pub fn backfill(
    &self,
    children: Vec<Child>,
    transactions: HashMap<String, Vec<Transaction>>,
    goals: HashMap<String, Vec<DomainGoal>>,
    progress_tx: mpsc::Sender<BackfillProgress>,
) -> Result<BackfillResult>
```

The caller (UI layer) reads all local data and passes it in. SyncEngine only depends on `RemoteStorage` and domain types — no repository knowledge.

### BackfillProgress

```rust
pub enum BackfillProgress {
    Starting { total_entities: usize },
    ChildInitialized { child_name: String },
    EntitiesPushed { count: usize, total: usize },
    Completed { total_pushed: usize },
    Failed { error: String, pushed_so_far: usize },
}
```

### BackfillResult

```rust
pub struct BackfillResult {
    pub children_synced: usize,
    pub transactions_synced: usize,
    pub goals_synced: usize,
}
```

## Serialization

Entity tables store opaque JSON strings (the `data` attribute in DynamoDB). The sync-service does not parse entity content. The MCP server reads entities as `serde_json::Value`.

The domain types (`Child`, `Transaction`, `DomainGoal`) all derive `Serialize`. Backfill calls `serde_json::to_string()` directly — no mapping layer needed.

Entity IDs are stable (they come from local storage), so re-pushing the same entity overwrites with identical data.

## Sync-Service Query Optimization

Add optional query parameters to `GET /entities/{entity_type}/{child_id}`:

| Parameter | Values | Default | DynamoDB mapping |
|-----------|--------|---------|------------------|
| `sort` | `asc`, `desc` | `asc` | `ScanIndexForward` |
| `limit` | positive integer | none (return all) | `Limit` |

This exposes existing DynamoDB Query capabilities through the HTTP API.

Benefits:
- MCP `get_balance` changes from "fetch all transactions, sort client-side" to `GET /entities/transaction/{child_id}?sort=desc&limit=1`
- MCP `list_recent_transactions` uses `?sort=desc&limit=N` instead of client-side truncation

## UI Integration

### Settings Menu

Add "Initial Sync" item to the settings dropdown menu, alongside existing items (Export Data, Data Directory, etc.).

### Backfill Modal

Follows the existing modal pattern (`export_modal.rs`):

- **Before running**: Shows entity counts ("Ready to sync: 1 child, 187 transactions, 3 goals") + "Start Sync" button
- **While running**: Shows progress ("Syncing... 45/191 entities") updating per batch
- **On completion**: Shows result ("Synced 191 entities successfully") + "Close" button
- **On failure**: Shows error message + "Retry" button

### State

Add to `SettingsState`:
```rust
pub show_backfill_modal: bool,
pub backfill: BackfillFormState,
```

`BackfillFormState`:
```rust
pub struct BackfillFormState {
    pub is_running: bool,
    pub progress_rx: Option<mpsc::Receiver<BackfillProgress>>,
    pub entities_pushed: usize,
    pub total_entities: usize,
    pub result_message: Option<String>,
}
```

### Background Thread

Backfill runs on a background thread (matching the existing sync thread pattern in `SyncUiState`). The UI polls `progress_rx` via `try_recv()` each frame — never blocks.

### Auto-trigger

When sync is first enabled (user enters remote URL and toggles sync on), automatically open the backfill modal pre-populated with entity counts, prompting the user to start the initial sync.

## MCP Server Changes

Update two tools in `zephytop-brain/services/allowance-tracker/src/mcp.rs`:

- `get_balance`: Call `GET /entities/transaction/{child_id}?sort=desc&limit=1` instead of fetching all transactions
- `list_recent_transactions`: Call `GET /entities/transaction/{child_id}?sort=desc&limit={limit}` instead of fetching all and truncating

Update `sync_client.rs` to pass query parameters.

## Out of Scope

- Selective sync (always syncs all entities for all children)
- Cancel mid-backfill (batches are small, total time ~8 seconds)
- Conflict handling during backfill (remote tables are empty on first run; on re-sync, upserts overwrite)
- Downloading remote data to local (backfill is local→remote only)
