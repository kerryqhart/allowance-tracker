//! # Domain Module
//!
//! Contains all business logic for the allowance tracker application.
//!
//! This module encapsulates the core business rules, entities, and services
//! that define how allowances are modeled, calculated, and managed. It operates
//! independently of any specific UI framework or storage mechanism.
//!
//! ## Key Responsibilities
//!
//! - **Transaction Management**: Creating, validating, and processing allowance transactions
//! - **Balance Calculations**: Computing current balances from transaction history
//! - **Business Rule Enforcement**: Ensuring transactions follow allowance rules
//! - **Data Validation**: Validating input data according to business requirements
//! - **Service Layer**: Providing high-level operations for the application
//!
//! ## Core Concepts
//!
//! - **Transaction**: A single allowance event (earning, spending, bonus, deduction)
//! - **Balance**: The current amount available in the allowance account
//! - **Transaction Service**: The main service that orchestrates transaction operations
//!
//! ## Business Rules
//!
//! - Transactions must have non-empty descriptions
//! - Amounts can be positive (earning) or negative (spending)
//! - Each transaction is timestamped for proper chronological ordering
//! - Balance calculations consider all historical transactions
//!
//! ## Design Principles
//!
//! - **Domain-Driven Design**: Models real-world allowance concepts
//! - **Single Responsibility**: Each service has a focused purpose
//! - **Testability**: Pure functions and clear interfaces for easy testing
//! - **Storage Agnostic**: Works with any storage implementation

pub mod domain;

pub use domain::*; 