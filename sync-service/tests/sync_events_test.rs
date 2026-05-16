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
async fn test_push_event_increments_sequence() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-1";

    // Initialize metadata
    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Upsert first entity (tx-1)
    store
        .upsert_entity_with_event(
            child_id,
            EntityType::Transaction,
            "tx-1",
            r#"{"id":"tx-1"}"#,
            SyncSource::Local,
        )
        .await
        .expect("Failed to upsert entity tx-1");

    // Upsert second entity (tx-2)
    store
        .upsert_entity_with_event(
            child_id,
            EntityType::Transaction,
            "tx-2",
            r#"{"id":"tx-2"}"#,
            SyncSource::Local,
        )
        .await
        .expect("Failed to upsert entity tx-2");

    // Read events back and verify sequences are 1 and 2
    let events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 2, "Should have exactly two events");
    let seq1 = events[0].sequence.expect("Event 1 missing sequence");
    let seq2 = events[1].sequence.expect("Event 2 missing sequence");
    assert_eq!(seq1, 1, "First event should have sequence 1");
    assert_eq!(seq2, 2, "Second event should have sequence 2");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_push_event_stores_correct_attributes() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-2";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Upsert the entity.  Because it doesn't exist yet, action will be Created.
    // (The old test used SyncAction::Updated with a hand-built SyncEvent; under the
    // new API the action is derived server-side.  A first write is always Created.)
    // source_timestamp is now assigned by upsert_entity_with_event (Utc::now()); we
    // can only assert that it is present and recent, not that it matches a caller value.
    store
        .upsert_entity_with_event(
            child_id,
            EntityType::Goal,
            "goal-1",
            r#"{"id":"goal-1"}"#,
            SyncSource::Remote,
        )
        .await
        .expect("Failed to upsert entity");

    // Retrieve the event back
    let events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 1, "Should have exactly one event");
    let retrieved = &events[0];

    // event_id is now deterministic: ev::created::{entity_id}
    assert_eq!(retrieved.event_id, "ev::created::goal-1");
    assert_eq!(retrieved.entity_type, EntityType::Goal);
    assert_eq!(retrieved.entity_id, "goal-1");
    assert_eq!(retrieved.child_id, child_id);
    // First write to an entity is always Created under the new API
    assert_eq!(retrieved.action, SyncAction::Created);
    assert_eq!(retrieved.source, SyncSource::Remote);
    assert!(retrieved.source_timestamp.timestamp() > 0, "source_timestamp should be set, not epoch");
    assert!(retrieved.sequence.is_some(), "Event should have a sequence");
    let seq = retrieved.sequence.unwrap();
    assert_eq!(seq, 1, "Only event should have sequence 1");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_push_multiple_events_sequential_sequences() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-3";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Upsert 10 entities, each with a unique entity_id
    for i in 1..=10 {
        let entity_id = format!("tx-{}", i);
        let entity_json = format!(r#"{{"id":"{}"}}"#, entity_id);
        store
            .upsert_entity_with_event(
                child_id,
                EntityType::Transaction,
                &entity_id,
                &entity_json,
                SyncSource::Local,
            )
            .await
            .expect(&format!("Failed to upsert entity {}", i));
    }

    // Read all events and verify sequences are gapless 1..10
    let events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 10, "Should have 10 events");
    for (idx, event) in events.iter().enumerate() {
        let expected_seq = (idx + 1) as u64;
        assert_eq!(
            event.sequence,
            Some(expected_seq),
            "Event at index {} should have sequence {}",
            idx,
            expected_seq
        );
    }

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_get_events_since_returns_correct_range() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-4";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Upsert 10 entities
    for i in 1..=10 {
        let entity_id = format!("tx-{}", i);
        let entity_json = format!(r#"{{"id":"{}"}}"#, entity_id);
        store
            .upsert_entity_with_event(
                child_id,
                EntityType::Transaction,
                &entity_id,
                &entity_json,
                SyncSource::Local,
            )
            .await
            .expect(&format!("Failed to upsert entity {}", i));
    }

    // Query for events since sequence 5 (should return sequences 6-10)
    let events = store
        .get_events_since(child_id, 5)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 5, "Should return 5 events (sequences 6-10)");

    for (idx, event) in events.iter().enumerate() {
        let expected_seq = 6 + idx as u64;
        assert_eq!(
            event.sequence,
            Some(expected_seq),
            "Event at index {} should have sequence {}",
            idx,
            expected_seq
        );
    }

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_get_events_since_empty_when_at_latest() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-5";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    store
        .upsert_entity_with_event(
            child_id,
            EntityType::Transaction,
            "tx-1",
            r#"{"id":"tx-1"}"#,
            SyncSource::Local,
        )
        .await
        .expect("Failed to upsert entity");

    // Read back to get the assigned sequence
    let all_events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");
    assert_eq!(all_events.len(), 1, "Should have exactly one event");
    let seq = all_events[0].sequence.expect("Event missing sequence");

    // Query since the same sequence (should be empty)
    let events = store
        .get_events_since(child_id, seq)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 0, "Should return no events when querying at latest");

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_duplicate_event_push_idempotent() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-6";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Upsert the entity the first time
    store
        .upsert_entity_with_event(
            child_id,
            EntityType::Transaction,
            "tx-1",
            r#"{"id":"tx-1"}"#,
            SyncSource::Local,
        )
        .await
        .expect("Failed to upsert entity first time");

    // Upsert the same entity again with identical content.
    // upsert_entity_with_event short-circuits when content is unchanged,
    // so no second event is emitted.
    store
        .upsert_entity_with_event(
            child_id,
            EntityType::Transaction,
            "tx-1",
            r#"{"id":"tx-1"}"#,
            SyncSource::Local,
        )
        .await
        .expect("Failed to upsert entity second time");

    // Verify only one event exists in the store
    let events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 1, "Should have exactly one event stored (dedup)");
    // event_id is deterministic: ev::created::tx-1
    assert_eq!(events[0].event_id, "ev::created::tx-1");
    assert_eq!(events[0].sequence, Some(1));

    ctx.cleanup().await;
}
