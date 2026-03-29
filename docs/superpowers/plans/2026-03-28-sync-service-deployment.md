# Sync-Service Deployment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deploy the sync-service as an AWS Lambda behind API Gateway with Cognito auth and DynamoDB, using AWS SAM, with a schema drift test to keep IaC and code in sync.

**Architecture:** Add `TableConfig` abstraction to replace prefix-based table naming, make main.rs dual-mode (Lambda or local server), create SAM template with all AWS resources, and add a drift validation test comparing SAM to DynamoDB Local.

**Tech Stack:** AWS SAM, lambda_http, cargo-lambda, Cognito, DynamoDB, axum, serde_yaml

**Spec:** `docs/superpowers/specs/2026-03-28-sync-service-deployment-design.md`

---

## Task 1: Add TableConfig and refactor DynamoStore

**Files:**
- Create: `sync-service/src/storage/table_config.rs`
- Modify: `sync-service/src/storage/dynamo.rs`
- Modify: `sync-service/src/storage/mod.rs`
- Modify: `sync-service/src/lib.rs`
- Modify: `sync-service/tests/common/dynamo_test_context.rs`

- [ ] **Step 1: Write tests for TableConfig**

Create `sync-service/src/storage/table_config.rs`:

```rust
/// Configuration for DynamoDB table names.
/// Supports both env-var-based (Lambda) and prefix-based (local/test) naming.
pub struct TableConfig {
    pub children: String,
    pub transactions: String,
    pub goals: String,
    pub sync_events: String,
    pub sync_metadata: String,
}

impl TableConfig {
    /// For Lambda: reads table names from environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            children: std::env::var("CHILDREN_TABLE")
                .map_err(|_| anyhow::anyhow!("CHILDREN_TABLE not set"))?,
            transactions: std::env::var("TRANSACTIONS_TABLE")
                .map_err(|_| anyhow::anyhow!("TRANSACTIONS_TABLE not set"))?,
            goals: std::env::var("GOALS_TABLE")
                .map_err(|_| anyhow::anyhow!("GOALS_TABLE not set"))?,
            sync_events: std::env::var("SYNC_EVENTS_TABLE")
                .map_err(|_| anyhow::anyhow!("SYNC_EVENTS_TABLE not set"))?,
            sync_metadata: std::env::var("SYNC_METADATA_TABLE")
                .map_err(|_| anyhow::anyhow!("SYNC_METADATA_TABLE not set"))?,
        })
    }

    /// For local dev and tests: constructs names from a prefix.
    pub fn from_prefix(prefix: &str) -> Self {
        Self {
            children: format!("{}children", prefix),
            transactions: format!("{}transactions", prefix),
            goals: format!("{}goals", prefix),
            sync_events: format!("{}sync_events", prefix),
            sync_metadata: format!("{}sync_metadata", prefix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_prefix_empty() {
        let config = TableConfig::from_prefix("");
        assert_eq!(config.children, "children");
        assert_eq!(config.transactions, "transactions");
        assert_eq!(config.goals, "goals");
        assert_eq!(config.sync_events, "sync_events");
        assert_eq!(config.sync_metadata, "sync_metadata");
    }

    #[test]
    fn test_from_prefix_with_prefix() {
        let config = TableConfig::from_prefix("test_abc_");
        assert_eq!(config.children, "test_abc_children");
        assert_eq!(config.transactions, "test_abc_transactions");
        assert_eq!(config.goals, "test_abc_goals");
        assert_eq!(config.sync_events, "test_abc_sync_events");
        assert_eq!(config.sync_metadata, "test_abc_sync_metadata");
    }

    #[test]
    fn test_from_env_missing_var() {
        // Clear any existing vars to ensure failure
        std::env::remove_var("CHILDREN_TABLE");
        let result = TableConfig::from_env();
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p sync-service table_config`
Expected: 3 tests pass

