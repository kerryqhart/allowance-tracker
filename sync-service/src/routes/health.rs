use axum::{Router, routing::get};
use std::sync::Arc;
use crate::storage::DynamoStore;

async fn health_check() -> &'static str {
    "ok"
}

pub fn routes() -> Router<Arc<DynamoStore>> {
    Router::new().route("/health", get(health_check))
}
