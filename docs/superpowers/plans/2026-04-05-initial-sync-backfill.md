# Initial Sync Backfill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Populate remote DynamoDB tables with existing local data so the MCP server and future sync clients can access all children, transactions, and goals.

**Architecture:** Add a `backfill()` method to `SyncEngine` that accepts pre-loaded local entities and pushes them to remote via `RemoteStorage` (entity upserts + sync events in batches of 25). The egui frontend gets a settings modal to trigger and monitor backfill progress. The sync-service gains `sort`/`limit` query params on the list endpoint to enable efficient queries from the MCP server.

**Tech Stack:** Rust, egui, axum, DynamoDB, AWS SAM, cargo-lambda

---

## File Structure

**sync-service (allowance-tracker repo):**
- Modify: `sync-service/src/routes/entities.rs` — add query param parsing for sort/limit
- Modify: `sync-service/src/storage/dynamo.rs` — add sort/limit to `list_entities_for_child`
- Modify: `sync-service/tests/entity_crud_test.rs` — add sort/limit integration tests

**backend (allowance-tracker repo):**
- Modify: `backend/domain/sync_manager.rs` — add `backfill()`, `BackfillProgress`, `BackfillResult`
- Modify: `backend/storage/http_remote.rs` — fix wire format mismatch for `push_events`

**egui-frontend (allowance-tracker repo):**
- Modify: `egui-frontend/src/ui/components/settings/state.rs` — add `BackfillFormState`
- Modify: `egui-frontend/src/ui/components/settings/mod.rs` — add `backfill_modal` module
- Create: `egui-frontend/src/ui/components/settings/backfill_modal.rs` — modal UI
- Modify: `egui-frontend/src/ui/state/modal_state.rs` — add `InitialSync` to `SettingsAction`
- Modify: `egui-frontend/src/ui/components/header.rs` — add menu item
- Modify: `egui-frontend/src/ui/app_state.rs` — add `execute_settings_action` handler
- Modify: `egui-frontend/src/ui/components/modals/shared/mod.rs` — register modal in `render_modals`

**zephytop-brain (zephytop-brain repo):**
- Modify: `services/allowance-tracker/src/sync_client.rs` — add query param support to `list_entities`
- Modify: `services/allowance-tracker/src/mcp.rs` — use sort/limit for `get_balance` and `list_recent_transactions`

---

### Task 1: Fix HttpRemoteClient push_events wire format

The `HttpRemoteClient.push_events()` sends events as a bare JSON array `[...]`, but the server's `POST /sync/events` endpoint expects `{"events": [...]}` (deserialized as `PushEventsRequest`). The response has the same issue — the server returns `{"sequences": [...]}` but the client tries to deserialize a bare `Vec<u64>`.

**Files:**
- Modify: `backend/storage/http_remote.rs:27-38`

- [ ] **Step 1: Write the fix**

In `backend/storage/http_remote.rs`, replace the `push_events` method:

