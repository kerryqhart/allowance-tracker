pub mod db;
pub mod domain;
pub mod rest;

use std::sync::Arc;
use axum::{
    http::{HeaderValue, Method},
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use anyhow::Result;

pub use db::*;
pub use domain::*;
pub use rest::*;

/// Create application state with initialized services
pub async fn create_app_state() -> Result<AppState> {
    tracing::info!("Setting up database");
    let db_conn = DbConnection::init().await?;
    
    tracing::info!("Setting up domain model");
    let transaction_service = TransactionService::new(db_conn);

    tracing::info!("Setting up application state");
    let app_state = AppState::new(transaction_service);
    
    Ok(app_state)
}

/// Create the Axum router with all routes configured
pub fn create_router(app_state: AppState) -> Router {
    // CORS setup to allow frontend to make requests
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:8080".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    // Set up our application routes
    let api_routes = Router::new()
        .route("/transactions", get(rest::list_transactions).post(rest::create_transaction));

    // Define our main application router
    Router::new()
        .nest("/api", api_routes)
        .layer(cors)
        .with_state(app_state)
} 