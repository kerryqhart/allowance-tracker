//! The single canonical CSV codec for transaction rows.
//!
//! Two machines syncing the same data must converge on identical BYTES, not
//! merely equal values. That requires `render(parse(x))` to be a fixed point:
//! same canonical `(date, id)` order and the same rendering of every field on
//! every write, not only after a merge. It also requires every parse failure
//! to be a hard error rather than a fallback — a fallback (current time on an
//! unparseable date, a derived type on an unrecognised one) makes
//! read-modify-write non-idempotent, so one bad row would make the file
//! change on every cycle and the two machines would re-merge forever.
use crate::money::Money;
use crate::row::{TxRow, TxType};
use chrono::DateTime;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("csv error: {0}")]
    Csv(String),
    #[error("row {id}: unparseable date {value:?}")]
    Date { id: String, value: String },
    #[error("row {id}: unparseable money {value:?}")]
    Money { id: String, value: String },
    #[error("row {id}: unknown transaction type {value:?} — refusing rather than guessing")]
    Type { id: String, value: String },
}

pub const HEADER: [&str; 7] =
    ["id", "child_id", "date", "description", "amount", "balance", "type"];

/// The result of a parse: the canonically-ordered rows, plus how many needed
/// legacy-precision rounding.
///
/// `rows_rounded` exists so the caller can *tell the user* their data was
/// rewritten, even though the rewrite is correct (rounding an f64 artifact
/// like `"14.620000000000001"` to its real 1462 cents changes no value — see
/// `Money::parse_rounding`). It must never be silently absorbed.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTransactions {
    pub rows: Vec<TxRow>,
    /// Number of rows where `amount` and/or `balance` needed rounding to the
    /// nearest cent. A row with both fields rounded still counts once.
    pub rows_rounded: usize,
}

pub fn parse_transactions(text: &str) -> Result<ParsedTransactions, CodecError> {
    let mut reader = csv::Reader::from_reader(text.as_bytes());
    let mut rows = Vec::new();
    let mut rows_rounded = 0usize;
    for record in reader.records() {
        let r = record.map_err(|e| CodecError::Csv(e.to_string()))?;
        let id = r.get(0).unwrap_or_default().to_string();
        let date_raw = r.get(2).unwrap_or_default();

        // RFC3339 only. No current-time fallback, and no chrono::Local path:
        // both make read-modify-write non-idempotent, so one bad row would
        // make the file change on every cycle and both machines re-merge
        // forever.
        let date = DateTime::parse_from_rfc3339(date_raw)
            .map_err(|_| CodecError::Date { id: id.clone(), value: date_raw.to_string() })?;

        // `Money::parse_rounding`, not the strict `FromStr`: legacy data
        // written before `Money` existed carries f64-precision noise like
        // "14.620000000000001", which means 1462 cents and nothing else.
        // Rejecting it on this read path would refuse the user's own history
        // over a rendering artifact, not a real ambiguity. `rounded` is
        // reported back rather than absorbed — see `ParsedTransactions`.
        let parse_money = |idx: usize| -> Result<(Money, bool), CodecError> {
            let raw = r.get(idx).unwrap_or_default();
            Money::parse_rounding(raw)
                .map_err(|_| CodecError::Money { id: id.clone(), value: raw.to_string() })
        };

        let type_raw = r.get(6).unwrap_or_default();
        let tx_type = match type_raw.to_lowercase().as_str() {
            "allowance" => TxType::Allowance,
            "income" | "oneoffincome" => TxType::OneOffIncome,
            "expense" => TxType::Expense,
            "future_allowance" | "futureallowance" => TxType::FutureAllowance,
            _ => return Err(CodecError::Type { id, value: type_raw.to_string() }),
        };

        let (amount, amount_rounded) = parse_money(4)?;
        let (balance, balance_rounded) = parse_money(5)?;
        if amount_rounded || balance_rounded {
            rows_rounded += 1;
        }

        rows.push(TxRow {
            id: id.clone(),
            child_id: r.get(1).unwrap_or_default().to_string(),
            date,
            description: r.get(3).unwrap_or_default().to_string(),
            amount,
            balance,
            tx_type,
        });
    }
    rows.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    Ok(ParsedTransactions { rows, rows_rounded })
}

