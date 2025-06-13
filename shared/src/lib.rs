use serde::{Deserialize, Serialize};

/// A simple key-value pair for storage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]  
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

/// API routes for the application
pub mod routes {
    pub const GET_VALUE: &str = "/api/values/:key";
    pub const PUT_VALUE: &str = "/api/values";
}