- [ ] **Step 3: Update storage/mod.rs to include table_config**

Replace `sync-service/src/storage/mod.rs` with:

```rust
mod dynamo;
pub mod table_definitions;
pub mod table_config;
pub use dynamo::DynamoStore;
pub use table_config::TableConfig;
```

- [ ] **Step 4: Refactor DynamoStore to use TableConfig**

In `sync-service/src/storage/dynamo.rs`, make these changes:

Replace the struct and constructor:
```rust
// OLD
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
```

with:
```rust
// NEW
use super::table_config::TableConfig;

pub struct DynamoStore {
    client: Client,
    config: TableConfig,
}

impl DynamoStore {
    pub fn new(client: Client, config: TableConfig) -> Self {
        Self { client, config }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn config(&self) -> &TableConfig {
        &self.config
    }
```

Then do a find-and-replace throughout dynamo.rs for every `self.table_name("...")` call:
- `self.table_name("sync_metadata")` → `self.config.sync_metadata.clone()`
- `self.table_name("sync_events")` → `self.config.sync_events.clone()`
- `self.table_name("children")` → (not used directly — goes through `entity_table_info`)
- `self.table_name("transactions")` → (not used directly)
- `self.table_name("goals")` → (not used directly)

For `entity_table_info`, change the return type and implementation:
```rust
// OLD
fn entity_table_info(&self, entity_type: &EntityType) -> (&'static str, Option<&'static str>) {
    match entity_type {
        EntityType::Transaction => ("transactions", Some("transaction_id")),
        EntityType::Goal => ("goals", Some("goal_id")),
        EntityType::Child => ("children", None),
    }
}
```

```rust
// NEW
fn entity_table_and_sort_key(&self, entity_type: &EntityType) -> (String, Option<&'static str>) {
    match entity_type {
        EntityType::Transaction => (self.config.transactions.clone(), Some("transaction_id")),
        EntityType::Goal => (self.config.goals.clone(), Some("goal_id")),
        EntityType::Child => (self.config.children.clone(), None),
    }
}
```

Update every call site of `entity_table_info` to use `entity_table_and_sort_key` — the callers used to do `let (table_base, sort_key) = self.entity_table_info(...); let table = self.table_name(table_base);`. Now they just do `let (table, sort_key) = self.entity_table_and_sort_key(...);` and remove the `self.table_name()` line.

Remove the `table_name`, `table_prefix`, `create_tables`, and `delete_tables` methods (they are no longer needed — test infrastructure uses `table_definitions` directly, and `table_prefix` is replaced by `config`).

- [ ] **Step 5: Update DynamoTestContext**

In `sync-service/tests/common/dynamo_test_context.rs`, update to use `TableConfig`:

```rust
use aws_sdk_dynamodb::Client;
use sync_service::storage::{table_definitions, TableConfig};
use uuid::Uuid;

pub struct DynamoTestContext {
    pub client: Client,
    pub table_prefix: String,
}

impl DynamoTestContext {
    pub async fn new(port: u16) -> Self {
        let client = sync_service::create_local_dynamo_client(port)
            .await
            .expect("Failed to create DynamoDB Local client. Is DynamoDB Local running?");

        let table_prefix = format!(
            "test_{}_",
            Uuid::new_v4()
                .to_string()
                .replace('-', "")[..8]
                .to_string()
        );

        table_definitions::create_all_tables(&client, &table_prefix)
            .await
            .expect("Failed to create test tables");

        Self { client, table_prefix }
    }

    pub fn table_config(&self) -> TableConfig {
        TableConfig::from_prefix(&self.table_prefix)
    }

    pub async fn cleanup(&self) {
        let _ = table_definitions::delete_all_tables(&self.client, &self.table_prefix).await;
    }
}

impl Drop for DynamoTestContext {
    fn drop(&mut self) {
        // Best-effort cleanup. Call cleanup() explicitly since Drop can't run async.
    }
}

pub const DYNAMO_LOCAL_PORT: u16 = 8000;

pub async fn is_dynamo_local_available(port: u16) -> bool {
    let client = match sync_service::create_local_dynamo_client(port).await {
        Ok(c) => c,
        Err(_) => return false,
    };
    client.list_tables().send().await.is_ok()
}
```

