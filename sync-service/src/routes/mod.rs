mod health;
mod sync;
mod entities;

use axum::{Router, middleware, extract::Request, response::Response};
use std::sync::Arc;
use crate::storage::DynamoStore;

async fn log_request(req: Request, next: middleware::Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let response = next.run(req).await;
    let status = response.status();
    eprintln!("{} {} -> {}", method, uri, status.as_u16());
    response
}

pub fn build_router(store: DynamoStore) -> Router {
    let store = Arc::new(store);

    let api_routes = Router::new()
        .merge(health::routes())
        .merge(sync::routes())
        .merge(entities::routes());

    Router::new()
        .merge(api_routes.clone())
        .nest("/internal", api_routes)
        .layer(middleware::from_fn(log_request))
        .with_state(store)
}
