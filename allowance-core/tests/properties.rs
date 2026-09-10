use allowance_core::balance::validate;
use allowance_core::codec::{parse_transactions, render_transactions};
use allowance_core::merge::merge;
use allowance_core::money::Money;
use allowance_core::row::{Provenance, Sided, TxRow, TxType};
use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;

fn row_strategy() -> impl Strategy<Value = TxRow> {
    (
        "[a-z]{1,6}",
        0i64..5,
        -100_000i64..100_000,
        "[a-z ]{0,12}",
    )
        .prop_map(|(id, day, cents, desc)| TxRow {
            id,
            child_id: "c".to_string(),
            date: DateTime::from(Utc.timestamp_opt(1_760_000_000 + day * 86_400, 0).unwrap()),
            description: desc,
            // Integer cents only: with f64 the property would fail for reasons
            // unrelated to the merge, and the repair would be to widen an epsilon.
            amount: Money::from_cents(cents),
            balance: Money::from_cents(0),
            tx_type: TxType::Expense,
        })
}

fn sided_strategy() -> impl Strategy<Value = Sided> {
    (prop::collection::vec(row_strategy(), 0..8), 0i64..1000, 0u8..255).prop_map(
        |(mut rows, epoch, oid)| {
            // Sort BEFORE dedup: `dedup_by` only removes *consecutive*
            // duplicates, so an unsorted vec keeps duplicate ids. The merge
            // indexes rows by id, so a duplicate would be silently dropped and
            // symmetry would appear to fail for a reason that is purely an
            // artifact of the generator.
            rows.sort_by(|a, b| a.id.cmp(&b.id));
            rows.dedup_by(|a, b| a.id == b.id);
            Sided { rows, provenance: Provenance { committer_epoch: epoch, commit_oid: [oid; 20] } }
        },
    )
}

proptest! {
    /// Symmetry — "prefer ours" would make each machine choose its own side and
    /// the two would diverge permanently.
    #[test]
    fn symmetric(base in prop::collection::vec(row_strategy(), 0..5),
                 a in sided_strategy(), b in sided_strategy()) {
        let ab = merge(Some(&base), &a, &b);
        let ba = merge(Some(&base), &b, &a);
        prop_assert_eq!(render_transactions(&ab.rows), render_transactions(&ba.rows));
    }

    /// Idempotence — merging a side with itself is just canonicalisation.
    #[test]
    fn idempotent(a in sided_strategy()) {
        let out = merge(Some(&a.rows), &a, &a);
        prop_assert_eq!(render_transactions(&out.rows), render_transactions(&{
            let mut rows = a.rows.clone();
            allowance_core::balance::recompute_running_balances(&mut rows);
            rows
        }));
    }

    /// FIXED POINT — re-merging a merged result against one of its inputs
    /// changes nothing. This is what proves the machines stop re-merging.
    #[test]
    fn fixed_point(base in prop::collection::vec(row_strategy(), 0..5),
                   a in sided_strategy(), b in sided_strategy()) {
        let first = merge(Some(&base), &a, &b);
        let merged_side = Sided { rows: first.rows.clone(), provenance: a.provenance };
        let second = merge(Some(&base), &merged_side, &b);
        prop_assert_eq!(render_transactions(&first.rows), render_transactions(&second.rows));
    }

    /// Byte round-trip — render(parse(x)) == x for canonical x.
    #[test]
    fn round_trip_is_byte_stable(rows in prop::collection::vec(row_strategy(), 0..10)) {
        let once = render_transactions(&rows);
        let twice = render_transactions(&parse_transactions(&once).unwrap().rows);
        prop_assert_eq!(once, twice);
    }

    /// Money is always self-consistent after a merge.
    #[test]
    fn balances_validate(base in prop::collection::vec(row_strategy(), 0..5),
                         a in sided_strategy(), b in sided_strategy()) {
        let out = merge(Some(&base), &a, &b);
        prop_assert!(validate(&out.rows).is_empty());
    }
}

#[test]
fn merge_output_does_not_depend_on_the_machine_timezone() {
    // Two Macs in different timezones must produce identical bytes. The codec
    // parses RFC3339 only and never resolves through chrono::Local, so this
    // holds — the test is here to keep it holding.
    let csv = "id,child_id,date,description,amount,balance,type\n\
a,c,2026-01-01T00:00:00+00:00,x,1.00,1.00,expense\n";
    std::env::set_var("TZ", "UTC");
    let utc = render_transactions(&parse_transactions(csv).unwrap().rows);
    std::env::set_var("TZ", "America/Los_Angeles");
    let la = render_transactions(&parse_transactions(csv).unwrap().rows);
    assert_eq!(utc, la);
}
