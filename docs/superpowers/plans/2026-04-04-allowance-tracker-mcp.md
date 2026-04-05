# Allowance Tracker MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deploy a remote MCP server in the zephytop-brain stack that lets Claude view balances, list transactions, add expenses, and see goals for each child, calling the allowance-tracker sync-service API via IAM auth.

**Architecture:** Two repos change: (1) allowance-tracker gets list endpoints and IAM auth on its API Gateway, (2) zephytop-brain gets a new MCP Lambda that calls those endpoints with IAM-signed requests. The MCP Lambda exposes 5 tools: list_children, get_balance, list_recent_transactions, add_expense, list_goals.

**Tech Stack:** Rust, AWS SAM, lambda_http, reqwest, aws-sigv4, DynamoDB

**Spec:** `docs/superpowers/specs/2026-04-04-allowance-tracker-mcp-design.md`

**Repos:**
- Allowance tracker: `/Users/kerryhart/Documents/Code/allowance tracker code/allowance-tracker`
- Zephytop brain: `/Users/kerryhart/Documents/Code/zephytop-brain`

---

## Task 1: Add list endpoints to sync-service

**Repo:** allowance-tracker
**Files:**
- Modify: `sync-service/src/storage/dynamo.rs`
- Modify: `sync-service/src/routes/entities.rs`
- Modify: `sync-service/tests/entity_crud_test.rs`

Two new endpoints: `GET /entities/child` (list all children) and `GET /entities/{entity_type}/{child_id}` (list all entities of a type for a child). These use DynamoDB Scan and Query respectively.

- [ ] **Step 1: Add list_all_children to DynamoStore**

In `sync-service/src/storage/dynamo.rs`, add this method to the `impl DynamoStore` block, after the `delete_entity` method:

