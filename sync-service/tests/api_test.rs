use sync_service::{create_local_dynamo_client, storage::DynamoStore, routes::build_router};
use shared::sync::{SyncEvent, SyncAction, SyncSource, EntityType};

const DYNAMO_LOCAL_PORT: u16 = 8000;

async fn is_dynamo_local_available(port: u16) -> bool {
    match tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

async fn start_test_server() -> Option<String> {
    if !is_dynamo_local_available(DYNAMO_LOCAL_PORT).await {
        return None;
    }

    let dynamo_client = match create_local_dynamo_client(DYNAMO_LOCAL_PORT).await {
        Ok(client) => client,
        Err(_) => return None,
    };

    let prefix = format!("test_{}", uuid::Uuid::new_v4());
    let store = DynamoStore::new(dynamo_client, prefix);

    if store.create_tables().await.is_err() {
        return None;
    }

    let router = build_router(store);
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
        Ok(l) => l,
        Err(_) => return None,
    };

    let addr = match listener.local_addr() {
        Ok(a) => a,
        Err(_) => return None,
    };

    let base_url = format!("http://{}", addr);

    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    Some(base_url)
}

#[tokio::test]
async fn test_health_endpoint() {
    let Some(base_url) = start_test_server().await else {
        println!("DynamoDB Local not available, skipping test");
        return;
    };

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .expect("Failed to make request");

    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "ok");
}

#[tokio::test]
async fn test_push_events_endpoint() {
    let Some(base_url) = start_test_server().await else {
        println!("DynamoDB Local not available, skipping test");
        return;
    };

    let client = reqwest::Client::new();

    // Initialize child
    let res = client
        .post(format!("{}/sync/initialize/child1", base_url))
        .send()
        .await
        .expect("Failed to initialize child");
    assert_eq!(res.status(), 201);

    // Push events
    let event = SyncEvent::new(
        EntityType::Transaction,
        "tx1".to_string(),
        "child1".to_string(),
        SyncAction::Created,
        SyncSource::Local,
    );

    let res = client
        .post(format!("{}/sync/events", base_url))
        .json(&serde_json::json!({ "events": vec![event.clone()] }))
        .send()
        .await
        .expect("Failed to push events");

    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["sequences"].is_array());
    assert_eq!(body["sequences"].as_array().unwrap().len(), 1);

    // Get events
    let res = client
        .get(format!("{}/sync/events?child_id=child1&since=0", base_url))
        .send()
        .await
        .expect("Failed to get events");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["events"].is_array());
    assert!(body["events"].as_array().unwrap().len() > 0);
}

#[tokio::test]
async fn test_entity_crud_endpoints() {
    let Some(base_url) = start_test_server().await else {
        println!("DynamoDB Local not available, skipping test");
        return;
    };

    let client = reqwest::Client::new();

    // PUT: Create entity
    let entity_json = serde_json::json!({
        "id": "goal1",
        "name": "Save $100",
        "amount": 100
    });

    let res = client
        .put(format!("{}/entities/goal/child1/goal1", base_url))
        .body(entity_json.to_string())
        .send()
        .await
        .expect("Failed to create entity");

    assert_eq!(res.status(), 200);

    // GET: Retrieve entity
    let res = client
        .get(format!("{}/entities/goal/child1/goal1", base_url))
        .send()
        .await
        .expect("Failed to get entity");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["id"], "goal1");
    assert_eq!(body["name"], "Save $100");

    // DELETE: Remove entity
    let res = client
        .delete(format!("{}/entities/goal/child1/goal1", base_url))
        .send()
        .await
        .expect("Failed to delete entity");

    assert_eq!(res.status(), 204);

    // GET: Verify deletion (should return 404)
    let res = client
        .get(format!("{}/entities/goal/child1/goal1", base_url))
        .send()
        .await
        .expect("Failed to verify deletion");

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_checkpoint_endpoints() {
    let Some(base_url) = start_test_server().await else {
        println!("DynamoDB Local not available, skipping test");
        return;
    };

    let client = reqwest::Client::new();

    // Initialize child
    let res = client
        .post(format!("{}/sync/initialize/child1", base_url))
        .send()
        .await
        .expect("Failed to initialize child");
    assert_eq!(res.status(), 201);

    // GET: Get checkpoint
    let res = client
        .get(format!("{}/sync/checkpoint/child1", base_url))
        .send()
        .await
        .expect("Failed to get checkpoint");

    assert_eq!(res.status(), 200);
    let checkpoint: serde_json::Value = res.json().await.unwrap();
    assert_eq!(checkpoint["child_id"], "child1");
    assert_eq!(checkpoint["event_sequence"], 0);
    assert_eq!(checkpoint["local_watermark"], 0);
    assert_eq!(checkpoint["remote_watermark"], 0);

    // PUT: Update watermark
    let res = client
        .put(format!("{}/sync/checkpoint/child1", base_url))
        .json(&serde_json::json!({
            "which": "local",
            "value": 42
        }))
        .send()
        .await
        .expect("Failed to update checkpoint");

    assert_eq!(res.status(), 200);

    // GET: Verify watermark was updated
    let res = client
        .get(format!("{}/sync/checkpoint/child1", base_url))
        .send()
        .await
        .expect("Failed to verify checkpoint");

    assert_eq!(res.status(), 200);
    let checkpoint: serde_json::Value = res.json().await.unwrap();
    assert_eq!(checkpoint["local_watermark"], 42);
}
