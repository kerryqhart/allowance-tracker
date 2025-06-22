//! # SQLite Storage Module
//!
//! This module contains all SQLite-based storage implementations.
//! All SQLite code has been moved here to keep it separate from the main storage module
//! which now focuses on CSV-based storage for the primary application functionality.
//!
//! ## Components
//!
//! - **connection.rs** - SQLite database connection management
//! - **db.rs** - Legacy database operations (kept for compatibility)
//! - **repositories/** - SQLite-based repository implementations
//!
//! ## Usage
//!
//! This module is preserved for future use but is not actively used by the main application,
//! which has migrated to CSV-based storage in the Documents folder.

pub mod connection;
pub mod db;
pub mod repositories;

// Re-export the main types for external use
pub use connection::DbConnection;
pub use repositories::{
    TransactionRepository as SqliteTransactionRepository,
    ChildRepository as SqliteChildRepository,
    AllowanceRepository as SqliteAllowanceRepository,
    ParentalControlRepository as SqliteParentalControlRepository,
}; 