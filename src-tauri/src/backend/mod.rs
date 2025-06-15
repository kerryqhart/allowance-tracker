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
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use anyhow::Result;
use crate::backend::domain::{TransactionService, CalendarService, TransactionTableService};
use crate::backend::storage::DbConnection;
use log::info;

pub use storage::*;
pub use domain::*;
pub use io::*;

/// Main application state that holds all services
#[derive(Clone)]
pub struct AppState {
    pub transaction_service: TransactionService,
    pub calendar_service: CalendarService,
    pub transaction_table_service: TransactionTableService,
}

/// Initialize the backend with all required services
pub async fn initialize_backend() -> Result<AppState> {
    info!("Setting up database");
    let db_conn = DbConnection::init().await?;

    info!("Setting up domain model");
    let transaction_service = TransactionService::new(db_conn);
    let calendar_service = CalendarService::new();
    let transaction_table_service = TransactionTableService::new();

    info!("Setting up application state");
    let app_state = AppState {
        transaction_service,
        calendar_service,
        transaction_table_service,
    };

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
        .route("/transactions", get(io::list_transactions).post(io::create_transaction))
        .route("/transactions/table", get(io::get_transaction_table))
        .route("/transactions/validate", post(io::validate_transaction))
        .route("/calendar/month", get(io::get_calendar_month));

    // Define our main application router
    Router::new()
        .nest("/api", api_routes)
        .layer(cors)
        .with_state(app_state)
}