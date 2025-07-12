//! # Domain Module
//!
//! Contains all business logic for the allowance tracker application.
//!
//! This module encapsulates the core business rules, entities, and services
//! that define how allowances are modeled, calculated, and managed. It operates
//! independently of any specific UI framework or storage mechanism.
//!
//! ## Module Organization
//!
//! - **transaction_service**: Core transaction CRUD operations and business logic
//! - **transaction_table**: Transaction table formatting and display logic
//! - **calendar**: Calendar view generation and date-based transaction organization
//! - **money_management**: Money form handling, validation, and user interactions
//!
//! ## Key Responsibilities
//!
//! - **Transaction Management**: Creating, validating, and processing allowance transactions
//! - **Balance Calculations**: Computing current balances from transaction history
//! - **Business Rule Enforcement**: Ensuring transactions follow allowance rules
//! - **Data Validation**: Validating input data according to business requirements
//! - **Service Layer**: Providing high-level operations for the application
//! - **Calendar Operations**: Managing calendar views and date-based transaction organization
//! - **Transaction Display**: Formatting and validating transaction table data
//! - **Form Management**: Handling money addition forms and user input validation
//!
//! ## Core Concepts
//!
//! - **Transaction**: A single allowance event (earning, spending, bonus, deduction)
//! - **Balance**: The current amount available in the allowance account
//! - **Transaction Service**: The main service that orchestrates transaction operations
//! - **Calendar Service**: Handles calendar views, date calculations, and transaction organization
//! - **Transaction Table Service**: Manages transaction display formatting and validation
//! - **Money Management Service**: Handles money addition forms and validation
//!
//! ## Business Rules
//!
//! - Transactions must have non-empty descriptions
//! - Amounts can be positive (earning) or negative (spending)
//! - Each transaction is timestamped for proper chronological ordering
//! - Balance calculations consider all historical transactions
//! - Calendar views organize transactions by date with proper balance calculations
//! - Transaction table displays format data consistently for user consumption
//! - Money forms validate input before allowing submission
//!
//! ## Design Principles
//!
//! - **Domain-Driven Design**: Models real-world allowance concepts
//! - **Single Responsibility**: Each service has a focused purpose
//! - **Testability**: Pure functions and clear interfaces for easy testing
//! - **Storage Agnostic**: Works with any storage implementation
//! - **UI Agnostic**: Business logic separate from presentation concerns
//! - **Configuration Driven**: Flexible formatting and display options

pub mod transaction_service;
pub mod transaction_table;
pub mod calendar;
pub mod money_management;
pub mod child_service;
pub mod parental_control_service;
pub mod allowance_service;
pub mod balance_service;
pub mod goal_service;
pub mod data_directory_service;
pub mod export_service;
pub mod commands;
pub mod models;

pub use transaction_service::*;
pub use transaction_table::*;
pub use calendar::*;
pub use money_management::*;
pub use parental_control_service::*;
pub use allowance_service::*;
pub use balance_service::*;
pub use goal_service::*;
pub use data_directory_service::*;
pub use export_service::*;
pub use commands::*; 