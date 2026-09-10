use crate::money::Money;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxType { Allowance, OneOffIncome, Expense, FutureAllowance }

impl TxType {
    pub fn as_csv(&self) -> &'static str {
        match self {
            TxType::Allowance => "allowance",
            TxType::OneOffIncome => "income",
            TxType::Expense => "expense",
            TxType::FutureAllowance => "future_allowance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxRow {
    pub id: String,
    pub child_id: String,
    pub date: DateTime<FixedOffset>,
    pub description: String,
    pub amount: Money,
    /// Derived output. Never compared during a merge — see `intrinsic_eq`.
    pub balance: Money,
    pub tx_type: TxType,
}

impl TxRow {
    /// The canonical total order. `date` alone is not enough: the sort would be
    /// stable over *input* order, which differs per machine.
    pub fn sort_key(&self) -> (DateTime<FixedOffset>, &str) { (self.date, &self.id) }

    /// Equality over intrinsic fields only. `balance` is regenerated output, so
    /// including it would make one non-tail insert mark most of the file as
    /// "changed" and fire the conflict rule across rows that never really
    /// conflicted.
    ///
    /// `date` is compared by its rendered RFC 3339 form, not by instant.
    /// `DateTime<FixedOffset>`'s `PartialEq` compares instants, so two rows at
    /// the same instant but different UTC offsets would compare equal here
    /// while rendering to different bytes on disk. Machines in different
    /// timezones genuinely produce different offsets for "the same" write
    /// (see `transaction_service.rs`'s `Local::now().fixed_offset()`), so
    /// without this, two machines could each believe their own row is
    /// unchanged, keep it, and ping-pong forever instead of converging.
    pub fn intrinsic_eq(&self, other: &TxRow) -> bool {
        self.id == other.id
            && self.child_id == other.child_id
            && self.date.to_rfc3339() == other.date.to_rfc3339()
            && self.description == other.description
            && self.amount == other.amount
            && self.tx_type == other.tx_type
    }
}

/// A row paired with the provenance of the side it came from.
///
/// Provenance is resolved by the caller and passed in, so `merge` stays a total
/// function over data and never walks git history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    pub committer_epoch: i64,
    /// Full commit oid as hex. Ties break on this, so it must be stable and
    /// identical on both machines.
    pub commit_oid: [u8; 20],
}

impl Provenance {
    pub fn short_hex(&self) -> String {
        self.commit_oid.iter().take(4).map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Clone)]
pub struct Sided {
    pub rows: Vec<TxRow>,
    pub provenance: Provenance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;

    fn row_at(id: &str, rfc3339: &str) -> TxRow {
        TxRow {
            id: id.to_string(),
            child_id: "c".to_string(),
            date: DateTime::parse_from_rfc3339(rfc3339).unwrap(),
            description: "d".to_string(),
            amount: Money::from_cents(-100),
            balance: Money::from_cents(0),
            tx_type: TxType::Expense,
        }
    }

    #[test]
    fn intrinsic_eq_treats_same_instant_different_offset_as_different() {
        // Same instant (00:00 UTC == 01:00+01:00), different rendered bytes.
        // Two machines in different timezones would otherwise each believe
        // their own copy is untouched and never converge.
        let utc = row_at("a", "2026-01-01T00:00:00+00:00");
        let plus_one = row_at("a", "2026-01-01T01:00:00+01:00");
        assert_eq!(utc.date, plus_one.date, "instants must actually be equal for this test to mean anything");
        assert!(!utc.intrinsic_eq(&plus_one));
    }

    #[test]
    fn intrinsic_eq_is_true_for_byte_identical_dates() {
        let a = row_at("a", "2026-01-01T00:00:00+00:00");
        let b = row_at("a", "2026-01-01T00:00:00+00:00");
        assert!(a.intrinsic_eq(&b));
    }
}
