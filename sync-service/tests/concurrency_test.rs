mod common;

use common::{DynamoTestContext, DYNAMO_LOCAL_PORT, is_dynamo_local_available};
use shared::sync::*;
use sync_service::storage::DynamoStore;
use std::sync::Arc;

async fn setup() -> Option<(DynamoTestContext, Arc<DynamoStore>)> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        eprintln!(
            "SKIPPING: DynamoDB Local not available on port {}",
            DYNAMO_LOCAL_PORT
        );
        return None;
    }
    let ctx = DynamoTestContext::new(DYNAMO_LOCAL_PORT).await;
    let store = Arc::new(DynamoStore::new(ctx.client.clone(), ctx.table_config()));
    Some((ctx, store))
}

fn assert_gapless_sequence(sequences: &[u64]) {
    let mut sorted = sequences.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        sequences.len(),
        "Duplicate sequence numbers found"
    );
    let expected: Vec<u64> = (1..=sorted.len() as u64).collect();
    assert_eq!(sorted, expected, "Sequences are not gapless 1..N");
}

/// Test concurrent push from multiple tasks for the same child.
/// 10 tokio tasks, each upserting 10 entities for "child1".
/// Verify all 100 resulting sequences are gapless 1-100.
#[tokio::test]
async fn test_concurrent_sequence_increment_no_duplicates() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "child1";

    // Initialize metadata
    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Spawn 10 concurrent tasks, each upserting 10 entities
    let mut handles = vec![];
    for task_id in 0..10 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            for event_num in 0..10 {
                let entity_id = format!("tx-task{}-evt{}", task_id, event_num);
                let entity_json = format!(r#"{{"id":"{}"}}"#, entity_id);
                store_clone
                    .upsert_entity_with_event(
                        child_id,
                        EntityType::Transaction,
                        &entity_id,
                        &entity_json,
                        SyncSource::Local,
                    )
                    .await
                    .unwrap_or_else(|e| panic!("Task {} failed to upsert entity: {}", task_id, e));
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task join error");
    }

    // Collect all sequences after all writes
    let all_events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to query events");

    assert_eq!(
        all_events.len(),
        100,
        "Should have 100 events from 10 tasks * 10 entities"
    );

    let all_sequences: Vec<u64> = all_events.iter().filter_map(|e| e.sequence).collect();
    assert_eq!(all_sequences.len(), 100, "All events should have sequences");

    // Verify all sequences are gapless 1-100
    assert_gapless_sequence(&all_sequences);

    ctx.cleanup().await;
}

/// Test concurrent push for different children is independent.
/// 5 children, 20 entities each upserted concurrently.
/// Verify each child has gapless 1-20 sequences independently.
#[tokio::test]
async fn test_concurrent_push_different_children_independent() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let children = ["child-a", "child-b", "child-c", "child-d", "child-e"];

    // Initialize metadata for all children
    for child_id in children.iter() {
        store
            .initialize_child_metadata(child_id)
            .await
            .expect("Failed to initialize metadata");
    }

    // Spawn one task per child, each upserting 20 entities
    let mut handles = vec![];
    for child_id in children.iter() {
        let store_clone = Arc::clone(&store);
        let child_id_copy = child_id.to_string();
        let handle = tokio::spawn(async move {
            for event_num in 0..20 {
                let entity_id = format!("goal-{}-{}", child_id_copy, event_num);
                let entity_json = format!(r#"{{"id":"{}"}}"#, entity_id);
                store_clone
                    .upsert_entity_with_event(
                        &child_id_copy,
                        EntityType::Goal,
                        &entity_id,
                        &entity_json,
                        SyncSource::Local,
                    )
                    .await
                    .unwrap_or_else(|e| panic!("Child {} event {} failed: {}", child_id_copy, event_num, e));
            }
            child_id_copy
        });
        handles.push(handle);
    }

    // Collect results per child and verify sequences
    for handle in handles {
        let child_id = handle.await.expect("Task join error");
        let events = store
            .get_events_since(&child_id, 0)
            .await
            .expect("Failed to query events");
        assert_eq!(
            events.len(),
            20,
            "Child {} should have 20 events",
            child_id
        );
        let sequences: Vec<u64> = events.iter().filter_map(|e| e.sequence).collect();
        assert_eq!(sequences.len(), 20, "All events for child {} should have sequences", child_id);
        assert_gapless_sequence(&sequences);
    }

    ctx.cleanup().await;
}

