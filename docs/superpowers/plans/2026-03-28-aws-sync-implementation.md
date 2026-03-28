# AWS Sync Feature Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add bidirectional sync between the local git-backed allowance tracker and a remote DynamoDB backend, with conflict detection and resolution UI.

**Architecture:** Event-sourced sync with per-child sequence ordering. Three layers: sync-service (axum + DynamoDB), RemoteStorage trait (with HTTP, in-process, and mock impls), and SyncManager (background thread + conflict UI). All tested against DynamoDB Local.

**Tech Stack:** Rust, axum, aws-sdk-dynamodb, tokio (sync-service only), reqwest (blocking), serde/serde_json, std::thread + mpsc (local app)

**Spec:** `docs/superpowers/specs/2026-03-28-aws-sync-design.md`

---

## Phase 1: Shared Sync Types

Shared types used by both the local app and sync-service. These go in the `shared` crate so both workspace members can depend on them.

### Task 1: Add sync types to shared crate

**Files:**
- Create: `shared/src/sync.rs`
- Modify: `shared/src/lib.rs` (add `pub mod sync;`)
- Modify: `shared/Cargo.toml` (add `uuid` dependency)

- [ ] **Step 1: Add uuid dependency to shared/Cargo.toml**

Add under `[dependencies]`:
```toml
uuid = { version = "1.0", features = ["v4", "serde"] }
```

- [ ] **Step 2: Write the sync types module**

Create `shared/src/sync.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntityType {
    Transaction,
    Goal,
    Child, // Includes allowance config (1:1)
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Transaction => "transaction",
            EntityType::Goal => "goal",
            EntityType::Child => "child",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "transaction" => Ok(EntityType::Transaction),
            "goal" => Ok(EntityType::Goal),
            "child" => Ok(EntityType::Child),
            _ => Err(format!("Unknown entity type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SyncAction {
    Created,
    Updated,
    Deleted,
}

impl SyncAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncAction::Created => "created",
            SyncAction::Updated => "updated",
            SyncAction::Deleted => "deleted",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "created" => Ok(SyncAction::Created),
            "updated" => Ok(SyncAction::Updated),
            "deleted" => Ok(SyncAction::Deleted),
            _ => Err(format!("Unknown sync action: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SyncSource {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncEvent {
    pub event_id: String,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub child_id: String,
    pub action: SyncAction,
    pub source: SyncSource,
    pub source_timestamp: DateTime<Utc>,
    /// Sequence number assigned by the remote service. None until pushed.
    pub sequence: Option<u64>,
}

impl SyncEvent {
    pub fn new(
        entity_type: EntityType,
        entity_id: String,
        child_id: String,
        action: SyncAction,
        source: SyncSource,
    ) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            entity_type,
            entity_id,
            child_id,
            action,
            source,
            source_timestamp: Utc::now(),
            sequence: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    pub child_id: String,
    pub event_sequence: u64,
    pub local_watermark: u64,
    pub remote_watermark: u64,
}

impl SyncCheckpoint {
    pub fn new(child_id: String) -> Self {
        Self {
            child_id,
            event_sequence: 0,
            local_watermark: 0,
            remote_watermark: 0,
        }
    }

    /// Events with sequence <= this value are eligible for TTL cleanup.
    pub fn min_watermark(&self) -> u64 {
        self.local_watermark.min(self.remote_watermark)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub id: String,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub child_id: String,
    pub local_event: SyncEvent,
    pub remote_event: SyncEvent,
    pub status: ConflictStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConflictStatus {
    Pending,
    ResolvedKeepLocal,
    ResolvedKeepRemote,
    ResolvedMerged,
}
```

- [ ] **Step 3: Declare the module in shared/src/lib.rs**

Add to the top of `shared/src/lib.rs`:
```rust
pub mod sync;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p shared`
Expected: compiles with no errors

- [ ] **Step 5: Write unit tests for sync types**

Add to the bottom of `shared/src/sync.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_type_round_trip() {
        for et in [EntityType::Transaction, EntityType::Goal, EntityType::Child] {
            let s = et.as_str();
            let parsed = EntityType::from_str(s).unwrap();
            assert_eq!(et, parsed);
        }
    }

    #[test]
    fn test_sync_action_round_trip() {
        for action in [SyncAction::Created, SyncAction::Updated, SyncAction::Deleted] {
            let s = action.as_str();
            let parsed = SyncAction::from_str(s).unwrap();
            assert_eq!(action, parsed);
        }
    }

    #[test]
    fn test_sync_event_new_generates_unique_ids() {
        let e1 = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        let e2 = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        assert_ne!(e1.event_id, e2.event_id);
        assert!(e1.sequence.is_none());
    }

    #[test]
    fn test_checkpoint_min_watermark() {
        let mut cp = SyncCheckpoint::new("child1".to_string());
        cp.local_watermark = 10;
        cp.remote_watermark = 5;
        assert_eq!(cp.min_watermark(), 5);

        cp.local_watermark = 3;
        assert_eq!(cp.min_watermark(), 3);
    }

    #[test]
    fn test_entity_type_from_str_invalid() {
        assert!(EntityType::from_str("invalid").is_err());
    }

    #[test]
    fn test_sync_action_from_str_invalid() {
        assert!(SyncAction::from_str("invalid").is_err());
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p shared`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add shared/Cargo.toml shared/src/sync.rs shared/src/lib.rs
git commit -m "feat: add shared sync types (SyncEvent, SyncCheckpoint, SyncConflict)"
```

---

## Phase 2: Sync-Service Crate

A standalone REST microservice backed by DynamoDB. Tested against DynamoDB Local.

### Task 2: Scaffold sync-service crate

**Files:**
- Create: `sync-service/Cargo.toml`
- Create: `sync-service/src/main.rs`
- Create: `sync-service/src/lib.rs`
- Modify: `Cargo.toml` (add workspace member)

- [ ] **Step 1: Create Cargo.toml for sync-service**

Create `sync-service/Cargo.toml`:

```toml
[package]
name = "sync-service"
version = "0.1.0"
edition = "2021"

[dependencies]
# Shared types
shared = { path = "../shared" }

# Web framework
axum = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }

# AWS
aws-config = "1"
aws-sdk-dynamodb = "1"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Date/time
chrono = { version = "0.4", features = ["serde"] }

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Logging
log = "0.4"
env_logger = "0.11"

# Unique IDs
uuid = { version = "1.0", features = ["v4"] }

[dev-dependencies]
reqwest = { version = "0.12", features = ["json", "blocking"] }
tempfile = "3.0"
```

- [ ] **Step 2: Create stub main.rs**

Create `sync-service/src/main.rs`:

```rust
use sync_service::create_app;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let app = create_app().await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3030").await?;
    log::info!("sync-service listening on 0.0.0.0:3030");
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 3: Create stub lib.rs**

Create `sync-service/src/lib.rs`:

```rust
pub mod storage;
pub mod routes;

use axum::Router;

pub async fn create_app() -> anyhow::Result<Router> {
    let dynamo_client = create_dynamo_client().await?;
    let store = storage::DynamoStore::new(dynamo_client, "".to_string());
    let app = routes::build_router(store);
    Ok(app)
}

pub async fn create_dynamo_client() -> anyhow::Result<aws_sdk_dynamodb::Client> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    Ok(aws_sdk_dynamodb::Client::new(&config))
}

/// Create a DynamoDB client pointing at DynamoDB Local for testing.
pub async fn create_local_dynamo_client(port: u16) -> anyhow::Result<aws_sdk_dynamodb::Client> {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(format!("http://localhost:{}", port))
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_dynamodb::config::Credentials::new(
            "fakeAccessKeyId",
            "fakeSecretAccessKey",
            None,
            None,
            "test",
        ))
        .load()
        .await;
    Ok(aws_sdk_dynamodb::Client::new(&config))
}
```

- [ ] **Step 4: Create empty module files**

Create `sync-service/src/storage/mod.rs`:
```rust
mod dynamo;
pub use dynamo::DynamoStore;
```

Create `sync-service/src/storage/dynamo.rs`:
```rust
use aws_sdk_dynamodb::Client;

pub struct DynamoStore {
    client: Client,
    table_prefix: String,
}

impl DynamoStore {
    pub fn new(client: Client, table_prefix: String) -> Self {
        Self { client, table_prefix }
    }

    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }
}
```

Create `sync-service/src/routes/mod.rs`:
```rust
mod health;

use axum::Router;
use std::sync::Arc;
use crate::storage::DynamoStore;

pub fn build_router(store: DynamoStore) -> Router {
    let store = Arc::new(store);
    Router::new()
        .merge(health::routes())
}
```

Create `sync-service/src/routes/health.rs`:
```rust
use axum::{Router, routing::get};

async fn health_check() -> &'static str {
    "ok"
}

pub fn routes() -> Router {
    Router::new().route("/health", get(health_check))
}
```

- [ ] **Step 5: Add sync-service to workspace**

In the root `Cargo.toml`, change the `members` list:

```toml
members = [
    "shared",
    "egui-frontend",
    "sync-service",
]
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p sync-service`
Expected: compiles with no errors

- [ ] **Step 7: Commit**

```bash
git add sync-service/ Cargo.toml
git commit -m "feat: scaffold sync-service crate with axum + DynamoDB"
```

### Task 3: DynamoDB table creation and DynamoStore core

**Files:**
- Modify: `sync-service/src/storage/dynamo.rs`
- Create: `sync-service/src/storage/table_definitions.rs`
- Modify: `sync-service/src/storage/mod.rs`

- [ ] **Step 1: Write the table definition module**

Create `sync-service/src/storage/table_definitions.rs`:

