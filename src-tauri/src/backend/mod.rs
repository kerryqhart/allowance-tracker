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
//! ```text
//! UI Layer (Yew frontend)
//!     |
//!     v
//! IO Layer (REST API, handlers)
//!     |
//!     v
//! Domain Layer (Business logic, services)
//!     |
//!     v
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
use crate::backend::domain::{TransactionService, CalendarService, TransactionTableService, MoneyManagementService, child_service::ChildService, ParentalControlService, AllowanceService, BalanceService, GoalService, DataDirectoryService, ExportService};
use crate::backend::storage::CsvConnection;
use log::info;

// Re-exports removed to avoid unused import warnings

/// Main application state that holds all services
#[derive(Clone)]
pub struct AppState {
    pub transaction_service: TransactionService<CsvConnection>,
    pub calendar_service: CalendarService,
    pub transaction_table_service: TransactionTableService,
    pub money_management_service: MoneyManagementService,
    pub child_service: ChildService,
    pub parental_control_service: ParentalControlService,
    pub allowance_service: AllowanceService,
    pub balance_service: BalanceService<CsvConnection>,
    pub goal_service: GoalService,
    pub data_directory_service: DataDirectoryService,
    pub export_service: ExportService,
}

/// Initialize the backend with all required services
pub async fn initialize_backend() -> Result<AppState> {
    info!("Setting up CSV storage for all data");
    let csv_conn = std::sync::Arc::new(CsvConnection::new_default()?);
    
    // Log the data directory location for user reference
    info!("Allowance Tracker data directory: {:?}", csv_conn.base_directory());

    info!("Setting up domain model with CSV storage");
    let calendar_service = CalendarService::new();
    let transaction_table_service = TransactionTableService::new();
    let money_management_service = MoneyManagementService::new();
    let child_service = ChildService::new(csv_conn.clone());
    let parental_control_service = ParentalControlService::new(csv_conn.clone());
    let allowance_service = AllowanceService::new(csv_conn.clone());
    let balance_service = BalanceService::new(csv_conn.clone());
    let transaction_service = TransactionService::new(csv_conn.clone(), child_service.clone(), allowance_service.clone(), balance_service.clone());
    let goal_service = GoalService::new(csv_conn.clone(), child_service.clone(), allowance_service.clone(), transaction_service.clone(), balance_service.clone());
    let data_directory_service = DataDirectoryService::new(csv_conn.clone(), std::sync::Arc::new(child_service.clone()));
    let export_service = ExportService::new();

    info!("Setting up application state");
    let app_state = AppState {
        transaction_service,
        calendar_service,
        transaction_table_service,
        money_management_service,
        child_service,
        parental_control_service,
        allowance_service,
        balance_service,
        goal_service,
        data_directory_service,
        export_service,
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
        .route("/transactions", get(io::transaction_apis::list_transactions).post(io::transaction_apis::create_transaction).delete(io::transaction_apis::delete_transactions))
        .nest("/calendar", io::calendar_apis::router())
        .nest("/transactions", io::transaction_table_apis::router())
        .route("/money/add", post(io::money_management_apis::add_money))
        .route("/money/spend", post(io::money_management_apis::spend_money))
        .route("/children", get(io::child_apis::list_children).post(io::child_apis::create_child))
        .route("/children/:id", get(io::child_apis::get_child_by_id).put(io::child_apis::update_child).delete(io::child_apis::delete_child))
        .route("/active-child", get(io::child_apis::get_active_child).post(io::child_apis::set_active_child))
        .nest("/parental-control", io::parental_control_apis::router())
        .nest("/allowance", io::allowance_apis::router())
        .nest("/goals", io::goal_apis::router())
        .nest("/export", io::export_apis::router())
        .nest("/data-directory", io::data_directory_apis::create_data_directory_routes())
        .route("/logs", post(io::logging_apis::log_message));

    // Define our main application router
    Router::new()
        .nest("/api", api_routes)
        .layer(cors)
        .with_state(app_state)
}