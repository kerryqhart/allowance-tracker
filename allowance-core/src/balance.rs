use crate::money::Money;
use crate::row::TxRow;

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceMismatch {
    pub id: String,
    pub expected: Money,
    pub found: Money,
}

/// Sort into canonical order and rewrite every running balance.
///
/// Pure: no repository, no filesystem. The repository-bound
/// `recalculate_balances_from_date` calls this rather than reimplementing it.
pub fn recompute_running_balances(rows: &mut [TxRow]) {
    rows.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    let mut running = Money::from_cents(0);
    for row in rows.iter_mut() {
        running = running + row.amount;
        row.balance = running;
    }
}

/// Exact check — no epsilon. With `Money` there is nothing to tolerate.
pub fn validate(rows: &[TxRow]) -> Vec<BalanceMismatch> {
    let mut sorted: Vec<&TxRow> = rows.iter().collect();
    sorted.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    let mut running = Money::from_cents(0);
    let mut out = Vec::new();
    for row in sorted {
        running = running + row.amount;
        if row.balance != running {
            out.push(BalanceMismatch {
                id: row.id.clone(),
                expected: running,
                found: row.balance,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::row::{TxRow, TxType};
    use chrono::DateTime;

    fn row(id: &str, date: &str, cents: i64) -> TxRow {
        TxRow {
            id: id.to_string(),
            child_id: "c".to_string(),
            date: DateTime::parse_from_rfc3339(date).unwrap(),
            description: "d".to_string(),
            amount: Money::from_cents(cents),
            balance: Money::from_cents(0),
            tx_type: TxType::Expense,
        }
    }

    #[test]
    fn running_balance_accumulates_in_canonical_order() {
        let mut rows = vec![
            row("b", "2026-01-02T00:00:00Z", -200),
            row("a", "2026-01-01T00:00:00Z", 1000),
        ];
        recompute_running_balances(&mut rows);
        assert_eq!(rows[0].id, "a", "must be sorted before accumulating");
        assert_eq!(rows[0].balance, Money::from_cents(1000));
        assert_eq!(rows[1].balance, Money::from_cents(800));
    }

    #[test]
    fn same_timestamp_rows_are_ordered_by_id_not_input_order() {
        let mut one = vec![
            row("z", "2026-01-01T00:00:00Z", 100),
            row("a", "2026-01-01T00:00:00Z", 200),
        ];
        let mut two = vec![
            row("a", "2026-01-01T00:00:00Z", 200),
            row("z", "2026-01-01T00:00:00Z", 100),
        ];
        recompute_running_balances(&mut one);
        recompute_running_balances(&mut two);
        assert_eq!(one, two, "input order must not affect the result");
    }

    #[test]
    fn validate_is_empty_after_recompute_and_names_the_row_otherwise() {
        let mut rows = vec![row("a", "2026-01-01T00:00:00Z", 500)];
        recompute_running_balances(&mut rows);
        assert!(validate(&rows).is_empty());

        rows[0].balance = Money::from_cents(1);
        let errs = validate(&rows);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].id, "a");
        assert_eq!(errs[0].expected, Money::from_cents(500));
        assert_eq!(errs[0].found, Money::from_cents(1));
    }
}