```rust
use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
    ProvisionedThroughput,
};
use aws_sdk_dynamodb::Client;

/// Table schemas for the sync service.
/// Each function creates one table if it doesn't already exist.

pub async fn create_all_tables(client: &Client, prefix: &str) -> anyhow::Result<()> {
    create_children_table(client, prefix).await?;
    create_transactions_table(client, prefix).await?;
    create_goals_table(client, prefix).await?;
    create_sync_events_table(client, prefix).await?;
    create_sync_metadata_table(client, prefix).await?;
    Ok(())
}

async fn create_table_if_not_exists(
    client: &Client,
    table_name: &str,
    key_schema: Vec<KeySchemaElement>,
    attribute_definitions: Vec<AttributeDefinition>,
) -> anyhow::Result<()> {
    match client
        .create_table()
        .table_name(table_name)
        .set_key_schema(Some(key_schema))
        .set_attribute_definitions(Some(attribute_definitions))
        .provisioned_throughput(
            ProvisionedThroughput::builder()
                .read_capacity_units(5)
                .write_capacity_units(5)
                .build()?,
        )
        .send()
        .await
    {
        Ok(_) => {
            log::info!("Created table: {}", table_name);
            Ok(())
        }
        Err(e) => {
            let service_error = e.into_service_error();
            if service_error.is_resource_in_use_exception() {
                log::debug!("Table already exists: {}", table_name);
                Ok(())
            } else {
                Err(anyhow::anyhow!("Failed to create table {}: {}", table_name, service_error))
            }
        }
    }
}

/// children table: PK = child_id
async fn create_children_table(client: &Client, prefix: &str) -> anyhow::Result<()> {
    create_table_if_not_exists(
        client,
        &format!("{}children", prefix),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(KeyType::Hash)
                .build()?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()?,
        ],
    ).await
}

/// transactions table: PK = child_id, SK = transaction_id
async fn create_transactions_table(client: &Client, prefix: &str) -> anyhow::Result<()> {
    create_table_if_not_exists(
        client,
        &format!("{}transactions", prefix),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(KeyType::Hash)
                .build()?,
            KeySchemaElement::builder()
                .attribute_name("transaction_id")
                .key_type(KeyType::Range)
                .build()?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()?,
            AttributeDefinition::builder()
                .attribute_name("transaction_id")
                .attribute_type(ScalarAttributeType::S)
                .build()?,
        ],
    ).await
}

/// goals table: PK = child_id, SK = goal_id
async fn create_goals_table(client: &Client, prefix: &str) -> anyhow::Result<()> {
    create_table_if_not_exists(
        client,
        &format!("{}goals", prefix),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(KeyType::Hash)
                .build()?,
            KeySchemaElement::builder()
                .attribute_name("goal_id")
                .key_type(KeyType::Range)
                .build()?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()?,
            AttributeDefinition::builder()
                .attribute_name("goal_id")
                .attribute_type(ScalarAttributeType::S)
                .build()?,
        ],
    ).await
}

/// sync_events table: PK = child_id, SK = sequence (Number)
async fn create_sync_events_table(client: &Client, prefix: &str) -> anyhow::Result<()> {
    create_table_if_not_exists(
        client,
        &format!("{}sync_events", prefix),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(KeyType::Hash)
                .build()?,
            KeySchemaElement::builder()
                .attribute_name("sequence")
                .key_type(KeyType::Range)
                .build()?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()?,
            AttributeDefinition::builder()
                .attribute_name("sequence")
                .attribute_type(ScalarAttributeType::N)
                .build()?,
        ],
    ).await
}

/// sync_metadata table: PK = child_id
async fn create_sync_metadata_table(client: &Client, prefix: &str) -> anyhow::Result<()> {
    create_table_if_not_exists(
        client,
        &format!("{}sync_metadata", prefix),
        vec![
            KeySchemaElement::builder()
                .attribute_name("child_id")
                .key_type(KeyType::Hash)
                .build()?,
        ],
        vec![
            AttributeDefinition::builder()
                .attribute_name("child_id")
                .attribute_type(ScalarAttributeType::S)
                .build()?,
        ],
    ).await
}

/// Delete all tables with the given prefix. Used for test cleanup.
pub async fn delete_all_tables(client: &Client, prefix: &str) -> anyhow::Result<()> {
    for table in ["children", "transactions", "goals", "sync_events", "sync_metadata"] {
        let table_name = format!("{}{}", prefix, table);
        match client.delete_table().table_name(&table_name).send().await {
            Ok(_) => log::debug!("Deleted table: {}", table_name),
            Err(_) => log::debug!("Table not found for deletion: {}", table_name),
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Update storage/mod.rs to include table_definitions**

```rust
mod dynamo;
pub mod table_definitions;
pub use dynamo::DynamoStore;
```

- [ ] **Step 3: Add create_tables method to DynamoStore**

Add to `sync-service/src/storage/dynamo.rs`:

```rust
use aws_sdk_dynamodb::Client;
use super::table_definitions;

pub struct DynamoStore {
    client: Client,
    table_prefix: String,
}

impl DynamoStore {
    pub fn new(client: Client, table_prefix: String) -> Self {
        Self { client, table_prefix }
    }

    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn table_prefix(&self) -> &str {
        &self.table_prefix
    }

    pub async fn create_tables(&self) -> anyhow::Result<()> {
        table_definitions::create_all_tables(&self.client, &self.table_prefix).await
    }

