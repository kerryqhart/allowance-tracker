//! # CSV Storage Module
//!
//! This module provides a CSV-based storage implementation for the allowance tracker.
//! It demonstrates that the domain logic is completely storage-agnostic by providing
//! an alternative to the SQL database implementation.
//!
//! ## Features
//!
//! - File-based transaction storage using CSV format
//! - Per-child transaction files (`{child_id}_transactions.csv`)
//! - Full CRUD operations with atomic file writes
//! - Compatible with the same `TransactionStorage` trait as the database implementation
//!
//! ## File Format
//!
//! CSV files have the following structure:
//! ```csv
//! id,child_id,date,description,amount,balance
//! tx_1234567890,child_abc,2024-01-15T10:30:00Z,"Allowance",10.00,10.00
//! tx_1234567891,child_abc,2024-01-16T15:45:00Z,"Spent on toy",-5.00,5.00
//! ```

pub mod connection;
pub mod transaction_repository;
pub mod child_repository;
pub mod allowance_repository;
pub mod parental_control_repository;
pub mod global_config_repository;
pub mod goal_repository;

#[cfg(test)]
pub mod test_utils;

pub use connection::CsvConnection;
pub use transaction_repository::TransactionRepository;
pub use child_repository::ChildRepository;
pub use allowance_repository::AllowanceRepository;
pub use parental_control_repository::ParentalControlRepository;
pub use global_config_repository::{GlobalConfigRepository, GlobalConfig, GlobalConfigStorage};
pub use goal_repository::GoalRepository; 