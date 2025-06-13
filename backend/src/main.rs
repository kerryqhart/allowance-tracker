use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use shared::KeyValue;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{info, Level};

// Import our modules
mod db;
mod domain;

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
    let value_store = domain::ValueStore::new(db_conn);

    // CORS setup to allow frontend to make requests
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:8080".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(Any);

    // Set up our application routes
    let api_routes = Router::new()
        .route("/values/:key", get(get_value))
        .route("/values", post(put_value));

    // Define our main application router
    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(ServeDir::new(PathBuf::from("../frontend/dist")))
        .layer(cors)
        .with_state(AppState { value_store });

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Starting server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on {}", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}

// We'll use this to store our value store in request extensions
#[derive(Clone)]
struct AppState {
    value_store: domain::ValueStore,
}

// API handlers
async fn get_value(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    // Use the domain model to retrieve the value
    match state.value_store.retrieve_value(&key).await {
        Ok(Some(kv)) => (StatusCode::OK, Json(kv)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Error retrieving value").into_response(),
    }
}

async fn put_value(
    State(state): State<AppState>,
    Json(kv): Json<KeyValue>,
) -> impl IntoResponse {
    // Use the domain model to store the value
    match state.value_store.store_value(&kv.key, &kv.value).await {
        Ok(stored_kv) => (StatusCode::CREATED, Json(stored_kv)).into_response(),
        Err(e) => {
            println!("Error: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store value").into_response()
        },
    }
}