    pub async fn delete_tables(&self) -> anyhow::Result<()> {
        table_definitions::delete_all_tables(&self.client, &self.table_prefix).await
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p sync-service`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add sync-service/src/storage/
git commit -m "feat: add DynamoDB table definitions for sync-service"
```

### Task 4: DynamoDB test infrastructure

**Files:**
- Create: `sync-service/tests/common/mod.rs`
- Create: `sync-service/tests/common/dynamo_test_context.rs`

These provide the `DynamoTestContext` used by all integration tests.

- [ ] **Step 1: Write the test context**

Create `sync-service/tests/common/mod.rs`:
```rust
pub mod dynamo_test_context;
pub use dynamo_test_context::DynamoTestContext;
```

Create `sync-service/tests/common/dynamo_test_context.rs`:

```rust
use aws_sdk_dynamodb::Client;
use sync_service::storage::table_definitions;
use uuid::Uuid;

/// Test context that creates isolated DynamoDB tables with a unique prefix.
/// Tables are cleaned up on drop.
pub struct DynamoTestContext {
    pub client: Client,
    pub table_prefix: String,
}

impl DynamoTestContext {
    /// Create a new test context. Requires DynamoDB Local running on the given port.
    /// Creates all tables with a unique prefix for isolation.
    pub async fn new(port: u16) -> Self {
        let client = sync_service::create_local_dynamo_client(port)
            .await
            .expect("Failed to create DynamoDB Local client. Is DynamoDB Local running?");

        let table_prefix = format!("test_{}_", Uuid::new_v4().to_string().replace('-', "")[..8].to_string());

        table_definitions::create_all_tables(&client, &table_prefix)
            .await
            .expect("Failed to create test tables");

        Self { client, table_prefix }
    }

    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }

    pub async fn cleanup(&self) {
        let _ = table_definitions::delete_all_tables(&self.client, &self.table_prefix).await;
    }
}

impl Drop for DynamoTestContext {
    fn drop(&mut self) {
        // Best-effort cleanup. Integration test runners should also call cleanup() explicitly
        // since Drop can't run async code reliably.
        // The unique prefix ensures leftover tables don't interfere with other tests.
    }
}

/// Default DynamoDB Local port
pub const DYNAMO_LOCAL_PORT: u16 = 8000;

/// Check if DynamoDB Local is reachable. Returns false if not running.
pub async fn is_dynamo_local_available(port: u16) -> bool {
    let client = match sync_service::create_local_dynamo_client(port).await {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.list_tables().send().await.is_ok()
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo test -p sync-service --no-run`
Expected: compiles with no errors (tests won't run without DynamoDB Local)

- [ ] **Step 3: Commit**

```bash
git add sync-service/tests/
git commit -m "feat: add DynamoDB test infrastructure with table isolation"
```

### Task 5: Implement sync event push with conditional writes

**Files:**
- Modify: `sync-service/src/storage/dynamo.rs`

This is the most critical piece — the atomic sequence increment + event write.

- [ ] **Step 1: Write the failing integration test**

Create `sync-service/tests/sync_events_test.rs`:

```rust
mod common;

use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use shared::sync::*;
use sync_service::storage::DynamoStore;
use chrono::Utc;

async fn setup() -> Option<(DynamoTestContext, DynamoStore)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_prefix.clone());
    Some((ctx, store))
}

#[tokio::test]
async fn test_push_event_increments_sequence() {
    let Some((ctx, store)) = setup().await else { return };

    // Initialize metadata for the child
    store.initialize_child_metadata("child1").await.unwrap();

    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx1".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let sequence = store.push_event(&event).await.unwrap();
    assert_eq!(sequence, 1);

    let event2 = SyncEvent::new(
        EntityType::Transaction,
        "tx2".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let sequence2 = store.push_event(&event2).await.unwrap();
    assert_eq!(sequence2, 2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_push_event_stores_correct_attributes() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let event = SyncEvent::new(
        EntityType::Goal,
        "goal1".to_string(),
        "child1".to_string(),
        SyncAction::Updated,
        SyncSource::Remote,
    );
    let event_id = event.event_id.clone();

    let seq = store.push_event(&event).await.unwrap();

    // Read back from DDB
    let events = store.get_events_since("child1", 0).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, event_id);
    assert_eq!(events[0].entity_type, EntityType::Goal);
    assert_eq!(events[0].entity_id, "goal1");
    assert_eq!(events[0].action, SyncAction::Updated);
    assert_eq!(events[0].source, SyncSource::Remote);
    assert_eq!(events[0].sequence, Some(seq));

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_push_multiple_events_sequential_sequences() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let mut sequences = Vec::new();
    for i in 0..10 {
        let event = SyncEvent::new(
            EntityType::Transaction,
            format!("tx{}", i),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        let seq = store.push_event(&event).await.unwrap();
        sequences.push(seq);
    }

    // Verify gapless sequence: 1, 2, 3, ..., 10
    let expected: Vec<u64> = (1..=10).collect();
    assert_eq!(sequences, expected);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_get_events_since_returns_correct_range() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    for i in 0..10 {
        let event = SyncEvent::new(
            EntityType::Transaction,
            format!("tx{}", i),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        store.push_event(&event).await.unwrap();
    }

    // Get events since sequence 5 (should return 6, 7, 8, 9, 10)
    let events = store.get_events_since("child1", 5).await.unwrap();
    assert_eq!(events.len(), 5);
    let sequences: Vec<u64> = events.iter().map(|e| e.sequence.unwrap()).collect();
    assert_eq!(sequences, vec![6, 7, 8, 9, 10]);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_get_events_since_empty_when_at_latest() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx1".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );
    store.push_event(&event).await.unwrap();

    let events = store.get_events_since("child1", 1).await.unwrap();
    assert!(events.is_empty());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_duplicate_event_push_idempotent() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx1".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let seq1 = store.push_event(&event).await.unwrap();
    let seq2 = store.push_event(&event).await.unwrap();

    // Same event_id should return same sequence, not allocate a new one
    assert_eq!(seq1, seq2);

    let events = store.get_events_since("child1", 0).await.unwrap();
    assert_eq!(events.len(), 1);

    ctx.cleanup().await;
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p sync-service --test sync_events_test -- --nocapture`
Expected: compilation error — `push_event`, `get_events_since`, `initialize_child_metadata` don't exist yet

- [ ] **Step 3: Implement the sync event methods on DynamoStore**

Add to `sync-service/src/storage/dynamo.rs`:

```rust
use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::AttributeValue;
use shared::sync::*;
use super::table_definitions;

pub struct DynamoStore {
    client: Client,
    table_prefix: String,
}

impl DynamoStore {
    pub fn new(client: Client, table_prefix: String) -> Self {
        Self { client, table_prefix }
    }

    pub fn table_name(&self, base: &str) -> String {
        format!("{}{}", self.table_prefix, base)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn table_prefix(&self) -> &str {
        &self.table_prefix
    }

    pub async fn create_tables(&self) -> anyhow::Result<()> {
        table_definitions::create_all_tables(&self.client, &self.table_prefix).await
    }

    pub async fn delete_tables(&self) -> anyhow::Result<()> {
        table_definitions::delete_all_tables(&self.client, &self.table_prefix).await
    }

    /// Initialize sync_metadata for a child. Must be called before pushing events.
    pub async fn initialize_child_metadata(&self, child_id: &str) -> anyhow::Result<()> {
        self.client
            .put_item()
            .table_name(self.table_name("sync_metadata"))
            .item("child_id", AttributeValue::S(child_id.to_string()))
            .item("event_sequence", AttributeValue::N("0".to_string()))
            .item("local_watermark", AttributeValue::N("0".to_string()))
            .item("remote_watermark", AttributeValue::N("0".to_string()))
            .condition_expression("attribute_not_exists(child_id)")
            .send()
            .await
            .or_else(|e| {
                let service_error = e.into_service_error();
                if service_error.is_conditional_check_failed_exception() {
                    Ok(Default::default())
                } else {
                    Err(anyhow::anyhow!("Failed to initialize metadata: {}", service_error))
                }
            })?;
        Ok(())
    }

    /// Push a sync event. Returns the assigned sequence number.
    /// Idempotent: if an event with the same event_id already exists, returns its sequence.
    pub async fn push_event(&self, event: &SyncEvent) -> anyhow::Result<u64> {
        // Check for duplicate event_id first
        if let Some(existing_seq) = self.find_event_by_id(&event.child_id, &event.event_id).await? {
            return Ok(existing_seq);
        }

        // Atomically increment the sequence counter
        let update_result = self.client
            .update_item()
            .table_name(self.table_name("sync_metadata"))
            .key("child_id", AttributeValue::S(event.child_id.clone()))
            .update_expression("SET event_sequence = event_sequence + :inc")
            .expression_attribute_values(":inc", AttributeValue::N("1".to_string()))
            .condition_expression("attribute_exists(event_sequence)")
            .return_values(aws_sdk_dynamodb::types::ReturnValue::UpdatedNew)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to increment sequence: {}", e.into_service_error()))?;

        let new_sequence = update_result
            .attributes()
            .and_then(|attrs| attrs.get("event_sequence"))
            .and_then(|v| match v {
                AttributeValue::N(n) => n.parse::<u64>().ok(),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("Failed to read new sequence number"))?;

        // Write the event with the assigned sequence
        self.client
            .put_item()
            .table_name(self.table_name("sync_events"))
            .item("child_id", AttributeValue::S(event.child_id.clone()))
            .item("sequence", AttributeValue::N(new_sequence.to_string()))
            .item("event_id", AttributeValue::S(event.event_id.clone()))
            .item("entity_type", AttributeValue::S(event.entity_type.as_str().to_string()))
            .item("entity_id", AttributeValue::S(event.entity_id.clone()))
            .item("action", AttributeValue::S(event.action.as_str().to_string()))
            .item("source", AttributeValue::S(match &event.source {
                SyncSource::Local => "local".to_string(),
                SyncSource::Remote => "remote".to_string(),
            }))
            .item("source_timestamp", AttributeValue::S(event.source_timestamp.to_rfc3339()))
            .condition_expression("attribute_not_exists(#seq)")
            .expression_attribute_names("#seq", "sequence")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to write event: {}", e.into_service_error()))?;

        Ok(new_sequence)
    }

    /// Find an existing event by event_id. Returns its sequence number if found.
    /// Scans the sync_events table for the child — acceptable because deduplication
    /// only happens during push retries, which are infrequent.
    async fn find_event_by_id(&self, child_id: &str, event_id: &str) -> anyhow::Result<Option<u64>> {
        let result = self.client
            .query()
            .table_name(self.table_name("sync_events"))
            .key_condition_expression("child_id = :cid")
            .filter_expression("event_id = :eid")
            .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
            .expression_attribute_values(":eid", AttributeValue::S(event_id.to_string()))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query for duplicate: {}", e.into_service_error()))?;

        if let Some(items) = result.items() {
            if let Some(item) = items.first() {
                if let Some(AttributeValue::N(seq_str)) = item.get("sequence") {
                    return Ok(Some(seq_str.parse::<u64>()?));
                }
            }
        }

        Ok(None)
    }

    /// Get all sync events for a child since the given sequence number (exclusive).
    pub async fn get_events_since(&self, child_id: &str, since_sequence: u64) -> anyhow::Result<Vec<SyncEvent>> {
        let result = self.client
            .query()
            .table_name(self.table_name("sync_events"))
            .key_condition_expression("child_id = :cid AND #seq > :since")
            .expression_attribute_names("#seq", "sequence")
            .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
            .expression_attribute_values(":since", AttributeValue::N(since_sequence.to_string()))
            .scan_index_forward(true) // ascending order
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query events: {}", e.into_service_error()))?;

        let mut events = Vec::new();
        if let Some(items) = result.items() {
            for item in items {
                events.push(self.parse_sync_event(item)?);
            }
        }

        Ok(events)
    }

    fn parse_sync_event(&self, item: &std::collections::HashMap<String, AttributeValue>) -> anyhow::Result<SyncEvent> {
        let get_s = |key: &str| -> anyhow::Result<String> {
            match item.get(key) {
                Some(AttributeValue::S(s)) => Ok(s.clone()),
                _ => Err(anyhow::anyhow!("Missing or invalid attribute: {}", key)),
            }
        };
        let get_n = |key: &str| -> anyhow::Result<u64> {
            match item.get(key) {
                Some(AttributeValue::N(n)) => Ok(n.parse()?),
                _ => Err(anyhow::anyhow!("Missing or invalid attribute: {}", key)),
            }
        };

        let source_str = get_s("source")?;
        let source = match source_str.as_str() {
            "local" => SyncSource::Local,
            "remote" => SyncSource::Remote,
            _ => return Err(anyhow::anyhow!("Unknown source: {}", source_str)),
        };

        Ok(SyncEvent {
            event_id: get_s("event_id")?,
            entity_type: EntityType::from_str(&get_s("entity_type")?).map_err(|e| anyhow::anyhow!(e))?,
            entity_id: get_s("entity_id")?,
            child_id: get_s("child_id")?,
            action: SyncAction::from_str(&get_s("action")?).map_err(|e| anyhow::anyhow!(e))?,
            source,
            source_timestamp: chrono::DateTime::parse_from_rfc3339(&get_s("source_timestamp")?)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| anyhow::anyhow!("Failed to parse timestamp: {}", e))?,
            sequence: Some(get_n("sequence")?),
        })
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p sync-service --test sync_events_test -- --nocapture`
Expected: all tests pass (requires DynamoDB Local running on port 8000)

- [ ] **Step 5: Commit**

```bash
git add sync-service/src/storage/dynamo.rs sync-service/tests/
git commit -m "feat: implement sync event push with conditional writes and deduplication"
```

### Task 6: Implement entity CRUD on DynamoStore

**Files:**
- Modify: `sync-service/src/storage/dynamo.rs`
- Create: `sync-service/tests/entity_crud_test.rs`

- [ ] **Step 1: Write the failing integration tests**

Create `sync-service/tests/entity_crud_test.rs`:

```rust
mod common;

use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use shared::sync::EntityType;
use sync_service::storage::DynamoStore;

async fn setup() -> Option<(DynamoTestContext, DynamoStore)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_prefix.clone());
    Some((ctx, store))
}

#[tokio::test]
async fn test_upsert_and_get_transaction() {
    let Some((ctx, store)) = setup().await else { return };

    let entity_json = r#"{"id":"tx1","child_id":"child1","date":"2026-03-28T10:00:00Z","description":"Test","amount":5.0,"balance":5.0,"transaction_type":"allowance"}"#;

    store.upsert_entity("child1", EntityType::Transaction, "tx1", entity_json).await.unwrap();

    let result = store.get_entity("child1", EntityType::Transaction, "tx1").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap(), entity_json);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_upsert_overwrites_existing() {
    let Some((ctx, store)) = setup().await else { return };

    let v1 = r#"{"id":"tx1","amount":5.0}"#;
    let v2 = r#"{"id":"tx1","amount":10.0}"#;

    store.upsert_entity("child1", EntityType::Transaction, "tx1", v1).await.unwrap();
    store.upsert_entity("child1", EntityType::Transaction, "tx1", v2).await.unwrap();

    let result = store.get_entity("child1", EntityType::Transaction, "tx1").await.unwrap();
    assert_eq!(result.unwrap(), v2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_get_nonexistent_entity_returns_none() {
    let Some((ctx, store)) = setup().await else { return };

    let result = store.get_entity("child1", EntityType::Transaction, "nope").await.unwrap();
    assert!(result.is_none());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_delete_entity() {
    let Some((ctx, store)) = setup().await else { return };

    let entity_json = r#"{"id":"tx1"}"#;
    store.upsert_entity("child1", EntityType::Transaction, "tx1", entity_json).await.unwrap();

    store.delete_entity("child1", EntityType::Transaction, "tx1").await.unwrap();

    let result = store.get_entity("child1", EntityType::Transaction, "tx1").await.unwrap();
    assert!(result.is_none());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_goal_crud() {
    let Some((ctx, store)) = setup().await else { return };

    let goal_json = r#"{"id":"goal1","child_id":"child1","description":"Save for bike","target_amount":50.0,"state":"active"}"#;

    store.upsert_entity("child1", EntityType::Goal, "goal1", goal_json).await.unwrap();

    let result = store.get_entity("child1", EntityType::Goal, "goal1").await.unwrap();
    assert_eq!(result.unwrap(), goal_json);

    store.delete_entity("child1", EntityType::Goal, "goal1").await.unwrap();

    let result = store.get_entity("child1", EntityType::Goal, "goal1").await.unwrap();
    assert!(result.is_none());

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_child_crud() {
    let Some((ctx, store)) = setup().await else { return };

    let child_json = r#"{"id":"child1","name":"Alice","birthdate":"2015-05-01"}"#;

    store.upsert_entity("child1", EntityType::Child, "child1", child_json).await.unwrap();

    let result = store.get_entity("child1", EntityType::Child, "child1").await.unwrap();
    assert_eq!(result.unwrap(), child_json);

    ctx.cleanup().await;
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p sync-service --test entity_crud_test -- --nocapture`
Expected: compilation error — methods don't exist yet

- [ ] **Step 3: Implement entity CRUD methods**

Add to `sync-service/src/storage/dynamo.rs` (inside `impl DynamoStore`):

```rust
    /// Upsert an entity. The entity_json is stored as-is for round-trip fidelity.
    pub async fn upsert_entity(
        &self,
        child_id: &str,
        entity_type: EntityType,
        entity_id: &str,
        entity_json: &str,
    ) -> anyhow::Result<()> {
        let (table_base, sort_key_name) = self.entity_table_info(&entity_type);

        let mut req = self.client
            .put_item()
            .table_name(self.table_name(table_base))
            .item("child_id", AttributeValue::S(child_id.to_string()))
            .item("data", AttributeValue::S(entity_json.to_string()));

        if let Some(sk_name) = sort_key_name {
            req = req.item(sk_name, AttributeValue::S(entity_id.to_string()));
        }

        req.send().await
            .map_err(|e| anyhow::anyhow!("Failed to upsert entity: {}", e.into_service_error()))?;

        Ok(())
    }

    /// Get an entity by type and ID. Returns the JSON blob or None.
    pub async fn get_entity(
        &self,
        child_id: &str,
        entity_type: EntityType,
        entity_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let (table_base, sort_key_name) = self.entity_table_info(&entity_type);

        let mut req = self.client
            .get_item()
            .table_name(self.table_name(table_base))
            .key("child_id", AttributeValue::S(child_id.to_string()));

        if let Some(sk_name) = sort_key_name {
            req = req.key(sk_name, AttributeValue::S(entity_id.to_string()));
        }

        let result = req.send().await
            .map_err(|e| anyhow::anyhow!("Failed to get entity: {}", e.into_service_error()))?;

        Ok(result.item().and_then(|item| {
            item.get("data").and_then(|v| match v {
                AttributeValue::S(s) => Some(s.clone()),
                _ => None,
            })
        }))
    }

    /// Delete an entity.
    pub async fn delete_entity(
        &self,
        child_id: &str,
        entity_type: EntityType,
        entity_id: &str,
    ) -> anyhow::Result<()> {
        let (table_base, sort_key_name) = self.entity_table_info(&entity_type);

        let mut req = self.client
            .delete_item()
            .table_name(self.table_name(table_base))
            .key("child_id", AttributeValue::S(child_id.to_string()));

        if let Some(sk_name) = sort_key_name {
            req = req.key(sk_name, AttributeValue::S(entity_id.to_string()));
        }

        req.send().await
            .map_err(|e| anyhow::anyhow!("Failed to delete entity: {}", e.into_service_error()))?;

        Ok(())
    }

    /// Map entity type to (table_base_name, optional_sort_key_name).
    fn entity_table_info(&self, entity_type: &EntityType) -> (&'static str, Option<&'static str>) {
        match entity_type {
            EntityType::Transaction => ("transactions", Some("transaction_id")),
            EntityType::Goal => ("goals", Some("goal_id")),
            EntityType::Child => ("children", None),
        }
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p sync-service --test entity_crud_test -- --nocapture`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add sync-service/src/storage/dynamo.rs sync-service/tests/entity_crud_test.rs
git commit -m "feat: implement entity CRUD operations on DynamoStore"
```

### Task 7: Implement checkpoint management

**Files:**
- Modify: `sync-service/src/storage/dynamo.rs`
- Create: `sync-service/tests/checkpoint_test.rs`

- [ ] **Step 1: Write the failing integration tests**

Create `sync-service/tests/checkpoint_test.rs`:

```rust
mod common;

use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use sync_service::storage::DynamoStore;

async fn setup() -> Option<(DynamoTestContext, DynamoStore)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_prefix.clone());
    Some((ctx, store))
}

#[tokio::test]
async fn test_checkpoint_round_trip() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let cp = store.get_checkpoint("child1").await.unwrap();
    assert_eq!(cp.event_sequence, 0);
    assert_eq!(cp.local_watermark, 0);
    assert_eq!(cp.remote_watermark, 0);

