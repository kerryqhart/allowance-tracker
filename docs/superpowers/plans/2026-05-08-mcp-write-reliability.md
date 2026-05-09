# MCP Write Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make MCP `add_expense` writes idempotent under claude.ai timeout-retry, and atomically pair every entity write with its sync event so partial-failure orphans cannot occur.

**Architecture:** Server-side sync-service (`allowance-tracker/sync-service/`) gains an "atomic upsert with event" path: every entity PUT auto-emits a sync event in the same DynamoDB transaction, with a content-equality short-circuit for retry no-ops. MCP-side (`zephytop-brain/services/allowance-tracker/`) generates deterministic transaction IDs from input + 1-hour bucket, with a previous-bucket lookback for boundary-crossing retries, and stops pushing sync events explicitly.

**Tech Stack:** Rust, AWS Lambda, DynamoDB (`TransactWriteItems`), `sha2`/`hex` for hashing, `chrono` for time bucketing.

**Spec:** `docs/superpowers/specs/2026-05-08-mcp-write-reliability-design.md`

**Repos:**
- A: `/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker` — sync-service changes
- B: `/Users/kerryhart/Documents/Code/zephytop-brain` — MCP changes

**Deployment order:** Repo A first (server auto-emit must be live before MCP stops pushing events explicitly), then Repo B.

---

## Repo A — sync-service changes

### Task 1: Helper module — content hashing and event_id derivation

**Files:**
- Create: `sync-service/src/storage/event_id.rs`
- Modify: `sync-service/src/storage/mod.rs` (export new module)
- Modify: `sync-service/Cargo.toml` (add sha2, hex)

- [ ] **Step 1: Add deps to `sync-service/Cargo.toml`**

In the `[dependencies]` section, add:
```toml
sha2 = "0.10"
hex = "0.4"
```

- [ ] **Step 2: Write the failing test file**

Create `sync-service/src/storage/event_id.rs`:

```rust
use sha2::{Digest, Sha256};
use shared::sync::SyncAction;

/// Compute the first 8 hex characters of SHA-256 over the given bytes.
/// Used as a stable content fingerprint inside event ids and content checks.
pub fn content_sha8(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    hex::encode(&digest[..4])  // 4 bytes = 8 hex chars
}

/// Derive a deterministic event_id for an entity write.
///
/// `Created` → `ev::created::{entity_id}`
/// `Updated` → `ev::updated::{entity_id}::{content_sha8}`
///
/// `Deleted` is currently produced by the dedicated delete path with a uuid
/// and is not changed by this module.
pub fn event_id_for(action: &SyncAction, entity_id: &str, entity_json: &str) -> String {
    match action {
        SyncAction::Created => format!("ev::created::{entity_id}"),
        SyncAction::Updated => format!("ev::updated::{entity_id}::{}", content_sha8(entity_json)),
        SyncAction::Deleted => format!("ev::deleted::{entity_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_sha8_is_stable() {
        assert_eq!(content_sha8(r#"{"a":1}"#), content_sha8(r#"{"a":1}"#));
    }

    #[test]
    fn content_sha8_distinct_for_different_input() {
        assert_ne!(content_sha8(r#"{"a":1}"#), content_sha8(r#"{"a":2}"#));
    }

    #[test]
    fn content_sha8_is_8_hex_chars() {
        let h = content_sha8("anything");
        assert_eq!(h.len(), 8);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn event_id_created_format() {
        let id = event_id_for(&SyncAction::Created, "tx1", r#"{"foo":1}"#);
        assert_eq!(id, "ev::created::tx1");
    }

    #[test]
    fn event_id_updated_includes_content_hash() {
        let id = event_id_for(&SyncAction::Updated, "tx1", r#"{"foo":1}"#);
        assert!(id.starts_with("ev::updated::tx1::"));
        assert_eq!(id.len(), "ev::updated::tx1::".len() + 8);
    }

    #[test]
    fn event_id_updated_changes_with_content() {
        let a = event_id_for(&SyncAction::Updated, "tx1", r#"{"foo":1}"#);
        let b = event_id_for(&SyncAction::Updated, "tx1", r#"{"foo":2}"#);
        assert_ne!(a, b);
    }
}
```

