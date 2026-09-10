use crate::balance::recompute_running_balances;
use crate::row::{Provenance, Sided, TxRow};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    TookOurs { id: String },
    TookTheirs { id: String },
    KeptBothReKeyed { original: String, re_keyed: String },
    Deleted { id: String },
}

#[derive(Debug, Clone)]
pub struct MergeOutcome {
    pub rows: Vec<TxRow>,
    /// Every non-trivial choice, so a surprising result is auditable after the
    /// fact. The caller logs these.
    pub decisions: Vec<Decision>,
}

/// Three-way semantic merge.
///
/// Pure and total: provenance is supplied by the caller, never looked up, so
/// this never touches git. `base` is `None` when the histories are unrelated.
pub fn merge(base: Option<&[TxRow]>, ours: &Sided, theirs: &Sided) -> MergeOutcome {
    let index = |rows: &[TxRow]| -> BTreeMap<String, TxRow> {
        rows.iter().map(|r| (r.id.clone(), r.clone())).collect()
    };
    let base_map = base.map(index).unwrap_or_default();
    let base_known = base.is_some();
    let ours_map = index(&ours.rows);
    let theirs_map = index(&theirs.rows);

    let all: BTreeSet<&String> = ours_map.keys().chain(theirs_map.keys()).collect();
    let mut rows = Vec::new();
    let mut decisions = Vec::new();

    for id in all {
        let b = base_map.get(id);
        let o = ours_map.get(id);
        let t = theirs_map.get(id);

        match (o, t) {
            (Some(o), None) => {
                // Deleted on theirs. Only a real delete if the base had it AND
                // we did not change it — otherwise an edit beats the delete.
                let unchanged_by_us = b.map(|b| b.intrinsic_eq(o)).unwrap_or(false);
                if base_known && b.is_some() && unchanged_by_us {
                    decisions.push(Decision::Deleted { id: id.clone() });
                } else {
                    rows.push(o.clone());
                }
            }
            (None, Some(t)) => {
                let unchanged_by_them = b.map(|b| b.intrinsic_eq(t)).unwrap_or(false);
                if base_known && b.is_some() && unchanged_by_them {
                    decisions.push(Decision::Deleted { id: id.clone() });
                } else {
                    rows.push(t.clone());
                }
            }
            (Some(o), Some(t)) => {
                if o.intrinsic_eq(t) {
                    rows.push(o.clone());
                } else if b.is_none() {
                    // add/add: two DIFFERENT rows that collided on a key.
                    // Keeping one destroys a real transaction, so keep both and
                    // re-key deterministically.
                    //
                    // The suffix is derived from the ROW'S OWN CONTENT, never
                    // from provenance. A provenance-derived suffix is not a
                    // fixed point: re-merging the result against the same side
                    // sees the same collision on the original id and re-keys
                    // again with a fresh suffix, forever. Content-derived plus
                    // the dedupe below means the second merge produces exactly
                    // the first merge's rows.
                    let (keep, rekey) = if wins(&ours.provenance, &theirs.provenance) {
                        (o, t)
                    } else {
                        (t, o)
                    };
                    let mut moved = rekey.clone();
                    moved.id = format!("{}-{}", rekey.id, content_suffix(rekey));
                    decisions.push(Decision::KeptBothReKeyed {
                        original: rekey.id.clone(),
                        re_keyed: moved.id.clone(),
                    });
                    rows.push(keep.clone());
                    rows.push(moved);
                } else {
                    // edit/edit: one row, two edits. Pick one.
                    if wins(&ours.provenance, &theirs.provenance) {
                        decisions.push(Decision::TookOurs { id: id.clone() });
                        rows.push(o.clone());
                    } else {
                        decisions.push(Decision::TookTheirs { id: id.clone() });
                        rows.push(t.clone());
                    }
                }
            }
            (None, None) => unreachable!("id came from one of the two maps"),
        }
    }

    // A re-keyed row can equal a row the other side already carries (exactly
    // what makes the second merge a fixed point). Collapse those.
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows.dedup_by(|a, b| a.id == b.id);

    recompute_running_balances(&mut rows);
    MergeOutcome { rows, decisions }
}

/// A stable fingerprint of a row's intrinsic fields.
///
/// FNV-1a, written out explicitly. `DefaultHasher` would be wrong here: Rust
/// does not guarantee its output is stable across compiler versions, and this
/// value becomes part of a transaction id that both machines must agree on
/// while building from separate toolchains.
fn content_suffix(row: &TxRow) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(row.child_id.as_bytes());
    eat(b"\x1f");
    eat(row.date.to_rfc3339().as_bytes());
    eat(b"\x1f");
    eat(row.description.as_bytes());
    eat(b"\x1f");
    eat(row.amount.cents().to_string().as_bytes());
    eat(b"\x1f");
    eat(row.tx_type.as_csv().as_bytes());
    format!("{:08x}", hash as u32)
}

