# Sync-Service Deployment Design

## Overview

Deploy the allowance-tracker sync-service as an AWS Lambda behind API Gateway, with Cognito auth and DynamoDB tables, using AWS SAM. All infrastructure-as-code lives in the allowance-tracker repo alongside the service code.

### Scope

- SAM template defining Lambda, API Gateway, Cognito, and DynamoDB resources
- Dual-mode main.rs (Lambda or local server)
- TableConfig abstraction to support both env-var and prefix-based table naming
- Schema drift validation test (SAM template vs DynamoDB Local tables)
- SAM deployment configuration

### Out of Scope

- CI/CD pipeline (manual `sam deploy`)
- MCP server implementation (future work — Cognito is set up to support it)
- Desktop app integration with deployed service (HttpRemoteClient already exists, just needs a URL)

## Repository Layout

```
allowance-tracker/
  infrastructure/
    template.yaml        # SAM template
    samconfig.toml       # SAM deploy config
  sync-service/
    Cargo.toml           # updated: adds lambda_http
    src/
      main.rs            # updated: dual-mode (Lambda or local)
      lib.rs             # updated: TableConfig, create_app takes config
      storage/
        dynamo.rs        # updated: uses TableConfig instead of table_prefix
        table_definitions.rs  # unchanged
    tests/
      schema_drift_test.rs    # new: SAM vs DynamoDB Local comparison
```

Infrastructure lives in this repo (not zephytop-brain) because the sync-service shares compile-time dependencies with the rest of the workspace (shared crate, domain types). Zephytop-brain works well for small self-contained services, but allowance-tracker's sync-service is tightly coupled to the workspace.

## Dual-Mode main.rs

The binary detects its runtime environment and starts accordingly:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // In Lambda: TableConfig from env vars (SAM-injected)
    // Locally: TableConfig from prefix (empty string or test prefix)
    let config = if std::env::var("AWS_LAMBDA_RUNTIME_API").is_ok() {
        TableConfig::from_env()?
    } else {
        TableConfig::from_prefix("")
    };

    let app = create_app(config).await?;

    if std::env::var("AWS_LAMBDA_RUNTIME_API").is_ok() {
        lambda_http::run(app).await?;
    } else {
        env_logger::init();
        let listener = tokio::net::TcpListener::bind("0.0.0.0:3030").await?;
        log::info!("sync-service listening on 0.0.0.0:3030");
        axum::serve(listener, app).await?;
    }
    Ok(())
}
```

Local dev experience is unchanged: `cargo run -p sync-service` starts an HTTP server on port 3030.

## TableConfig

Replaces the current `table_prefix: String` on DynamoStore with explicit table name resolution.

```rust
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
            children: std::env::var("CHILDREN_TABLE")?,
            transactions: std::env::var("TRANSACTIONS_TABLE")?,
            goals: std::env::var("GOALS_TABLE")?,
            sync_events: std::env::var("SYNC_EVENTS_TABLE")?,
            sync_metadata: std::env::var("SYNC_METADATA_TABLE")?,
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
```

`DynamoStore` changes: replace `table_prefix: String` with `config: TableConfig`. Replace all `self.table_name("transactions")` calls with `self.config.transactions.clone()` (or `&self.config.transactions`). The `table_name` method is removed.

Test infrastructure (`DynamoTestContext`) continues using `TableConfig::from_prefix` with unique test prefixes.

## SAM Template

**Stack:** `allowance-tracker-sync`
**Region:** `us-east-2` (matches zephytop-brain)

```yaml
AWSTemplateFormatVersion: '2010-09-09'
Transform: AWS::Serverless-2016-10-31
Description: Allowance Tracker Sync Service

Globals:
  Function:
    Timeout: 30
    MemorySize: 128
    Architectures:
      - arm64
    Runtime: provided.al2023

