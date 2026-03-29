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
async fn test_checkpoint_round_trip() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-1";

    // Initialize metadata
    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Get checkpoint (should have all zeros)
    let checkpoint = store
        .get_checkpoint(child_id)
        .await
        .expect("Failed to get checkpoint");

    assert_eq!(checkpoint.child_id, child_id);
    assert_eq!(checkpoint.event_sequence, 0);
    assert_eq!(checkpoint.local_watermark, 0);
    assert_eq!(checkpoint.remote_watermark, 0);

    // Update local watermark to 5
    store
        .update_watermark(child_id, "local", 5)
        .await
        .expect("Failed to update local watermark");

    // Update remote watermark to 3
    store
        .update_watermark(child_id, "remote", 3)
        .await
        .expect("Failed to update remote watermark");

    // Get checkpoint again
    let checkpoint = store
        .get_checkpoint(child_id)
        .await
        .expect("Failed to get checkpoint");

    assert_eq!(checkpoint.child_id, child_id);
    assert_eq!(checkpoint.event_sequence, 0);
    assert_eq!(checkpoint.local_watermark, 5);
    assert_eq!(checkpoint.remote_watermark, 3);

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_watermark_only_moves_forward() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "test-child-2";

    // Initialize metadata
    store
        .initialize_child_metadata(child_id)
        .await
        .expect("Failed to initialize metadata");

    // Set watermark to 10
    store
        .update_watermark(child_id, "local", 10)
        .await
        .expect("Failed to set local watermark to 10");

    // Get checkpoint to verify
    let checkpoint = store
        .get_checkpoint(child_id)
        .await
        .expect("Failed to get checkpoint");
    assert_eq!(checkpoint.local_watermark, 10);

    // Try to set it to 5 (should be ignored due to conditional check)
    store
        .update_watermark(child_id, "local", 5)
        .await
        .expect("Failed to attempt to update watermark (should be idempotent)");

    // Get checkpoint again and verify it's still 10
    let checkpoint = store
        .get_checkpoint(child_id)
        .await
        .expect("Failed to get checkpoint");

    assert_eq!(
        checkpoint.local_watermark, 10,
        "Watermark should not move backward"
    );

    ctx.cleanup().await;
}

#[tokio::test]
async fn test_checkpoint_for_uninitialized_child() {
    let Some((ctx, store)) = setup().await else {
        return;
    };

    let child_id = "nonexistent-child";

    // Try to get checkpoint for a child that was never initialized
    let result = store.get_checkpoint(child_id).await;

    assert!(
        result.is_err(),
        "get_checkpoint should return error for uninitialized child"
    );

    ctx.cleanup().await;
}
