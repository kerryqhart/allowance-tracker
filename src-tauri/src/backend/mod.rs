//! # Backend Module
//!
//! Contains all non-UI logic for the allowance tracker application.
//! 
//! This module serves as the orchestration layer that brings together:
//! - **Domain**: Business logic and rules for allowance management
//! - **Storage**: Data persistence mechanisms (database, file system, etc.)
//! - **IO**: Interface layer that exposes functionality to the UI
//!
//! The backend is designed to be UI-agnostic, meaning it could theoretically
//! support different frontend frameworks or even CLI interfaces without modification.
//! 
//! ## Architecture
//! 
//! The backend follows a layered architecture:
//! ```
//! UI Layer (Yew frontend)
//!     ↓
//! IO Layer (REST API, handlers)
//!     ↓
//! Domain Layer (Business logic, services)
//!     ↓
//! Storage Layer (Database, persistence)
//! ```
//!
//! ## Key Responsibilities
//! 
//! - Initialize and configure the application state
//! - Set up the REST API router with proper CORS configuration
//! - Coordinate between domain logic and data persistence
//! - Provide a clean separation of concerns for maintainability

pub mod storage;
pub mod domain;
pub mod io;


use axum::{
    http::{HeaderValue, Method},
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use anyhow::Result;

pub use storage::*;
pub use domain::*;
pub use io::*;

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
        .route("/transactions", get(list_transactions).post(create_transaction));

    // Define our main application router
    Router::new()
        .nest("/api", api_routes)
        .layer(cors)
        .with_state(app_state)
}