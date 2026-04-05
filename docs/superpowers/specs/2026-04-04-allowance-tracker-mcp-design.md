# Allowance Tracker MCP Server Design

## Goal

Deploy a remote MCP server that lets Claude view allowance balances, describe recent transactions, add expenses, and see savings goals for each child. The MCP server lives in the zephytop-brain stack (shared Cognito auth) and calls the deployed allowance-tracker sync-service API.

## Architecture

```
Claude.ai
  → zephytop-brain API Gateway (Cognito JWT auth)
    → AllowanceTrackerMCP Lambda (Rust)
      → allowance-tracker-sync API Gateway (IAM auth)
        → SyncFunction Lambda
          → DynamoDB
```

- **MCP Lambda** lives in the zephytop-brain stack alongside bought/anylist services.
- **Auth to Claude**: Cognito JWT via zephytop-brain's existing User Pool, API Gateway, and OAuth metadata endpoints.
- **Auth to sync-service**: IAM-signed requests. The MCP Lambda's execution role gets permission to invoke the allowance-tracker-sync API Gateway.
- **No direct DynamoDB access** from the MCP Lambda. All data flows through the sync-service REST API.

## MCP Tools

### `list_children`

Lists all children in the system with their allowance configuration.

- **Parameters**: none
- **Calls**: `GET /entities/child`
- **Returns**: Array of children:
  ```json
  [
    {
      "child_id": "child::1234567890",
      "name": "Emma",
      "allowance_amount": 10.0,
      "allowance_day_of_week": 6,
      "allowance_is_active": true
    }
  ]
  ```

### `get_balance`

Gets the current balance for a child.

- **Parameters**: `{ "child_id": "child::1234567890" }`
- **Calls**: `GET /entities/transaction/{child_id}`
- **Logic**: Finds the most recent transaction by date. Its `balance` field is the current balance (running balance is stored per transaction).
- **Returns**:
  ```json
  { "child_id": "child::1234567890", "name": "Emma", "balance": 42.50 }
  ```

### `list_recent_transactions`

Lists recent transactions for a child, sorted by date descending.

- **Parameters**: `{ "child_id": "child::1234567890", "limit": 10 }` (limit optional, default 10)
- **Calls**: `GET /entities/transaction/{child_id}`
- **Logic**: Sorts by date descending, takes first `limit` entries.
- **Returns**:
  ```json
  [
    {
      "date": "2026-04-01T00:00:00Z",
      "description": "Weekly allowance",
      "amount": 10.0,
      "balance": 42.50,
      "transaction_type": "Allowance"
    }
  ]
  ```

### `add_expense`

Records a new expense for a child.

- **Parameters**: `{ "child_id": "child::1234567890", "amount": 5.99, "description": "Ice cream" }` (amount is a positive number)
- **Logic**:
  1. `GET /entities/transaction/{child_id}` to determine current balance
  2. Construct a Transaction entity with negated amount (`-5.99`) and updated running balance
  3. `PUT /entities/transaction/{child_id}/{transaction_id}` to store it
  4. `POST /sync/events` to push a `Created` sync event so the local app picks it up
- **Transaction ID format**: `transaction::expense::{timestamp_ms}`
- **Returns**:
  ```json
  { "description": "Ice cream", "amount": -5.99, "new_balance": 36.51 }
  ```

### `list_goals`

Lists savings goals for a child.

- **Parameters**: `{ "child_id": "child::1234567890" }`
- **Calls**: `GET /entities/goal/{child_id}`
- **Returns**:
  ```json
  [
    {
      "description": "New bicycle",
      "target_amount": 150.0,
      "state": "Active",
      "created_at": "2026-03-15T00:00:00Z"
    }
  ]
  ```

## New Sync-Service Endpoints

The sync-service currently only supports single-entity CRUD. Two list endpoints are needed:

| Method | Path | Description | DynamoDB Operation |
|--------|------|-------------|--------------------|
| `GET` | `/entities/child` | List all children | `Scan` on children table |
| `GET` | `/entities/{entity_type}/{child_id}` | List all entities of type for a child | `Query` on partition key `child_id` |

These endpoints use the same IAM auth as the other MCP-facing routes.

## Infrastructure Changes

### allowance-tracker-sync (template.yaml)

- Add IAM authorizer to the HTTP API Gateway alongside the existing Cognito JWT authorizer.
- Add IAM-authenticated routes for all endpoints the MCP Lambda needs:
  - `GET /entities/child`
  - `GET /entities/{entity_type}/{child_id}`
  - `GET /entities/{entity_type}/{child_id}/{entity_id}`
  - `PUT /entities/{entity_type}/{child_id}/{entity_id}`
  - `POST /sync/events`
- Export the API URL and API execution ARN via CloudFormation outputs for cross-stack reference.

### zephytop-brain (template.yaml)

- New Lambda function: `AllowanceTrackerFunction`
  - Runtime: `provided.al2023` (Rust via cargo-lambda)
  - Architecture: arm64
  - Memory: 128 MB
  - Timeout: 30 seconds
- New API Gateway route: `POST /allowance-tracker/mcp` (Cognito JWT auth)
- Environment variable: `SYNC_SERVICE_API_URL` (the allowance-tracker-sync API Gateway URL)
- IAM policy on the Lambda execution role: `execute-api:Invoke` on the allowance-tracker-sync API Gateway ARN.

## Code Structure

```
zephytop-brain/services/allowance-tracker/
  Cargo.toml
  src/
    main.rs            # Lambda handler (POST → MCP dispatch, GET → health check)
    mcp.rs             # JSON-RPC 2.0 dispatch: initialize, tools/list, tools/call
    sync_client.rs     # HTTP client for sync-service API with IAM request signing
```

## MCP Protocol

Follows the same pattern as zephytop-brain's bought/anylist services:

| Method | Response |
|--------|----------|
| `initialize` | Server info (name: `allowance-tracker`), protocol version `2025-03-26`, capabilities `{ tools: {} }` |
| `notifications/initialized` | `null` (no-op) |
| `tools/list` | Array of 5 tool schemas with JSON Schema `inputSchema` definitions |
| `tools/call` | Dispatch to tool handler, return `{ content: [{ type: "text", text: result }] }` |
| Unknown method | JSON-RPC error code `-32601` |

Error codes:
- `-32601`: Unknown method
- `-32602`: Invalid/missing parameters
- `-32603`: Internal error (parse failure, sync-service unreachable)
- `-32001`: Domain error (child not found, insufficient balance)

## Dependencies

```toml
[dependencies]
lambda_http = "0.13"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4"] }
aws-config = "1"
aws-sigv4 = "1"
aws-credential-types = "1"
aws-smithy-runtime-api = "1"
```

## Out of Scope

- Local stdio MCP server (remote only for now)
- Adding income or managing goals through Claude (read-only for goals)
- Conflict resolution UI in Claude (sync events handle this)
- Balance validation (allowing negative balances — parent can manage this in the app)
