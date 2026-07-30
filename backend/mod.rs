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
use crate::backend::domain::SyncNotifier;

// Domain modules
pub mod domain;
pub mod storage;

// Re-export commonly used types
pub use storage::csv::CsvConnection;

/// Main backend struct that orchestrates all services
pub struct Backend {
    pub child_service: domain::child_service::ChildService,
    pub transaction_service: Arc<domain::TransactionService>,
    pub calendar_service: domain::CalendarService,
    pub allowance_service: domain::AllowanceService,
    pub goal_service: domain::GoalService,
    pub parental_control_service: domain::ParentalControlService,
    pub balance_service: domain::BalanceService,
    pub data_directory_service: domain::DataDirectoryService,
    pub export_service: domain::ExportService,
    /// Base data directory (e.g. ~/Documents/Allowance Tracker)
    pub data_dir: std::path::PathBuf,
}

impl Backend {
    /// Resolve the default data directory (`~/Documents/Allowance Tracker`).
    /// Exposed so startup code can load sync persistence before constructing
    /// Backend (which lets us decide whether to pass `Some(SyncNotifier)` at all).
    pub fn default_data_dir() -> Result<std::path::PathBuf> {
        let home_dir = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        Ok(home_dir.join("Documents").join("Allowance Tracker"))
    }

    /// Create a new backend instance with all services
    pub fn new(sync_notifier: Option<SyncNotifier>) -> Result<Self> {
        // Use the real data directory in ~/Documents/Allowance Tracker
        let data_path = Self::default_data_dir()?;
        Self::with_data_dir(data_path, sync_notifier)
    }

    /// Create a backend rooted at a specific data directory.
    ///
    /// [`Backend::new`] delegates here with the default
    /// `~/Documents/Allowance Tracker` path. Tests use it to run against a
    /// temporary directory instead of touching real user data.
    pub fn with_data_dir(
        data_path: std::path::PathBuf,
        sync_notifier: Option<SyncNotifier>,
    ) -> Result<Self> {
        // Load email config path before moving data_path
        let email_config_path = data_path.join("email_config.toml");

        // Create the CSV connection with the real data directory
        log::info!("Backend::new() using real data path: {:?}", data_path);
        let csv_connection = Arc::new(CsvConnection::new(data_path.clone())?);

        // Create services using the Arc<CsvConnection> pattern
        let child_service = domain::child_service::ChildService::new(csv_connection.clone(), sync_notifier.clone());
        let allowance_service = domain::AllowanceService::new(csv_connection.clone());
        let balance_service = domain::BalanceService::new(csv_connection.clone())
            .with_sync_notifier(sync_notifier.clone());

        // Load email config and create TransactionService with email support
        let email_config = domain::EmailConfigService::load_config_or_default(&email_config_path);
        log::info!("Email config loaded: SMTP server = {}", email_config.smtp_server);

        let transaction_service = Arc::new(domain::TransactionService::with_email_service(
            csv_connection.clone(),
            child_service.clone(),
            allowance_service.clone(),
            balance_service.clone(),
            email_config,
            sync_notifier.clone(),
        )?);

        let calendar_service = domain::CalendarService::new();

        let goal_service = domain::GoalService::new(
            csv_connection.clone(),
            child_service.clone(),
            allowance_service.clone(),
            transaction_service.clone(), // Pass Arc
            balance_service.clone(),
            sync_notifier.clone(),
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
            data_dir: data_path,
        })
    }
} 