    store.update_watermark("child1", "local", 5).await.unwrap();
    store.update_watermark("child1", "remote", 3).await.unwrap();

    let cp = store.get_checkpoint("child1").await.unwrap();
    assert_eq!(cp.local_watermark, 5);
    assert_eq!(cp.remote_watermark, 3);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_watermark_only_moves_forward() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    store.update_watermark("child1", "local", 10).await.unwrap();

    // Attempt to move backwards — should be a no-op
    store.update_watermark("child1", "local", 5).await.unwrap();

    let cp = store.get_checkpoint("child1").await.unwrap();
    assert_eq!(cp.local_watermark, 10);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_checkpoint_for_uninitialized_child() {
    let Some((ctx, store)) = setup().await else { return };

    let result = store.get_checkpoint("nonexistent").await;
    assert!(result.is_err() || {
        let cp = result.unwrap();
        cp.event_sequence == 0
    });

    ctx.cleanup().await;
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p sync-service --test checkpoint_test -- --nocapture`
Expected: compilation error

- [ ] **Step 3: Implement checkpoint methods**

Add to `sync-service/src/storage/dynamo.rs` (inside `impl DynamoStore`):

```rust
    /// Get the sync checkpoint for a child.
    pub async fn get_checkpoint(&self, child_id: &str) -> anyhow::Result<SyncCheckpoint> {
        let result = self.client
            .get_item()
            .table_name(self.table_name("sync_metadata"))
            .key("child_id", AttributeValue::S(child_id.to_string()))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get checkpoint: {}", e.into_service_error()))?;

        match result.item() {
            Some(item) => {
                let get_n = |key: &str| -> u64 {
                    item.get(key)
                        .and_then(|v| match v {
                            AttributeValue::N(n) => n.parse().ok(),
                            _ => None,
                        })
                        .unwrap_or(0)
                };

                Ok(SyncCheckpoint {
                    child_id: child_id.to_string(),
                    event_sequence: get_n("event_sequence"),
                    local_watermark: get_n("local_watermark"),
                    remote_watermark: get_n("remote_watermark"),
                })
            }
            None => Err(anyhow::anyhow!("No metadata found for child: {}", child_id)),
        }
    }

    /// Update a watermark. Only moves forward (conditional: new > current).
    /// `which` is "local" or "remote".
    pub async fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> anyhow::Result<()> {
        let attr_name = format!("{}_watermark", which);

        self.client
            .update_item()
            .table_name(self.table_name("sync_metadata"))
            .key("child_id", AttributeValue::S(child_id.to_string()))
            .update_expression("SET #wm = :val")
            .condition_expression("attribute_exists(child_id) AND #wm < :val")
            .expression_attribute_names("#wm", &attr_name)
            .expression_attribute_values(":val", AttributeValue::N(value.to_string()))
            .send()
            .await
            .or_else(|e| {
                let service_error = e.into_service_error();
                if service_error.is_conditional_check_failed_exception() {
                    // Watermark is already >= value, that's fine
                    Ok(Default::default())
                } else {
                    Err(anyhow::anyhow!("Failed to update watermark: {}", service_error))
                }
            })?;

        Ok(())
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p sync-service --test checkpoint_test -- --nocapture`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add sync-service/src/storage/dynamo.rs sync-service/tests/checkpoint_test.rs
git commit -m "feat: implement checkpoint and watermark management with forward-only constraint"
```

### Task 8: Concurrency stress tests

**Files:**
- Create: `sync-service/tests/concurrency_test.rs`

These are the critical tests that validate the conditional write ordering guarantees under real concurrent access.

- [ ] **Step 1: Write the concurrency tests**

Create `sync-service/tests/concurrency_test.rs`:

```rust
mod common;

use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use shared::sync::*;
use sync_service::storage::DynamoStore;
use std::sync::{Arc, Barrier};

async fn setup() -> Option<(DynamoTestContext, Arc<DynamoStore>)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = Arc::new(DynamoStore::new(ctx.client.clone(), ctx.table_prefix.clone()));
    Some((ctx, store))
}

fn assert_gapless_sequence(sequences: &[u64]) {
    let mut sorted = sequences.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), sequences.len(), "Duplicate sequence numbers found: {:?}", sequences);
    let expected: Vec<u64> = (1..=sorted.len() as u64).collect();
    assert_eq!(sorted, expected, "Sequences are not gapless 1..N: {:?}", sorted);
}

/// THE critical test: 10 threads, each pushing 10 events for the same child.
/// All 100 events must get unique, gapless sequences 1-100.
#[tokio::test]
async fn test_concurrent_sequence_increment_no_duplicates() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let num_threads = 10;
    let events_per_thread = 10;
    let barrier = Arc::new(Barrier::new(num_threads));

    let mut handles = Vec::new();

    for thread_idx in 0..num_threads {
        let store = store.clone();
        let barrier = barrier.clone();

        let handle = tokio::spawn(async move {
            // Wait for all threads to be ready before starting
            barrier.wait();

            let mut sequences = Vec::new();
            for event_idx in 0..events_per_thread {
                let event = SyncEvent::new(
                    EntityType::Transaction,
                    format!("tx_{}_{}", thread_idx, event_idx),
                    "child1".to_string(),
                    SyncAction::Created,
                    SyncSource::Local,
                );
                let seq = store.push_event(&event).await.unwrap();
                sequences.push(seq);
            }
            sequences
        });

        handles.push(handle);
    }

    let mut all_sequences = Vec::new();
    for handle in handles {
        let sequences = handle.await.unwrap();
        all_sequences.extend(sequences);
    }

    assert_eq!(all_sequences.len(), 100);
    assert_gapless_sequence(&all_sequences);

    // Verify all events are readable
    let events = store.get_events_since("child1", 0).await.unwrap();
    assert_eq!(events.len(), 100);

    ctx.cleanup().await;
}

/// Threads pushing events for different children should be independent.
#[tokio::test]
async fn test_concurrent_push_different_children_independent() {
    let Some((ctx, store)) = setup().await else { return };

    let num_children = 5;
    let events_per_child = 20;

    for i in 0..num_children {
        store.initialize_child_metadata(&format!("child{}", i)).await.unwrap();
    }

    let barrier = Arc::new(Barrier::new(num_children));
    let mut handles = Vec::new();

    for child_idx in 0..num_children {
        let store = store.clone();
        let barrier = barrier.clone();
        let child_id = format!("child{}", child_idx);

        let handle = tokio::spawn(async move {
            barrier.wait();

            let mut sequences = Vec::new();
            for event_idx in 0..events_per_child {
                let event = SyncEvent::new(
                    EntityType::Transaction,
                    format!("tx_{}_{}", child_idx, event_idx),
                    child_id.clone(),
                    SyncAction::Created,
                    SyncSource::Local,
                );
                let seq = store.push_event(&event).await.unwrap();
                sequences.push(seq);
            }
            (child_id, sequences)
        });

        handles.push(handle);
    }

    for handle in handles {
        let (child_id, sequences) = handle.await.unwrap();
        assert_eq!(sequences.len(), events_per_child);
        assert_gapless_sequence(&sequences);

        let events = store.get_events_since(&child_id, 0).await.unwrap();
        assert_eq!(events.len(), events_per_child);
    }

    ctx.cleanup().await;
}

/// Simulate two clients: one pushing, one polling. The poller must never see gaps.
#[tokio::test]
async fn test_client_a_pushes_while_client_b_polls() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let push_store = store.clone();
    let poll_store = store.clone();
    let total_events = 50;

    // Client A: push events
    let pusher = tokio::spawn(async move {
        for i in 0..total_events {
            let event = SyncEvent::new(
                EntityType::Transaction,
                format!("tx_{}", i),
                "child1".to_string(),
                SyncAction::Created,
                SyncSource::Local,
            );
            push_store.push_event(&event).await.unwrap();
        }
    });

    // Client B: poll repeatedly, verify no gaps
    let poller = tokio::spawn(async move {
        let mut watermark = 0u64;
        let mut total_seen = 0;
        let mut attempts = 0;
        let max_attempts = 500; // safety bound

        while total_seen < total_events && attempts < max_attempts {
            let events = poll_store.get_events_since("child1", watermark).await.unwrap();

            // Verify events are in order and contiguous from watermark
            for (i, event) in events.iter().enumerate() {
                let expected_seq = watermark + 1 + i as u64;
                assert_eq!(
                    event.sequence.unwrap(), expected_seq,
                    "Gap detected! Expected sequence {}, got {}. Watermark was {}",
                    expected_seq, event.sequence.unwrap(), watermark
                );
            }

            if !events.is_empty() {
                watermark = events.last().unwrap().sequence.unwrap();
                total_seen += events.len();
            }

            attempts += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }

        assert_eq!(total_seen, total_events, "Poller didn't see all events within attempt limit");
    });

    pusher.await.unwrap();
    poller.await.unwrap();

    ctx.cleanup().await;
}

/// Both clients modify the same watermark concurrently. It must only move forward.
#[tokio::test]
async fn test_watermark_update_race() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();

    // Two "clients" trying to update the same watermark with interleaved values
    for client_idx in 0..2 {
        let store = store.clone();
        let barrier = barrier.clone();

        let handle = tokio::spawn(async move {
            barrier.wait();
            for i in 0..50 {
                // Client 0 writes even numbers, client 1 writes odd
                let value = (i * 2 + client_idx) as u64 + 1;
                store.update_watermark("child1", "local", value).await.unwrap();
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Watermark should be at the maximum value written
    let cp = store.get_checkpoint("child1").await.unwrap();
    assert!(cp.local_watermark >= 99, "Watermark should be at least 99, got {}", cp.local_watermark);

    ctx.cleanup().await;
}

/// Simulate a stale client that was offline while 500 events accumulated.
#[tokio::test]
async fn test_client_offline_then_syncs() {
    let Some((ctx, store)) = setup().await else { return };

    store.initialize_child_metadata("child1").await.unwrap();

    // Push 500 events (simulating activity while client A is offline)
    for i in 0..500 {
        let event = SyncEvent::new(
            EntityType::Transaction,
            format!("tx_{}", i),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Remote,
        );
        store.push_event(&event).await.unwrap();
    }

    // Advance remote watermark but leave local at 0
    store.update_watermark("child1", "remote", 500).await.unwrap();

    // Client A comes online and polls from 0
    let events = store.get_events_since("child1", 0).await.unwrap();
    assert_eq!(events.len(), 500);

    // Verify ordering
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.sequence.unwrap(), (i + 1) as u64);
    }

    ctx.cleanup().await;
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p sync-service --test concurrency_test -- --nocapture`
Expected: all tests pass. If any fail, investigate the conditional write logic.

- [ ] **Step 3: Commit**

```bash
git add sync-service/tests/concurrency_test.rs
git commit -m "test: add concurrency stress tests for sync event ordering guarantees"
```

### Task 9: REST API routes

**Files:**
- Modify: `sync-service/src/routes/mod.rs`
- Create: `sync-service/src/routes/sync.rs`
- Create: `sync-service/src/routes/entities.rs`
- Modify: `sync-service/src/lib.rs`
- Create: `sync-service/tests/api_test.rs`

- [ ] **Step 1: Write the failing API tests**

Create `sync-service/tests/api_test.rs`:

```rust
mod common;

use common::{DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use sync_service::storage::DynamoStore;
use shared::sync::*;

async fn start_test_server() -> Option<(String, sync_service::storage::DynamoStore)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available");
        return None;
    }
    let client = sync_service::create_local_dynamo_client(DYNAMO_LOCAL_PORT).await.unwrap();
    let prefix = format!("api_test_{}_", uuid::Uuid::new_v4().to_string().replace('-', "")[..8].to_string());

    let store = DynamoStore::new(client.clone(), prefix.clone());
    store.create_tables().await.unwrap();

    let app = sync_service::routes::build_router(store);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let cleanup_store = DynamoStore::new(client, prefix);
    Some((base_url, cleanup_store))
}

#[tokio::test]
async fn test_health_endpoint() {
    let Some((base_url, cleanup_store)) = start_test_server().await else { return };

    let resp = reqwest::get(format!("{}/health", base_url)).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");

    cleanup_store.delete_tables().await.unwrap();
}

#[tokio::test]
async fn test_push_events_endpoint() {
    let Some((base_url, cleanup_store)) = start_test_server().await else { return };

    // Initialize child metadata
    let client = reqwest::Client::new();
    client.post(format!("{}/sync/initialize/child1", base_url))
        .send().await.unwrap();

    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx1".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let resp = client.post(format!("{}/sync/events", base_url))
        .json(&vec![event])
        .send().await.unwrap();

    assert_eq!(resp.status(), 201);

    // Verify via GET
    let resp = client.get(format!("{}/sync/events?child_id=child1&since=0", base_url))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);

    let events: Vec<SyncEvent> = resp.json().await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].entity_id, "tx1");
    assert_eq!(events[0].sequence, Some(1));

    cleanup_store.delete_tables().await.unwrap();
}

#[tokio::test]
async fn test_entity_crud_endpoints() {
    let Some((base_url, cleanup_store)) = start_test_server().await else { return };

    let client = reqwest::Client::new();
    let entity_json = r#"{"id":"tx1","amount":5.0}"#;

    // PUT entity
    let resp = client.put(format!("{}/entities/transaction/child1/tx1", base_url))
        .body(entity_json.to_string())
        .header("content-type", "application/json")
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // GET entity
    let resp = client.get(format!("{}/entities/transaction/child1/tx1", base_url))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), entity_json);

    // DELETE entity
    let resp = client.delete(format!("{}/entities/transaction/child1/tx1", base_url))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // GET after delete — 404
    let resp = client.get(format!("{}/entities/transaction/child1/tx1", base_url))
        .send().await.unwrap();
    assert_eq!(resp.status(), 404);

    cleanup_store.delete_tables().await.unwrap();
}

#[tokio::test]
async fn test_checkpoint_endpoints() {
    let Some((base_url, cleanup_store)) = start_test_server().await else { return };

    let client = reqwest::Client::new();

    client.post(format!("{}/sync/initialize/child1", base_url))
        .send().await.unwrap();

    // GET checkpoint
    let resp = client.get(format!("{}/sync/checkpoint/child1", base_url))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let cp: SyncCheckpoint = resp.json().await.unwrap();
    assert_eq!(cp.local_watermark, 0);

    // PUT watermark
    let resp = client.put(format!("{}/sync/checkpoint/child1", base_url))
        .json(&serde_json::json!({"which": "local", "value": 5}))
        .send().await.unwrap();
    assert_eq!(resp.status(), 200);

    // Verify
    let resp = client.get(format!("{}/sync/checkpoint/child1", base_url))
        .send().await.unwrap();
    let cp: SyncCheckpoint = resp.json().await.unwrap();
    assert_eq!(cp.local_watermark, 5);

    cleanup_store.delete_tables().await.unwrap();
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p sync-service --test api_test -- --nocapture`
Expected: compilation error — routes don't exist yet

- [ ] **Step 3: Implement sync routes**

Create `sync-service/src/routes/sync.rs`:

```rust
use axum::{
    Router,
    routing::{get, post, put},
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use serde::Deserialize;
use shared::sync::*;
use std::sync::Arc;
use crate::storage::DynamoStore;

#[derive(Deserialize)]
pub struct EventsQuery {
    child_id: String,
    since: u64,
}

#[derive(Deserialize)]
pub struct WatermarkUpdate {
    which: String,
    value: u64,
}

async fn push_events(
    State(store): State<Arc<DynamoStore>>,
    Json(events): Json<Vec<SyncEvent>>,
) -> impl IntoResponse {
    let mut sequences = Vec::new();
    for event in &events {
        match store.push_event(event).await {
            Ok(seq) => sequences.push(seq),
            Err(e) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        }
    }
    (StatusCode::CREATED, Json(sequences)).into_response()
}

async fn get_events(
    State(store): State<Arc<DynamoStore>>,
    Query(query): Query<EventsQuery>,
) -> impl IntoResponse {
    match store.get_events_since(&query.child_id, query.since).await {
        Ok(events) => (StatusCode::OK, Json(events)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn initialize_child(
    State(store): State<Arc<DynamoStore>>,
    Path(child_id): Path<String>,
) -> impl IntoResponse {
    match store.initialize_child_metadata(&child_id).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn get_checkpoint(
    State(store): State<Arc<DynamoStore>>,
    Path(child_id): Path<String>,
) -> impl IntoResponse {
    match store.get_checkpoint(&child_id).await {
        Ok(cp) => (StatusCode::OK, Json(cp)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
    }
}

async fn update_checkpoint(
    State(store): State<Arc<DynamoStore>>,
    Path(child_id): Path<String>,
    Json(update): Json<WatermarkUpdate>,
) -> impl IntoResponse {
    match store.update_watermark(&child_id, &update.which, update.value).await {
        Ok(_) => StatusCode::OK,
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new()
        .route("/sync/events", post(push_events).get(get_events))
        .route("/sync/initialize/{child_id}", post(initialize_child))
        .route("/sync/checkpoint/{child_id}", get(get_checkpoint).put(update_checkpoint))
}
```

- [ ] **Step 4: Implement entity routes**

Create `sync-service/src/routes/entities.rs`:

```rust
use axum::{
    Router,
    routing::{get, put, delete},
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    body::Bytes,
};
use shared::sync::EntityType;
use std::sync::Arc;
use crate::storage::DynamoStore;

async fn upsert_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
    body: Bytes,
) -> impl IntoResponse {
    let entity_type = match EntityType::from_str(&entity_type_str) {
        Ok(et) => et,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    let json = String::from_utf8_lossy(&body).to_string();

    match store.upsert_entity(&child_id, entity_type, &entity_id, &json).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let entity_type = match EntityType::from_str(&entity_type_str) {
        Ok(et) => et,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    match store.get_entity(&child_id, entity_type, &entity_id).await {
        Ok(Some(json)) => (StatusCode::OK, json).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn delete_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let entity_type = match EntityType::from_str(&entity_type_str) {
        Ok(et) => et,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    match store.delete_entity(&child_id, entity_type, &entity_id).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new()
        .route("/entities/{entity_type}/{child_id}/{entity_id}",
            put(upsert_entity).get(get_entity).delete(delete_entity))
}
```

- [ ] **Step 5: Update routes/mod.rs to wire everything together**

Replace `sync-service/src/routes/mod.rs`:

```rust
mod health;
mod sync;
mod entities;

use axum::Router;
use std::sync::Arc;
use crate::storage::DynamoStore;

pub fn build_router(store: DynamoStore) -> Router {
    let store = Arc::new(store);
    Router::new()
        .merge(health::routes())
        .merge(sync::routes())
        .merge(entities::routes())
        .with_state(store)
}
```

- [ ] **Step 6: Update health routes to use state**

Replace `sync-service/src/routes/health.rs`:

```rust
use axum::{Router, routing::get};
use std::sync::Arc;
use crate::storage::DynamoStore;

async fn health_check() -> &'static str {
    "ok"
}

pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new().route("/health", get(health_check))
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p sync-service --test api_test -- --nocapture`
Expected: all tests pass

- [ ] **Step 8: Commit**

```bash
git add sync-service/src/routes/ sync-service/tests/api_test.rs
git commit -m "feat: implement REST API routes for sync events, entities, and checkpoints"
```

---

## Phase 3: RemoteStorage Trait & Implementations

### Task 10: Define RemoteStorage trait in backend

**Files:**
- Create: `backend/storage/remote.rs`
- Modify: `backend/storage/mod.rs`

- [ ] **Step 1: Create the trait**

Create `backend/storage/remote.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use shared::sync::*;

/// Trait abstracting communication with the remote sync service.
/// Implementations: HttpRemoteClient (production), InProcessRemoteClient (integration test),
/// MockRemoteClient (unit test).
pub trait RemoteStorage: Send + Sync {
    fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>>;
    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>>;

    fn upsert_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str, entity_json: &str) -> Result<()>;
    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>>;
    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()>;

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint>;
    fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> Result<()>;

    fn initialize_child(&self, child_id: &str) -> Result<()>;
    fn health_check(&self) -> Result<bool>;
}
```

- [ ] **Step 2: Declare the module**

Add to `backend/storage/mod.rs`:
```rust
pub mod remote;
```

And add to its re-exports:
```rust
pub use remote::RemoteStorage;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p allowance-tracker-egui`
Expected: compiles (trait is defined but not yet implemented)

- [ ] **Step 4: Commit**

```bash
git add backend/storage/remote.rs backend/storage/mod.rs
git commit -m "feat: define RemoteStorage trait for sync abstraction"
```

### Task 11: MockRemoteClient for unit tests

**Files:**
- Create: `backend/storage/mock_remote.rs`
- Modify: `backend/storage/mod.rs`

- [ ] **Step 1: Write tests for the mock**

Add to the bottom of the file we're about to create (we'll write tests first, then the impl above them):

Create `backend/storage/mock_remote.rs`:

```rust
use anyhow::Result;
use shared::sync::*;
use std::sync::Mutex;
use std::collections::HashMap;

/// In-memory mock of RemoteStorage for unit tests.
/// Stores events and entities in memory. Deterministic, no I/O.
pub struct MockRemoteClient {
    events: Mutex<HashMap<String, Vec<SyncEvent>>>,       // child_id -> events
    entities: Mutex<HashMap<String, String>>,               // "child_id#type#id" -> json
    metadata: Mutex<HashMap<String, SyncCheckpoint>>,       // child_id -> checkpoint
    /// If set, all operations return this error. For testing failure paths.
    pub force_error: Mutex<Option<String>>,
}

impl MockRemoteClient {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(HashMap::new()),
            entities: Mutex::new(HashMap::new()),
            metadata: Mutex::new(HashMap::new()),
            force_error: Mutex::new(None),
        }
    }

    fn check_error(&self) -> Result<()> {
        let err = self.force_error.lock().unwrap();
        if let Some(msg) = &*err {
            Err(anyhow::anyhow!("{}", msg))
        } else {
            Ok(())
        }
    }

    fn entity_key(child_id: &str, entity_type: &EntityType, entity_id: &str) -> String {
        format!("{}#{}#{}", child_id, entity_type.as_str(), entity_id)
    }
}

impl super::remote::RemoteStorage for MockRemoteClient {
    fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>> {
        self.check_error()?;
        let mut store = self.events.lock().unwrap();
        let mut metadata = self.metadata.lock().unwrap();
        let mut sequences = Vec::new();

        for event in events {
            let child_events = store.entry(event.child_id.clone()).or_default();
            let cp = metadata.entry(event.child_id.clone()).or_insert_with(|| SyncCheckpoint::new(event.child_id.clone()));

            // Check dedup
            if let Some(existing) = child_events.iter().find(|e| e.event_id == event.event_id) {
                sequences.push(existing.sequence.unwrap());
                continue;
            }

            cp.event_sequence += 1;
            let seq = cp.event_sequence;
            let mut event_with_seq = event.clone();
            event_with_seq.sequence = Some(seq);
            child_events.push(event_with_seq);
            sequences.push(seq);
        }

        Ok(sequences)
    }

    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>> {
        self.check_error()?;
        let store = self.events.lock().unwrap();
        let events = store.get(child_id).map(|evts| {
            evts.iter()
                .filter(|e| e.sequence.unwrap_or(0) > since_sequence)
                .cloned()
                .collect()
        }).unwrap_or_default();
        Ok(events)
    }

    fn upsert_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str, entity_json: &str) -> Result<()> {
        self.check_error()?;
        let key = Self::entity_key(child_id, &entity_type, entity_id);
        self.entities.lock().unwrap().insert(key, entity_json.to_string());
        Ok(())
    }

    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>> {
        self.check_error()?;
        let key = Self::entity_key(child_id, &entity_type, entity_id);
        Ok(self.entities.lock().unwrap().get(&key).cloned())
    }

    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()> {
        self.check_error()?;
        let key = Self::entity_key(child_id, &entity_type, entity_id);
        self.entities.lock().unwrap().remove(&key);
        Ok(())
    }

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint> {
        self.check_error()?;
        let metadata = self.metadata.lock().unwrap();
        Ok(metadata.get(child_id).cloned().unwrap_or_else(|| SyncCheckpoint::new(child_id.to_string())))
    }

    fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> Result<()> {
        self.check_error()?;
        let mut metadata = self.metadata.lock().unwrap();
        let cp = metadata.entry(child_id.to_string()).or_insert_with(|| SyncCheckpoint::new(child_id.to_string()));
        match which {
            "local" => { if value > cp.local_watermark { cp.local_watermark = value; } }
            "remote" => { if value > cp.remote_watermark { cp.remote_watermark = value; } }
            _ => return Err(anyhow::anyhow!("Unknown watermark: {}", which)),
        }
        Ok(())
    }

    fn initialize_child(&self, child_id: &str) -> Result<()> {
        self.check_error()?;
        let mut metadata = self.metadata.lock().unwrap();
        metadata.entry(child_id.to_string()).or_insert_with(|| SyncCheckpoint::new(child_id.to_string()));
        Ok(())
    }

    fn health_check(&self) -> Result<bool> {
        self.check_error()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::remote::RemoteStorage;

    #[test]
    fn test_push_and_get_events() {
        let mock = MockRemoteClient::new();
        mock.initialize_child("child1").unwrap();

        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        let seqs = mock.push_events(&[event]).unwrap();
        assert_eq!(seqs, vec![1]);

        let events = mock.get_events_since("child1", 0).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, Some(1));
    }

    #[test]
    fn test_deduplication() {
        let mock = MockRemoteClient::new();
        mock.initialize_child("child1").unwrap();

        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        let seq1 = mock.push_events(&[event.clone()]).unwrap();
        let seq2 = mock.push_events(&[event]).unwrap();
        assert_eq!(seq1, seq2);

        let events = mock.get_events_since("child1", 0).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_entity_crud() {
        let mock = MockRemoteClient::new();
        mock.upsert_entity("c1", EntityType::Transaction, "tx1", r#"{"id":"tx1"}"#).unwrap();

        let result = mock.get_entity("c1", EntityType::Transaction, "tx1").unwrap();
        assert_eq!(result, Some(r#"{"id":"tx1"}"#.to_string()));

        mock.delete_entity("c1", EntityType::Transaction, "tx1").unwrap();
        let result = mock.get_entity("c1", EntityType::Transaction, "tx1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_watermark_only_moves_forward() {
        let mock = MockRemoteClient::new();
        mock.initialize_child("child1").unwrap();

        mock.update_watermark("child1", "local", 10).unwrap();
        mock.update_watermark("child1", "local", 5).unwrap();

        let cp = mock.get_checkpoint("child1").unwrap();
        assert_eq!(cp.local_watermark, 10);
    }

    #[test]
    fn test_force_error() {
        let mock = MockRemoteClient::new();
        *mock.force_error.lock().unwrap() = Some("network down".to_string());

        assert!(mock.health_check().is_err());
        assert!(mock.push_events(&[]).is_err());
    }
}
```

- [ ] **Step 2: Declare the module**

Add to `backend/storage/mod.rs`:
```rust
pub mod mock_remote;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p allowance-tracker-egui -- mock_remote`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add backend/storage/mock_remote.rs backend/storage/mod.rs
git commit -m "feat: implement MockRemoteClient for unit testing sync"
```

### Task 12: HttpRemoteClient

**Files:**
- Create: `backend/storage/http_remote.rs`
- Modify: `backend/storage/mod.rs`
- Modify: `egui-frontend/Cargo.toml` (add reqwest)

- [ ] **Step 1: Add reqwest dependency**

Add to `egui-frontend/Cargo.toml` under `[dependencies]`:
```toml
reqwest = { version = "0.12", features = ["json", "blocking"] }
```

- [ ] **Step 2: Implement HttpRemoteClient**

Create `backend/storage/http_remote.rs`:

```rust
use anyhow::Result;
use shared::sync::*;
use super::remote::RemoteStorage;

/// HTTP client implementation of RemoteStorage.
/// Calls the sync-service REST API using reqwest blocking client.
pub struct HttpRemoteClient {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl HttpRemoteClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url,
        }
    }
}

