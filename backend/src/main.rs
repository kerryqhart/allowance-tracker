use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use shared::KeyValue;
use sqlx::Pool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{info, Level};

// Import our database module
mod db;

// Application state that will be shared across handlers
struct AppState {
    db_pool: Pool<sqlx::Sqlite>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Setting up database");
    // Initialize the database using our db module
    let db_pool = db::init_db().await?;
    
    // Set up our application state
    let state = Arc::new(AppState { db_pool });

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
        .with_state(state);

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Starting server on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on {}", addr);
    
    axum::serve(listener, app).await?;

    Ok(())
}

// API handlers
async fn get_value(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    // Use the database module to get the value
    match db::get_value(&state.db_pool, &key).await {
        Ok(Some(kv)) => (StatusCode::OK, Json(kv)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn put_value(
    State(state): State<Arc<AppState>>,
    Json(kv): Json<KeyValue>,
) -> impl IntoResponse {
    // Use the database module to store the value
    match db::put_value(&state.db_pool, &kv).await {
        Ok(_) => (StatusCode::CREATED, Json(kv)).into_response(),
        Err(e) => {
            println!("Error: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store value").into_response()
        },
    }
}
