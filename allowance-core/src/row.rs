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
    pub fn intrinsic_eq(&self, other: &TxRow) -> bool {
        self.id == other.id
            && self.child_id == other.child_id
            && self.date == other.date
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