- [ ] **Step 3: Wire into `sync-service/src/storage/mod.rs`**

Add the line `pub mod event_id;` to the existing module exports in that file (place it next to `pub mod dynamo;` etc.).

- [ ] **Step 4: Run tests, verify all pass**

```bash
cargo test -p sync-service event_id
```
Expected: all 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" add sync-service/Cargo.toml sync-service/src/storage/event_id.rs sync-service/src/storage/mod.rs
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" commit -m "feat(sync-service): add deterministic event_id and content hashing helpers

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 2: Add `upsert_entity_with_event` to DynamoStore

**Files:**
- Modify: `sync-service/src/storage/dynamo.rs:253` (alongside existing `upsert_entity`)

This task adds a new method. We deliberately leave the existing `upsert_entity` intact so the older `POST /sync/events` flow (used by the local app) keeps working until the local-app cleanup follow-up.

- [ ] **Step 1: Read the existing `push_event` to understand sequence allocation**

Open `sync-service/src/storage/dynamo.rs` lines 60–103. The pattern is: `UpdateItem` on metadata table to atomically increment `event_sequence`, get the new value back, then write event row with new sequence. We will reuse this pattern but inside a transactional flow.

- [ ] **Step 2: Add a private sequence allocator helper**

In `sync-service/src/storage/dynamo.rs`, add this method to `impl DynamoStore` (place it near `push_event`):

```rust
/// Atomically allocate the next event sequence number for a child.
/// Wasted on transaction rollback; sequences are monotonic, not contiguous.
async fn allocate_event_sequence(&self, child_id: &str) -> anyhow::Result<u64> {
    let metadata_table = self.config.sync_metadata.clone();
    let update_response = self.client
        .update_item()
        .table_name(&metadata_table)
        .key("child_id", AttributeValue::S(child_id.to_string()))
        .update_expression("SET event_sequence = event_sequence + :inc")
        .expression_attribute_values(":inc", AttributeValue::N("1".to_string()))
        .condition_expression("attribute_exists(event_sequence)")
        .return_values(aws_sdk_dynamodb::types::ReturnValue::AllNew)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to increment sequence: {}", e))?;

    let new_sequence = update_response
        .attributes()
        .and_then(|attrs| attrs.get("event_sequence"))
        .and_then(|attr| attr.as_n().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(|| anyhow::anyhow!("Failed to parse new sequence number"))?;

    Ok(new_sequence)
}
```

- [ ] **Step 3: Add the new `upsert_entity_with_event` method**

In the same file, just below the existing `upsert_entity` method (around line 283), add:

