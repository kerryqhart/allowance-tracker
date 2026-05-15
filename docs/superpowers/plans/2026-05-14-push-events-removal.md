# Push Events Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the redundant `POST /sync/events` write path from local app and server, and bring `DELETE /entities` to parity with `PUT /entities` (atomic entity + event via TransactWriteItems).

**Architecture:** Two-side change. Server gains `delete_entity_with_event` (atomic delete + Deleted event) and its DELETE route reads `X-Sync-Source`. Local app stops calling `push_events`, sends `X-Sync-Source: local` on PUT/DELETE so server-emitted events are tagged `Local` and the existing echo-skip on poll keeps working. The `RemoteStorage::push_events` trait method, `MockRemoteClient::push_events`, the HTTP client's `push_events`, and the server's `POST /sync/events` route all disappear. Sync_manager's dead `enqueue_event`/`push_pending` code goes with them; `backfill()` is rewritten to drive everything through `upsert_entity`.

**Tech Stack:** Rust, Axum (sync-service), reqwest (local app HTTP client), DynamoDB Local for sync-service integration tests, Cargo workspaces.

**Spec:** [2026-05-14-push-events-removal-design.md](../specs/2026-05-14-push-events-removal-design.md)

---

## File Structure

**Server (`sync-service/`):**
- Modify `src/storage/dynamo.rs` — add `delete_entity_with_event`, remove `push_event`, remove `delete_entity`.
- Modify `src/routes/entities.rs` — DELETE handler reads `X-Sync-Source`, calls `delete_entity_with_event`.
- Modify `src/routes/sync.rs` — remove `push_events` handler, types, and `POST /sync/events` route registration.
- Modify `tests/atomic_upsert_test.rs` — add three DELETE-path tests.
- Modify `tests/api_test.rs` — remove `test_push_events_endpoint`.

**Local app (`backend/`):**
- Modify `storage/remote.rs` — remove `push_events` from `RemoteStorage` trait.
- Modify `storage/http_remote.rs` — add `X-Sync-Source: local` headers on PUT/DELETE; remove `push_events` impl.
- Modify `storage/mock_remote.rs` — `upsert_entity` and `delete_entity` emit events; add `seed_event` helper; remove `push_events` impl; update mock's own tests.
- Modify `domain/sync_thread.rs::push_event` — drop both `push_events` calls.
- Modify `domain/sync_manager.rs` — remove `enqueue_event`, `push_pending`, `pending_push` field; remove the two unit tests that exercised them; migrate other tests from `mock.push_events(...)` seeding to `mock.seed_event(...)`; rewrite `backfill()` to use `upsert_entity` only; update `test_backfill_pushes_all_entities`.

---

## Task 1: Server — `DynamoStore::delete_entity_with_event` (TDD)

**Files:**
- Modify: `sync-service/src/storage/dynamo.rs` (add method ~line 442)
- Test: `sync-service/tests/atomic_upsert_test.rs` (append three tests)

- [ ] **Step 1: Write the first failing integration test**

Append to `sync-service/tests/atomic_upsert_test.rs`:

