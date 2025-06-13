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
use sqlx::{migrate::MigrateDatabase, Pool, Row, Sqlite, SqlitePool};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{info, Level};

// Application state that will be shared across handlers
struct AppState {
    db: Pool<Sqlite>,
}

const DATABASE_URL: &str = "sqlite:keyvalue.db";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Setting up database");
    // Create database if it doesn't exist
    if !Sqlite::database_exists(DATABASE_URL).await.unwrap_or(false) {
        info!("Creating database {}", DATABASE_URL);
        Sqlite::create_database(DATABASE_URL).await?
    }

    // Connect to the database
    let db = SqlitePool::connect(DATABASE_URL).await?;

    // Setup database schema if needed
    setup_database(&db).await?;

    // Set up our application state
    let state = Arc::new(AppState { db });

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

async fn setup_database(db: &SqlitePool) -> anyhow::Result<()> {
    // Create our database table if it doesn't exist
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS key_values (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .execute(db)
    .await?;

    Ok(())
}



// API handlers
async fn get_value(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    // Get value for the given key
    let result = sqlx::query("SELECT value FROM key_values WHERE key = ?")
        .bind(key.clone())
        .map(|row: sqlx::sqlite::SqliteRow| {
            row.get::<String, _>("value")
        })
        .fetch_optional(&state.db)
        .await;

    match result {
        Ok(Some(value)) => {
            let kv = KeyValue {
                key,
                value,
            };
            (StatusCode::OK, Json(kv)).into_response()
        },
        Ok(None) => (StatusCode::NOT_FOUND, "Key not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

async fn put_value(
    State(state): State<Arc<AppState>>,
    Json(kv): Json<KeyValue>,
) -> impl IntoResponse {
    // Insert or replace value for the given key
    let result = sqlx::query(
        "INSERT OR REPLACE INTO key_values (key, value) VALUES (?, ?)",
    )
    .bind(&kv.key)
    .bind(&kv.value)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(kv)).into_response(),
        Err(e) => {
            println!("Error: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to store value").into_response()
        },
    }
}