```rust
/// Upsert an entity AND emit a sync event atomically via DynamoDB TransactWriteItems.
/// Idempotent on retry: identical content (string-equal entity_json) is detected
/// before any write and short-circuits to Ok(()) with no event emitted.
pub async fn upsert_entity_with_event(
    &self,
    child_id: &str,
    entity_type: EntityType,
    entity_id: &str,
    entity_json: &str,
    source: SyncSource,
) -> anyhow::Result<()> {
    use aws_sdk_dynamodb::types::{Put, TransactWriteItem};
    use chrono::Utc;
    use crate::storage::event_id::event_id_for;

    // 1. Read prior entity to determine action and short-circuit on identical retry.
    let prior_json = self.get_entity(child_id, entity_type.clone(), entity_id).await?;
    if let Some(ref prior) = prior_json {
        if prior == entity_json {
            // Same content already present — retry no-op.
            return Ok(());
        }
    }

    let action = if prior_json.is_none() {
        SyncAction::Created
    } else {
        SyncAction::Updated
    };

    let event_id = event_id_for(&action, entity_id, entity_json);

    // 2. Allocate sequence (wasted if the transaction below fails — acceptable).
    let new_sequence = self.allocate_event_sequence(child_id).await?;

    let event = SyncEvent {
        event_id,
        entity_type: entity_type.clone(),
        entity_id: entity_id.to_string(),
        child_id: child_id.to_string(),
        action,
        source,
        source_timestamp: Utc::now(),
        sequence: Some(new_sequence),
    };

    // 3. Build entity item.
    let (entity_table, sort_key) = self.entity_table_and_sort_key(&entity_type);

    let mut entity_item = HashMap::from([
        ("child_id".to_string(), AttributeValue::S(child_id.to_string())),
        ("data".to_string(), AttributeValue::S(entity_json.to_string())),
    ]);
    if let Some(sk_name) = sort_key {
        entity_item.insert(sk_name.to_string(), AttributeValue::S(entity_id.to_string()));
    }
    if matches!(entity_type, EntityType::Transaction) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(entity_json) {
            if let Some(date_str) = parsed.get("date").and_then(|v| v.as_str()) {
                entity_item.insert("sort_date".to_string(), AttributeValue::S(date_str.to_string()));
            }
        }
    }

    // 4. Build event item.
    let event_item = self.event_to_item(&event, new_sequence);

    let entity_put = Put::builder()
        .table_name(&entity_table)
        .set_item(Some(entity_item))
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build entity Put: {}", e))?;

    let event_put = Put::builder()
        .table_name(&self.config.sync_events)
        .set_item(Some(event_item))
        .condition_expression("attribute_not_exists(#seq)")
        .expression_attribute_names("#seq", "sequence")
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build event Put: {}", e))?;

    // 5. Atomic write.
    self.client
        .transact_write_items()
        .transact_items(TransactWriteItem::builder().put(entity_put).build())
        .transact_items(TransactWriteItem::builder().put(event_put).build())
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(
            "TransactWriteItems failed for {}/{}: {}",
            entity_type.as_str(), entity_id,
            aws_sdk_dynamodb::error::DisplayErrorContext(&e)
        ))?;

    Ok(())
}
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p sync-service
```
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" add sync-service/src/storage/dynamo.rs
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" commit -m "feat(sync-service): add upsert_entity_with_event for atomic entity+event writes

Uses DynamoDB TransactWriteItems to write the entity and a sync event in a
single transaction. Content-equality short-circuit makes identical retries
a no-op without consuming a sequence number on the second attempt.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 3: Wire `X-Sync-Source` header through `PUT /entities/...` route

**Files:**
- Modify: `sync-service/src/routes/entities.rs:14-36` (upsert_entity handler)

- [ ] **Step 1: Read the current `upsert_entity` handler**

Open `sync-service/src/routes/entities.rs:14-36`. It currently calls `store.upsert_entity(...)`. We replace that call with `store.upsert_entity_with_event(...)`, extracting the optional `X-Sync-Source` header (defaulting to `Remote`).

- [ ] **Step 2: Replace the handler body**

In `sync-service/src/routes/entities.rs`, replace the existing `upsert_entity` async fn (lines 14-36) with:

```rust
// PUT /entities/{entity_type}/{child_id}/{entity_id} - upsert entity and emit event atomically
async fn upsert_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
    headers: axum::http::HeaderMap,
    body: Body,
) -> Result<StatusCode, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let entity_json = String::from_utf8(bytes.to_vec())
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let source = match headers.get("x-sync-source").and_then(|v| v.to_str().ok()) {
        Some("local") => shared::sync::SyncSource::Local,
        _ => shared::sync::SyncSource::Remote,  // default for absent or any other value
    };

    match store.upsert_entity_with_event(&child_id, entity_type, &entity_id, &entity_json, source).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            eprintln!("upsert_entity_with_event failed for {}/{}: {:?}", child_id, entity_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p sync-service
```
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" add sync-service/src/routes/entities.rs
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" commit -m "feat(sync-service): PUT /entities/... auto-emits sync event atomically

Reads optional X-Sync-Source header (defaults to remote) and routes through
upsert_entity_with_event so every entity write produces a paired sync event
in the same DynamoDB transaction.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 4: Integration test against DynamoDB Local

**Files:**
- Create: `sync-service/tests/atomic_upsert.rs` (or add to an existing integration test file if one fits)

