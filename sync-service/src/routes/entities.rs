use axum::{
    extract::{State, Path},
    http::StatusCode,
    Router, routing::{get, put, delete},
    body::Body,
};
use std::sync::Arc;
use shared::sync::EntityType;
use crate::storage::DynamoStore;

// PUT /entities/{entity_type}/{child_id}/{entity_id} - upsert entity
async fn upsert_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
    body: Body,
) -> Result<StatusCode, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let entity_json = String::from_utf8(bytes.to_vec())
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match store.upsert_entity(&child_id, entity_type, &entity_id, &entity_json).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// GET /entities/{entity_type}/{child_id}/{entity_id} - get entity
async fn get_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
) -> Result<String, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match store.get_entity(&child_id, entity_type, &entity_id).await {
        Ok(Some(json)) => Ok(json),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// DELETE /entities/{entity_type}/{child_id}/{entity_id} - delete entity
async fn delete_entity(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id, entity_id)): Path<(String, String, String)>,
) -> Result<StatusCode, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match store.delete_entity(&child_id, entity_type, &entity_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new()
        .route("/entities/{entity_type}/{child_id}/{entity_id}", put(upsert_entity))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", get(get_entity))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", delete(delete_entity))
}