/// Always emits canonical order and canonical money. This is the only writer;
/// the repository calls it rather than keeping a second serializer that drifts.
pub fn render_transactions(rows: &[TxRow]) -> String {
    let mut sorted: Vec<&TxRow> = rows.iter().collect();
    sorted.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(HEADER).expect("in-memory write");
    for row in sorted {
        writer
            .write_record([
                row.id.as_str(),
                row.child_id.as_str(),
                &row.date.to_rfc3339(),
                row.description.as_str(),
                &row.amount.render(),
                &row.balance.render(),
                row.tx_type.as_csv(),
            ])
            .expect("in-memory write");
    }
    String::from_utf8(writer.into_inner().expect("in-memory flush")).expect("utf-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "id,child_id,date,description,amount,balance,type\n\
ex-2-b,keiko,2026-01-02T00:00:00+00:00,Slime,-5.50,4.50,expense\n\
in-1-a,keiko,2026-01-01T00:00:00+00:00,Allowance,10.00,10.00,allowance\n";

    #[test]
    fn parse_then_render_is_canonically_ordered() {
        let rows = parse_transactions(CSV).unwrap().rows;
        let out = render_transactions(&rows);
        let ids: Vec<&str> = out.lines().skip(1).map(|l| l.split(',').next().unwrap()).collect();
        assert_eq!(ids, vec!["in-1-a", "ex-2-b"], "output must be sorted by (date, id)");
    }

    #[test]
    fn render_is_byte_stable_across_a_round_trip() {
        let once = render_transactions(&parse_transactions(CSV).unwrap().rows);
        let twice = render_transactions(&parse_transactions(&once).unwrap().rows);
        assert_eq!(once, twice, "render(parse(x)) must be a fixed point");
    }

    #[test]
    fn money_renders_with_exactly_two_decimals() {
        let out = render_transactions(&parse_transactions(CSV).unwrap().rows);
        assert!(out.contains(",10.00,10.00,"), "got: {out}");
        assert!(out.contains(",-5.50,4.50,"), "got: {out}");
    }

    #[test]
    fn an_unparseable_date_is_an_error_not_the_current_time() {
        let bad = "id,child_id,date,description,amount,balance,type\n\
x,keiko,not-a-date,d,1.00,1.00,expense\n";
        let err = parse_transactions(bad).unwrap_err();
        assert!(matches!(err, CodecError::Date { .. }), "got {err:?}");
    }

    #[test]
    fn a_date_only_value_is_an_error_not_a_local_midnight() {
        // Resolving through chrono::Local would parse differently in two
        // timezones, so the two machines would never converge.
        let bad = "id,child_id,date,description,amount,balance,type\n\
x,keiko,2026-01-01,d,1.00,1.00,expense\n";
        assert!(parse_transactions(bad).is_err());
    }

    #[test]
    fn an_unknown_type_is_refused_not_derived() {
        let future = "id,child_id,date,description,amount,balance,type\n\
x,keiko,2026-01-01T00:00:00+00:00,d,1.00,1.00,rebate\n";
        // Derivation would silently downgrade a row an older app does not know,
        // and then push it. Refuse instead.
        assert!(parse_transactions(future).is_err());
    }

    #[test]
    fn legacy_f64_precision_is_rounded_not_refused_and_the_count_is_reported() {
        let legacy = "id,child_id,date,description,amount,balance,type\n\
x,keiko,2026-01-01T00:00:00+00:00,d,1.00,14.620000000000001,expense\n\
y,keiko,2026-01-02T00:00:00+00:00,d,2.00,4.00,expense\n";
        let parsed = parse_transactions(legacy).unwrap();
        assert_eq!(parsed.rows_rounded, 1, "only the row with excess precision counts");
        let row = parsed.rows.iter().find(|r| r.id == "x").unwrap();
        assert_eq!(row.balance, Money::from_cents(1462));
    }

    #[test]
    fn a_row_rounded_on_both_amount_and_balance_still_counts_once() {
        let legacy = "id,child_id,date,description,amount,balance,type\n\
x,keiko,2026-01-01T00:00:00+00:00,d,1.005,14.620000000000001,expense\n";
        let parsed = parse_transactions(legacy).unwrap();
        assert_eq!(parsed.rows_rounded, 1);
    }
}
