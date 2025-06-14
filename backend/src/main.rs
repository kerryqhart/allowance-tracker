use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    http::{HeaderValue, Method},
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{info, Level};

// Import our modules
mod db;
mod domain;
mod rest;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Setting up database");
    // Initialize the database
    let db_conn = db::DbConnection::init().await?;
    
    // Create our domain model
    info!("Setting up domain model");
    let transaction_service = domain::TransactionService::new(db_conn);

    // Create application state
    info!("Setting up application state");
    let app_state = rest::AppState::new(transaction_service);

    // CORS setup to allow frontend to make requests
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:8080".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    // Set up our application routes
    let api_routes = Router::new()
        .route("/transactions", get(rest::list_transactions).post(rest::create_transaction));

    // Define our main application router
    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(ServeDir::new(PathBuf::from("frontend/dist")))
        .layer(cors)
        .with_state(app_state);

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Starting server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on {}", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}