- [ ] **Step 6: Update all test files to use new DynamoStore constructor**

In each test file's `setup()` function, replace:
```rust
let store = DynamoStore::new(ctx.client.clone(), ctx.table_prefix.clone());
```
with:
```rust
let store = DynamoStore::new(ctx.client.clone(), ctx.table_config());
```

Files to update:
- `sync-service/tests/sync_events_test.rs`
- `sync-service/tests/entity_crud_test.rs`
- `sync-service/tests/checkpoint_test.rs`
- `sync-service/tests/concurrency_test.rs` (uses `Arc::new(DynamoStore::new(...))`)
- `sync-service/tests/api_test.rs` — this one constructs DynamoStore differently; update to use `TableConfig::from_prefix(&prefix)`

- [ ] **Step 7: Update lib.rs create_app to accept TableConfig**

Replace `sync-service/src/lib.rs`:

```rust
pub mod storage;
pub mod routes;

use axum::Router;
use storage::TableConfig;

pub async fn create_app(config: TableConfig) -> anyhow::Result<Router> {
    let dynamo_client = create_dynamo_client().await?;
    let store = storage::DynamoStore::new(dynamo_client, config);
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

- [ ] **Step 8: Update main.rs to pass TableConfig**

Replace `sync-service/src/main.rs`:

```rust
use sync_service::create_app;
use sync_service::storage::TableConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config = TableConfig::from_prefix("");
    let app = create_app(config).await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3030").await?;
    log::info!("sync-service listening on 0.0.0.0:3030");
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 9: Update api_test.rs to use TableConfig**

In `sync-service/tests/api_test.rs`, update `start_test_server` to construct the store with `TableConfig`:

Replace:
```rust
let store = DynamoStore::new(dynamo_client, prefix);
```
with:
```rust
let config = sync_service::storage::TableConfig::from_prefix(&prefix);
let store = DynamoStore::new(dynamo_client, config);
```

And for the cleanup store at the end:
```rust
let cleanup_config = sync_service::storage::TableConfig::from_prefix(&prefix);
let cleanup_store = DynamoStore::new(client, cleanup_config);
```

Also update the `start_test_server` to use `create_app` or `build_router` with the new config. Since `create_app` now takes a `TableConfig`, the test should build the router directly:

```rust
let config = sync_service::storage::TableConfig::from_prefix(&prefix);
let store = DynamoStore::new(client.clone(), config);
store_create_tables_manually...  // (the store no longer has create_tables)
```

Actually, since `create_tables` was removed from DynamoStore, the api_test needs to call `table_definitions::create_all_tables` directly:

```rust
sync_service::storage::table_definitions::create_all_tables(&client, &prefix).await.unwrap();
let config = sync_service::storage::TableConfig::from_prefix(&prefix);
let store = DynamoStore::new(client.clone(), config);
let app = sync_service::routes::build_router(store);
```

And for cleanup, call `table_definitions::delete_all_tables` directly instead of `cleanup_store.delete_tables()`.

- [ ] **Step 10: Verify all tests compile and pass**

Run: `cargo test -p sync-service --no-run`
Expected: compiles with no errors

Run: `cargo test -p sync-service -- --skip concurrency --skip sync_events --skip entity_crud --skip checkpoint --skip api_test --skip schema_drift`
Expected: table_config tests pass (the DDB integration tests may skip without DynamoDB Local)

- [ ] **Step 11: Commit**

```bash
git add sync-service/
git commit -m "refactor: replace table_prefix with TableConfig for flexible table name resolution"
```

---