Resources:
  # --- Cognito ---
  UserPool:
    Type: AWS::Cognito::UserPool
    Properties:
      UserPoolName: allowance-tracker-sync
      AutoVerifiedAttributes:
        - email
      UsernameAttributes:
        - email
      Policies:
        PasswordPolicy:
          MinimumLength: 12
          RequireUppercase: true
          RequireLowercase: true
          RequireNumbers: true
          RequireSymbols: false

  UserPoolDomain:
    Type: AWS::Cognito::UserPoolDomain
    Properties:
      Domain: allowance-tracker-sync
      UserPoolId: !Ref UserPool

  AppClient:
    Type: AWS::Cognito::UserPoolClient
    Properties:
      UserPoolId: !Ref UserPool
      ClientName: allowance-tracker
      GenerateSecret: true
      AllowedOAuthFlows:
        - code
      AllowedOAuthFlowsUserPoolClient: true
      AllowedOAuthScopes:
        - openid
        - email
        - profile
      SupportedIdentityProviders:
        - COGNITO
      CallbackURLs:
        - https://claude.ai/api/mcp/auth_callback
        - https://claude.com/api/mcp/auth_callback
      ExplicitAuthFlows:
        - ALLOW_REFRESH_TOKEN_AUTH
        - ALLOW_USER_SRP_AUTH
      AccessTokenValidity: 1
      IdTokenValidity: 1
      RefreshTokenValidity: 30
      TokenValidityUnits:
        AccessToken: hours
        IdToken: hours
        RefreshToken: days

  # --- API Gateway ---
  HttpApi:
    Type: AWS::Serverless::HttpApi
    Properties:
      StageName: $default
      Auth:
        DefaultAuthorizer: CognitoAuthorizer
        Authorizers:
          CognitoAuthorizer:
            IdentitySource: $request.header.Authorization
            JwtConfiguration:
              issuer: !GetAtt UserPool.ProviderURL
              audience:
                - !Ref AppClient

  # --- Lambda ---
  SyncFunction:
    Type: AWS::Serverless::Function
    Metadata:
      BuildMethod: rust-cargolambda
    Properties:
      CodeUri: ../sync-service
      Handler: bootstrap
      Environment:
        Variables:
          CHILDREN_TABLE: !Ref ChildrenTable
          TRANSACTIONS_TABLE: !Ref TransactionsTable
          GOALS_TABLE: !Ref GoalsTable
          SYNC_EVENTS_TABLE: !Ref SyncEventsTable
          SYNC_METADATA_TABLE: !Ref SyncMetadataTable
      Policies:
        - DynamoDBCrudPolicy:
            TableName: !Ref ChildrenTable
        - DynamoDBCrudPolicy:
            TableName: !Ref TransactionsTable
        - DynamoDBCrudPolicy:
            TableName: !Ref GoalsTable
        - DynamoDBCrudPolicy:
            TableName: !Ref SyncEventsTable
        - DynamoDBCrudPolicy:
            TableName: !Ref SyncMetadataTable
      Events:
        CatchAll:
          Type: HttpApi
          Properties:
            ApiId: !Ref HttpApi
            Path: /{proxy+}
            Method: ANY
        Health:
          Type: HttpApi
          Properties:
            ApiId: !Ref HttpApi
            Path: /health
            Method: GET
            Auth:
              Authorizer: NONE

  # --- DynamoDB Tables ---
  ChildrenTable:
    Type: AWS::DynamoDB::Table
    Properties:
      TableName: allowance-tracker-children
      BillingMode: PAY_PER_REQUEST
      AttributeDefinitions:
        - AttributeName: child_id
          AttributeType: S
      KeySchema:
        - AttributeName: child_id
          KeyType: HASH

  TransactionsTable:
    Type: AWS::DynamoDB::Table
    Properties:
      TableName: allowance-tracker-transactions
      BillingMode: PAY_PER_REQUEST
      AttributeDefinitions:
        - AttributeName: child_id
          AttributeType: S
        - AttributeName: transaction_id
          AttributeType: S
      KeySchema:
        - AttributeName: child_id
          KeyType: HASH
        - AttributeName: transaction_id
          KeyType: RANGE

  GoalsTable:
    Type: AWS::DynamoDB::Table
    Properties:
      TableName: allowance-tracker-goals
      BillingMode: PAY_PER_REQUEST
      AttributeDefinitions:
        - AttributeName: child_id
          AttributeType: S
        - AttributeName: goal_id
          AttributeType: S
      KeySchema:
        - AttributeName: child_id
          KeyType: HASH
        - AttributeName: goal_id
          KeyType: RANGE

  SyncEventsTable:
    Type: AWS::DynamoDB::Table
    Properties:
      TableName: allowance-tracker-sync-events
      BillingMode: PAY_PER_REQUEST
      AttributeDefinitions:
        - AttributeName: child_id
          AttributeType: S
        - AttributeName: sequence
          AttributeType: "N"
      KeySchema:
        - AttributeName: child_id
          KeyType: HASH
        - AttributeName: sequence
          KeyType: RANGE

  SyncMetadataTable:
    Type: AWS::DynamoDB::Table
    Properties:
      TableName: allowance-tracker-sync-metadata
      BillingMode: PAY_PER_REQUEST
      AttributeDefinitions:
        - AttributeName: child_id
          AttributeType: S
      KeySchema:
        - AttributeName: child_id
          KeyType: HASH