```rust
fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>> {
    let url = format!("{}/sync/events", self.base_url);

    #[derive(serde::Serialize)]
    struct PushRequest<'a> {
        events: &'a [SyncEvent],
    }

    #[derive(serde::Deserialize)]
    struct PushResponse {
        sequences: Vec<u64>,
    }

    let response = self
        .client
        .post(&url)
        .json(&PushRequest { events })
        .send()?;

    self.check_status(response.status().as_u16())?;
    let parsed: PushResponse = response.json()?;
    Ok(parsed.sequences)
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p backend`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add backend/storage/http_remote.rs
git commit -m "fix: match push_events wire format to server's PushEventsRequest"
```

---

### Task 2: Add sort/limit query params to sync-service list endpoint

Add optional `sort` and `limit` query parameters to `GET /entities/{entity_type}/{child_id}`. These map directly to DynamoDB Query's `ScanIndexForward` and `Limit` options.

**Files:**
- Modify: `sync-service/src/routes/entities.rs`
- Modify: `sync-service/src/storage/dynamo.rs`
- Modify: `sync-service/tests/entity_crud_test.rs`

- [ ] **Step 1: Write the failing test**

Add to `sync-service/tests/entity_crud_test.rs`:

```rust
#[tokio::test]
async fn test_list_entities_with_sort_and_limit() {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available");
        return;
    }

    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_config());

    let child_id = "child::sort-test";

    // Insert 3 transactions with sort keys that have natural ordering
    let tx1 = r#"{"id":"tx-aaa","child_id":"child::sort-test","amount":10.0}"#;
    let tx2 = r#"{"id":"tx-bbb","child_id":"child::sort-test","amount":20.0}"#;
    let tx3 = r#"{"id":"tx-ccc","child_id":"child::sort-test","amount":30.0}"#;
    store.upsert_entity(child_id, EntityType::Transaction, "tx-aaa", tx1).await.unwrap();
    store.upsert_entity(child_id, EntityType::Transaction, "tx-bbb", tx2).await.unwrap();
    store.upsert_entity(child_id, EntityType::Transaction, "tx-ccc", tx3).await.unwrap();

    // Test descending sort
    let results = store.list_entities_for_child_with_options(child_id, EntityType::Transaction, Some("desc"), None).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].0, "tx-ccc");
    assert_eq!(results[2].0, "tx-aaa");

    // Test limit
    let results = store.list_entities_for_child_with_options(child_id, EntityType::Transaction, None, Some(2)).await.unwrap();
    assert_eq!(results.len(), 2);

    // Test desc + limit=1 (most recent)
    let results = store.list_entities_for_child_with_options(child_id, EntityType::Transaction, Some("desc"), Some(1)).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "tx-ccc");

    ctx.cleanup().await;
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sync-service --test entity_crud_test test_list_entities_with_sort_and_limit -- --nocapture`
Expected: FAIL — `list_entities_for_child_with_options` does not exist yet.

- [ ] **Step 3: Add `list_entities_for_child_with_options` to DynamoStore**

In `sync-service/src/storage/dynamo.rs`, add a new method after `list_entities_for_child`:

```rust
/// List entities for a child with optional sort direction and limit.
/// sort_direction: "asc" (default) or "desc". Maps to DynamoDB ScanIndexForward.
/// limit: max items to return.
pub async fn list_entities_for_child_with_options(
    &self,
    child_id: &str,
    entity_type: EntityType,
    sort_direction: Option<&str>,
    limit: Option<i32>,
) -> anyhow::Result<Vec<(String, String)>> {
    let (table, sort_key) = self.entity_table_and_sort_key(&entity_type);

    let scan_forward = match sort_direction {
        Some("desc") => false,
        _ => true, // default ascending
    };

    let mut query = self.client
        .query()
        .table_name(&table)
        .key_condition_expression("child_id = :cid")
        .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
        .scan_index_forward(scan_forward);

    if let Some(l) = limit {
        query = query.limit(l);
    }

    let response = query
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query table: {}", e))?;

    let mut results = Vec::new();
    if let Some(items) = response.items {
        for item in items {
            let entity_id = if let Some(sk_name) = sort_key {
                item.get(sk_name)
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            } else {
                item.get("child_id")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            };
            let data = item
                .get("data")
                .and_then(|v| v.as_s().ok())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "{}".to_string());
            results.push((entity_id, data));
        }
    }

    Ok(results)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sync-service --test entity_crud_test test_list_entities_with_sort_and_limit -- --nocapture`
Expected: PASS (requires DynamoDB Local running on port 8000).

- [ ] **Step 5: Add query params to the HTTP handler**

In `sync-service/src/routes/entities.rs`, add a query param struct and update the `list_entities` handler:

```rust
use axum::extract::Query;

#[derive(Debug, serde::Deserialize)]
struct ListEntitiesQuery {
    sort: Option<String>,
    limit: Option<i32>,
}

