mod health;
mod sync;
mod entities;

use axum::Router;
use std::sync::Arc;
use crate::storage::DynamoStore;

pub fn build_router(store: DynamoStore) -> Router {
    let store = Arc::new(store);
    Router::new()
        .merge(health::routes())
        .merge(sync::routes())
        .merge(entities::routes())
        .with_state(store)
}