impl RemoteStorage for HttpRemoteClient {
    fn push_events(&self, events: &[SyncEvent]) -> Result<Vec<u64>> {
        let resp = self.client
            .post(format!("{}/sync/events", self.base_url))
            .json(events)
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Push events failed ({}): {}", status, body));
        }

        let sequences: Vec<u64> = resp.json()?;
        Ok(sequences)
    }

    fn get_events_since(&self, child_id: &str, since_sequence: u64) -> Result<Vec<SyncEvent>> {
        let resp = self.client
            .get(format!("{}/sync/events", self.base_url))
            .query(&[("child_id", child_id), ("since", &since_sequence.to_string())])
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Get events failed ({}): {}", status, body));
        }

        let events: Vec<SyncEvent> = resp.json()?;
        Ok(events)
    }

    fn upsert_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str, entity_json: &str) -> Result<()> {
        let resp = self.client
            .put(format!("{}/entities/{}/{}/{}", self.base_url, entity_type.as_str(), child_id, entity_id))
            .header("content-type", "application/json")
            .body(entity_json.to_string())
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Upsert entity failed ({}): {}", status, body));
        }

        Ok(())
    }

    fn get_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<Option<String>> {
        let resp = self.client
            .get(format!("{}/entities/{}/{}/{}", self.base_url, entity_type.as_str(), child_id, entity_id))
            .send()?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Get entity failed ({}): {}", status, body));
        }

        Ok(Some(resp.text()?))
    }

    fn delete_entity(&self, child_id: &str, entity_type: EntityType, entity_id: &str) -> Result<()> {
        let resp = self.client
            .delete(format!("{}/entities/{}/{}/{}", self.base_url, entity_type.as_str(), child_id, entity_id))
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Delete entity failed ({}): {}", status, body));
        }

        Ok(())
    }

    fn get_checkpoint(&self, child_id: &str) -> Result<SyncCheckpoint> {
        let resp = self.client
            .get(format!("{}/sync/checkpoint/{}", self.base_url, child_id))
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Get checkpoint failed ({}): {}", status, body));
        }

        let cp: SyncCheckpoint = resp.json()?;
        Ok(cp)
    }

    fn update_watermark(&self, child_id: &str, which: &str, value: u64) -> Result<()> {
        let resp = self.client
            .put(format!("{}/sync/checkpoint/{}", self.base_url, child_id))
            .json(&serde_json::json!({"which": which, "value": value}))
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Update watermark failed ({}): {}", status, body));
        }

        Ok(())
    }

    fn initialize_child(&self, child_id: &str) -> Result<()> {
        let resp = self.client
            .post(format!("{}/sync/initialize/{}", self.base_url, child_id))
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow::anyhow!("Initialize child failed ({}): {}", status, body));
        }

        Ok(())
    }

    fn health_check(&self) -> Result<bool> {
        let resp = self.client
            .get(format!("{}/health", self.base_url))
            .send()?;

        Ok(resp.status().is_success())
    }
}
```

- [ ] **Step 3: Add serde_json dependency to egui-frontend if not present**

Check `egui-frontend/Cargo.toml`. If `serde_json` is not already listed, add:
```toml
serde_json = "1.0"
```

- [ ] **Step 4: Declare the module**

Add to `backend/storage/mod.rs`:
```rust
pub mod http_remote;
```

And add re-export:
```rust
pub use http_remote::HttpRemoteClient;
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p allowance-tracker-egui`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add backend/storage/http_remote.rs backend/storage/mod.rs egui-frontend/Cargo.toml
git commit -m "feat: implement HttpRemoteClient for sync-service REST API"
```