// Replace the existing list_entities handler:
async fn list_entities(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id)): Path<(String, String)>,
    Query(params): Query<ListEntitiesQuery>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match store.list_entities_for_child_with_options(
        &child_id,
        entity_type,
        params.sort.as_deref(),
        params.limit,
    ).await {
        Ok(entities) => {
            let json_list: Vec<String> = entities.into_iter().map(|(_, data)| data).collect();
            Ok(Json(json_list))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p sync-service`
Expected: Compiles without errors.

- [ ] **Step 7: Commit**

```bash
git add sync-service/src/routes/entities.rs sync-service/src/storage/dynamo.rs sync-service/tests/entity_crud_test.rs
git commit -m "feat: add sort/limit query params to list entities endpoint"
```

---

### Task 3: Add backfill method to SyncEngine

Add `BackfillProgress`, `BackfillResult`, and `backfill()` to the SyncEngine. The method accepts pre-loaded local entities, pushes them to remote via `RemoteStorage`, and reports progress through an `mpsc` channel.

**Files:**
- Modify: `backend/domain/sync_manager.rs`
- Modify: `backend/storage/mock_remote.rs` (for tests)

- [ ] **Step 1: Add BackfillProgress and BackfillResult types**

At the top of `backend/domain/sync_manager.rs` (after the existing `SyncMessage` enum), add:

```rust
use std::sync::mpsc;
use backend::domain::models::child::Child;
use backend::domain::models::transaction::Transaction;
use backend::domain::models::goal::DomainGoal;

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
```

Note: The import paths for `Child`, `Transaction`, and `DomainGoal` depend on the module structure. In this codebase, the backend domain models are at:
- `crate::backend::domain::models::child::Child`
- `crate::backend::domain::models::transaction::Transaction`
- `crate::backend::domain::models::goal::DomainGoal`

Adjust the imports to match the actual module paths visible from `sync_manager.rs`. If the file is at `backend/domain/sync_manager.rs`, the models may be accessible via `super::models::child::Child` etc.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p backend`
Expected: Compiles (types are defined but not yet used).

- [ ] **Step 3: Add the backfill method to SyncEngine**

Add this method to the `impl SyncEngine` block in `backend/domain/sync_manager.rs`:

```rust
/// Push all local entities to remote. Reports progress via the channel.
/// Safe to retry: entity upserts overwrite, sync event dedup prevents duplicate sequences.
pub fn backfill(
    &self,
    children: Vec<Child>,
    transactions: std::collections::HashMap<String, Vec<Transaction>>,
    goals: std::collections::HashMap<String, Vec<DomainGoal>>,
    progress_tx: mpsc::Sender<BackfillProgress>,
) -> Result<BackfillResult> {
    // Count total entities
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
        // Initialize child on remote
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

        // Upsert child entity
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

        // Upsert transactions for this child
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

                // Push events in batches
                if events.len() >= batch_size {
                    self.remote.push_events(&events)?;
                    let _ = progress_tx.send(BackfillProgress::EntitiesPushed {
                        count: pushed,
                        total,
                    });
                    events.clear();
                }
            }
        }

        // Upsert goals for this child
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
                    let _ = progress_tx.send(BackfillProgress::EntitiesPushed {
                        count: pushed,
                        total,
                    });
                    events.clear();
                }
            }
        }

        // Push remaining events for this child
        if !events.is_empty() {
            self.remote.push_events(&events)?;
            let _ = progress_tx.send(BackfillProgress::EntitiesPushed {
                count: pushed,
                total,
            });
        }
    }

    let _ = progress_tx.send(BackfillProgress::Completed { total_pushed: pushed });

    Ok(BackfillResult {
        children_synced,
        transactions_synced,
        goals_synced,
    })
}
```

- [ ] **Step 4: Write unit test for backfill**

Add to the `#[cfg(test)] mod tests` block at the bottom of `backend/domain/sync_manager.rs`:

```rust
#[test]
fn test_backfill_pushes_all_entities() {
    let (mut engine, mock) = make_engine_with_mock();
    let (tx, rx) = mpsc::channel();

    let child = Child {
        id: "child1".to_string(),
        name: "Alice".to_string(),
        birthdate: chrono::NaiveDate::from_ymd_opt(2018, 1, 1).unwrap(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let transaction = Transaction {
        id: "tx-1".to_string(),
        child_id: "child1".to_string(),
        date: chrono::Utc::now().fixed_offset(),
        description: "Allowance".to_string(),
        amount: 10.0,
        balance: 10.0,
        transaction_type: crate::backend::domain::models::transaction::TransactionType::Allowance,
    };

    let goal = DomainGoal {
        id: "goal-1".to_string(),
        child_id: "child1".to_string(),
        description: "Bicycle".to_string(),
        target_amount: 100.0,
        state: crate::backend::domain::models::goal::DomainGoalState::Active,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };

    let mut transactions = std::collections::HashMap::new();
    transactions.insert("child1".to_string(), vec![transaction]);
    let mut goals = std::collections::HashMap::new();
    goals.insert("child1".to_string(), vec![goal]);

    let result = engine.backfill(vec![child], transactions, goals, tx).unwrap();

    assert_eq!(result.children_synced, 1);
    assert_eq!(result.transactions_synced, 1);
    assert_eq!(result.goals_synced, 1);

    // Verify progress messages
    let mut messages = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        messages.push(msg);
    }
    assert!(messages.len() >= 3); // Starting, ChildInitialized, EntitiesPushed/Completed

    // Verify entities were pushed to mock
    let entity = mock.get_entity("child1", EntityType::Child, "child1").unwrap();
    assert!(entity.is_some());

    let entity = mock.get_entity("child1", EntityType::Transaction, "tx-1").unwrap();
    assert!(entity.is_some());

    let entity = mock.get_entity("child1", EntityType::Goal, "goal-1").unwrap();
    assert!(entity.is_some());
}
```

Note: The `MockRemoteClient` needs to support `upsert_entity` and `get_entity`. Check `backend/storage/mock_remote.rs` — if it doesn't implement these trait methods yet, add stub implementations that store entities in a `HashMap<String, String>` behind a `Mutex`. The `RemoteStorage` trait requires these methods so they should already be implemented.

- [ ] **Step 5: Run tests**

Run: `cargo test -p backend test_backfill_pushes_all_entities -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/domain/sync_manager.rs
git commit -m "feat: add backfill method to SyncEngine for initial data sync"
```

---

### Task 4: Add backfill UI to egui frontend

Add a settings menu item, modal, and background thread integration for triggering and monitoring the backfill operation.

**Files:**
- Modify: `egui-frontend/src/ui/state/modal_state.rs`
- Modify: `egui-frontend/src/ui/components/settings/state.rs`
- Modify: `egui-frontend/src/ui/components/settings/mod.rs`
- Create: `egui-frontend/src/ui/components/settings/backfill_modal.rs`
- Modify: `egui-frontend/src/ui/components/header.rs`
- Modify: `egui-frontend/src/ui/app_state.rs`
- Modify: `egui-frontend/src/ui/components/modals/shared/mod.rs`

- [ ] **Step 1: Add `InitialSync` to `SettingsAction`**

In `egui-frontend/src/ui/state/modal_state.rs`, add to the `SettingsAction` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    ShowProfile,
    CreateChild,
    ConfigureAllowance,
    DeleteTransactions,
    ExportData,
    DataDirectory,
    InitialSync,  // NEW
}
```

- [ ] **Step 2: Add `BackfillFormState` to settings state**

In `egui-frontend/src/ui/components/settings/state.rs`, add the form state struct:

```rust
use std::sync::mpsc;
use crate::backend::domain::sync_manager::BackfillProgress;

/// Form state for the initial sync / backfill modal
#[derive(Debug)]
pub struct BackfillFormState {
    pub is_running: bool,
    pub progress_rx: Option<mpsc::Receiver<BackfillProgress>>,
    pub entities_pushed: usize,
    pub total_entities: usize,
    pub result_message: Option<String>,
    pub error_message: Option<String>,
    /// Pre-computed entity counts for display before starting
    pub child_count: usize,
    pub transaction_count: usize,
    pub goal_count: usize,
}

impl BackfillFormState {
    pub fn new() -> Self {
        Self {
            is_running: false,
            progress_rx: None,
            entities_pushed: 0,
            total_entities: 0,
            result_message: None,
            error_message: None,
            child_count: 0,
            transaction_count: 0,
            goal_count: 0,
        }
    }

    pub fn clear(&mut self) {
        self.is_running = false;
        self.progress_rx = None;
        self.entities_pushed = 0;
        self.total_entities = 0;
        self.result_message = None;
        self.error_message = None;
        self.child_count = 0;
        self.transaction_count = 0;
        self.goal_count = 0;
    }

    /// Poll progress messages from the background thread. Call each frame.
    pub fn poll_progress(&mut self) {
        let Some(rx) = &self.progress_rx else { return };

        while let Ok(msg) = rx.try_recv() {
            match msg {
                BackfillProgress::Starting { total_entities } => {
                    self.total_entities = total_entities;
                    self.entities_pushed = 0;
                }
                BackfillProgress::ChildInitialized { .. } => {}
                BackfillProgress::EntitiesPushed { count, total } => {
                    self.entities_pushed = count;
                    self.total_entities = total;
                }
                BackfillProgress::Completed { total_pushed } => {
                    self.is_running = false;
                    self.entities_pushed = total_pushed;
                    self.result_message = Some(format!("Synced {} entities successfully", total_pushed));
                }
                BackfillProgress::Failed { error, pushed_so_far } => {
                    self.is_running = false;
                    self.entities_pushed = pushed_so_far;
                    self.error_message = Some(error);
                }
            }
        }
    }
}

impl Default for BackfillFormState {
    fn default() -> Self {
        Self::new()
    }
}
```

Then add the new fields to `SettingsState`:

```rust
pub struct SettingsState {
    // ... existing fields ...

    /// Whether the backfill modal is visible
    pub show_backfill_modal: bool,

    /// Backfill form state
    pub backfill_form: BackfillFormState,
}
```

Update `SettingsState::new()` to initialize these:

```rust
show_backfill_modal: false,
backfill_form: BackfillFormState::new(),
```

Update `hide_all_modals()` to include:

```rust
self.show_backfill_modal = false;
```

Update `reset_all_forms()` to include:

```rust
self.backfill_form.clear();
```

- [ ] **Step 3: Add the menu item to the settings dropdown**

In `egui-frontend/src/ui/components/header.rs`, add a new `DropdownMenuItem` to the `menu_items` vec (after "Data directory"):

```rust
DropdownMenuItem {
    label: "Initial sync".to_string(),
    icon: Some("🔄".to_string()),
    is_current: false,
    is_enabled: true,
},
```

Update the match statement that maps index to `SettingsAction`:

```rust
if let Some(index) = selected_index {
    let settings_action = match index {
        0 => crate::ui::state::modal_state::SettingsAction::ShowProfile,
        1 => crate::ui::state::modal_state::SettingsAction::CreateChild,
        2 => crate::ui::state::modal_state::SettingsAction::ConfigureAllowance,
        3 => crate::ui::state::modal_state::SettingsAction::DeleteTransactions,
        4 => crate::ui::state::modal_state::SettingsAction::ExportData,
        5 => crate::ui::state::modal_state::SettingsAction::DataDirectory,
        6 => crate::ui::state::modal_state::SettingsAction::InitialSync,  // NEW
        _ => {
            log::warn!("Unknown settings menu item clicked: {}", index);
            return;
        }
    };
    // ... rest unchanged
```

- [ ] **Step 4: Add the execute_settings_action handler**

In `egui-frontend/src/ui/app_state.rs`, add the `InitialSync` arm to the `execute_settings_action` match:

```rust
SettingsAction::InitialSync => {
    info!("Initial sync action - opening modal");
    self.settings.show_backfill_modal = true;
    self.settings.backfill_form.clear();

    // Pre-compute entity counts
    if let Ok(children_result) = self.backend().child_service.list_children() {
        let children = &children_result.children;
        self.settings.backfill_form.child_count = children.len();

        let mut tx_count = 0;
        let mut goal_count = 0;
        for child in children {
            // Count transactions
            let query = crate::backend::domain::commands::TransactionListQuery {
                after: None,
                limit: None,
                start_date: None,
                end_date: None,
            };
            if let Ok(tx_result) = self.backend().transaction_service.list_transactions_for_child(&child.id, query) {
                tx_count += tx_result.transactions.len();
            }
            // Count goals
            if let Ok(goal_result) = self.backend().goal_service.get_goal_history_for_child(&child.id, None) {
                goal_count += goal_result.goals.len();
            }
        }
        self.settings.backfill_form.transaction_count = tx_count;
        self.settings.backfill_form.goal_count = goal_count;
        self.settings.backfill_form.total_entities = self.settings.backfill_form.child_count + tx_count + goal_count;
    }
}
```

Note: The exact method names for listing transactions/goals for a specific child may differ. Check the actual service API:
- Transactions: look for a method on `TransactionService` that takes a `child_id` parameter. If only `list_transactions` exists (which uses the active child), you may need to use the repository directly or add a method that accepts `child_id`.
- Goals: similar — `GoalService` may require a command with `child_id`. Use `GetGoalHistoryCommand { child_id, limit: None }`.

Adapt the method calls to match the actual API.

- [ ] **Step 5: Create the backfill modal**

Create `egui-frontend/src/ui/components/settings/backfill_modal.rs`:

```rust
use egui::{Align2, Area, Color32, Frame, Id, Order, RichText, Vec2};
use crate::ui::app_state::AllowanceTrackerApp;

impl AllowanceTrackerApp {
    pub fn render_backfill_modal(&mut self, ctx: &egui::Context) {
        if !self.settings.show_backfill_modal {
            return;
        }

        // Poll for progress updates
        self.settings.backfill_form.poll_progress();

        // If running, request repaint to keep polling
        if self.settings.backfill_form.is_running {
            ctx.request_repaint();
        }

        // Dark backdrop
        let screen_rect = ctx.screen_rect();
        Area::new(Id::new("backfill_backdrop"))
            .fixed_pos(screen_rect.min)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let (response, _) = ui.allocate_exact_size(
                    screen_rect.size(),
                    egui::Sense::click(),
                );
                ui.painter().rect_filled(
                    screen_rect,
                    0.0,
                    Color32::from_black_alpha(128),
                );
                // Click backdrop to close (only if not running)
                if response.clicked() && !self.settings.backfill_form.is_running {
                    self.settings.show_backfill_modal = false;
                }
            });

        // Modal content
        Area::new(Id::new("backfill_modal"))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::window(ui.style())
                    .inner_margin(20.0)
                    .show(ui, |ui| {
                        ui.set_width(350.0);
                        ui.heading("Initial Sync");
                        ui.add_space(10.0);

                        let form = &self.settings.backfill_form;

                        if let Some(ref result) = form.result_message {
                            // Completed state
                            ui.label(RichText::new(result).color(Color32::GREEN));
                            ui.add_space(10.0);
                            if ui.button("Close").clicked() {
                                self.settings.show_backfill_modal = false;
                            }
                        } else if let Some(ref error) = form.error_message {
                            // Error state
                            ui.label(RichText::new(format!("Error: {}", error)).color(Color32::RED));
                            if form.entities_pushed > 0 {
                                ui.label(format!("({} entities synced before failure)", form.entities_pushed));
                            }
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Retry").clicked() {
                                    self.start_backfill();
                                }
                                if ui.button("Close").clicked() {
                                    self.settings.show_backfill_modal = false;
                                }
                            });
                        } else if form.is_running {
                            // Running state
                            ui.label(format!(
                                "Syncing... {}/{}",
                                form.entities_pushed, form.total_entities
                            ));
                            let progress = if form.total_entities > 0 {
                                form.entities_pushed as f32 / form.total_entities as f32
                            } else {
                                0.0
                            };
                            ui.add(egui::ProgressBar::new(progress));
                        } else {
                            // Ready state
                            ui.label(format!(
                                "Ready to sync {} entities to remote:",
                                form.total_entities
                            ));
                            ui.add_space(5.0);
                            ui.label(format!("  {} children", form.child_count));
                            ui.label(format!("  {} transactions", form.transaction_count));
                            ui.label(format!("  {} goals", form.goal_count));
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.button("Start Sync").clicked() {
                                    self.start_backfill();
                                }
                                if ui.button("Cancel").clicked() {
                                    self.settings.show_backfill_modal = false;
                                }
                            });
                        }
                    });
            });
    }
}
```

- [ ] **Step 6: Add `start_backfill` method**

In `egui-frontend/src/ui/app_state.rs`, add the method that spawns the background thread:

```rust
impl AllowanceTrackerApp {
    fn start_backfill(&mut self) {
        use std::sync::mpsc;
        use std::thread;

        self.settings.backfill_form.is_running = true;
        self.settings.backfill_form.result_message = None;
        self.settings.backfill_form.error_message = None;
        self.settings.backfill_form.entities_pushed = 0;

        let (progress_tx, progress_rx) = mpsc::channel();
        self.settings.backfill_form.progress_rx = Some(progress_rx);

        // Load all local data
        let children = match self.backend().child_service.list_children() {
            Ok(result) => result.children,
            Err(e) => {
                self.settings.backfill_form.is_running = false;
                self.settings.backfill_form.error_message = Some(format!("Failed to load children: {}", e));
                return;
            }
        };

        let mut transactions = std::collections::HashMap::new();
        let mut goals = std::collections::HashMap::new();

        for child in &children {
            // Load transactions for this child
            let query = crate::backend::domain::commands::TransactionListQuery {
                after: None,
                limit: None,
                start_date: None,
                end_date: None,
            };
            if let Ok(tx_result) = self.backend().transaction_service.list_transactions_for_child(&child.id, query) {
                transactions.insert(child.id.clone(), tx_result.transactions);
            }

            // Load goals for this child
            if let Ok(goal_result) = self.backend().goal_service.get_goal_history_for_child(&child.id, None) {
                goals.insert(child.id.clone(), goal_result.goals);
            }
        }

        // Clone the remote storage reference for the background thread
        // The SyncEngine needs an Arc<dyn RemoteStorage> — get it from the existing sync infrastructure
        let remote = self.get_remote_storage();

        thread::spawn(move || {
            let engine = crate::backend::domain::sync_manager::SyncEngine::new(remote);
            match engine.backfill(children, transactions, goals, progress_tx.clone()) {
                Ok(_) => {} // Completed message already sent by backfill()
                Err(e) => {
                    let _ = progress_tx.send(crate::backend::domain::sync_manager::BackfillProgress::Failed {
                        error: e.to_string(),
                        pushed_so_far: 0,
                    });
                }
            }
        });
    }
}
```

Note: `self.get_remote_storage()` needs to return an `Arc<dyn RemoteStorage>`. This method may not exist yet. You'll need to either:
- Create it by constructing an `HttpRemoteClient` from the sync service URL in the app's configuration
- Or extract it from the existing sync infrastructure if sync is already set up

Check how the existing sync thread gets its `RemoteStorage` reference and follow the same pattern. The remote URL is likely stored in sync state/config.

- [ ] **Step 7: Register the modal module and render call**

In `egui-frontend/src/ui/components/settings/mod.rs`, add:

```rust
pub mod backfill_modal;
```

In `egui-frontend/src/ui/components/modals/shared/mod.rs`, add to `render_modals()`:

```rust
self.render_backfill_modal(ctx);
```

- [ ] **Step 8: Verify compilation**

Run: `cargo check -p egui-frontend`
Expected: Compiles without errors. May require adjusting import paths and method signatures to match the actual codebase.

- [ ] **Step 9: Commit**

```bash
git add egui-frontend/src/ui/components/settings/state.rs egui-frontend/src/ui/components/settings/mod.rs egui-frontend/src/ui/components/settings/backfill_modal.rs egui-frontend/src/ui/state/modal_state.rs egui-frontend/src/ui/components/header.rs egui-frontend/src/ui/app_state.rs egui-frontend/src/ui/components/modals/shared/mod.rs
git commit -m "feat: add initial sync backfill modal to settings"
```

---

### Task 5: Update MCP server to use sort/limit query params

Update the MCP Lambda's sync client and tool implementations to use the new `sort` and `limit` query params, eliminating the need to fetch all transactions client-side.

**Files:**
- Modify: `zephytop-brain/services/allowance-tracker/src/sync_client.rs`
- Modify: `zephytop-brain/services/allowance-tracker/src/mcp.rs`

- [ ] **Step 1: Add query param support to sync_client.rs**

In `zephytop-brain/services/allowance-tracker/src/sync_client.rs`, update the `list_entities` method to accept optional query params:

```rust
/// GET /internal/entities/{entity_type}/{child_id} — list entities for a child
/// Optional query params: sort (asc/desc), limit (positive integer)
pub async fn list_entities(
    &self,
    entity_type: &str,
    child_id: &str,
    sort: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<Value>, String> {
    let mut url = format!("{}/internal/entities/{}/{}", self.base_url, entity_type, child_id);

    let mut params = Vec::new();
    if let Some(s) = sort {
        params.push(format!("sort={}", s));
    }
    if let Some(l) = limit {
        params.push(format!("limit={}", l));
    }
    if !params.is_empty() {
        url = format!("{}?{}", url, params.join("&"));
    }

    let resp = self.http.get(&url).send().await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("list_entities returned {}", resp.status()));
    }

    let json_strings: Vec<String> = resp.json().await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    let mut results = Vec::new();
    for s in json_strings {
        let val: Value = serde_json::from_str(&s)
            .map_err(|e| format!("Failed to parse entity JSON: {e}"))?;
        results.push(val);
    }
    Ok(results)
}
```

- [ ] **Step 2: Update all callers of list_entities in mcp.rs**

In `zephytop-brain/services/allowance-tracker/src/mcp.rs`, update each tool that calls `list_entities`:

**`tool_list_children`** — no change needed (uses `list_children`, not `list_entities`)

**`tool_get_balance`** — replace the transaction fetching logic:

```rust
async fn tool_get_balance(client: &SyncClient, child_id: &str) -> Result<String, String> {
    // Get child info for the name
    let children = client.list_children().await?;
    let child = children.iter()
        .find(|c| c["id"].as_str() == Some(child_id))
        .ok_or_else(|| format!("Child not found: {child_id}"))?;
    let name = child["name"].as_str().unwrap_or("Unknown");

    // Get most recent transaction (sorted desc, limit 1)
    let transactions = client.list_entities("transaction", child_id, Some("desc"), Some(1)).await?;

    let balance = transactions.first()
        .and_then(|tx| tx["balance"].as_f64())
        .unwrap_or(0.0);

    let result = json!({
        "child_id": child_id,
        "name": name,
        "balance": balance,
    });
    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}
