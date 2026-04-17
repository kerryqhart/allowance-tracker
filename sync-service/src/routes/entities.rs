use axum::{
    extract::{State, Path, Query},
    http::StatusCode,
    Json,
    Router, routing::{get, put, delete},
    body::Body,
};
use serde_json;
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
        Err(e) => {
            eprintln!("upsert_entity failed for {}/{}: {:?}", child_id, entity_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
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
        Err(e) => {
            eprintln!("get_entity failed for {}/{}: {:?}", child_id, entity_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
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
        Err(e) => {
            eprintln!("delete_entity failed for {}/{}: {:?}", child_id, entity_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /entities/child - list all children
async fn list_children(
    State(store): State<Arc<DynamoStore>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    match store.list_all_entities_in_table(EntityType::Child).await {
        Ok(entities) => {
            let json_list: Vec<String> = entities.into_iter().map(|(_, data)| data).collect();
            Ok(Json(json_list))
        }
        Err(e) => {
            eprintln!("list_children failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct ListEntitiesQuery {
    sort: Option<String>,
    limit: Option<i32>,
}

// GET /entities/{entity_type}/{child_id} - list all entities of type for a child
async fn list_entities(
    State(store): State<Arc<DynamoStore>>,
    Path((entity_type_str, child_id)): Path<(String, String)>,
    Query(params): Query<ListEntitiesQuery>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let entity_type = EntityType::from_str(&entity_type_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match store.list_entities_for_child_with_options(
        &child_id,
        entity_type,
        params.sort.as_deref(),
        params.limit,
    ).await {
        Ok(entities) => {
            let json_list: Vec<String> = entities.into_iter().map(|(_, data)| data).collect();
            Ok(Json(json_list))
        }
        Err(e) => {
            eprintln!("list_entities failed for {}/{}: {:?}", entity_type_str, child_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// GET /balance/{child_id} - get current balance for a child
async fn get_balance(
    State(store): State<Arc<DynamoStore>>,
    Path(child_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match store.get_latest_balance(&child_id).await {
        Ok(Some(balance)) => Ok(Json(serde_json::json!({
            "child_id": child_id,
            "balance": balance,
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            eprintln!("get_balance failed for {}: {:?}", child_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new()
        .route("/entities/child", get(list_children))
        .route("/entities/{entity_type}/{child_id}", get(list_entities))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", put(upsert_entity))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", get(get_entity))
        .route("/entities/{entity_type}/{child_id}/{entity_id}", delete(delete_entity))
        .route("/balance/{child_id}", get(get_balance))
}