Outputs:
  ApiUrl:
    Description: API Gateway URL
    Value: !Sub 'https://${HttpApi}.execute-api.${AWS::Region}.amazonaws.com'
  UserPoolId:
    Description: Cognito User Pool ID
    Value: !Ref UserPool
  AppClientId:
    Description: Cognito App Client ID
    Value: !Ref AppClient
  CognitoHostedUiDomain:
    Description: Cognito hosted UI domain
    Value: !Sub 'allowance-tracker-sync.auth.${AWS::Region}.amazoncognito.com'
```

### SAM Deploy Config

```toml
# infrastructure/samconfig.toml
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

## Schema Drift Validation Test

An integration test in `sync-service/tests/schema_drift_test.rs` that ensures the SAM template and `create_all_tables` code produce identical table schemas.

**How it works:**

1. Parse `infrastructure/template.yaml` with `serde_yaml`
2. Find all `AWS::DynamoDB::Table` resources
3. Extract each table's `AttributeDefinitions` and `KeySchema`
4. Start DynamoDB Local, call `create_all_tables` with a test prefix
5. For each table created: call `describe_table` via the SDK, extract its key schema and attribute definitions
6. Compare the two sets directly: same attribute names, same key types (HASH/RANGE), same attribute types (S/N)
7. Fail with a descriptive message if any table's schema differs

**No hardcoded expectations.** The test's only assertion is "these two sources agree." If either changes, the test catches it.

**Mapping SAM table names to code table names:** The test maps SAM tables to code tables by stripping the `allowance-tracker-` prefix from the SAM `TableName` property and converting hyphens to underscores (e.g., `allowance-tracker-sync-events` -> `sync_events`). This is then matched against the table names produced by `TableConfig::from_prefix("")`.

## Dependencies

Add to `sync-service/Cargo.toml`:

```toml
lambda_http = "0.13"
```

No other new dependencies — `serde_yaml` is available in the workspace, and `aws-sdk-dynamodb` (for `describe_table`) is already present.

## Deployment Workflow

```bash
# First time setup
cd allowance-tracker/infrastructure
sam build
sam deploy --guided   # interactive first-time setup

# Subsequent deploys
sam build && sam deploy
```

**Prerequisites:**
- AWS CLI configured with credentials
- SAM CLI installed
- cargo-lambda installed (used by SAM's `rust-cargolambda` build method)

## Local Dev (Unchanged)

```bash
# Start sync-service locally
cargo run -p sync-service
# Uses DynamoDB Local on port 8000, prefix-based table names

# Run tests
cargo test -p sync-service
```