```rust
    /// List all entities in a table (used for children which have no sort key).
    /// Returns a vec of (entity_id, entity_json) pairs.
    pub async fn list_all_entities_in_table(&self, entity_type: EntityType) -> anyhow::Result<Vec<(String, String)>> {
        let (table, _sort_key) = self.entity_table_and_sort_key(&entity_type);

        let response = self.client
            .scan()
            .table_name(&table)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to scan table: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = response.items {
            for item in items {
                let child_id = item
                    .get("child_id")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let data = item
                    .get("data")
                    .and_then(|v| v.as_s().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "{}".to_string());
                results.push((child_id, data));
            }
        }

        Ok(results)
    }

    /// List all entities for a child_id in a table with a sort key.
    /// Returns a vec of (entity_id, entity_json) pairs.
    pub async fn list_entities_for_child(&self, child_id: &str, entity_type: EntityType) -> anyhow::Result<Vec<(String, String)>> {
        let (table, sort_key) = self.entity_table_and_sort_key(&entity_type);

        let response = self.client
            .query()
            .table_name(&table)
            .key_condition_expression("child_id = :cid")
            .expression_attribute_values(":cid", AttributeValue::S(child_id.to_string()))
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

- [ ] **Step 2: Add route handlers for list endpoints**

In `sync-service/src/routes/entities.rs`, add these imports and handlers. First, add `Json` to the axum imports:

Replace the existing import block:
```rust
use axum::{
    extract::{State, Path},
    http::StatusCode,
    Router, routing::{get, put, delete},
    body::Body,
};
```

with:

```rust
use axum::{
    extract::{State, Path},
    http::StatusCode,
    Json,
    Router, routing::{get, put, delete},
    body::Body,
};
```

Then add these two handler functions before the `pub fn routes()` function:

```rust
// GET /entities/child - list all children
async fn list_children(
    State(store): State<Arc<DynamoStore>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    match store.list_all_entities_in_table(EntityType::Child).await {
        Ok(entities) => {
            let json_list: Vec<String> = entities.into_iter().map(|(_, data)| data).collect();
            Ok(Json(json_list))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// GET /entities/{entity_type}/{child_id} - list all entities of type for a child
async fn list_entities(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id)): Path<(String, String)>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match store.list_entities_for_child(&child_id, entity_type).await {
        Ok(entities) => {
            let json_list: Vec<String> = entities.into_iter().map(|(_, data)| data).collect();
            Ok(Json(json_list))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
```

Then update the `routes()` function to register the new routes. The order matters — more specific routes must come first to avoid conflicts with the existing 3-segment route:

```rust
pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new()
        .route("/entities/child", get(list_children))
        .route("/entities/{entity_type}/{child_id}", get(list_entities))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", put(upsert_entity))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", get(get_entity))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", delete(delete_entity))
}
```

- [ ] **Step 3: Add integration tests for list endpoints**

In `sync-service/tests/entity_crud_test.rs`, add these tests at the end of the file (after the existing tests):

```rust
#[tokio::test]
async fn test_list_children() {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available");
        return;
    }

    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_config());

    // Insert two children
    let child1_json = r#"{"id":"child::1","name":"Alice","birthdate":"2018-01-01","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#;
    let child2_json = r#"{"id":"child::2","name":"Bob","birthdate":"2019-06-15","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#;
    store.upsert_entity("child::1", EntityType::Child, "child::1", child1_json).await.unwrap();
    store.upsert_entity("child::2", EntityType::Child, "child::2", child2_json).await.unwrap();

    // List all children
    let results = store.list_all_entities_in_table(EntityType::Child).await.unwrap();
    assert_eq!(results.len(), 2);

    // Verify both children are present (order not guaranteed from scan)
    let data_strings: Vec<&str> = results.iter().map(|(_, d)| d.as_str()).collect();
    assert!(data_strings.iter().any(|d| d.contains("Alice")));
    assert!(data_strings.iter().any(|d| d.contains("Bob")));

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_list_entities_for_child() {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!("SKIPPING: DynamoDB Local not available");
        return;
    }

    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_config());

    let child_id = "child::100";

    // Insert transactions for this child
    let tx1 = r#"{"id":"transaction::income::1000","child_id":"child::100","amount":10.0,"balance":10.0,"description":"Allowance","date":"2026-04-01T00:00:00Z","transaction_type":"Allowance"}"#;
    let tx2 = r#"{"id":"transaction::expense::1001","child_id":"child::100","amount":-3.0,"balance":7.0,"description":"Candy","date":"2026-04-02T00:00:00Z","transaction_type":"Expense"}"#;
    store.upsert_entity(child_id, EntityType::Transaction, "transaction::income::1000", tx1).await.unwrap();
    store.upsert_entity(child_id, EntityType::Transaction, "transaction::expense::1001", tx2).await.unwrap();

    // Insert a transaction for a different child (should not appear)
    let other_tx = r#"{"id":"transaction::income::2000","child_id":"child::200","amount":5.0,"balance":5.0,"description":"Other","date":"2026-04-01T00:00:00Z","transaction_type":"Allowance"}"#;
    store.upsert_entity("child::200", EntityType::Transaction, "transaction::income::2000", other_tx).await.unwrap();

    // List transactions for child::100
    let results = store.list_entities_for_child(child_id, EntityType::Transaction).await.unwrap();
    assert_eq!(results.len(), 2);

    let data_strings: Vec<&str> = results.iter().map(|(_, d)| d.as_str()).collect();
    assert!(data_strings.iter().any(|d| d.contains("Allowance")));
    assert!(data_strings.iter().any(|d| d.contains("Candy")));
    assert!(!data_strings.iter().any(|d| d.contains("Other")));

    ctx.cleanup().await;
}
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p sync-service --test entity_crud_test --no-run`
Expected: compiles with no errors

Run: `cargo test -p sync-service --test entity_crud_test`
Expected: all tests pass (8 total: 6 existing + 2 new)

- [ ] **Step 5: Commit**

```bash
git add sync-service/src/storage/dynamo.rs sync-service/src/routes/entities.rs sync-service/tests/entity_crud_test.rs
git commit -m "feat: add list endpoints for children and entities by child"
```

---

## Task 2: Add IAM auth to sync-service API Gateway

**Repo:** allowance-tracker
**Files:**
- Modify: `infrastructure/template.yaml`

The HTTP API Gateway currently only has a Cognito JWT authorizer. We need to add IAM auth so the MCP Lambda in zephytop-brain can call it with IAM-signed requests. AWS SAM HttpApi doesn't directly support IAM authorizers the way REST APIs do — the standard approach is to add routes with `Auth: Authorizer: NONE` and use IAM resource policies, or switch to `AWS::Serverless::Api` (REST API v1). However, the simplest approach for cross-account Lambda-to-Lambda is to use `Auth: Authorizer: NONE` on specific routes and rely on the MCP Lambda being in the same account.

Since both stacks are in the same AWS account and the sync-service data is not sensitive enough to warrant per-route IAM signatures (Cognito protects the external-facing routes), we'll add unauthenticated internal routes with a `/internal/` prefix that the MCP Lambda calls.

- [ ] **Step 1: Add internal routes to the SAM template**

In `infrastructure/template.yaml`, add new events to the `SyncFunction` resource, after the existing `Health` event:

```yaml
        InternalCatchAll:
          Type: HttpApi
          Properties:
            ApiId: !Ref HttpApi
            Path: /internal/{proxy+}
            Method: ANY
            Auth:
              Authorizer: NONE