This task assumes the existing test infrastructure starts DynamoDB Local and creates the tables. Find the existing integration test file (e.g., `sync-service/tests/`) and follow its setup pattern. If no existing integration tests exist, the test below uses `sync_service::create_local_dynamo_client` (already exists in `lib.rs:20`).

- [ ] **Step 1: Reference the existing test pattern**

The integration tests live in `sync-service/tests/`. The pattern (see `entity_crud_test.rs`) is:

```rust
mod common;
use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use shared::sync::*;
use sync_service::storage::DynamoStore;

async fn setup() -> Option<(DynamoTestContext, DynamoStore)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_config());
    Some((ctx, store))
}
```

Tests follow `let Some((ctx, store)) = setup().await else { return; };` at the top, then `ctx.cleanup().await` at the end. Tests are skipped (not failed) when DynamoDB Local isn't running.

- [ ] **Step 2: Create `sync-service/tests/atomic_upsert_test.rs`**

```rust
mod common;

use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use shared::sync::*;
use sync_service::storage::DynamoStore;

async fn setup() -> Option<(DynamoTestContext, DynamoStore)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_config());
    Some((ctx, store))
}

#[tokio::test]
async fn two_identical_puts_produce_one_entity_and_one_event() {
    let Some((ctx, store)) = setup().await else { return; };
    let child_id = "atomic-child-1";
    store.initialize_child_metadata(child_id).await.unwrap();

    let tx_json = r#"{"id":"tx1","child_id":"atomic-child-1","amount":-5.0,"date":"2026-05-08T00:00:00+00:00","description":"test","balance":95.0,"transaction_type":"Expense"}"#;

    store.upsert_entity_with_event(
        child_id, EntityType::Transaction, "tx1", tx_json, SyncSource::Remote,
    ).await.unwrap();

    // Identical retry — should be a no-op.
    store.upsert_entity_with_event(
        child_id, EntityType::Transaction, "tx1", tx_json, SyncSource::Remote,
    ).await.unwrap();

    let events = store.get_events_since(child_id, 0).await.unwrap();
    assert_eq!(events.len(), 1, "expected exactly one event after identical retry");
    assert_eq!(events[0].action, SyncAction::Created);
    assert_eq!(events[0].event_id, "ev::created::tx1");

    ctx.cleanup().await;
}

#[tokio::test]
async fn put_with_new_content_emits_updated_event() {
    let Some((ctx, store)) = setup().await else { return; };
    let child_id = "atomic-child-2";
    store.initialize_child_metadata(child_id).await.unwrap();

    let tx_v1 = r#"{"id":"tx2","child_id":"atomic-child-2","amount":-5.0,"date":"2026-05-08T00:00:00+00:00","description":"v1","balance":95.0,"transaction_type":"Expense"}"#;
    let tx_v2 = r#"{"id":"tx2","child_id":"atomic-child-2","amount":-5.0,"date":"2026-05-08T00:00:00+00:00","description":"v2","balance":95.0,"transaction_type":"Expense"}"#;

    store.upsert_entity_with_event(child_id, EntityType::Transaction, "tx2", tx_v1, SyncSource::Remote).await.unwrap();
    store.upsert_entity_with_event(child_id, EntityType::Transaction, "tx2", tx_v2, SyncSource::Remote).await.unwrap();

    let events = store.get_events_since(child_id, 0).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].action, SyncAction::Created);
    assert_eq!(events[1].action, SyncAction::Updated);
    assert!(events[1].event_id.starts_with("ev::updated::tx2::"));

    ctx.cleanup().await;
}

#[tokio::test]
async fn entity_data_matches_request_body_after_write() {
    let Some((ctx, store)) = setup().await else { return; };
    let child_id = "atomic-child-3";
    store.initialize_child_metadata(child_id).await.unwrap();

    let tx_json = r#"{"id":"tx3","child_id":"atomic-child-3","description":"hello","amount":-1.0,"date":"2026-05-08T00:00:00+00:00","balance":99.0,"transaction_type":"Expense"}"#;
    store.upsert_entity_with_event(child_id, EntityType::Transaction, "tx3", tx_json, SyncSource::Remote).await.unwrap();

    let read_back = store.get_entity(child_id, EntityType::Transaction, "tx3").await.unwrap();
    assert_eq!(read_back, Some(tx_json.to_string()));

    ctx.cleanup().await;
}
```

