//! # Backend Module for egui Frontend
//!
//! This backend module provides direct access to domain services and storage
//! for the egui frontend. Unlike the Tauri version, this backend:
//! - Uses synchronous operations (no async/await)
//! - Provides direct access to domain services
//! - Excludes the IO/REST layer entirely
//! - Is optimized for desktop-only operation

use anyhow::Result;
use std::sync::Arc;

// Domain modules
pub mod domain;
pub mod storage;

// Re-export commonly used types
pub use storage::csv::CsvConnection;

/// Main backend struct that orchestrates all services
pub struct Backend {
    pub child_service: domain::child_service::ChildService,
    pub transaction_service: domain::TransactionService<CsvConnection>,
    pub calendar_service: domain::CalendarService,
    pub allowance_service: domain::AllowanceService,
    pub goal_service: domain::GoalService,
    pub parental_control_service: domain::ParentalControlService,
    pub balance_service: domain::BalanceService<CsvConnection>,
    pub data_directory_service: domain::DataDirectoryService,
    pub export_service: domain::ExportService,
}

impl Backend {
    /// Create a new backend instance with all services
    pub fn new() -> Result<Self> {
        // Use the real data directory in ~/Documents/Allowance Tracker
        let home_dir = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let data_path = home_dir.join("Documents").join("Allowance Tracker");
        
        // Create the CSV connection with the real data directory
        log::info!("🔍 Backend::new() using real data path: {:?}", data_path);
        let csv_connection = Arc::new(CsvConnection::new(data_path)?);
        
        // Create services using the Arc<CsvConnection> pattern
        let child_service = domain::child_service::ChildService::new(csv_connection.clone());
        let allowance_service = domain::AllowanceService::new(csv_connection.clone());
        let balance_service = domain::BalanceService::new(csv_connection.clone());
        
        let transaction_service = domain::TransactionService::new(
            csv_connection.clone(),
            child_service.clone(),
            allowance_service.clone(),
            balance_service.clone(),
        );
        
        let calendar_service = domain::CalendarService::new();
        
        let goal_service = domain::GoalService::new(
            csv_connection.clone(),
            child_service.clone(),
            allowance_service.clone(),
            transaction_service.clone(),
            balance_service.clone(),
        );
        
        let parental_control_service = domain::ParentalControlService::new(csv_connection.clone());
        
        let data_directory_service = domain::DataDirectoryService::new(
            csv_connection.clone(),
            Arc::new(child_service.clone()),
        );
        
        let export_service = domain::ExportService::new();
        
        Ok(Backend {
            child_service,
            transaction_service,
            calendar_service,
            allowance_service,
            goal_service,
            parental_control_service,
            balance_service,
            data_directory_service,
            export_service,
        })
    }
} 