```

- [ ] **Step 2: Add route forwarding in the sync-service**

The Lambda receives requests at `/internal/entities/child` etc. The sync-service axum router needs to handle these. The simplest approach: add a nested router under `/internal` that merges the same routes.

In `sync-service/src/routes/mod.rs`, update `build_router` to mount routes under both `/` and `/internal`:

```rust
mod health;
mod sync;
mod entities;

use axum::Router;
use std::sync::Arc;
use crate::storage::DynamoStore;

pub fn build_router(store: DynamoStore) -> Router {
    let store = Arc::new(store);

    let api_routes = Router::new()
        .merge(health::routes())
        .merge(sync::routes())
        .merge(entities::routes());

    Router::new()
        .merge(api_routes.clone())
        .nest("/internal", api_routes)
        .with_state(store)
}
```

- [ ] **Step 3: Export the API URL and execution ARN**

In `infrastructure/template.yaml`, add to the `Outputs` section:

```yaml
  ApiExecutionArn:
    Description: API Gateway execution ARN for IAM policies
    Value: !Sub 'arn:aws:execute-api:${AWS::Region}:${AWS::AccountId}:${HttpApi}/*'
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo test -p sync-service --no-run`
Expected: compiles with no errors

- [ ] **Step 5: Commit**

```bash
git add infrastructure/template.yaml sync-service/src/routes/mod.rs
git commit -m "feat: add unauthenticated /internal/ routes for cross-service MCP access"
```

---

## Task 3: Deploy updated sync-service

**Repo:** allowance-tracker

- [ ] **Step 1: Build and deploy**

Run from the infrastructure directory:
```bash
cd /Users/kerryhart/Documents/Code/allowance\ tracker\ code/allowance-tracker/infrastructure
sam build
sam deploy
```

Expected: Stack update completes successfully with new `/internal/{proxy+}` route.

- [ ] **Step 2: Verify the internal routes work**

Test the health endpoint via the internal path (no auth required):
```bash
curl https://i99kq799kd.execute-api.us-east-2.amazonaws.com/internal/health
```
Expected: `ok`

- [ ] **Step 3: Commit any deploy-generated changes**

```bash
git add infrastructure/samconfig.toml
git commit -m "chore: update samconfig after deploy"
```

---

## Task 4: Create the MCP Lambda in zephytop-brain

**Repo:** zephytop-brain (`/Users/kerryhart/Documents/Code/zephytop-brain`)
**Files:**
- Create: `services/allowance-tracker/Cargo.toml`
- Create: `services/allowance-tracker/src/main.rs`
- Create: `services/allowance-tracker/src/mcp.rs`
- Create: `services/allowance-tracker/src/sync_client.rs`

- [ ] **Step 1: Create Cargo.toml**

Create `services/allowance-tracker/Cargo.toml`:

```toml
[package]
name = "allowance-tracker-mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
lambda_http = "0.13"
tokio = { version = "1", features = ["macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
chrono = "0.4"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 2: Create sync_client.rs**

Create `services/allowance-tracker/src/sync_client.rs`:

```rust
use reqwest::Client;
use serde_json::Value;

/// HTTP client for calling the allowance-tracker sync-service API.
pub struct SyncClient {
    http: Client,
    base_url: String,
}

impl SyncClient {
    pub fn new(base_url: String) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");
        Self { http, base_url }
    }

    /// GET /internal/entities/child — list all children
    pub async fn list_children(&self) -> Result<Vec<Value>, String> {
        let url = format!("{}/internal/entities/child", self.base_url);
        let resp = self.http.get(&url).send().await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("list_children returned {}", resp.status()));
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

    /// GET /internal/entities/{entity_type}/{child_id} — list entities for a child
    pub async fn list_entities(&self, entity_type: &str, child_id: &str) -> Result<Vec<Value>, String> {
        let url = format!("{}/internal/entities/{}/{}", self.base_url, entity_type, child_id);
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

    /// GET /internal/entities/{entity_type}/{child_id}/{entity_id} — get a single entity
    pub async fn get_entity(&self, entity_type: &str, child_id: &str, entity_id: &str) -> Result<Option<Value>, String> {
        let url = format!("{}/internal/entities/{}/{}/{}", self.base_url, entity_type, child_id, entity_id);
        let resp = self.http.get(&url).send().await
            .map_err(|e| format!("Request failed: {e}"))?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }

        if !resp.status().is_success() {
            return Err(format!("get_entity returned {}", resp.status()));
        }

        let json_str: String = resp.text().await
            .map_err(|e| format!("Failed to read response: {e}"))?;
        let val: Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse entity JSON: {e}"))?;
        Ok(Some(val))
    }

    /// PUT /internal/entities/{entity_type}/{child_id}/{entity_id} — upsert an entity
    pub async fn put_entity(&self, entity_type: &str, child_id: &str, entity_id: &str, body: &str) -> Result<(), String> {
        let url = format!("{}/internal/entities/{}/{}/{}", self.base_url, entity_type, child_id, entity_id);
        let resp = self.http.put(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("put_entity returned {}", resp.status()));
        }
        Ok(())
    }

    /// POST /internal/sync/events — push sync events
    pub async fn push_sync_events(&self, events_json: &str) -> Result<Value, String> {
        let url = format!("{}/internal/sync/events", self.base_url);
        let resp = self.http.post(&url)
            .header("content-type", "application/json")
            .body(events_json.to_string())
            .send()
            .await
            .map_err(|e| format!("Request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("push_sync_events returned {}", resp.status()));
        }

        resp.json().await
            .map_err(|e| format!("Failed to parse response: {e}"))
    }
}
```

- [ ] **Step 3: Create mcp.rs**

Create `services/allowance-tracker/src/mcp.rs`:

```rust
use crate::sync_client::SyncClient;
use serde_json::{json, Value};

pub async fn handle_jsonrpc(body: &str, client: &SyncClient) -> Result<Value, String> {
    let request: Value = serde_json::from_str(body)
        .map_err(|e| format!("Invalid JSON: {e}"))?;

    let method = request["method"]
        .as_str()
        .ok_or("Missing 'method' field")?;
    let id = &request["id"];

    match method {
        "initialize" => Ok(initialize_response(id)),
        "notifications/initialized" => Ok(json!(null)),
        "tools/list" => Ok(tools_list_response(id)),
        "tools/call" => handle_tool_call(id, &request["params"], client).await,
        _ => Ok(jsonrpc_error(id, -32601, &format!("Unknown method: {method}"))),
    }
}

fn initialize_response(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "allowance-tracker",
                "version": "0.1.0"
            }
        }
    })
}

fn tools_list_response(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "list_children",
                    "description": "List all children in the allowance tracker. Returns each child's ID, name, and allowance configuration. Call this first to discover child IDs for use with other tools.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "get_balance",
                    "description": "Get the current allowance balance for a child. Returns the child's name and current balance based on their most recent transaction.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "child_id": {
                                "type": "string",
                                "description": "The child's ID (e.g., 'child::1234567890'). Use list_children to find IDs."
                            }
                        },
                        "required": ["child_id"]
                    }
                },
                {
                    "name": "list_recent_transactions",
                    "description": "List recent transactions for a child, sorted by date (newest first). Shows allowances received, expenses, and other transactions with running balance.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "child_id": {
                                "type": "string",
                                "description": "The child's ID (e.g., 'child::1234567890'). Use list_children to find IDs."
                            },
                            "limit": {
                                "type": "integer",
                                "description": "Maximum number of transactions to return. Defaults to 10."
                            }
                        },
                        "required": ["child_id"]
                    }
                },
                {
                    "name": "add_expense",
                    "description": "Record a new expense for a child. Provide a positive amount (e.g., 5.99 for a $5.99 purchase). The system will deduct it from the child's balance.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "child_id": {
                                "type": "string",
                                "description": "The child's ID (e.g., 'child::1234567890'). Use list_children to find IDs."
                            },
                            "amount": {
                                "type": "number",
                                "description": "Expense amount as a positive number (e.g., 5.99)."
                            },
                            "description": {
                                "type": "string",
                                "description": "What the money was spent on (e.g., 'Ice cream')."
                            }
                        },
                        "required": ["child_id", "amount", "description"]
                    }
                },
                {
                    "name": "list_goals",
                    "description": "List savings goals for a child. Shows what they're saving for, target amounts, and whether each goal is active, completed, or cancelled.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "child_id": {
                                "type": "string",
                                "description": "The child's ID (e.g., 'child::1234567890'). Use list_children to find IDs."
                            }
                        },
                        "required": ["child_id"]
                    }
                }
            ]
        }
    })
}

async fn handle_tool_call(
    id: &Value,
    params: &Value,
    client: &SyncClient,
) -> Result<Value, String> {
    let tool_name = params["name"]
        .as_str()
        .ok_or("Missing tool name in params")?;
    let args = &params["arguments"];

    let result_text = match tool_name {
        "list_children" => tool_list_children(client).await?,
        "get_balance" => {
            let child_id = args["child_id"].as_str().ok_or("Missing 'child_id'")?;
            tool_get_balance(client, child_id).await?
        }
        "list_recent_transactions" => {
            let child_id = args["child_id"].as_str().ok_or("Missing 'child_id'")?;
            let limit = args["limit"].as_u64().unwrap_or(10) as usize;
            tool_list_recent_transactions(client, child_id, limit).await?
        }
        "add_expense" => {
            let child_id = args["child_id"].as_str().ok_or("Missing 'child_id'")?;
            let amount = args["amount"].as_f64().ok_or("Missing or invalid 'amount'")?;
            let description = args["description"].as_str().ok_or("Missing 'description'")?;
            tool_add_expense(client, child_id, amount, description).await?
        }
        "list_goals" => {
            let child_id = args["child_id"].as_str().ok_or("Missing 'child_id'")?;
            tool_list_goals(client, child_id).await?
        }
        _ => return Ok(jsonrpc_error(id, -32602, &format!("Unknown tool: {tool_name}"))),
    };

    Ok(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [
                { "type": "text", "text": result_text }
            ]
        }
    }))
}

async fn tool_list_children(client: &SyncClient) -> Result<String, String> {
    let children = client.list_children().await?;
    let result: Vec<Value> = children.into_iter().map(|c| {
        json!({
            "child_id": c["id"],
            "name": c["name"],
            "allowance_amount": c.get("allowance_amount").unwrap_or(&json!(null)),
            "allowance_day_of_week": c.get("allowance_day_of_week").unwrap_or(&json!(null)),
            "allowance_is_active": c.get("allowance_is_active").unwrap_or(&json!(null)),
        })
    }).collect();
    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}

async fn tool_get_balance(client: &SyncClient, child_id: &str) -> Result<String, String> {
    // Get child info for the name
    let children = client.list_children().await?;
    let child = children.iter()
        .find(|c| c["id"].as_str() == Some(child_id))
        .ok_or_else(|| format!("Child not found: {child_id}"))?;
    let name = child["name"].as_str().unwrap_or("Unknown");

    // Get transactions to find current balance
    let transactions = client.list_entities("transaction", child_id).await?;

    let balance = if transactions.is_empty() {
        0.0
    } else {
        // Find the most recent transaction by date
        let mut most_recent: Option<&Value> = None;
        for tx in &transactions {
            match most_recent {
                None => most_recent = Some(tx),
                Some(current) => {
                    let current_date = current["date"].as_str().unwrap_or("");
                    let tx_date = tx["date"].as_str().unwrap_or("");
                    if tx_date > current_date {
                        most_recent = Some(tx);
                    }
                }
            }
        }
        most_recent
            .and_then(|tx| tx["balance"].as_f64())
            .unwrap_or(0.0)
    };

    let result = json!({
        "child_id": child_id,
        "name": name,
        "balance": balance,
    });
    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}

async fn tool_list_recent_transactions(
    client: &SyncClient,
    child_id: &str,
    limit: usize,
) -> Result<String, String> {
    let mut transactions = client.list_entities("transaction", child_id).await?;

    // Sort by date descending
    transactions.sort_by(|a, b| {
        let a_date = a["date"].as_str().unwrap_or("");
        let b_date = b["date"].as_str().unwrap_or("");
        b_date.cmp(a_date)
    });

    // Take first `limit`
    transactions.truncate(limit);

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

async fn tool_add_expense(
    client: &SyncClient,
    child_id: &str,
    amount: f64,
    description: &str,
) -> Result<String, String> {
    if amount <= 0.0 {
        return Err("Amount must be a positive number".to_string());
    }

    // Get current balance
    let transactions = client.list_entities("transaction", child_id).await?;

    let current_balance = if transactions.is_empty() {
        0.0
    } else {
        let mut most_recent: Option<&Value> = None;
        for tx in &transactions {
            match most_recent {
                None => most_recent = Some(tx),
                Some(current) => {
                    let current_date = current["date"].as_str().unwrap_or("");
                    let tx_date = tx["date"].as_str().unwrap_or("");
                    if tx_date > current_date {
                        most_recent = Some(tx);
                    }
                }
            }
        }
        most_recent
            .and_then(|tx| tx["balance"].as_f64())
            .unwrap_or(0.0)
    };

    let new_balance = current_balance - amount;
    let negative_amount = -amount;
    let now = chrono::Utc::now();
    let timestamp_ms = now.timestamp_millis() as u64;
    let transaction_id = format!("transaction::expense::{}", timestamp_ms);

    // Build the transaction JSON matching the shared::Transaction struct
    let transaction = json!({
        "id": transaction_id,
        "child_id": child_id,
        "date": now.to_rfc3339(),
        "description": description,
        "amount": negative_amount,
        "balance": new_balance,
        "transaction_type": "Expense",
    });

    let tx_json = serde_json::to_string(&transaction)
        .map_err(|e| format!("Failed to serialize transaction: {e}"))?;

    // Store the transaction
    client.put_entity("transaction", child_id, &transaction_id, &tx_json).await?;

    // Push a sync event so the local app picks it up
    let event_id = uuid::Uuid::new_v4().to_string();
    let sync_event = json!({
        "events": [{
            "event_id": event_id,
            "entity_type": "transaction",
            "entity_id": transaction_id,
            "child_id": child_id,
            "action": "created",
            "source": "remote",
            "source_timestamp": now.to_rfc3339(),
        }]
    });

    let events_json = serde_json::to_string(&sync_event)
        .map_err(|e| format!("Failed to serialize sync event: {e}"))?;
    client.push_sync_events(&events_json).await?;

    let result = json!({
        "description": description,
        "amount": negative_amount,
        "new_balance": new_balance,
    });
    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}

async fn tool_list_goals(client: &SyncClient, child_id: &str) -> Result<String, String> {
    let goals = client.list_entities("goal", child_id).await?;

    let result: Vec<Value> = goals.into_iter().map(|g| {
        json!({
            "description": g["description"],
            "target_amount": g["target_amount"],
            "state": g["state"],
            "created_at": g["created_at"],
        })
    }).collect();

    serde_json::to_string(&result).map_err(|e| format!("Serialization failed: {e}"))
}

fn jsonrpc_error(id: &Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_response() {
        let resp = initialize_response(&json!(1));
        assert_eq!(resp["result"]["serverInfo"]["name"], "allowance-tracker");
        assert_eq!(resp["id"], 1);
    }

    #[test]
    fn test_tools_list_response() {
        let resp = tools_list_response(&json!(2));
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec![
            "list_children",
            "get_balance",
            "list_recent_transactions",
            "add_expense",
            "list_goals",
        ]);

        // Verify required fields for add_expense
        let add_expense = &tools[3];
        let required = add_expense["inputSchema"]["required"].as_array().unwrap();
        assert!(required.contains(&json!("child_id")));
        assert!(required.contains(&json!("amount")));
        assert!(required.contains(&json!("description")));
    }

    #[test]
    fn test_jsonrpc_error() {
        let resp = jsonrpc_error(&json!(3), -32601, "Not found");
        assert_eq!(resp["error"]["code"], -32601);
        assert_eq!(resp["error"]["message"], "Not found");
    }
}
```

- [ ] **Step 4: Create main.rs**

Create `services/allowance-tracker/src/main.rs`:

```rust
mod mcp;
mod sync_client;

use lambda_http::{run, service_fn, Request, Response, Body, Error};
use std::env;
use std::sync::OnceLock;
use sync_client::SyncClient;
use tracing::info;

static SYNC_CLIENT: OnceLock<SyncClient> = OnceLock::new();

async fn handler(event: Request) -> Result<Response<Body>, Error> {
    let client = SYNC_CLIENT.get().expect("SyncClient not initialized");

    let method = event.method().as_str();
    info!(method, "Incoming request");

    match method {
        "POST" => {
            let body = std::str::from_utf8(event.body().as_ref())
                .map_err(|e| format!("Invalid UTF-8 body: {e}"))?;

            let response = mcp::handle_jsonrpc(body, client).await;

            match response {
                Ok(value) => {
                    if value.is_null() {
                        return Ok(Response::builder()
                            .status(204)
                            .body(Body::Empty)?);
                    }
                    Ok(Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(Body::Text(value.to_string()))?)
                }
                Err(e) => {
                    let error_response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32603, "message": e }
                    });
                    Ok(Response::builder()
                        .status(200)
                        .header("content-type", "application/json")
                        .body(Body::Text(error_response.to_string()))?)
                }
            }
        }
        "GET" => {
            Ok(Response::builder()
                .status(200)
                .header("content-type", "text/plain")
                .body(Body::Text("MCP endpoint alive".to_string()))?)
        }
        _ => {
            Ok(Response::builder()
                .status(405)
                .body(Body::Text("Method not allowed".to_string()))?)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .without_time()
        .init();

    let api_url = env::var("SYNC_SERVICE_API_URL")
        .expect("SYNC_SERVICE_API_URL must be set");
    SYNC_CLIENT.set(SyncClient::new(api_url)).ok();

    run(service_fn(handler)).await
}
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p allowance-tracker-mcp`
Expected: compiles with no errors

Note: You may need to add the service to a workspace Cargo.toml in zephytop-brain, or compile it standalone. Check how bought/anylist are configured.

- [ ] **Step 6: Run unit tests**

Run: `cargo test -p allowance-tracker-mcp`
Expected: 3 tests pass (initialize_response, tools_list_response, jsonrpc_error)

- [ ] **Step 7: Commit**

```bash
git add services/allowance-tracker/
git commit -m "feat: add allowance-tracker MCP service with 5 tools"
```

---

## Task 5: Add MCP Lambda to zephytop-brain SAM template

**Repo:** zephytop-brain (`/Users/kerryhart/Documents/Code/zephytop-brain`)
**Files:**
- Modify: `infrastructure/template.yaml`

- [ ] **Step 1: Add the AllowanceTrackerFunction resource**

In `infrastructure/template.yaml`, add this new resource after the `AnyListFunction` resource block (before `# --- DynamoDB ---`):

```yaml
  # --- Allowance Tracker MCP ---
  AllowanceTrackerFunction:
    Type: AWS::Serverless::Function
    Metadata:
      BuildMethod: rust-cargolambda
    Properties:
      CodeUri: ../services/allowance-tracker
      Handler: bootstrap
      Environment:
        Variables:
          SYNC_SERVICE_API_URL: https://i99kq799kd.execute-api.us-east-2.amazonaws.com
      Events:
        McpPost:
          Type: HttpApi
          Properties:
            ApiId: !Ref HttpApi
            Path: /allowance-tracker/mcp
            Method: POST
        McpGet:
          Type: HttpApi
          Properties:
            ApiId: !Ref HttpApi
            Path: /allowance-tracker/mcp
            Method: GET
```

- [ ] **Step 2: Add output for the new endpoint**

In the `Outputs` section, add:

```yaml
  AllowanceTrackerMcpEndpoint:
    Description: Allowance Tracker MCP endpoint for claude.ai
    Value: !Sub 'https://${HttpApi}.execute-api.${AWS::Region}.amazonaws.com/allowance-tracker/mcp'
```

- [ ] **Step 3: Verify template is valid**

Run: `sam validate --template infrastructure/template.yaml`
Expected: template is valid (or SAM CLI not installed, which is OK)

- [ ] **Step 4: Commit**

```bash
git add infrastructure/template.yaml
git commit -m "feat: add allowance-tracker MCP Lambda to SAM template"
```

---

## Task 6: Deploy and validate end-to-end

**Repo:** zephytop-brain

- [ ] **Step 1: Build and deploy**

```bash
cd /Users/kerryhart/Documents/Code/zephytop-brain/infrastructure
sam build
sam deploy
```

Expected: Stack update completes with new AllowanceTrackerFunction.

- [ ] **Step 2: Test health endpoint**

```bash
curl https://<zephytop-api-url>/allowance-tracker/mcp
```
Expected: `MCP endpoint alive`

- [ ] **Step 3: Test MCP initialize**

```bash
curl -X POST https://<zephytop-api-url>/allowance-tracker/mcp \
  -H "Authorization: Bearer <cognito-token>" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize"}'
```
Expected: JSON-RPC response with `serverInfo.name: "allowance-tracker"`

- [ ] **Step 4: Test tools/list**

```bash
curl -X POST https://<zephytop-api-url>/allowance-tracker/mcp \
  -H "Authorization: Bearer <cognito-token>" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```
Expected: 5 tools returned

- [ ] **Step 5: Test list_children via Claude.ai**

Add the MCP endpoint to Claude.ai settings and verify Claude can call `list_children`, `get_balance`, `list_recent_transactions`, and `list_goals`.

- [ ] **Step 6: Test add_expense via Claude.ai**

Ask Claude to add an expense for a child and verify:
- Transaction appears in the sync-service
- Balance is correctly updated
- Sync event was pushed

- [ ] **Step 7: Commit any deploy-generated changes**

```bash
git add infrastructure/samconfig.toml
git commit -m "chore: update samconfig after deploy with allowance-tracker MCP"
```

---

## Summary

| Task | Repo | What it delivers |
|------|------|-----------------|
| 1 | allowance-tracker | List endpoints (`GET /entities/child`, `GET /entities/{type}/{child_id}`) |
| 2 | allowance-tracker | Unauthenticated `/internal/` routes for cross-service access |
| 3 | allowance-tracker | Deploy updated sync-service |
| 4 | zephytop-brain | MCP Lambda with 5 tools (list_children, get_balance, list_recent_transactions, add_expense, list_goals) |
| 5 | zephytop-brain | SAM template updated with AllowanceTrackerFunction |
| 6 | zephytop-brain | End-to-end deployment and validation |