---

## Phase 4: SyncManager & Local Integration

### Task 13: SyncNotifier channel

**Files:**
- Create: `backend/domain/sync_notifier.rs`
- Modify: `backend/domain/mod.rs`

- [ ] **Step 1: Write tests and implementation**

Create `backend/domain/sync_notifier.rs`:

```rust
use shared::sync::*;
use std::sync::mpsc;

/// Non-blocking sender for sync events. Injected into repositories.
/// Sending is best-effort: if the channel is full or disconnected,
/// the event is dropped with a warning log. Local writes must never
/// fail because of sync.
#[derive(Clone)]
pub struct SyncNotifier {
    tx: mpsc::Sender<SyncEvent>,
}

impl SyncNotifier {
    pub fn new(tx: mpsc::Sender<SyncEvent>) -> Self {
        Self { tx }
    }

    pub fn notify(&self, event: SyncEvent) {
        if let Err(e) = self.tx.send(event) {
            log::warn!("Failed to send sync event (channel disconnected): {}", e);
        }
    }
}

/// Create a (SyncNotifier, Receiver) pair.
pub fn sync_channel() -> (SyncNotifier, mpsc::Receiver<SyncEvent>) {
    let (tx, rx) = mpsc::channel();
    (SyncNotifier::new(tx), rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notify_sends_event() {
        let (notifier, rx) = sync_channel();

        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        notifier.notify(event.clone());

        let received = rx.recv().unwrap();
        assert_eq!(received.event_id, event.event_id);
        assert_eq!(received.entity_id, "tx1");
    }

    #[test]
    fn test_notify_on_disconnected_channel_does_not_panic() {
        let (tx, rx) = mpsc::channel::<SyncEvent>();
        let notifier = SyncNotifier::new(tx);
        drop(rx); // disconnect the receiver

        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        // Should not panic
        notifier.notify(event);
    }

    #[test]
    fn test_notifier_is_clone() {
        let (notifier, rx) = sync_channel();
        let notifier2 = notifier.clone();

        let event = SyncEvent::new(
            EntityType::Transaction,
            "tx1".to_string(),
            "child1".to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );

        notifier2.notify(event);
        let _ = rx.recv().unwrap();
    }
}
```

