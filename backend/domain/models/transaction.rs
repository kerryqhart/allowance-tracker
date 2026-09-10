//! Domain model for a transaction.
use serde::{Deserialize, Serialize};
use chrono::{DateTime, FixedOffset};
use allowance_core::money::Money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionType {
    Allowance,
    OneOffIncome,
    Expense,
    FutureAllowance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub child_id: String,
    pub date: DateTime<FixedOffset>,  // FIXED: Now uses proper DateTime object
    pub description: String,
    pub amount: Money,
    pub balance: Money,
    pub transaction_type: TransactionType,
}

impl Transaction {
    /// Sentinel balance for a transaction (e.g. a not-yet-materialized future
    /// allowance) whose balance `BalanceService` has not calculated yet.
    ///
    /// The old `f64` balance field used `f64::NAN` for this. `Money` has no
    /// NaN, so this in-domain sentinel takes its place. No real transaction
    /// balance can reach `i64::MIN` cents, so it is safe to reuse as a marker.
    pub const BALANCE_PENDING: Money = Money::from_cents(i64::MIN);

    /// Generate a unique transaction ID based on amount and current timestamp.
    /// Format: <type>-<timestamp_ms>-<random_suffix>
    /// Example: in-1625846400123-af3c
    pub fn generate_id(amount: Money, timestamp_ms: u64) -> String {
        let tx_type = if amount.cents() >= 0 { "in" } else { "ex" };
        format!("{}-{}-{:04x}", tx_type, timestamp_ms, rand::random::<u16>())
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

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_differ_within_one_clock_tick() {
        // The old implementation derived the suffix from the clock, so ids minted
        // inside one tick collided — the exact case two machines hit at once.
        let ids: std::collections::HashSet<String> = (0..1000)
            .map(|_| Transaction::generate_id(Money::from_cents(-500), 1_702_516_125_000))
            .collect();
        assert!(ids.len() > 990, "only {} distinct ids from 1000 draws", ids.len());
    }

    #[test]
    fn format_is_unchanged_so_existing_ids_still_parse() {
        let id = Transaction::generate_id(Money::from_cents(-500), 1_702_516_125_000);
        let (kind, ts) = Transaction::parse_id(&id).unwrap();
        assert_eq!(kind, "ex");
        assert_eq!(ts, 1_702_516_125_000);
        assert_eq!(Transaction::parse_id("in-1625846400123-af3c").unwrap().0, "in");
    }
}