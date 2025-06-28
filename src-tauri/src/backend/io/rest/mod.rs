//! # REST API Interface Layer
//!
//! Provides HTTP REST endpoints for the allowance tracker application.
//! This layer handles:
//! - HTTP request/response serialization and deserialization  
//! - Input validation and sanitization
//! - Error translation from domain to HTTP status codes
//! - CORS configuration for frontend integration
//! - Request logging and monitoring
//!
//! ## Key Responsibilities
//!
//! - **API Endpoints**: RESTful HTTP interfaces for all operations
//! - **Error Handling**: Converting domain errors to proper HTTP responses  
//! - **Serialization**: JSON request/response handling
//! - **Input Validation**: Basic input checking before domain layer processing
//! - **Logging**: Request/response logging for debugging and monitoring
//!
//! ## Design Principles
//!
//! - **REST Compliance**: Following RESTful design patterns
//! - **Error Transparency**: Clear error messages for debugging
//! - **Request Logging**: Comprehensive logging for troubleshooting
//! - **Domain Separation**: Pure translation layer without business logic

// Module declarations
pub mod transaction_apis;
pub mod calendar_apis; 
pub mod transaction_table_apis;
pub mod money_management_apis;
pub mod child_apis;
pub mod parental_control_apis;
pub mod allowance_apis;
pub mod data_directory_apis;
pub mod logging_apis;
pub mod goal_apis;
pub mod export_apis;