/// Later committer timestamp wins; ties break on commit oid.
///
/// Must be symmetric — a "prefer ours" rule would have each machine choose its
/// own side and the two would never converge.
fn wins(ours: &Provenance, theirs: &Provenance) -> bool {
    match ours.committer_epoch.cmp(&theirs.committer_epoch) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => ours.commit_oid > theirs.commit_oid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use crate::row::{Provenance, Sided, TxRow, TxType};
    use chrono::DateTime;

    fn row(id: &str, desc: &str, cents: i64) -> TxRow {
        TxRow {
            id: id.to_string(),
            child_id: "c".to_string(),
            date: DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00").unwrap(),
            description: desc.to_string(),
            amount: Money::from_cents(cents),
            balance: Money::from_cents(0),
            tx_type: TxType::Expense,
        }
    }

    fn sided(rows: Vec<TxRow>, epoch: i64, oid_byte: u8) -> Sided {
        Sided { rows, provenance: Provenance { committer_epoch: epoch, commit_oid: [oid_byte; 20] } }
    }

    fn ids(o: &MergeOutcome) -> Vec<&str> { o.rows.iter().map(|r| r.id.as_str()).collect() }

    #[test]
    fn keeps_adds_from_both_sides() {
        let base = vec![];
        let ours = sided(vec![row("a", "x", -100)], 10, 1);
        let theirs = sided(vec![row("b", "y", -200)], 20, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(ids(&out), vec!["a", "b"]);
    }

    #[test]
    fn drops_a_row_deleted_on_one_side_and_untouched_on_the_other() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![], 10, 1);
        let theirs = sided(vec![row("a", "x", -100)], 20, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert!(ids(&out).is_empty(), "a genuine delete must not resurrect");
    }

    #[test]
    fn an_edit_beats_a_delete() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![], 10, 1);
        let theirs = sided(vec![row("a", "edited", -100)], 20, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(ids(&out), vec!["a"]);
        assert_eq!(out.rows[0].description, "edited");
    }

    #[test]
    fn edit_edit_resolves_to_the_later_committer_timestamp() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "ours", -100)], 10, 1);
        let theirs = sided(vec![row("a", "theirs", -100)], 99, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(out.rows[0].description, "theirs");
    }

    #[test]
    fn edit_edit_ties_break_on_commit_oid_not_on_side() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "ours", -100)], 50, 9);
        let theirs = sided(vec![row("a", "theirs", -100)], 50, 2);
        // Same epoch: the higher oid wins, so both machines agree regardless of
        // which side each is standing on.
        assert_eq!(merge(Some(&base), &ours, &theirs).rows[0].description, "ours");
        assert_eq!(merge(Some(&base), &theirs, &ours).rows[0].description, "ours");
    }

    #[test]
    fn add_add_with_identical_content_keeps_one_row() {
        let ours = sided(vec![row("a", "same", -100)], 10, 1);
        let theirs = sided(vec![row("a", "same", -100)], 20, 2);
        let out = merge(Some(&[]), &ours, &theirs);
        assert_eq!(out.rows.len(), 1);
    }

    #[test]
    fn add_add_with_differing_content_keeps_both_rows() {
        // THE data-loss case. Two Macs mint the same id for two different
        // transactions; picking one destroys real money.
        let ours = sided(vec![row("a", "slime kit", -100)], 10, 1);
        let theirs = sided(vec![row("a", "book fair", -250)], 20, 2);
        let out = merge(Some(&[]), &ours, &theirs);
        assert_eq!(out.rows.len(), 2, "both transactions must survive");
        let descs: Vec<&str> = out.rows.iter().map(|r| r.description.as_str()).collect();
        assert!(descs.contains(&"slime kit"));
        assert!(descs.contains(&"book fair"));
        let re_keyed: Vec<&str> = out.rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(re_keyed.iter().collect::<std::collections::HashSet<_>>().len(), 2);
    }

    #[test]
    fn add_add_re_keys_the_same_side_on_both_machines() {
        let ours = sided(vec![row("a", "slime kit", -100)], 10, 1);
        let theirs = sided(vec![row("a", "book fair", -250)], 20, 2);
        let one = merge(Some(&[]), &ours, &theirs);
        let two = merge(Some(&[]), &theirs, &ours);
        assert_eq!(ids(&one), ids(&two), "both machines must pick the same loser");
    }

    #[test]
    fn no_merge_base_unions_both_sides() {
        // Independent `git init` on each machine. With no common ancestor,
        // nothing can be shown to have been deleted, so nothing is dropped.
        let ours = sided(vec![row("a", "x", -100)], 10, 1);
        let theirs = sided(vec![row("b", "y", -200)], 20, 2);
        let out = merge(None, &ours, &theirs);
        assert_eq!(ids(&out), vec!["a", "b"]);
    }

    #[test]
    fn output_balances_are_recomputed_and_valid() {
        let ours = sided(vec![row("a", "x", 1000)], 10, 1);
        let theirs = sided(vec![row("b", "y", -400)], 20, 2);
        let out = merge(Some(&[]), &ours, &theirs);
        assert!(crate::balance::validate(&out.rows).is_empty());
    }
}
