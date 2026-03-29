use axum::{Router, routing::get};

async fn health_check() -> &'static str {
    "ok"
}

pub fn routes() -> Router {
    Router::new().route("/health", get(health_check))
}