- [ ] **Step 2: Declare the module**

Add to `backend/domain/mod.rs`:
```rust
pub mod sync_notifier;
```

And add re-export:
```rust
pub use sync_notifier::{SyncNotifier, sync_channel};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p allowance-tracker-egui -- sync_notifier`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add backend/domain/sync_notifier.rs backend/domain/mod.rs
git commit -m "feat: implement SyncNotifier channel for repository -> sync thread communication"
```

### Task 14: SyncManager core logic

**Files:**
- Create: `backend/domain/sync_manager.rs`
- Modify: `backend/domain/mod.rs`

- [ ] **Step 1: Write the SyncMessage enum and SyncManager struct with tests**

Create `backend/domain/sync_manager.rs`:

```rust
use anyhow::Result;
use shared::sync::*;
use crate::backend::storage::remote::RemoteStorage;
use std::sync::{Arc, mpsc, atomic::{AtomicBool, Ordering}};
use std::collections::HashMap;
use std::time::Duration;

/// Messages from the sync background thread to the UI.
#[derive(Debug, Clone)]
pub enum SyncMessage {
    StatusChanged(SyncStatus),
    EntitiesUpdated { child_id: String, entity_type: EntityType, count: usize },
    ConflictDetected(SyncConflict),
    PushFailed { event_id: String, error: String },
    Error(String),
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
        *mock.force_error.lock().unwrap() = Some("network error".to_string());
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
}
```

- [ ] **Step 2: Declare the module**

Add to `backend/domain/mod.rs`:
```rust
pub mod sync_manager;
```

And add re-exports:
```rust
pub use sync_manager::{SyncEngine, SyncMessage, SyncStatus, PollResult};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p allowance-tracker-egui -- sync_manager`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add backend/domain/sync_manager.rs backend/domain/mod.rs
git commit -m "feat: implement SyncEngine with conflict detection, resolution, and watermark management"
```

### Task 15: Background sync thread

**Files:**
- Create: `backend/domain/sync_thread.rs`
- Modify: `backend/domain/mod.rs`

- [ ] **Step 1: Implement the background thread**

Create `backend/domain/sync_thread.rs`:

