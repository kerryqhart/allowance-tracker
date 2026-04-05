mod common;

use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use shared::sync::*;
use sync_service::storage::DynamoStore;

async fn setup() -> Option<(DynamoTestContext, DynamoStore)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!(
            "SKIPPING: DynamoDB Local not available on port {}",
            DYNAMO_LOCAL_PORT
        );
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_config());
    Some((ctx, store))
}

#[tokio::test]
async fn test_upsert_and_get_transaction() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-1";
    let tx_id = "tx-001";
    let tx_data = r#"{"amount": 10.50, "description": "allowance"}"#;

    // Upsert a transaction
    store
        .upsert_entity(child_id, EntityType::Transaction, tx_id, tx_data)
        .await
        .expect("Failed to upsert transaction");

    // Get the transaction back
    let retrieved = store
        .get_entity(child_id, EntityType::Transaction, tx_id)
        .await
        .expect("Failed to get transaction")
        .expect("Transaction should exist");

    assert_eq!(retrieved, tx_data);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_upsert_overwrites_existing() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-2";
    let tx_id = "tx-002";
    let tx_data_v1 = r#"{"amount": 10.00}"#;
    let tx_data_v2 = r#"{"amount": 15.00}"#;

    // Upsert first version
    store
        .upsert_entity(child_id, EntityType::Transaction, tx_id, tx_data_v1)
        .await
        .expect("Failed to upsert v1");

    // Upsert second version (overwrite)
    store
        .upsert_entity(child_id, EntityType::Transaction, tx_id, tx_data_v2)
        .await
        .expect("Failed to upsert v2");

    // Get the entity and verify it's the second version
    let retrieved = store
        .get_entity(child_id, EntityType::Transaction, tx_id)
        .await
        .expect("Failed to get transaction")
        .expect("Transaction should exist");

    assert_eq!(retrieved, tx_data_v2);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_get_nonexistent_entity_returns_none() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-3";
    let tx_id = "nonexistent-tx";

    let result = store
        .get_entity(child_id, EntityType::Transaction, tx_id)
        .await
        .expect("Failed to query for entity");

    assert_eq!(result, None, "Nonexistent entity should return None");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_delete_entity() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-4";
    let tx_id = "tx-004";
    let tx_data = r#"{"amount": 20.00}"#;

    // Upsert a transaction
    store
        .upsert_entity(child_id, EntityType::Transaction, tx_id, tx_data)
        .await
        .expect("Failed to upsert transaction");

    // Verify it exists
    let retrieved = store
        .get_entity(child_id, EntityType::Transaction, tx_id)
        .await
        .expect("Failed to get transaction")
        .expect("Transaction should exist");
    assert_eq!(retrieved, tx_data);

    // Delete it
    store
        .delete_entity(child_id, EntityType::Transaction, tx_id)
        .await
        .expect("Failed to delete transaction");

    // Verify it's gone
    let result = store
        .get_entity(child_id, EntityType::Transaction, tx_id)
        .await
        .expect("Failed to query for entity");
    assert_eq!(result, None, "Entity should be deleted");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_goal_crud() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-5";
    let goal_id = "goal-001";
    let goal_data = r#"{"name": "Save $100", "target": 100.00, "current": 50.00}"#;

    // Create a goal
    store
        .upsert_entity(child_id, EntityType::Goal, goal_id, goal_data)
        .await
        .expect("Failed to upsert goal");

    // Retrieve it
    let retrieved = store
        .get_entity(child_id, EntityType::Goal, goal_id)
        .await
        .expect("Failed to get goal")
        .expect("Goal should exist");

    assert_eq!(retrieved, goal_data);

    // Update it
    let updated_goal_data = r#"{"name": "Save $100", "target": 100.00, "current": 75.00}"#;
    store
        .upsert_entity(child_id, EntityType::Goal, goal_id, updated_goal_data)
        .await
        .expect("Failed to update goal");

    // Verify update
    let retrieved_updated = store
        .get_entity(child_id, EntityType::Goal, goal_id)
        .await
        .expect("Failed to get updated goal")
        .expect("Updated goal should exist");

    assert_eq!(retrieved_updated, updated_goal_data);

    // Delete it
    store
        .delete_entity(child_id, EntityType::Goal, goal_id)
        .await
        .expect("Failed to delete goal");

    // Verify deletion
    let result = store
        .get_entity(child_id, EntityType::Goal, goal_id)
        .await
        .expect("Failed to query for goal");
    assert_eq!(result, None, "Goal should be deleted");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_child_crud() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-6";
    let child_data = r#"{"name": "Alice", "age": 10, "email": "alice@example.com"}"#;

    // Create a child (note: child uses only child_id as key, no sort key)
    store
        .upsert_entity(child_id, EntityType::Child, "", child_data)
        .await
        .expect("Failed to upsert child");

    // Retrieve it (entity_id is ignored for Child entity type)
    let retrieved = store
        .get_entity(child_id, EntityType::Child, "")
        .await
        .expect("Failed to get child")
        .expect("Child should exist");

    assert_eq!(retrieved, child_data);

    // Update it
    let updated_child_data = r#"{"name": "Alice", "age": 11, "email": "alice@example.com"}"#;
    store
        .upsert_entity(child_id, EntityType::Child, "", updated_child_data)
        .await
        .expect("Failed to update child");

    // Verify update
    let retrieved_updated = store
        .get_entity(child_id, EntityType::Child, "")
        .await
        .expect("Failed to get updated child")
        .expect("Updated child should exist");

    assert_eq!(retrieved_updated, updated_child_data);

    // Delete it
    store
        .delete_entity(child_id, EntityType::Child, "")
        .await
        .expect("Failed to delete child");

    // Verify deletion
    let result = store
        .get_entity(child_id, EntityType::Child, "")
        .await
        .expect("Failed to query for child");
    assert_eq!(result, None, "Child should be deleted");

    ctx.cleanup().await;
}

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
