//! # IO Module
//!
//! Provides the interface layer between the user interface and the domain logic.
//!
//! This module serves as the adapter layer that translates UI requests into domain
//! operations and formats domain responses for UI consumption. It handles the
//! communication protocol (REST API), serialization/deserialization, and maintains
//! the boundary between the presentation layer and business logic.
//!
//! ## Key Responsibilities
//!
//! - **API Endpoints**: Exposing REST API endpoints for frontend consumption
//! - **Request/Response Handling**: Processing HTTP requests and formatting responses
//! - **Data Serialization**: Converting between JSON and domain objects
//! - **Error Translation**: Converting domain errors to appropriate HTTP status codes
//! - **Input Validation**: Validating incoming requests before passing to domain
//! - **CORS Management**: Handling cross-origin requests for web frontend
//!
//! ## Current Implementation
//!
//! - **Web Framework**: Axum for high-performance async HTTP handling
//! - **Serialization**: Serde for JSON serialization/deserialization
//! - **State Management**: Axum extractors for dependency injection
//! - **Error Handling**: Structured error responses with appropriate HTTP codes
//!
//! ## API Design Principles
//!
//! - **RESTful Architecture**: Standard HTTP methods and status codes
//! - **Resource-Oriented**: URLs represent resources (transactions, balances)
//! - **Stateless**: Each request contains all necessary information
//! - **Idempotent Operations**: Safe retry behavior for appropriate endpoints
//! - **Consistent Error Format**: Standardized error response structure
//!
//! ## Supported Operations
//!
//! - **GET /api/transactions**: List transactions with pagination and filtering
//! - **POST /api/transactions**: Create new transactions
//! - **Future**: Update, delete, and balance calculation endpoints
//!
//! ## Design Patterns
//!
//! - **Handler Pattern**: Separate handler functions for each endpoint
//! - **Dependency Injection**: Services injected via Axum state
//! - **Result Mapping**: Clean error handling with appropriate HTTP responses
//! - **Request/Response DTOs**: Dedicated types for API communication

pub mod rest;

pub use rest::*; 