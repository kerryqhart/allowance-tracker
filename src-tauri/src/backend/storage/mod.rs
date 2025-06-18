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
//! - **Primary Storage**: SQLite database with SQLx for type-safe queries
//! - **Development Mode**: Mock data for rapid prototyping
//! - **Future Flexibility**: Designed to support multiple storage backends
//!
//! ## Storage Features
//!
//! - **ACID Compliance**: Atomicity, Consistency, Isolation, Durability
//! - **Type Safety**: Compile-time checked SQL queries with SQLx macros
//! - **Async Operations**: Non-blocking database operations
//! - **Connection Pooling**: Efficient database connection management
//! - **Error Handling**: Graceful handling of storage failures
//!
//! ## Design Principles
//!
//! - **Repository Pattern**: Clean separation between domain and data access
//! - **Interface Segregation**: Focused interfaces for specific data operations
//! - **Dependency Inversion**: Domain depends on storage abstractions, not implementations
//! - **Testability**: Mock implementations for unit testing

pub mod connection;
pub mod repositories;

// Re-export the main types that other modules need
pub use connection::DbConnection;
pub use repositories::{
    TransactionRepository,
    ChildRepository, 
    ParentalControlRepository,
    AllowanceRepository
}; 