/// Test write and poll concurrently.
/// One task upserts 50 entities, another polls repeatedly with get_events_since.
/// Poller verifies no gaps in sequences across multiple polls.
#[tokio::test]
async fn test_client_a_pushes_while_client_b_polls() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "child1";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    let store_clone = Arc::clone(&store);
    let child_id_copy = child_id.to_string();

    // Task A: Upsert 50 entities with small delays
    let pusher = tokio::spawn(async move {
        for i in 0..50 {
            let entity_id = format!("tx-{}", i);
            let entity_json = format!(r#"{{"id":"{}"}}"#, entity_id);
            store_clone
                .upsert_entity_with_event(
                    &child_id_copy,
                    EntityType::Transaction,
                    &entity_id,
                    &entity_json,
                    SyncSource::Local,
                )
                .await
                .expect("Failed to upsert entity");
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
    });

    let store_clone = Arc::clone(&store);
    let child_id_copy = child_id.to_string();

    // Task B: Poll repeatedly and collect all sequences seen
    let poller = tokio::spawn(async move {
        let mut all_sequences = vec![];
        let mut last_seq = 0u64;

        // Poll until we have at least 50 events
        while all_sequences.len() < 50 {
            let events = store_clone
                .get_events_since(&child_id_copy, last_seq)
                .await
                .expect("Failed to poll events");

            for event in events {
                if let Some(seq) = event.sequence {
                    if seq > last_seq {
                        all_sequences.push(seq);
                        last_seq = seq;
                    }
                }
            }

            // Small delay between polls
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        all_sequences
    });

    // Wait for both tasks
    let _ = pusher.await;
    let collected = poller.await.expect("Poller task failed");

    assert_eq!(collected.len(), 50, "Should have collected 50 events");
    assert_gapless_sequence(&collected);

    ctx.cleanup().await;
}

/// Test watermark update race condition.
/// Two tasks update the same watermark with interleaved values (even/odd).
/// Verify watermark ends at >= 99.
#[tokio::test]
async fn test_watermark_update_race() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "child1";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    let store_clone = Arc::clone(&store);
    let child_id_copy = child_id.to_string();

    // Task A: Update watermark with even values (2, 4, 6, ..., 100)
    let task_a = tokio::spawn(async move {
        for i in 1..=50 {
            let value = i * 2;
            let _ = store_clone
                .update_watermark(&child_id_copy, "remote", value)
                .await;
        }
    });

    let store_clone = Arc::clone(&store);
    let child_id_copy = child_id.to_string();

    // Task B: Update watermark with odd values (1, 3, 5, ..., 99)
    let task_b = tokio::spawn(async move {
        for i in 0..50 {
            let value = i * 2 + 1;
            let _ = store_clone
                .update_watermark(&child_id_copy, "remote", value)
                .await;
        }
    });

    let _ = task_a.await;
    let _ = task_b.await;

    // Check final watermark
    let checkpoint = store
        .get_checkpoint(child_id)
        .await
        .expect("Failed to get checkpoint");

    assert!(
        checkpoint.remote_watermark >= 99,
        "Final watermark should be >= 99, but was {}",
        checkpoint.remote_watermark
    );

    ctx.cleanup().await;
}

/// Test offline client syncing.
/// Upsert 500 entities, advance remote watermark to 500, leave local at 0.
/// Then poll from 0 and verify all 500 events returned in order.
#[tokio::test]
async fn test_client_offline_then_syncs() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "child1";

    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Upsert 500 entities
    for i in 0..500 {
        let entity_id = format!("tx-{}", i);
        let entity_json = format!(r#"{{"id":"{}"}}"#, entity_id);
        store
            .upsert_entity_with_event(
                child_id,
                EntityType::Transaction,
                &entity_id,
                &entity_json,
                SyncSource::Remote,
            )
            .await
            .expect("Failed to upsert entity");
    }

    // Advance remote watermark to 500
    store
        .update_watermark(child_id, "remote", 500)
        .await
        .expect("Failed to update remote watermark");

    // Verify local watermark is still 0
    let checkpoint = store
        .get_checkpoint(child_id)
        .await
        .expect("Failed to get checkpoint");
    assert_eq!(checkpoint.local_watermark, 0, "Local watermark should be 0");
    assert_eq!(checkpoint.remote_watermark, 500, "Remote watermark should be 500");

    // Now "client comes online" and polls from 0
    let events = store
        .get_events_since(child_id, 0)
        .await
        .expect("Failed to poll events");

    assert_eq!(events.len(), 500, "Should retrieve all 500 events");

    // Extract sequences and verify they're gapless 1-500
    let sequences: Vec<u64> = events
        .iter()
        .filter_map(|e| e.sequence)
        .collect();

    assert_eq!(sequences.len(), 500, "All events should have sequences");
    assert_gapless_sequence(&sequences);

    ctx.cleanup().await;
}