```

**`tool_list_recent_transactions`** — replace with server-side sort/limit:

```rust
async fn tool_list_recent_transactions(
    client: &SyncClient,
    child_id: &str,
    limit: usize,
) -> Result<String, String> {
    let transactions = client.list_entities("transaction", child_id, Some("desc"), Some(limit)).await?;

    let result: Vec<Value> = transactions.into_iter().map(|tx| {
        json!({
            "date": tx["date"],
            "description": tx["description"],
            "amount": tx["amount"],
            "balance": tx["balance"],
            "transaction_type": tx["transaction_type"],
        })
    }).collect();

    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}
```

**`tool_add_expense`** — update the balance lookup:

```rust
// Replace the "Get current balance" section:
let transactions = client.list_entities("transaction", child_id, Some("desc"), Some(1)).await?;

let current_balance = transactions.first()
    .and_then(|tx| tx["balance"].as_f64())
    .unwrap_or(0.0);
```

**`tool_list_goals`** — add the extra params:

```rust
async fn tool_list_goals(client: &SyncClient, child_id: &str) -> Result<String, String> {
    let goals = client.list_entities("goal", child_id, None, None).await?;
    // ... rest unchanged
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p allowance-tracker` (from the zephytop-brain workspace)
Expected: Compiles without errors.

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p allowance-tracker`
Expected: All 3 existing unit tests pass (initialize_response, tools_list_response, jsonrpc_error). These don't call list_entities so they should still pass.

- [ ] **Step 5: Commit**

```bash
git -C /Users/kerryhart/Documents/Code/zephytop-brain add services/allowance-tracker/src/sync_client.rs services/allowance-tracker/src/mcp.rs
git -C /Users/kerryhart/Documents/Code/zephytop-brain commit -m "feat: use sort/limit query params for efficient MCP queries"
```

---

### Task 6: Deploy and validate end-to-end

Deploy both stacks and verify the backfill + MCP integration works.

**Files:** No code changes. Infrastructure deployment only.

- [ ] **Step 1: Deploy updated sync-service**

```bash
cd /Users/kerryhart/Documents/Code/allowance\ tracker\ code/allowance-tracker/infrastructure
sam build
sam deploy
```

Expected: Stack updates successfully with the new sort/limit query param support.

- [ ] **Step 2: Verify sync-service health**

```bash
curl https://i99kq799kd.execute-api.us-east-2.amazonaws.com/health
```

Expected: `{"status": "ok"}` (or similar 200 response).

- [ ] **Step 3: Deploy updated zephytop-brain**

```bash
cd /Users/kerryhart/Documents/Code/zephytop-brain/infrastructure
sam build
sam deploy
```

Expected: Stack updates successfully with the MCP sort/limit changes.

- [ ] **Step 4: Test backfill via the app**

1. Run the egui app: `cargo run -p egui-frontend`
2. Open Settings → Initial Sync
3. Verify entity counts are shown
4. Click "Start Sync"
5. Verify progress updates and completion message

- [ ] **Step 5: Verify MCP tools work in Claude.ai**

Ask Claude: "What tools do you have from the allowance tracker integration?"
Then: "What's Keiko's current balance?"

Expected: Claude can now list children, show balances, list transactions, and list goals using the backfilled data.

- [ ] **Step 6: Commit any deployment config changes**

If any template changes were needed, commit them.
