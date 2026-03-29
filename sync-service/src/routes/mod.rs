mod health;

use axum::Router;
use std::sync::Arc;
use crate::storage::DynamoStore;

pub fn build_router(store: DynamoStore) -> Router {
    let _store = Arc::new(store);
    Router::new()
        .merge(health::routes())
}