```rust
use shared::sync::*;
use crate::backend::storage::remote::RemoteStorage;
use super::sync_manager::{SyncEngine, SyncMessage, SyncStatus};
use std::sync::{Arc, mpsc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

const BASE_POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_POLL_INTERVAL: Duration = Duration::from_secs(300);

/// Handle to the background sync thread. Drop to shut down.
pub struct SyncThreadHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SyncThreadHandle {
    /// Spawn the background sync thread.
    /// `event_rx`: receives SyncEvents from SyncNotifier (domain writes).
    /// `message_tx`: sends SyncMessages to the UI thread.
    /// `child_ids`: list of children to poll for.
    pub fn spawn(
        remote: Arc<dyn RemoteStorage>,
        event_rx: mpsc::Receiver<SyncEvent>,
        message_tx: mpsc::Sender<SyncMessage>,
        child_ids: Vec<String>,
    ) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();

        let thread = std::thread::Builder::new()
            .name("sync-thread".to_string())
            .spawn(move || {
                sync_loop(remote, event_rx, message_tx, child_ids, shutdown_flag);
            })
            .expect("Failed to spawn sync thread");

        Self {
            shutdown,
            thread: Some(thread),
        }
    }

    /// Signal the thread to shut down and wait for it.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for SyncThreadHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn sync_loop(
    remote: Arc<dyn RemoteStorage>,
    event_rx: mpsc::Receiver<SyncEvent>,
    message_tx: mpsc::Sender<SyncMessage>,
    child_ids: Vec<String>,
    shutdown: Arc<AtomicBool>,
) {
    let mut engine = SyncEngine::new(remote);
    let mut poll_interval = BASE_POLL_INTERVAL;
    let mut had_activity;

    while !shutdown.load(Ordering::Relaxed) {
        had_activity = false;

        // 1. Drain any new local events from the notifier channel
        while let Ok(event) = event_rx.try_recv() {
            engine.enqueue_event(event);
            had_activity = true;
        }

        // 2. Push pending events
        let _ = message_tx.send(SyncMessage::StatusChanged(SyncStatus::Syncing));

        let failures = engine.push_pending();
        for (event, error) in &failures {
            let _ = message_tx.send(SyncMessage::PushFailed {
                event_id: event.event_id.clone(),
                error: error.clone(),
            });
            // Re-enqueue for retry
            engine.enqueue_event(event.clone());
        }

        // 3. Poll each child for remote events
        for child_id in &child_ids {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            match engine.poll_child(child_id) {
                Ok(result) => {
                    if !result.events_to_apply.is_empty() {
                        had_activity = true;
                        let _ = message_tx.send(SyncMessage::EntitiesUpdated {
                            child_id: child_id.clone(),
                            entity_type: result.events_to_apply[0].entity_type.clone(),
                            count: result.events_to_apply.len(),
                        });
                    }
                    for conflict in result.new_conflicts {
                        let _ = message_tx.send(SyncMessage::ConflictDetected(conflict));
                    }
                }
                Err(e) => {
                    let _ = message_tx.send(SyncMessage::Error(format!(
                        "Poll failed for {}: {}", child_id, e
                    )));
                }
            }
        }

        // 4. Update status
        let conflict_count = engine.pending_conflicts().iter()
            .filter(|c| c.status == ConflictStatus::Pending)
            .count();

        let status = if conflict_count > 0 {
            SyncStatus::HasConflicts(conflict_count)
        } else {
            SyncStatus::Idle
        };
        let _ = message_tx.send(SyncMessage::StatusChanged(status));

        // 5. Backoff
        if had_activity {
            poll_interval = BASE_POLL_INTERVAL;
        } else {
            poll_interval = (poll_interval * 2).min(MAX_POLL_INTERVAL);
        }

        // Sleep in small increments so we can check shutdown flag
        let sleep_end = std::time::Instant::now() + poll_interval;
        while std::time::Instant::now() < sleep_end && !shutdown.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::storage::mock_remote::MockRemoteClient;
    use std::time::Duration;

    #[test]
    fn test_thread_starts_and_shuts_down() {
        let mock = Arc::new(MockRemoteClient::new());
        let (_, event_rx) = mpsc::channel();
        let (message_tx, _message_rx) = mpsc::channel();

        let mut handle = SyncThreadHandle::spawn(
            mock,
            event_rx,
            message_tx,
            vec![],
        );

        // Give the thread a moment to start
        std::thread::sleep(Duration::from_millis(100));

        handle.shutdown();
        // If we get here without hanging, the test passes
    }

    #[test]
    fn test_thread_pushes_local_events() {
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();

        let (event_tx, event_rx) = mpsc::channel();
        let (message_tx, message_rx) = mpsc::channel();

        let mut handle = SyncThreadHandle::spawn(
            mock.clone(),
            event_rx,
            message_tx,
            vec!["child1".to_string()],
        );

        // Send a local event
        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );
        event_tx.send(event).unwrap();

        // Wait for the sync cycle to process it
        std::thread::sleep(Duration::from_millis(500));

        // Verify the event was pushed to remote
        let events = mock.get_events_since("child1", 0).unwrap();
        assert_eq!(events.len(), 1);

        handle.shutdown();
    }

    #[test]
    fn test_thread_detects_remote_events() {
        let mock = Arc::new(MockRemoteClient::new());
        mock.initialize_child("child1").unwrap();

        let (_, event_rx) = mpsc::channel();
        let (message_tx, message_rx) = mpsc::channel();

        // Push a remote event before starting the thread
        let remote_event = SyncEvent::new(
            EntityType::Transaction, "tx_remote".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Remote,
        );
        mock.push_events(&[remote_event]).unwrap();

        let mut handle = SyncThreadHandle::spawn(
            mock,
            event_rx,
            message_tx,
            vec!["child1".to_string()],
        );

        // Wait for sync cycle
        std::thread::sleep(Duration::from_millis(500));

        // Drain messages looking for EntitiesUpdated
        let mut found_update = false;
        while let Ok(msg) = message_rx.try_recv() {
            if let SyncMessage::EntitiesUpdated { child_id, count, .. } = msg {
                if child_id == "child1" && count == 1 {
                    found_update = true;
                }
            }
        }
        assert!(found_update, "Expected EntitiesUpdated message from sync thread");

        handle.shutdown();
    }
}
```

- [ ] **Step 2: Declare the module**

Add to `backend/domain/mod.rs`:
```rust
pub mod sync_thread;
```

And add re-export:
```rust
pub use sync_thread::SyncThreadHandle;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p allowance-tracker-egui -- sync_thread`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add backend/domain/sync_thread.rs backend/domain/mod.rs
git commit -m "feat: implement background sync thread with polling, push, and backoff"
```

### Task 16: Sync state persistence (YAML)

**Files:**
- Create: `backend/domain/sync_persistence.rs`
- Modify: `backend/domain/mod.rs`

- [ ] **Step 1: Write tests and implementation**

Create `backend/domain/sync_persistence.rs`:

```rust
use anyhow::Result;
use shared::sync::SyncEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SyncState {
    /// Per-child local watermark (last sequence we've processed).
    pub watermarks: HashMap<String, u64>,
    /// Whether sync is enabled.
    pub enabled: bool,
    /// Remote service URL.
    pub remote_url: Option<String>,
}

impl SyncState {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let state: SyncState = serde_yaml::from_str(&contents)?;
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RetryQueue {
    pub events: Vec<SyncEvent>,
}

impl RetryQueue {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        if contents.trim().is_empty() {
            return Ok(Self::default());
        }
        let queue: RetryQueue = serde_yaml::from_str(&contents)?;
        Ok(queue)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

/// Standard file names for sync persistence.
pub fn sync_state_path(base_dir: &Path) -> PathBuf {
    base_dir.join("sync_state.yaml")
}

pub fn retry_queue_path(base_dir: &Path) -> PathBuf {
    base_dir.join("sync_retry_queue.yaml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::sync::*;
    use tempfile::TempDir;

    #[test]
    fn test_sync_state_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = sync_state_path(dir.path());

        let mut state = SyncState::default();
        state.enabled = true;
        state.remote_url = Some("http://localhost:3030".to_string());
        state.watermarks.insert("child1".to_string(), 42);

        state.save(&path).unwrap();

        let loaded = SyncState::load(&path).unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.remote_url.unwrap(), "http://localhost:3030");
        assert_eq!(*loaded.watermarks.get("child1").unwrap(), 42);
    }

    #[test]
    fn test_sync_state_load_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = sync_state_path(dir.path());

        let state = SyncState::load(&path).unwrap();
        assert!(!state.enabled);
        assert!(state.remote_url.is_none());
    }

    #[test]
    fn test_retry_queue_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = retry_queue_path(dir.path());

        let event = SyncEvent::new(
            EntityType::Transaction, "tx1".to_string(), "child1".to_string(),
            SyncAction::Created, SyncSource::Local,
        );

        let queue = RetryQueue { events: vec![event.clone()] };
        queue.save(&path).unwrap();

        let loaded = RetryQueue::load(&path).unwrap();
        assert_eq!(loaded.events.len(), 1);
        assert_eq!(loaded.events[0].event_id, event.event_id);
    }

    #[test]
    fn test_retry_queue_load_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = retry_queue_path(dir.path());

        let queue = RetryQueue::load(&path).unwrap();
        assert!(queue.events.is_empty());
    }
}
```

- [ ] **Step 2: Declare the module**

Add to `backend/domain/mod.rs`:
```rust
pub mod sync_persistence;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p allowance-tracker-egui -- sync_persistence`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add backend/domain/sync_persistence.rs backend/domain/mod.rs
git commit -m "feat: implement YAML persistence for sync state and retry queue"
```

---

## Phase 5: UI Integration (Placeholder)

### Task 17: Add SyncStatus to app state

**Files:**
- Modify: `egui-frontend/src/ui/app_state.rs`
- Modify: `egui-frontend/src/ui/state/` (add sync state module)

This task wires the sync thread into the app lifecycle without building the full conflict modal yet. The conflict modal is a UI-heavy task that should be its own plan.

- [ ] **Step 1: Create sync UI state module**

Create `egui-frontend/src/ui/state/sync_state.rs`:

```rust
use crate::backend::domain::sync_manager::{SyncMessage, SyncStatus};
use shared::sync::SyncConflict;
use std::sync::mpsc;

pub struct SyncUiState {
    pub status: SyncStatus,
    pub conflicts: Vec<SyncConflict>,
    pub message_rx: Option<mpsc::Receiver<SyncMessage>>,
}

impl SyncUiState {
    pub fn new() -> Self {
        Self {
            status: SyncStatus::Disabled,
            conflicts: Vec::new(),
            message_rx: None,
        }
    }

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
            }
        }
    }

    pub fn pending_conflict_count(&self) -> usize {
        self.conflicts.iter()
            .filter(|c| c.status == shared::sync::ConflictStatus::Pending)
            .count()
    }
}
```

- [ ] **Step 2: Declare the module in state/mod.rs**

Add to `egui-frontend/src/ui/state/mod.rs` (check exact file — may need to find the state module declarations):
```rust
pub mod sync_state;
pub use sync_state::SyncUiState;
```

- [ ] **Step 3: Add SyncUiState to AllowanceTrackerApp**

In `egui-frontend/src/ui/app_state.rs`, add the field to the `AllowanceTrackerApp` struct:
```rust
pub sync: SyncUiState,
```

And in the `new()` constructor, add:
```rust
sync: SyncUiState::new(),
```

- [ ] **Step 4: Add poll_messages to the update loop**

In the `eframe::App` impl for `AllowanceTrackerApp`, find the `update` method and add near the top:
```rust
self.sync.poll_messages();
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p allowance-tracker-egui`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add egui-frontend/src/ui/state/ egui-frontend/src/ui/app_state.rs
git commit -m "feat: wire SyncUiState into app state with message polling"
```

---

## Summary

| Phase | Tasks | What it delivers |
|-------|-------|-----------------|
| 1 | Task 1 | Shared sync types used by all layers |
| 2 | Tasks 2-9 | Complete sync-service with DDB storage, REST API, and concurrency tests |
| 3 | Tasks 10-12 | RemoteStorage trait with Mock, HTTP, and (later) InProcess implementations |
| 4 | Tasks 13-16 | SyncManager, background thread, notifier, YAML persistence |
| 5 | Task 17 | UI wiring (sync status in app state, message polling) |

**Not in this plan (future work):**
- Conflict resolution modal (UI-heavy, deserves its own plan)
- InProcessRemoteClient implementation (needs sync-service as library dependency in egui-frontend)
- Integration of SyncNotifier into existing repositories
- End-to-end tests (full app + DynamoDB Local)
- Sync settings UI (enable/disable, remote URL configuration)
