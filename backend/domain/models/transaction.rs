//! Domain model for a transaction.
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{DateTime, FixedOffset};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    Income,
    Expense,
    FutureAllowance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub child_id: String,
    pub date: DateTime<FixedOffset>,  // ✅ FIXED: Now uses proper DateTime object
    pub description: String,
    pub amount: f64,
    pub balance: f64,
    pub transaction_type: TransactionType,
}

impl Transaction {
    /// Generate a unique transaction ID based on amount and current timestamp.
    /// Format: <type>-<timestamp_ms>-<random_suffix>
    /// Example: in-1625846400123-af3c
    pub fn generate_id(amount: f64, timestamp_ms: u64) -> String {
        let tx_type = if amount >= 0.0 { "in" } else { "ex" };
        let random_suffix = Self::generate_random_suffix(4);
        format!("{}-{}-{}", tx_type, timestamp_ms, random_suffix)
    }

    /// Parse a transaction ID to extract its type and timestamp.
    pub fn parse_id(id: &str) -> Result<(&str, u64), String> {
        let parts: Vec<&str> = id.split('-').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid transaction ID format: {}", id));
        }
        let tx_type = parts[0];
        let timestamp = parts[1]
            .parse::<u64>()
            .map_err(|_| format!("Invalid timestamp in ID: {}", parts[1]))?;
        Ok((tx_type, timestamp))
    }

    /// Generate a random hex suffix for transaction IDs.
    fn generate_random_suffix(len: usize) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();
        format!("{:x}", now % (16_u128.pow(len as u32)))
            .chars()
            .take(len)
            .collect()
    }
}