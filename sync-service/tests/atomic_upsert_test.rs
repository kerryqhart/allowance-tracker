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
