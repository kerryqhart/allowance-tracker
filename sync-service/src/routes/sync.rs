use axum::{
    extract::{Query, State, Path},
    http::StatusCode,
    Router, routing::{get, post, put},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use shared::sync::{SyncEvent, SyncCheckpoint};
use crate::storage::DynamoStore;


#[derive(Debug, Deserialize)]
pub struct GetEventsQuery {
    pub child_id: String,
    pub since: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct GetEventsResponse {
    pub events: Vec<SyncEvent>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWatermarkRequest {
    pub which: String,
    pub value: u64,
}

// GET /sync/events?child_id=X&since=N - get events since sequence
async fn get_events(
    State(store): State<Arc<DynamoStore>>,
    Query(params): Query<GetEventsQuery>,
) -> Result<Json<GetEventsResponse>, StatusCode> {
    let since = params.since.unwrap_or(0);
    match store.get_events_since(&params.child_id, since).await {
        Ok(events) => Ok(Json(GetEventsResponse { events })),
        Err(e) => {
            eprintln!("get_events failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// POST /sync/initialize/{child_id} - initialize child metadata
async fn initialize_child(
    State(store): State<Arc<DynamoStore>>,
    Path(child_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    match store.initialize_child_metadata(&child_id).await {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(e) => {
            eprintln!("initialize_child failed for {}: {:?}", child_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /sync/checkpoint/{child_id} - get checkpoint
async fn get_checkpoint(
    State(store): State<Arc<DynamoStore>>,
    Path(child_id): Path<String>,
) -> Result<Json<SyncCheckpoint>, StatusCode> {
    match store.get_checkpoint(&child_id).await {
        Ok(checkpoint) => Ok(Json(checkpoint)),
        Err(e) => {
            eprintln!("get_checkpoint failed for {}: {:?}", child_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// PUT /sync/checkpoint/{child_id} - update watermark
async fn update_checkpoint(
    State(store): State<Arc<DynamoStore>>,
    Path(child_id): Path<String>,
    Json(req): Json<UpdateWatermarkRequest>,
) -> Result<StatusCode, StatusCode> {
    match store.update_watermark(&child_id, &req.which, req.value).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => {
            eprintln!("update_checkpoint failed for {}: {:?}", child_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new()
        .route("/sync/events", get(get_events))
        .route("/sync/initialize/{child_id}", post(initialize_child))
        .route("/sync/checkpoint/{child_id}", get(get_checkpoint))
        .route("/sync/checkpoint/{child_id}", put(update_checkpoint))
}