- [ ] **Step 3: Start DynamoDB Local (if not already running) and run tests**

If the project has a script to start DynamoDB Local, use it. Otherwise:

```bash
docker run -d --rm -p 8000:8000 --name dynamodb-local amazon/dynamodb-local -jar DynamoDBLocal.jar -inMemory
```

Then:

```bash
cargo test -p sync-service --test atomic_upsert_test
```

Expected: 3 tests pass. (If DynamoDB Local isn't running, tests print "SKIPPING" and return early — that means the harness is not actually validating the change. Make sure DynamoDB Local IS running before declaring this task done.)

- [ ] **Step 4: Commit**

```bash
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" add sync-service/tests/atomic_upsert.rs
git -C "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker" commit -m "test(sync-service): integration tests for atomic upsert+event behavior

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 5: Deploy sync-service

This is a manual deployment step, not a code change. It must complete before Repo B work begins, because the new MCP code stops pushing events explicitly and depends on the server's auto-emit being live.

- [ ] **Step 1: Build the Lambda artifact**

```bash
cd "/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker/infrastructure"
sam build
```
Expected: build succeeds.

- [ ] **Step 2: Deploy**

```bash
sam deploy
```
Expected: deployment succeeds. Note the API endpoint URL (should be the existing `https://i99kq799kd.execute-api.us-east-2.amazonaws.com`).

- [ ] **Step 3: Smoke test against deployed service**

```bash
curl -X PUT \
  -H "Content-Type: application/json" \
  -H "X-Sync-Source: remote" \
  -d '{"id":"transaction::expense::smoke-test-1","child_id":"test-entity","amount":-0.01,"date":"2026-05-08T00:00:00+00:00","description":"smoke test","balance":0.0,"transaction_type":"Expense"}' \
  "https://i99kq799kd.execute-api.us-east-2.amazonaws.com/internal/entities/transaction/test-entity/transaction::expense::smoke-test-1"
```
Expected: HTTP 200.

```bash
curl -s "https://i99kq799kd.execute-api.us-east-2.amazonaws.com/internal/sync/events?child_id=test-entity&since=0" | python3 -m json.tool
```
Expected: response includes an event with `event_id` = `ev::created::transaction::expense::smoke-test-1` and `action` = `Created`.

- [ ] **Step 4: Clean up smoke test data**

```bash
curl -X DELETE "https://i99kq799kd.execute-api.us-east-2.amazonaws.com/internal/entities/transaction/test-entity/transaction::expense::smoke-test-1"
```
Expected: HTTP 204.

(The smoke event remains in the events table — that's fine, it's harmless and won't hit a real child.)

---

## Repo B — MCP changes (zephytop-brain)

### Task 6: Add deps and helper module for deterministic IDs

**Files:**
- Modify: `services/allowance-tracker/Cargo.toml`
- Create: `services/allowance-tracker/src/idempotency.rs`
- Modify: `services/allowance-tracker/src/main.rs` or `lib.rs` (mod declaration)

- [ ] **Step 1: Add deps to `services/allowance-tracker/Cargo.toml`**

```toml
sha2 = "0.10"
hex = "0.4"
```

- [ ] **Step 2: Create the helper module with tests**

Create `services/allowance-tracker/src/idempotency.rs`:

```rust
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// Round a timestamp down to the start of its 1-hour bucket (UTC).
pub fn hour_bucket(now: DateTime<Utc>) -> DateTime<Utc> {
    let secs = (now.timestamp() / 3600) * 3600;
    DateTime::<Utc>::from_timestamp(secs, 0).expect("valid epoch seconds")
}

/// Compute a deterministic id stable across retries with identical inputs and bucket.
///
/// Format: `{prefix}::{YYYYMMDDTHH}::{16-hex-chars}`
pub fn deterministic_id(
    prefix: &str,
    child_id: &str,
    amount: f64,
    description: &str,
    bucket: DateTime<Utc>,
) -> String {
    let bucket_str = bucket.format("%Y%m%dT%H").to_string();
    // Convert amount to integer cents for stable f64 hashing (dodge NaN, rounding).
    let amount_cents = (amount * 100.0).round() as i64;
    let mut h = Sha256::new();
    h.update(child_id.as_bytes());
    h.update(amount_cents.to_be_bytes());
    h.update(description.trim().as_bytes());
    h.update(bucket_str.as_bytes());
    let short = &hex::encode(h.finalize())[..16];
    format!("{prefix}::{bucket_str}::{short}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, 0).unwrap()
    }

    #[test]
    fn hour_bucket_rounds_down() {
        let t = ts(2026, 5, 8, 14, 37);
        let b = hour_bucket(t);
        assert_eq!(b, ts(2026, 5, 8, 14, 0));
    }

    #[test]
    fn hour_bucket_already_at_boundary() {
        let t = ts(2026, 5, 8, 14, 0);
        assert_eq!(hour_bucket(t), t);
    }

    #[test]
    fn deterministic_id_stable_for_same_inputs() {
        let bucket = ts(2026, 5, 8, 3, 0);
        let a = deterministic_id("transaction::expense", "keiko_hart", 28.0, "Yoshi stuffie", bucket);
        let b = deterministic_id("transaction::expense", "keiko_hart", 28.0, "Yoshi stuffie", bucket);
        assert_eq!(a, b);
    }

    #[test]
    fn deterministic_id_format() {
        let bucket = ts(2026, 5, 8, 3, 0);
        let id = deterministic_id("transaction::expense", "keiko_hart", 28.0, "Yoshi stuffie", bucket);
        assert!(id.starts_with("transaction::expense::20260508T03::"));
        let suffix = id.strip_prefix("transaction::expense::20260508T03::").unwrap();
        assert_eq!(suffix.len(), 16);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn deterministic_id_differs_when_amount_differs() {
        let bucket = ts(2026, 5, 8, 3, 0);
        let a = deterministic_id("transaction::expense", "k", 1.0, "x", bucket);
        let b = deterministic_id("transaction::expense", "k", 2.0, "x", bucket);
        assert_ne!(a, b);
    }

    #[test]
    fn deterministic_id_differs_when_description_differs() {
        let bucket = ts(2026, 5, 8, 3, 0);
        let a = deterministic_id("transaction::expense", "k", 1.0, "x", bucket);
        let b = deterministic_id("transaction::expense", "k", 1.0, "y", bucket);
        assert_ne!(a, b);
    }

    #[test]
    fn deterministic_id_differs_when_child_differs() {
        let bucket = ts(2026, 5, 8, 3, 0);
        let a = deterministic_id("transaction::expense", "k1", 1.0, "x", bucket);
        let b = deterministic_id("transaction::expense", "k2", 1.0, "x", bucket);
        assert_ne!(a, b);
    }

    #[test]
    fn deterministic_id_differs_across_buckets() {
        let a = deterministic_id("transaction::expense", "k", 1.0, "x", ts(2026, 5, 8, 3, 0));
        let b = deterministic_id("transaction::expense", "k", 1.0, "x", ts(2026, 5, 8, 4, 0));
        assert_ne!(a, b);
    }

    #[test]
    fn deterministic_id_trims_description_whitespace() {
        let bucket = ts(2026, 5, 8, 3, 0);
        let a = deterministic_id("transaction::expense", "k", 1.0, "  hello  ", bucket);
        let b = deterministic_id("transaction::expense", "k", 1.0, "hello", bucket);
        assert_eq!(a, b);
    }

    #[test]
    fn deterministic_id_handles_fractional_cents() {
        // 1.234 and 1.235 round to 123 and 124 cents, distinct.
        let bucket = ts(2026, 5, 8, 3, 0);
        let a = deterministic_id("transaction::expense", "k", 1.234, "x", bucket);
        let b = deterministic_id("transaction::expense", "k", 1.235, "x", bucket);
        assert_ne!(a, b);
    }
}
```

- [ ] **Step 3: Declare the new module in `main.rs`**

`services/allowance-tracker/src/main.rs` already contains:

```rust
mod mcp;
mod sync_client;
```

Add `mod idempotency;` after those lines.

- [ ] **Step 4: Run tests**

```bash
cargo test --manifest-path /Users/kerryhart/Documents/Code/zephytop-brain/services/allowance-tracker/Cargo.toml idempotency
```
Expected: all 9 tests pass.

- [ ] **Step 5: Commit**

```bash
git -C /Users/kerryhart/Documents/Code/zephytop-brain add services/allowance-tracker/Cargo.toml services/allowance-tracker/src/idempotency.rs services/allowance-tracker/src/main.rs
git -C /Users/kerryhart/Documents/Code/zephytop-brain commit -m "feat(mcp): add deterministic_id and hour_bucket helpers

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 7: Refactor `tool_add_expense` to use deterministic IDs and remove explicit event push

**Files:**
- Modify: `services/allowance-tracker/src/mcp.rs:227-290`

- [ ] **Step 1: Read the current `tool_add_expense` again**

The current function (lines 227-290) generates `transaction_id` from `now()`, builds entity JSON, calls `client.put_entity`, then builds a separate sync event JSON and calls `client.push_sync_events`. We replace it with a lookback-based deterministic flow.

- [ ] **Step 2: Replace `tool_add_expense` with the new implementation**

In `services/allowance-tracker/src/mcp.rs`, replace the entire `async fn tool_add_expense(...) -> Result<String, String> { ... }` (lines 227-290) with:

```rust
async fn tool_add_expense(
    client: &SyncClient,
    child_id: &str,
    amount: f64,
    description: &str,
) -> Result<String, String> {
    use crate::idempotency::{deterministic_id, hour_bucket};
    use chrono::Duration;

    if amount <= 0.0 {
        return Err("Amount must be a positive number".to_string());
    }

    let now = chrono::Utc::now();
    let current_bucket = hour_bucket(now);
    let prev_bucket = current_bucket - Duration::hours(1);

    let current_id = deterministic_id("transaction::expense", child_id, amount, description, current_bucket);
    let prev_id = deterministic_id("transaction::expense", child_id, amount, description, prev_bucket);

    // Lookback: if either bucket already has this entity, this is a retry.
    for id in [&current_id, &prev_id] {
        if let Some(existing) = client.get_entity("transaction", child_id, id).await? {
            return response_from_existing(&existing, description);
        }
    }

    // First-time path: read current balance, compute new, write entity.
    let current_balance = match client.get_balance(child_id).await? {
        Some((bal, _)) => bal,
        None => 0.0,
    };
    let new_balance = current_balance - amount;
    let negative_amount = -amount;

    let transaction = json!({
        "id": current_id,
        "child_id": child_id,
        "date": now.to_rfc3339(),
        "description": description,
        "amount": negative_amount,
        "balance": new_balance,
        "transaction_type": "Expense",
    });

    let tx_json = serde_json::to_string(&transaction)
        .map_err(|e| format!("Failed to serialize transaction: {e}"))?;

    // Server now emits the sync event atomically with the entity write.
    client.put_entity("transaction", child_id, &current_id, &tx_json).await?;

    let result = json!({
        "description": description,
        "amount": negative_amount,
        "new_balance": new_balance,
    });
    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}

/// Build the success response from an entity that was found via lookback.
/// On retry the existing row already has the correct description/amount/balance.
fn response_from_existing(existing: &serde_json::Value, _expected_description: &str) -> Result<String, String> {
    let amount = existing["amount"].as_f64()
        .ok_or("existing entity missing amount")?;
    let balance = existing["balance"].as_f64()
        .ok_or("existing entity missing balance")?;
    let description = existing["description"].as_str()
        .ok_or("existing entity missing description")?
        .to_string();

    let result = json!({
        "description": description,
        "amount": amount,
        "new_balance": balance,
    });
    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check --manifest-path /Users/kerryhart/Documents/Code/zephytop-brain/services/allowance-tracker/Cargo.toml
```
Expected: no errors. Unused-import warnings for `uuid::Uuid` (since we no longer need to mint event_ids client-side) are expected — fix in next step.

- [ ] **Step 4: Remove unused imports**

If `cargo check` flags `uuid::Uuid` or any other now-unused import in `mcp.rs`, remove it. Re-run `cargo check`.

- [ ] **Step 5: Run unit tests**

```bash
cargo test --manifest-path /Users/kerryhart/Documents/Code/zephytop-brain/services/allowance-tracker/Cargo.toml
```
Expected: all existing tests still pass; idempotency tests pass.

- [ ] **Step 6: Commit**

```bash
git -C /Users/kerryhart/Documents/Code/zephytop-brain add services/allowance-tracker/src/mcp.rs
git -C /Users/kerryhart/Documents/Code/zephytop-brain commit -m "feat(mcp): deterministic transaction_id with bucket lookback in add_expense

Replaces timestamp-based id generation with content-derived deterministic id
plus 1-hour bucket and previous-bucket lookback. Drops explicit
push_sync_events call now that the server auto-emits events on entity PUT.

Eliminates duplicate transactions from claude.ai timeout-retry.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

### Task 8: Deploy MCP

- [ ] **Step 1: Build and deploy via SAM**

The infrastructure config is at `/Users/kerryhart/Documents/Code/zephytop-brain/infrastructure/`. Build from there:

```bash
cd /Users/kerryhart/Documents/Code/zephytop-brain/infrastructure
sam build
sam deploy
```

Check `/Users/kerryhart/Documents/Code/zephytop-brain/scripts/` first for a wrapping deploy script — if one exists, prefer it (it likely sets the right stack/region).

- [ ] **Step 2: Verify deployment**

Test via MCP from claude.ai: ask Claude to add a small expense (e.g., `$0.01 "MCP idempotency smoke test"` for `test-entity`). Then immediately ask again with the same prompt to provoke a possible retry, OR just have claude do it twice to simulate.

- [ ] **Step 3: Verify no duplicates**

```bash
curl -s "https://i99kq799kd.execute-api.us-east-2.amazonaws.com/internal/entities/transaction/test-entity?sort=desc&limit=5"
```
Expected: at most one row with description "MCP idempotency smoke test" within the current hour bucket.

- [ ] **Step 4: Clean up smoke test data**

```bash
curl -X DELETE "https://i99kq799kd.execute-api.us-east-2.amazonaws.com/internal/entities/transaction/test-entity/{the_id_from_step_3}"
```

---

## Self-review checks

Before declaring done:

- [ ] Spec file `2026-05-08-mcp-write-reliability-design.md` references match this plan's task numbering.
- [ ] No `cargo check` warnings for unused imports in either repo.
- [ ] `cargo test -p sync-service` passes with no skips.
- [ ] `cargo test --manifest-path /Users/kerryhart/Documents/Code/zephytop-brain/services/allowance-tracker/Cargo.toml` passes.
- [ ] Sync-service deployed and responds 200 to PUT with `X-Sync-Source: remote` header.
- [ ] MCP deployed; an `add_expense` invocation produces exactly one transaction and one event.
- [ ] A two-minute-apart retry of the same `add_expense` does NOT produce a duplicate (verified via list endpoint).

## Out of scope (already specced; separate follow-ups)

- **Local app `push_events` removal:** `backend/domain/sync_thread.rs:240,271` still calls `remote.push_events(...)`. After this plan ships, that code produces redundant events that get harmlessly deduped on the local-app side via the existing engine's idempotent `ApplyRemoteEntity`. Cleanup is its own plan.
- **Balance recompute on sync (b):** denormalized `balance` field on transaction rows still produces incorrect aggregates after divergent merges. Separate spec planned.
- **`source` field investigation:** verify no consumer reads `event.source` before deciding whether to keep or drop. Tracked in spec's Open Questions.
