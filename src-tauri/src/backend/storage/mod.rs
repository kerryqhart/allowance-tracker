//! # Storage Module
//!
//! Handles all data persistence operations for the allowance tracker application.
//!
//! This module abstracts away the specific storage implementation details and provides
//! a consistent interface for persisting and retrieving data. The implementation can
//! be swapped out (SQLite, PostgreSQL, flat files, cloud storage, etc.) without
//! affecting the domain logic or UI layers.
//!
//! ## Key Responsibilities
//!
//! - **Data Persistence**: Saving transactions and application state to disk
//! - **Data Retrieval**: Loading stored data back into memory
//! - **Storage Abstraction**: Providing a consistent API regardless of storage backend
//! - **Connection Management**: Handling database connections and lifecycle
//! - **Migration Support**: Managing database schema changes over time
//! - **Transaction Safety**: Ensuring data consistency through atomic operations
//!
//! ## Current Implementation
//!
//! - **Primary Storage**: CSV files in ~/Documents/Allowance Tracker
//! - **SQLite Storage**: Available in sqlite sub-module for future use
//! - **Development Mode**: Mock data for rapid prototyping
//! - **Future Flexibility**: Designed to support multiple storage backends
//!
//! ## Storage Features
//!
//! - **File-based Storage**: Human-readable CSV and YAML files
//! - **Cross-platform Compatibility**: Works on all platforms with Documents folder
//! - **Easy Backup**: Simple file copying for data backup
//! - **Version Control Friendly**: Text-based formats for easy diffing
//! - **Storage Abstraction**: Common traits for different storage backends
//!
//! ## Design Principles
//!
//! - **Repository Pattern**: Clean separation between domain and data access
//! - **Interface Segregation**: Focused interfaces for specific data operations
//! - **Dependency Inversion**: Domain depends on storage abstractions, not implementations
//! - **Testability**: Mock implementations for unit testing

pub mod traits;
pub mod csv;
pub mod sqlite;
pub mod git;

// Re-export the main types that other modules need
pub use csv::CsvConnection;
pub use traits::{Connection, TransactionStorage, ChildStorage, AllowanceStorage, ParentalControlStorage, GoalStorage};
pub use csv::{GlobalConfig, GlobalConfigStorage};
pub use git::GitManager;

// SQLite components are available via the sqlite sub-module
// Example: use crate::backend::storage::sqlite::DbConnection; 