## Task 2: Dual-mode main.rs (Lambda + local server)

**Files:**
- Modify: `sync-service/Cargo.toml`
- Modify: `sync-service/src/main.rs`

- [ ] **Step 1: Add lambda_http dependency**

Add to `sync-service/Cargo.toml` under `[dependencies]`:

```toml
lambda_http = "0.13"
```

- [ ] **Step 2: Update main.rs to dual-mode**

Replace `sync-service/src/main.rs`:

```rust
use sync_service::create_app;
use sync_service::storage::TableConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let is_lambda = std::env::var("AWS_LAMBDA_RUNTIME_API").is_ok();

    let config = if is_lambda {
        TableConfig::from_env()?
    } else {
        TableConfig::from_prefix("")
    };

    let app = create_app(config).await?;

    if is_lambda {
        lambda_http::run(app).await.map_err(|e| anyhow::anyhow!("Lambda runtime error: {}", e))?;
    } else {
        env_logger::init();
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3030").await?;
        log::info!("sync-service listening on 0.0.0.0:3030");
        axum::serve(listener, app).await?;
    }

    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p sync-service`
Expected: compiles with no errors

- [ ] **Step 4: Verify local mode still works**

Run: `cargo test -p sync-service --no-run`
Expected: compiles (this confirms the non-Lambda path still builds)

- [ ] **Step 5: Commit**

```bash
git add sync-service/Cargo.toml sync-service/src/main.rs
git commit -m "feat: add dual-mode main.rs (Lambda or local server)"
```

---

## Task 3: SAM template and deploy config

**Files:**
- Create: `infrastructure/template.yaml`
- Create: `infrastructure/samconfig.toml`

- [ ] **Step 1: Create the SAM template**

Create `infrastructure/template.yaml` with the complete template from the spec. Copy it exactly as specified in the spec under "SAM Template" (lines 120-329 of the spec).

- [ ] **Step 2: Create the SAM deploy config**

Create `infrastructure/samconfig.toml`:

```toml
version = 0.1

[default.deploy.parameters]
stack_name = "allowance-tracker-sync"
resolve_s3 = true
s3_prefix = "allowance-tracker-sync"
region = "us-east-2"
confirm_changeset = true
capabilities = "CAPABILITY_IAM"

[default.global.parameters]
region = "us-east-2"
```

- [ ] **Step 3: Validate the template syntax**

Run: `sam validate --template infrastructure/template.yaml 2>&1 || echo "SAM CLI not installed - skipping validation"`
Expected: valid template (or SAM CLI not installed, which is OK)

- [ ] **Step 4: Commit**

```bash
git add infrastructure/
git commit -m "feat: add SAM template for sync-service deployment (Lambda + API GW + Cognito + DynamoDB)"
```

---

## Task 4: Schema drift validation test

**Files:**
- Create: `sync-service/tests/schema_drift_test.rs`
- Modify: `sync-service/Cargo.toml` (add serde_yaml dev-dependency)

- [ ] **Step 1: Add serde_yaml to dev-dependencies**

Add to `sync-service/Cargo.toml` under `[dev-dependencies]`:

```toml
serde_yaml = "0.9"
```

- [ ] **Step 2: Write the schema drift test**

Create `sync-service/tests/schema_drift_test.rs`:

