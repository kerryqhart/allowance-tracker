//! src-tauri/src/backend/domain/models/child.rs

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Domain model representing a child in the system.
/// This model contains the core business information and logic for a child.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Child {
    pub id: String,
    pub name: String,
    pub birthdate: NaiveDate,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Child {
    /// Generate a unique ID for a child
    pub fn generate_id(timestamp_millis: u64) -> String {
        format!("child::{}", timestamp_millis)
    }
}

/// Represents the active child, which could be None if no child is selected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveChild {
    pub child: Option<Child>,
} 