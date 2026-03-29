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
    let store = DynamoStore::new(ctx.client.clone(), ctx.table_prefix.clone());
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

    // Create and push first event
    let event1 = SyncEvent::new(
        EntityType::Transaction,
        "tx-1".to_string(),
        child_id.to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );
    let seq1 = store
        .push_event(&event1)
        .await
        .expect("Failed to push event 1");
    assert_eq!(seq1, 1, "First event should have sequence 1");

    // Create and push second event
    let event2 = SyncEvent::new(
        EntityType::Transaction,
        "tx-2".to_string(),
        child_id.to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );
    let seq2 = store
        .push_event(&event2)
        .await
        .expect("Failed to push event 2");
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

    let event = SyncEvent::new(
        EntityType::Goal,
        "goal-1".to_string(),
        child_id.to_string(),
        SyncAction::Updated,
        SyncSource::Remote,
    );
    let original_timestamp = event.source_timestamp;
    let original_event_id = event.event_id.clone();

    let seq = store
        .push_event(&event)
        .await
        .expect("Failed to push event");

    // Retrieve the event back
    let events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 1, "Should have exactly one event");
    let retrieved = &events[0];

    assert_eq!(retrieved.event_id, original_event_id);
    assert_eq!(retrieved.entity_type, EntityType::Goal);
    assert_eq!(retrieved.entity_id, "goal-1");
    assert_eq!(retrieved.child_id, child_id);
    assert_eq!(retrieved.action, SyncAction::Updated);
    assert_eq!(retrieved.source, SyncSource::Remote);
    assert_eq!(retrieved.source_timestamp, original_timestamp);
    assert_eq!(retrieved.sequence, Some(seq));

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

    // Push 10 events
    for i in 1..=10 {
        let event = SyncEvent::new(
            EntityType::Transaction,
            format!("tx-{}", i),
            child_id.to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        let seq = store
            .push_event(&event)
            .await
            .expect(&format!("Failed to push event {}", i));
        assert_eq!(seq, i as u64, "Event {} should have sequence {}", i, i);
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

    // Push 10 events
    for i in 1..=10 {
        let event = SyncEvent::new(
            EntityType::Transaction,
            format!("tx-{}", i),
            child_id.to_string(),
            SyncAction::Created,
            SyncSource::Local,
        );
        let _ = store
            .push_event(&event)
            .await
            .expect(&format!("Failed to push event {}", i));
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

    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx-1".to_string(),
        child_id.to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let seq = store
        .push_event(&event)
        .await
        .expect("Failed to push event");

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

    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx-1".to_string(),
        child_id.to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );
    let event_id = event.event_id.clone();

    // Push the event the first time
    let seq1 = store
        .push_event(&event)
        .await
        .expect("Failed to push event first time");

    // Push the same event again (duplicate)
    let seq2 = store
        .push_event(&event)
        .await
        .expect("Failed to push event second time");

    // Both should return the same sequence
    assert_eq!(
        seq1, seq2,
        "Duplicate push should return same sequence"
    );

    // Verify only one event exists in the store
    let events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");

    assert_eq!(events.len(), 1, "Should have exactly one event stored");
    assert_eq!(events[0].event_id, event_id);
    assert_eq!(events[0].sequence, Some(seq1));

    ctx.cleanup().await;
}