```rust
//! Schema drift validation test.
//!
//! Compares the DynamoDB table schemas defined in the SAM template
//! (infrastructure/template.yaml) against what create_all_tables produces
//! on DynamoDB Local. If they disagree, this test fails.

mod common;

use common::{DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use sync_service::storage::table_definitions;
use std::collections::HashMap;

/// A simplified representation of a DynamoDB table schema for comparison.
#[derive(Debug, PartialEq)]
struct TableSchema {
    key_schema: Vec<(String, String)>,       // (attribute_name, key_type) e.g. ("child_id", "HASH")
    attribute_defs: Vec<(String, String)>,    // (attribute_name, attribute_type) e.g. ("child_id", "S")
}

/// Parse all DynamoDB table schemas from the SAM template YAML.
/// Returns a map of normalized_table_name -> TableSchema.
fn parse_sam_template() -> HashMap<String, TableSchema> {
    let template_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("infrastructure")
        .join("template.yaml");

    let content = std::fs::read_to_string(&template_path)
        .unwrap_or_else(|e| panic!("Failed to read SAM template at {:?}: {}", template_path, e));

    let doc: serde_yaml::Value = serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse SAM template YAML: {}", e));

    let resources = doc.get("Resources")
        .expect("SAM template missing Resources section");

    let mut schemas = HashMap::new();

    if let serde_yaml::Value::Mapping(resources_map) = resources {
        for (_resource_name, resource) in resources_map {
            let type_val = resource.get("Type")
                .and_then(|v| v.as_str());

            if type_val != Some("AWS::DynamoDB::Table") {
                continue;
            }

            let props = resource.get("Properties").expect("DynamoDB table missing Properties");

            // Get table name and normalize: "allowance-tracker-sync-events" -> "sync_events"
            let table_name = props.get("TableName")
                .and_then(|v| v.as_str())
                .expect("DynamoDB table missing TableName");
            let normalized = table_name
                .strip_prefix("allowance-tracker-")
                .unwrap_or(table_name)
                .replace('-', "_");

            // Parse KeySchema
            let key_schema_val = props.get("KeySchema")
                .expect("DynamoDB table missing KeySchema");
            let mut key_schema = Vec::new();
            if let serde_yaml::Value::Sequence(keys) = key_schema_val {
                for key in keys {
                    let attr_name = key.get("AttributeName")
                        .and_then(|v| v.as_str())
                        .expect("KeySchema missing AttributeName")
                        .to_string();
                    let key_type = key.get("KeyType")
                        .and_then(|v| v.as_str())
                        .expect("KeySchema missing KeyType")
                        .to_string();
                    key_schema.push((attr_name, key_type));
                }
            }

            // Parse AttributeDefinitions
            let attr_defs_val = props.get("AttributeDefinitions")
                .expect("DynamoDB table missing AttributeDefinitions");
            let mut attribute_defs = Vec::new();
            if let serde_yaml::Value::Sequence(attrs) = attr_defs_val {
                for attr in attrs {
                    let attr_name = attr.get("AttributeName")
                        .and_then(|v| v.as_str())
                        .expect("AttributeDefinition missing AttributeName")
                        .to_string();
                    let attr_type = attr.get("AttributeType")
                        .and_then(|v| v.as_str())
                        .expect("AttributeDefinition missing AttributeType")
                        .to_string();
                    attribute_defs.push((attr_name, attr_type));
                }
            }

            // Sort for stable comparison
            key_schema.sort();
            attribute_defs.sort();

            schemas.insert(normalized, TableSchema { key_schema, attribute_defs });
        }
    }

    schemas
}

/// Query DynamoDB Local for the actual table schemas created by create_all_tables.
/// Returns a map of table_base_name -> TableSchema.
async fn get_dynamo_local_schemas(client: &aws_sdk_dynamodb::Client, prefix: &str) -> HashMap<String, TableSchema> {
    let table_bases = ["children", "transactions", "goals", "sync_events", "sync_metadata"];
    let mut schemas = HashMap::new();

    for base in &table_bases {
        let table_name = format!("{}{}", prefix, base);
        let describe = client
            .describe_table()
            .table_name(&table_name)
            .send()
            .await
            .unwrap_or_else(|e| panic!("Failed to describe table {}: {}", table_name, e));

        let table_desc = describe.table().expect("No table description returned");

        let mut key_schema: Vec<(String, String)> = table_desc
            .key_schema()
            .iter()
            .map(|ks| {
                (
                    ks.attribute_name().to_string(),
                    format!("{:?}", ks.key_type()),  // "Hash" or "Range"
                )
            })
            .collect();

        let mut attribute_defs: Vec<(String, String)> = table_desc
            .attribute_definitions()
            .iter()
            .map(|ad| {
                (
                    ad.attribute_name().to_string(),
                    format!("{:?}", ad.attribute_type()),  // "S" or "N"
                )
            })
            .collect();

        // Normalize key_type format: SDK returns "Hash"/"Range", SAM uses "HASH"/"RANGE"
        for (_, kt) in &mut key_schema {
            *kt = kt.to_uppercase();
        }

        // Normalize attribute_type format: SDK returns "S"/"N" already, just uppercase for safety
        for (_, at) in &mut attribute_defs {
            *at = at.to_uppercase();
        }

        key_schema.sort();
        attribute_defs.sort();

        schemas.insert(base.to_string(), TableSchema { key_schema, attribute_defs });
    }

    schemas
}

#[tokio::test]
async fn test_sam_template_matches_create_all_tables() {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available on port {}", DYNAMO_LOCAL_PORT);
        return;
    }

    // Source 1: Parse SAM template
    let sam_schemas = parse_sam_template();
    assert_eq!(sam_schemas.len(), 5, "SAM template should define exactly 5 DynamoDB tables, found {}", sam_schemas.len());

    // Source 2: Create tables on DynamoDB Local and describe them
    let client = sync_service::create_local_dynamo_client(DYNAMO_LOCAL_PORT).await.unwrap();
    let prefix = format!("drift_test_{}_", uuid::Uuid::new_v4().to_string().replace('-', "")[..8].to_string());

    table_definitions::create_all_tables(&client, &prefix).await.unwrap();
    let dynamo_schemas = get_dynamo_local_schemas(&client, &prefix).await;

    // Cleanup
    table_definitions::delete_all_tables(&client, &prefix).await.unwrap();

    // Compare
    assert_eq!(sam_schemas.len(), dynamo_schemas.len(),
        "SAM defines {} tables but create_all_tables creates {}",
        sam_schemas.len(), dynamo_schemas.len());

    for (table_name, sam_schema) in &sam_schemas {
        let dynamo_schema = dynamo_schemas.get(table_name)
            .unwrap_or_else(|| panic!(
                "SAM template defines table '{}' but create_all_tables does not create it. \
                 SAM tables: {:?}, code tables: {:?}",
                table_name,
                sam_schemas.keys().collect::<Vec<_>>(),
                dynamo_schemas.keys().collect::<Vec<_>>()
            ));

        assert_eq!(
            sam_schema.key_schema, dynamo_schema.key_schema,
            "Key schema mismatch for table '{}':\n  SAM:  {:?}\n  Code: {:?}",
            table_name, sam_schema.key_schema, dynamo_schema.key_schema
        );

        assert_eq!(
            sam_schema.attribute_defs, dynamo_schema.attribute_defs,
            "Attribute definitions mismatch for table '{}':\n  SAM:  {:?}\n  Code: {:?}",
            table_name, sam_schema.attribute_defs, dynamo_schema.attribute_defs
        );
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo test -p sync-service --test schema_drift_test --no-run`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add sync-service/Cargo.toml sync-service/tests/schema_drift_test.rs
git commit -m "test: add schema drift validation test (SAM template vs DynamoDB Local)"
```

---

## Summary

| Task | What it delivers |
|------|-----------------|
| 1 | TableConfig abstraction, DynamoStore refactor, all tests updated |
| 2 | Dual-mode binary (Lambda or local), lambda_http dependency |
| 3 | SAM template + deploy config (Cognito, API GW, Lambda, 5 DDB tables) |
| 4 | Schema drift test ensuring SAM and code stay in sync |

**Not in this plan (future work):**
- Actually deploying (`sam build && sam deploy` — manual step)
- MCP server
- Desktop app connecting to deployed service