```rust
#[tokio::test]
async fn delete_emits_deleted_event_atomically() {
    let Some((ctx, store)) = setup().await else { return; };
    let child_id = "atomic-child-4";
    store.initialize_child_metadata(child_id).await.unwrap();

    let tx_json = r#"{"id":"tx4","child_id":"atomic-child-4","amount":-3.0,"date":"2026-05-14T00:00:00+00:00","description":"to delete","balance":97.0,"transaction_type":"Expense"}"#;
    store.upsert_entity_with_event(
        child_id, EntityType::Transaction, "tx4", tx_json, SyncSource::Remote,
    ).await.unwrap();

    store.delete_entity_with_event(
        child_id, EntityType::Transaction, "tx4", SyncSource::Local,
    ).await.unwrap();

    let events = store.get_events_since(child_id, 0).await.unwrap();
    assert_eq!(events.len(), 2, "expected created + deleted events");
    assert_eq!(events[1].action, SyncAction::Deleted);
    assert_eq!(events[1].event_id, "ev::deleted::tx4");
    assert_eq!(events[1].source, SyncSource::Local);

    let entity = store.get_entity(child_id, EntityType::Transaction, "tx4").await.unwrap();
    assert_eq!(entity, None, "entity should be gone after delete");

    ctx.cleanup().await;
}

#[tokio::test]
async fn delete_is_idempotent_on_retry() {
    let Some((ctx, store)) = setup().await else { return; };
    let child_id = "atomic-child-5";
    store.initialize_child_metadata(child_id).await.unwrap();

    let tx_json = r#"{"id":"tx5","child_id":"atomic-child-5","amount":-1.0,"date":"2026-05-14T00:00:00+00:00","description":"d","balance":99.0,"transaction_type":"Expense"}"#;
    store.upsert_entity_with_event(
        child_id, EntityType::Transaction, "tx5", tx_json, SyncSource::Remote,
    ).await.unwrap();

    store.delete_entity_with_event(
        child_id, EntityType::Transaction, "tx5", SyncSource::Remote,
    ).await.unwrap();
    // Retry: must succeed and not write a duplicate event.
    store.delete_entity_with_event(
        child_id, EntityType::Transaction, "tx5", SyncSource::Remote,
    ).await.unwrap();

    let events = store.get_events_since(child_id, 0).await.unwrap();
    let deleted_count = events.iter().filter(|e| e.action == SyncAction::Deleted).count();
    assert_eq!(deleted_count, 1, "retry should not produce a duplicate deleted event");

    ctx.cleanup().await;
}

#[tokio::test]
async fn delete_of_nonexistent_entity_still_emits_event() {
    let Some((ctx, store)) = setup().await else { return; };
    let child_id = "atomic-child-6";
    store.initialize_child_metadata(child_id).await.unwrap();

    store.delete_entity_with_event(
        child_id, EntityType::Transaction, "ghost", SyncSource::Local,
    ).await.unwrap();

    let events = store.get_events_since(child_id, 0).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].action, SyncAction::Deleted);
    assert_eq!(events[0].event_id, "ev::deleted::ghost");

    ctx.cleanup().await;
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sync-service --test atomic_upsert_test delete_ -- --nocapture`
Expected: 3 compile errors — `no method named delete_entity_with_event found for struct DynamoStore`. (DynamoDB Local must be running on the configured port; if not, tests skip and you cannot validate — start it first via the project's usual dev script.)

- [ ] **Step 3: Implement `delete_entity_with_event`**

In `sync-service/src/storage/dynamo.rs`, add this method **before** the existing `pub async fn delete_entity` (~line 442). Model it on `upsert_entity_with_event`:

```rust
/// Delete an entity AND emit a Deleted sync event atomically via TransactWriteItems.
/// Idempotent on retry: if the event already exists (same event_id), swallow the
/// ConditionalCheckFailed and return Ok. The entity Delete inside the failed
/// transaction is rolled back, but if it ran successfully on a prior call the
/// entity is already gone — which is what we want.
pub async fn delete_entity_with_event(
    &self,
    child_id: &str,
    entity_type: EntityType,
    entity_id: &str,
    source: SyncSource,
) -> anyhow::Result<()> {
    use aws_sdk_dynamodb::types::{Delete, Put, TransactWriteItem};
    use chrono::Utc;
    use crate::storage::event_id::event_id_for;

    let event_id = event_id_for(&SyncAction::Deleted, entity_id, "");
    let new_sequence = self.allocate_event_sequence(child_id).await?;

    let event = SyncEvent {
        event_id,
        entity_type: entity_type.clone(),
        entity_id: entity_id.to_string(),
        child_id: child_id.to_string(),
        action: SyncAction::Deleted,
        source,
        source_timestamp: Utc::now(),
        sequence: Some(new_sequence),
    };

    let (entity_table, sort_key) = self.entity_table_and_sort_key(&entity_type);
    let mut entity_key = HashMap::from([
        ("child_id".to_string(), AttributeValue::S(child_id.to_string())),
    ]);
    if let Some(sk_name) = sort_key {
        entity_key.insert(sk_name.to_string(), AttributeValue::S(entity_id.to_string()));
    }

    let entity_delete = Delete::builder()
        .table_name(&entity_table)
        .set_key(Some(entity_key))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build entity Delete: {}", e))?;

    let event_item = self.event_to_item(&event, new_sequence);
    let event_put = Put::builder()
        .table_name(&self.config.sync_events)
        .set_item(Some(event_item))
        .condition_expression("attribute_not_exists(#seq)")
        .expression_attribute_names("#seq", "sequence")
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build event Put: {}", e))?;

    let result = self.client
        .transact_write_items()
        .transact_items(TransactWriteItem::builder().delete(entity_delete).build())
        .transact_items(TransactWriteItem::builder().put(event_put).build())
        .send()
        .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = format!("{}", aws_sdk_dynamodb::error::DisplayErrorContext(&e));
            if msg.contains("ConditionalCheckFailed") {
                // Event already recorded by a prior call — treat as idempotent success.
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "TransactWriteItems (delete) failed for {}/{}: {}",
                    entity_type.as_str(), entity_id, msg
                ))
            }
        }
    }
}
```

Note: `event_id_for` already handles `SyncAction::Deleted` and ignores the `entity_json` arg for that variant (see `storage/event_id.rs:18`). Passing `""` is fine.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sync-service --test atomic_upsert_test delete_ -- --nocapture`
Expected: all 3 new tests pass. Existing tests in the file should still pass; run the full file: `cargo test -p sync-service --test atomic_upsert_test`.

- [ ] **Step 5: Commit**

```bash
git add sync-service/src/storage/dynamo.rs sync-service/tests/atomic_upsert_test.rs
git commit -m "feat(sync-service): add delete_entity_with_event atomic delete+event helper"
```

---

## Task 2: Server — wire DELETE route to `delete_entity_with_event`

**Files:**
- Modify: `sync-service/src/routes/entities.rs:63-77`

- [ ] **Step 1: Update the DELETE handler**

Replace the existing `delete_entity` handler in `sync-service/src/routes/entities.rs` (lines 63–77) with:

```rust
// DELETE /entities/{entity_type}/{child_id}/{entity_id} - delete entity and emit event atomically
async fn delete_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let source = match headers.get("x-sync-source").and_then(|v| v.to_str().ok()) {
        Some("local") => shared::sync::SyncSource::Local,
        _ => shared::sync::SyncSource::Remote,  // default for absent or any other value
    };

    match store.delete_entity_with_event(&child_id, entity_type, &entity_id, source).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            eprintln!("delete_entity_with_event failed for {}/{}: {:?}", child_id, entity_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build -p sync-service`
Expected: builds clean. Existing `store.delete_entity(...)` callers (if any) and the route registration at line 151 keep working.

- [ ] **Step 3: Run all sync-service tests**

Run: `cargo test -p sync-service`
Expected: all tests pass. (DynamoDB Local must be running for integration tests to actually execute; otherwise they will skip and you cannot validate behavior — start it.)

- [ ] **Step 4: Commit**

```bash
git add sync-service/src/routes/entities.rs
git commit -m "feat(sync-service): DELETE /entities reads X-Sync-Source and emits event atomically"
```

---

## Task 3: Server — remove unused `DynamoStore::delete_entity`

**Files:**
- Modify: `sync-service/src/storage/dynamo.rs:442-462`

- [ ] **Step 1: Confirm no remaining callers**

Run: `grep -rn "\.delete_entity(" sync-service/src/ sync-service/tests/`
Expected: no production callers (only the new `delete_entity_with_event` references should remain). If any test calls `store.delete_entity(...)` directly, migrate it to `store.delete_entity_with_event(..., SyncSource::Remote)` in this step.

- [ ] **Step 2: Delete the method**

In `sync-service/src/storage/dynamo.rs`, remove the entire `pub async fn delete_entity` block (currently ~lines 442–462).

- [ ] **Step 3: Build and test**

Run: `cargo build -p sync-service`
Then: `cargo test -p sync-service`
Expected: both pass.

- [ ] **Step 4: Commit**

```bash
git add sync-service/src/storage/dynamo.rs
git commit -m "refactor(sync-service): remove unused DynamoStore::delete_entity"
```

---

## Task 4: Local app — `X-Sync-Source: local` header on PUT and DELETE

**Files:**
- Modify: `backend/storage/http_remote.rs:71-93` (upsert_entity), `backend/storage/http_remote.rs:118-130` (delete_entity)

- [ ] **Step 1: Add header to `upsert_entity`**

In `backend/storage/http_remote.rs`, locate the `upsert_entity` method (lines 71–93) and update the request build:

```rust
fn upsert_entity(
    &self,
    child_id: &str,
    entity_type: EntityType,
    entity_id: &str,
    entity_json: &str,
) -> Result<()> {
    let url = format!(
        "{}/entities/{}/{}/{}",
        self.base_url,
        entity_type.as_str(),
        child_id,
        entity_id
    );
    let response = self
        .client
        .put(&url)
        .header("X-Sync-Source", "local")
        .body(entity_json.to_string())
        .send()?;

    self.check_response(&url, response)?;
    Ok(())
}
```

- [ ] **Step 2: Add header to `delete_entity`**

In the same file, update `delete_entity` (lines 118–130):

```rust
fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()> {
    let url = format!(
        "{}/entities/{}/{}/{}",
        self.base_url,
        entity_type.as_str(),
        child_id,
        entity_id
    );
    let response = self.client.delete(&url)
        .header("X-Sync-Source", "local")
        .send()?;

    self.check_response(&url, response)?;
    Ok(())
}
```

- [ ] **Step 3: Build and test**

Run: `cargo build -p allowance-tracker`
Then: `cargo test -p allowance-tracker --lib`
Expected: clean build, all unit tests pass. No behavioral change visible from `MockRemoteClient`-based tests.

- [ ] **Step 4: Commit**

```bash
git add backend/storage/http_remote.rs
git commit -m "feat(backend): send X-Sync-Source: local on PUT and DELETE /entities"
```

---

## Task 5: Local app — add `MockRemoteClient::seed_event`, migrate seeding-only call sites

**Files:**
- Modify: `backend/storage/mock_remote.rs` (add helper)
- Modify: `backend/domain/sync_manager.rs` (test seeding migrations)
- Modify: `backend/domain/sync_thread.rs:474` (test seeding migration)

This task is **purely additive** for the helper, then mechanical for the migrations. After this task, no test calls `mock.push_events(...)` to *seed* events; the production `push_events` impl on the mock is still in place and untouched.

- [ ] **Step 1: Add `seed_event` to `MockRemoteClient`**

In `backend/storage/mock_remote.rs`, add this method inside the bare `impl MockRemoteClient` block (between `clear_error` at line 32 and `check_error` at line 36 is fine):

```rust
/// Test-only helper: insert a `SyncEvent` directly into the mock's event log,
/// bypassing the trait surface. Used by tests that need to seed remote-origin
/// or local-origin events without going through upsert/delete. Assigns a
/// monotonic sequence the same way `push_events` does, and dedups by event_id.
#[cfg(test)]
pub fn seed_event(&self, event: SyncEvent) -> u64 {
    let mut event_dedup = self.event_dedup.lock().unwrap();
    let mut stored_events = self.events.lock().unwrap();
    let mut next_seq = self.next_sequence.lock().unwrap();

    if let Some(&existing_seq) = event_dedup.get(&event.event_id) {
        return existing_seq;
    }
    let seq = *next_seq;
    *next_seq += 1;
    let mut new_event = event;
    new_event.sequence = Some(seq);
    let id = new_event.event_id.clone();
    stored_events.push(new_event);
    event_dedup.insert(id, seq);
    seq
}
```

Note the `#[cfg(test)]` so the helper is only compiled in test builds. The mock is shared with non-test backend code through `Arc<dyn RemoteStorage>`, so external code cannot accidentally call this.

But: the mock is referenced by tests in *other* crates' test trees too (e.g., `backend/domain/sync_manager.rs` tests). Confirm visibility: if those tests are in the same crate (they are — `backend` is a single crate), `#[cfg(test)]` is fine. If after Step 2 you see "method not found" from a sibling crate's tests, drop `#[cfg(test)]` and re-run.

- [ ] **Step 2: Migrate `sync_manager.rs` seed sites**

In `backend/domain/sync_manager.rs`, change every test `mock.push_events(&[<event>]).unwrap();` that exists solely to seed remote-origin or local-origin events for poll tests to `mock.seed_event(<event>);`. Concretely the call sites are at lines 427, 443, 464, 485, 507 (current source). Each pattern:

Before:
```rust
mock.push_events(&[remote_event]).unwrap();
```

After:
```rust
mock.seed_event(remote_event);
```

Do not touch lines 393, 411 — those are `engine.enqueue_event(...)` calls, removed in Task 7.

- [ ] **Step 3: Migrate `sync_thread.rs:474`**

In `backend/domain/sync_thread.rs::test_thread_responds_to_poll_now` (~line 474):

Before:
```rust
mock.push_events(&[remote_event]).unwrap();
```

After:
```rust
mock.seed_event(remote_event);
```

- [ ] **Step 4: Build and run tests**

Run: `cargo build -p allowance-tracker --tests`
Then: `cargo test -p allowance-tracker --lib`
Expected: all green. Behavior unchanged.

- [ ] **Step 5: Commit**

```bash
git add backend/storage/mock_remote.rs backend/domain/sync_manager.rs backend/domain/sync_thread.rs
git commit -m "test(backend): add MockRemoteClient::seed_event and migrate seed sites"
```

---

## Task 6: Local app — mock emits events on upsert/delete + drop push_events from sync_thread

This is the **coupled change** at the heart of the cleanup. Mock starts emitting events on upsert/delete (matching new server behavior), and `sync_thread::push_event` stops issuing the now-duplicate `remote.push_events(...)` calls. Doing both at once avoids a transient state with duplicate events.

**Files:**
- Modify: `backend/storage/mock_remote.rs` (upsert_entity, delete_entity)
- Modify: `backend/domain/sync_thread.rs:226-275` (push_event function)

- [ ] **Step 1: Update `MockRemoteClient::upsert_entity` to emit an event**

In `backend/storage/mock_remote.rs`, replace the existing `upsert_entity` impl (~lines 91–107) with:

```rust
fn upsert_entity(
    &self,
    child_id: &str,
    entity_type: EntityType,
    entity_id: &str,
    entity_json: &str,
) -> Result<()> {
    self.check_error()?;

    let key = (
        child_id.to_string(),
        entity_type.as_str().to_string(),
        entity_id.to_string(),
    );

    let mut entities = self.entities.lock().unwrap();
    let prior = entities.get(&key).cloned();
    if prior.as_deref() == Some(entity_json) {
        // Identical content — no event, no-op. Mirrors server short-circuit.
        return Ok(());
    }
    let action = if prior.is_none() {
        SyncAction::Created
    } else {
        SyncAction::Updated
    };
    entities.insert(key, entity_json.to_string());
    drop(entities);

    // Emit event with deterministic id, source=Local (mirrors what the deployed
    // server will produce given an X-Sync-Source: local request — which is what
    // the http client now sends).
    let event_id = match action {
        SyncAction::Created => format!("ev::created::{entity_id}"),
        SyncAction::Updated => format!("ev::updated::{entity_id}::mock"),
        SyncAction::Deleted => unreachable!(),
    };
    let event = SyncEvent {
        event_id,
        entity_type,
        entity_id: entity_id.to_string(),
        child_id: child_id.to_string(),
        action,
        source: SyncSource::Local,
        source_timestamp: chrono::Utc::now(),
        sequence: None,
    };
    // Append via the same path push_events uses (dedup, sequence assignment).
    let _ = self.push_events(std::slice::from_ref(&event))?;
    Ok(())
}
```

- [ ] **Step 2: Update `MockRemoteClient::delete_entity` to emit an event**

In the same file, replace the existing `delete_entity` impl (~lines 120–130) with:

```rust
fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()> {
    self.check_error()?;

    let key = (
        child_id.to_string(),
        entity_type.as_str().to_string(),
        entity_id.to_string(),
    );
    self.entities.lock().unwrap().remove(&key);

    let event = SyncEvent {
        event_id: format!("ev::deleted::{entity_id}"),
        entity_type,
        entity_id: entity_id.to_string(),
        child_id: child_id.to_string(),
        action: SyncAction::Deleted,
        source: SyncSource::Local,
        source_timestamp: chrono::Utc::now(),
        sequence: None,
    };
    let _ = self.push_events(std::slice::from_ref(&event))?;
    Ok(())
}
```

- [ ] **Step 3: Drop `push_events` calls from `sync_thread::push_event`**

In `backend/domain/sync_thread.rs`, replace the entire `push_event` function (~lines 226–275) with:

```rust
/// Push a single local event to the remote. For non-delete events, first
/// requests the entity JSON from the UI thread via message_tx. The server's
/// PUT/DELETE on /entities emits the sync event atomically; this function no
/// longer calls a separate /sync/events endpoint.
fn push_event(
    remote: &Arc<dyn RemoteStorage>,
    event: &SyncEvent,
    messenger: &UiMessenger,
) -> Result<(), String> {
    if event.action == SyncAction::Deleted {
        remote.delete_entity(&event.child_id, event.entity_type.clone(), &event.entity_id)
            .map_err(|e| format!("delete_entity failed: {e}"))?;
        return Ok(());
    }

    // Request entity data from UI thread
    let (response_tx, response_rx) = mpsc::channel();
    messenger.send(SyncMessage::ReadEntityRequest {
        child_id: event.child_id.clone(),
        entity_type: event.entity_type.clone(),
        entity_id: event.entity_id.clone(),
        response_tx,
    }).map_err(|e| format!("failed to request entity from UI: {e}"))?;

    let entity_json = match response_rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Some(json)) => json,
        Ok(None) => {
            // Entity is gone locally (likely deleted after this event was queued).
            // The subsequent Deleted event will handle remote cleanup; discard
            // this event rather than retrying it forever.
            log::warn!(
                "Discarding sync event {} for missing entity {:?}/{}",
                event.event_id, event.entity_type, event.entity_id
            );
            return Ok(());
        }
        Err(e) => return Err(format!("timeout waiting for entity read: {e}")),
    };

    remote.upsert_entity(&event.child_id, event.entity_type.clone(), &event.entity_id, &entity_json)
        .map_err(|e| format!("upsert_entity failed: {e}"))?;

    Ok(())
}
```

The dangling-event-ordering comment is gone because the server now performs entity + event as a single atomic transaction.

- [ ] **Step 4: Build and test**

Run: `cargo build -p allowance-tracker --tests`
Then: `cargo test -p allowance-tracker --lib`
Expected: all tests pass.

Tests of interest:
- `sync_thread::tests::test_thread_pushes_local_event_with_ui_handler` — still passes because mock now emits the event from `upsert_entity`.
- `mock_remote::tests::test_entity_crud` — still passes; entity get/delete behavior is unchanged.
- `mock_remote::tests::test_push_and_get_events` — still passes (still exercises the old `push_events` trait method, which has NOT been removed yet).

If any tests fail, the most likely cause is a test that asserts the exact event count, where the mock used to count only `push_events` calls. In that case adjust the assertion to reflect that `upsert_entity` and `delete_entity` now each emit one event. Read the failing test and reason from the new mock semantics rather than reverting.

- [ ] **Step 5: Commit**

```bash
git add backend/storage/mock_remote.rs backend/domain/sync_thread.rs
git commit -m "refactor(backend): mock emits events on upsert/delete; drop sync_thread push_events calls"
```

---

## Task 7: Local app — remove `enqueue_event`, `push_pending`, `pending_push` from `sync_manager.rs`

**Files:**
- Modify: `backend/domain/sync_manager.rs`

- [ ] **Step 1: Remove the field and methods**

In `backend/domain/sync_manager.rs`:

1. Remove the `pending_push: Vec<SyncEvent>` field from the `SyncEngine` struct.
2. Remove its initialization from `SyncEngine::new` (the `pending_push: Vec::new(),` line).
3. Remove the `enqueue_event` method (currently ~lines 178–181).
4. Remove the `push_pending` method (currently ~lines 183–202).
5. Remove the `pending_push_count` method (currently ~lines 248–250) — it has no callers outside the soon-deleted tests.

- [ ] **Step 2: Remove the two dead tests**

In the `#[cfg(test)] mod tests` block at the bottom of the same file, remove:

- `test_enqueue_and_push` (currently ~lines 385–399)
- `test_push_failure_returns_events` (currently ~lines 401–416)

- [ ] **Step 3: Clean up remaining tests that called `engine.enqueue_event(...)` for setup only**

The poll tests that mixed seed + queue (e.g., `test_poll_returns_all_non_local_events_different_entities`, `test_poll_returns_all_non_local_events_different_entity_types`, `test_last_write_wins_no_conflict_detection`) used `engine.enqueue_event(local_event)` to verify the poll path ignores queued local events. With `enqueue_event` gone, simply drop those `engine.enqueue_event(...)` lines. The assertions about which remote events show up in `poll_child` results remain valid.

For each test that contains an `engine.enqueue_event(...)` line, delete just that line (and the `let local_event = ...` block that feeds it, if `local_event` is now unused). Do not change the assertions.

- [ ] **Step 4: Build and test**

Run: `cargo build -p allowance-tracker --tests`
Then: `cargo test -p allowance-tracker --lib sync_manager`
Expected: all remaining tests pass.

- [ ] **Step 5: Commit**

```bash
git add backend/domain/sync_manager.rs
git commit -m "refactor(backend): remove dead enqueue_event/push_pending from SyncEngine"
```

---

## Task 8: Local app — rewrite `SyncEngine::backfill` to use `upsert_entity` only

**Files:**
- Modify: `backend/domain/sync_manager.rs::backfill` (currently lines 254–360)
- Modify: `backend/domain/sync_manager.rs::test_backfill_pushes_all_entities`

- [ ] **Step 1: Rewrite `backfill`**

Replace the entire `pub fn backfill(...)` method in `sync_manager.rs` with the version below. The change: drop the per-child `events: Vec<SyncEvent>` accumulator, drop the `batch_size`, drop every `self.remote.push_events(&events)?` call. Server emits the sync event from each PUT.

```rust
/// Push all local entities to remote. Reports progress via the channel.
/// Safe to retry: entity upserts are idempotent server-side (identical content
/// short-circuits to no-op; the server emits sync events atomically with each PUT).
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
    let progress_every = 25usize;

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
        pushed += 1;
        children_synced += 1;
        if pushed % progress_every == 0 {
            let _ = progress_tx.send(BackfillProgress::EntitiesPushed { count: pushed, total });
        }

        if let Some(txns) = transactions.get(&child.id) {
            for tx in txns {
                let tx_json = serde_json::to_string(&tx)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize transaction: {}", e))?;
                self.remote.upsert_entity(&child.id, EntityType::Transaction, &tx.id, &tx_json)?;
                pushed += 1;
                transactions_synced += 1;
                if pushed % progress_every == 0 {
                    let _ = progress_tx.send(BackfillProgress::EntitiesPushed { count: pushed, total });
                }
            }
        }

        if let Some(child_goals) = goals.get(&child.id) {
            for goal in child_goals {
                let goal_json = serde_json::to_string(&goal)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize goal: {}", e))?;
                self.remote.upsert_entity(&child.id, EntityType::Goal, &goal.id, &goal_json)?;
                pushed += 1;
                goals_synced += 1;
                if pushed % progress_every == 0 {
                    let _ = progress_tx.send(BackfillProgress::EntitiesPushed { count: pushed, total });
                }
            }
        }

        // Final progress for this child (covers tail under the every-25 threshold).
        let _ = progress_tx.send(BackfillProgress::EntitiesPushed { count: pushed, total });
    }

    let _ = progress_tx.send(BackfillProgress::Completed { total_pushed: pushed });

    Ok(BackfillResult {
        children_synced,
        transactions_synced,
        goals_synced,
    })
}
```

- [ ] **Step 2: Update `test_backfill_pushes_all_entities`**

Open `test_backfill_pushes_all_entities` (currently ~line 517 in `sync_manager.rs`). The test originally asserted that the right number of events arrived on the mock after backfill. With the mock now emitting one event per `upsert_entity`, the count is one event per child + one per transaction + one per goal — same total as before. So the existing event-count assertion should still hold. **Verify by reading the test**: if it counts events expecting "child + transactions + goals" you are fine. If it counts events differently (e.g., expects no event for the child itself), update the expectation to match the new "one event per upserted entity" semantics. Write down what you change and why in the commit message.

If the test calls `mock.push_events(...)` anywhere, migrate to `seed_event` (or just remove — `backfill` no longer pushes events explicitly, so no setup events should be needed).

- [ ] **Step 3: Build and test**

Run: `cargo build -p allowance-tracker --tests`
Then: `cargo test -p allowance-tracker --lib backfill`
Expected: backfill test passes. Run the full crate's tests: `cargo test -p allowance-tracker --lib`.

- [ ] **Step 4: Commit**

```bash
git add backend/domain/sync_manager.rs
git commit -m "refactor(backend): rewrite backfill to drive sync events via upsert_entity"
```

---

## Task 9: Local app — remove `push_events` from trait, impls, and mock tests

**Files:**
- Modify: `backend/storage/remote.rs:4-14`
- Modify: `backend/storage/http_remote.rs:29-51`
- Modify: `backend/storage/mock_remote.rs` (impl + tests)

- [ ] **Step 1: Confirm no remaining production callers**

Run: `grep -rn "\.push_events(" backend/ shared/`
Expected: only the trait definition in `remote.rs`, the impls in `http_remote.rs` and `mock_remote.rs`, and the internal call inside `MockRemoteClient::upsert_entity` / `delete_entity` (added in Task 6). No callers from `sync_thread.rs`, `sync_manager.rs`, or the UI code.

If `grep` finds an unexpected caller, stop and update the plan rather than removing it blindly.

- [ ] **Step 2: Replace mock's internal use of `push_events` with direct list append**

Inside `MockRemoteClient`, the new `upsert_entity` and `delete_entity` impls (Task 6) call `self.push_events(...)`. Refactor to a private helper so the trait method can be removed:

In `mock_remote.rs`, add a private method on `MockRemoteClient`:

```rust
fn append_event_internal(&self, event: SyncEvent) -> u64 {
    let mut event_dedup = self.event_dedup.lock().unwrap();
    let mut stored_events = self.events.lock().unwrap();
    let mut next_seq = self.next_sequence.lock().unwrap();

    if let Some(&existing_seq) = event_dedup.get(&event.event_id) {
        return existing_seq;
    }
    let seq = *next_seq;
    *next_seq += 1;
    let mut new_event = event;
    new_event.sequence = Some(seq);
    let id = new_event.event_id.clone();
    stored_events.push(new_event);
    event_dedup.insert(id, seq);
    seq
}
```

Change `upsert_entity` and `delete_entity` to call `self.append_event_internal(event);` instead of `self.push_events(std::slice::from_ref(&event))?;`. The `?` and the slice ceremony are no longer needed.

Also have `seed_event` delegate to it:

```rust
#[cfg(test)]
pub fn seed_event(&self, event: SyncEvent) -> u64 {
    self.append_event_internal(event)
}
```

`append_event_internal` and `seed_event` together replace what `push_events` did, with `force_error` no longer interfering (mock events emitted from successful upserts shouldn't fail; the entity write was already accepted). If you want the mock to honor `force_error` on emission failure for parity with HTTP-level failure modes, add `self.check_error()?` at the start of `upsert_entity` / `delete_entity` (which it already has) and leave `append_event_internal` infallible. That matches reality: server emits the event atomically with the write, so once the write succeeds the event can't independently fail.

- [ ] **Step 3: Remove `push_events` from the trait**

In `backend/storage/remote.rs`, delete this line:

```rust
fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>>;
```

- [ ] **Step 4: Remove `push_events` impl from `HttpRemoteClient`**

In `backend/storage/http_remote.rs`, delete the entire `fn push_events(...)` impl (lines 29–51), including the `PushRequest` and `PushResponse` inner types.

- [ ] **Step 5: Remove `push_events` impl from `MockRemoteClient`**

In `backend/storage/mock_remote.rs`, delete the entire `fn push_events(...)` impl (~lines 51–77).

- [ ] **Step 6: Remove or rewrite mock tests that exercised `push_events` directly**

In the `#[cfg(test)] mod tests` block at the bottom of `mock_remote.rs`:

- `test_push_and_get_events` (~lines 186–203): rewrite to seed via `seed_event` and read back via `get_events_since`:

```rust
#[test]
fn test_seed_and_get_events() {
    let client = MockRemoteClient::new();
    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx1".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let seq = client.seed_event(event);
    assert_eq!(seq, 1);

    let retrieved = client.get_events_since("child1", 0).unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].sequence, Some(1));
}
```

- `test_deduplication` (~lines 205–222): rewrite to use `seed_event` twice and assert dedup:

```rust
#[test]
fn test_seed_event_dedup() {
    let client = MockRemoteClient::new();
    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx1".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let seq1 = client.seed_event(event.clone());
    let seq2 = client.seed_event(event);
    assert_eq!(seq1, seq2);

    let events = client.get_events_since("child1", 0).unwrap();
    assert_eq!(events.len(), 1);
}
```

- `test_force_error` (~lines 271–282): the line `assert!(client.push_events(&[]).is_err());` must change. Replace with a force-error check on `upsert_entity` (which still uses `check_error`):

```rust
#[test]
fn test_force_error() {
    let client = MockRemoteClient::new();
    client.force_error("Test error");

    assert!(client.upsert_entity("child1", EntityType::Goal, "goal1", "{}").is_err());
    assert!(client.get_entity("child1", EntityType::Goal, "goal1").is_err());
    assert!(client.health_check().is_err());

    client.clear_error();
    assert!(client.health_check().is_ok());
}
```

- [ ] **Step 7: Build and test**

Run: `cargo build -p allowance-tracker --tests`
Then: `cargo test -p allowance-tracker --lib`
Expected: all green.

- [ ] **Step 8: Commit**

```bash
git add backend/storage/remote.rs backend/storage/http_remote.rs backend/storage/mock_remote.rs
git commit -m "refactor(backend): remove RemoteStorage::push_events and HTTP impl"
```

---

## Task 10: Server — remove `POST /sync/events` route, `DynamoStore::push_event`, and api_test

**Files:**
- Modify: `sync-service/src/routes/sync.rs`
- Modify: `sync-service/src/storage/dynamo.rs` (remove `push_event` ~line 60)
- Modify: `sync-service/tests/api_test.rs` (remove `test_push_events_endpoint`)

- [ ] **Step 1: Remove the handler, types, and route registration in `sync.rs`**

In `sync-service/src/routes/sync.rs`:

1. Delete `PushEventsRequest` (lines 12–15) and `PushEventsResponse` (lines 17–20).
2. Delete the `push_events` async function (lines 39–55).
3. In `routes()` (line 115+), remove `.route("/sync/events", post(push_events))` (line 117). The `get(get_events)` route on the same path stays. After removal, the `post` import on line 4 may be unused; if so, drop it from the `use axum::...` line — if `post` is used elsewhere in the file (it is not, based on the file's current content) leave it.

The full updated `routes()`:

```rust
pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new()
        .route("/sync/events", get(get_events))
        .route("/sync/initialize/{child_id}", post(initialize_child))
        .route("/sync/checkpoint/{child_id}", get(get_checkpoint))
        .route("/sync/checkpoint/{child_id}", put(update_checkpoint))
}
```

Note `post` is still needed for `initialize_child`, so keep the import.

- [ ] **Step 2: Remove `DynamoStore::push_event`**

Run: `grep -rn "\.push_event(" sync-service/src/ sync-service/tests/`
Expected: only the now-deleted handler in `sync.rs` referenced it. After Step 1 the grep should find zero remaining callers.

In `sync-service/src/storage/dynamo.rs`, delete `pub async fn push_event(...)` (~line 60). Inspect the surrounding code: if there are private helpers used only by `push_event`, remove them. `allocate_event_sequence` (~line 107) and `event_to_item` (~line 257) are also used by `upsert_entity_with_event` and `delete_entity_with_event` — keep them.

- [ ] **Step 3: Remove `test_push_events_endpoint`**

In `sync-service/tests/api_test.rs`, delete the `test_push_events_endpoint` function (around line 71). Inspect imports at the top of the file; remove any that became unused as a result.

- [ ] **Step 4: Build and run the full sync-service test suite**

Run: `cargo build -p sync-service`
Then: `cargo test -p sync-service`
Expected: clean build, all remaining tests pass.

- [ ] **Step 5: Commit**

```bash
git add sync-service/src/routes/sync.rs sync-service/src/storage/dynamo.rs sync-service/tests/api_test.rs
git commit -m "refactor(sync-service): remove POST /sync/events route and push_event store helper"
```

---

## Final verification

- [ ] **Workspace-wide build:**

Run: `cargo build --workspace`
Expected: clean.

- [ ] **Workspace-wide test:**

Run: `cargo test --workspace`
Expected: all green. (DynamoDB Local must be running for sync-service integration tests to actually execute.)

- [ ] **Manual sanity check (optional but recommended):**

Bring up sync-service against DynamoDB Local, point the local app at it, and exercise:
1. Add a transaction in the local app → confirm it appears via `GET /entities/Transaction/{child_id}` and `GET /sync/events?child_id={child_id}&since=0` shows one event with `source=Local`.
2. Delete that transaction in the local app → confirm `GET /entities/...` returns 404 and a `Deleted` event with `source=Local` is visible.
3. Restart the local app (forces a poll on startup) → confirm the echo events are not re-applied locally (no duplicate entries, no errors in logs).

- [ ] **Update follow-up tracking:**

After the PR merges, remove the "Local app `push_events` removal" bullet and the "POST /internal/sync/events still on the wire" bullet from the `Sync and MCP project status` memory.

---

## Deploy sequencing (informational, not part of the PR)

Per the spec: **deploy local app first, then server.** Shipping server first leaves any old local-app build in the wild making `POST /sync/events` calls that 404 and queue retries indefinitely. Reverse order is safe — a new local app stops calling the route, and the still-deployed-but-unused route is harmless.

After merging, deploy in this order:
1. Build and ship the local-app binary.
2. Verify the deployed local app round-trips entities and events through the new path (use the manual sanity check above).
3. Deploy sync-service.
