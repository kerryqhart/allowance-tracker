use crate::balance::recompute_running_balances;
use crate::money::Money;
use crate::row::{Provenance, Sided, TxRow};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    TookOurs { id: String },
    TookTheirs { id: String },
    KeptBothReKeyed { original: String, re_keyed: String },
    Deleted { id: String },
    /// One side deleted a row while the other edited it. The edit wins, which
    /// means a deliberate deletion was undone — the user who deleted it needs
    /// to be able to see that happened.
    EditBeatDelete { id: String },
    /// A row was removed by the post-loop dedupe (its id collided with
    /// another surviving row, e.g. a re-keyed row landing on an id another
    /// side already used). A row disappearing must never be silent — that is
    /// the stated goal of this whole design — so every drop is logged with
    /// enough to reconstruct what vanished: description alone doesn't say
    /// what money disappeared, so the amount and the row's rendered date
    /// come along too.
    DuplicateDropped {
        id: String,
        discarded_description: String,
        discarded_amount: Money,
        discarded_date: String,
    },
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
                    // `b.is_some()` here means the row existed in base and we
                    // changed it (unchanged_by_us is false), so theirs' delete
                    // is being overridden by our edit — a deliberate deletion
                    // undone. `b.is_none()` means this id was never in base at
                    // all (an add, not a delete/edit conflict), so nothing is
                    // logged in that case.
                    if base_known && b.is_some() {
                        decisions.push(Decision::EditBeatDelete { id: id.clone() });
                    }
                    rows.push(o.clone());
                }
            }
            (None, Some(t)) => {
                let unchanged_by_them = b.map(|b| b.intrinsic_eq(t)).unwrap_or(false);
                if base_known && b.is_some() && unchanged_by_them {
                    decisions.push(Decision::Deleted { id: id.clone() });
                } else {
                    if base_known && b.is_some() {
                        decisions.push(Decision::EditBeatDelete { id: id.clone() });
                    }
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
                    // edit/edit: base is known and had this id (the add/add
                    // branch above already caught `b.is_none()`), and the two
                    // sides differ from each other. The core three-way rule:
                    // ask whether each side is actually UNCHANGED from base
                    // before falling back to provenance. A side that never
                    // touched the row is not a competing edit — the other
                    // side's edit must win outright, regardless of epoch,
                    // or a stale untouched copy can resurrect over a real
                    // edit just because it happened to commit later.
                    let base_row = b.expect("b.is_none() handled above");
                    let o_unchanged = base_row.intrinsic_eq(o);
                    let t_unchanged = base_row.intrinsic_eq(t);
                    if o_unchanged && !t_unchanged {
                        decisions.push(Decision::TookTheirs { id: id.clone() });
                        rows.push(t.clone());
                    } else if t_unchanged && !o_unchanged {
                        decisions.push(Decision::TookOurs { id: id.clone() });
                        rows.push(o.clone());
                    } else {
                        // Both sides genuinely changed the row: a real
                        // conflict, resolved by provenance.
                        if wins(&ours.provenance, &theirs.provenance) {
                            decisions.push(Decision::TookOurs { id: id.clone() });
                            rows.push(o.clone());
                        } else {
                            decisions.push(Decision::TookTheirs { id: id.clone() });
                            rows.push(t.clone());
                        }
                    }
                }
            }
            (None, None) => unreachable!("id came from one of the two maps"),
        }
    }

    // A re-keyed row can equal a row the other side already carries (exactly
    // what makes the second merge a fixed point). Collapse those — but a row
    // disappearing must never be silent, so every drop is logged with what it
    // would have shown, not just discarded.
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    let mut deduped: Vec<TxRow> = Vec::with_capacity(rows.len());
    for candidate in rows {
        match deduped.last() {
            Some(kept) if kept.id == candidate.id => {
                decisions.push(Decision::DuplicateDropped {
                    id: candidate.id.clone(),
                    discarded_description: candidate.description.clone(),
                    discarded_amount: candidate.amount,
                    discarded_date: candidate.date.to_rfc3339(),
                });
            }
            _ => deduped.push(candidate),
        }
    }
    let mut rows = deduped;

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
    // Full 64 bits, not truncated to u32: a truncated suffix makes a
    // collision merely unlikely rather than negligible, and a suffix
    // collision is exactly what the dedupe pass below has to clean up.
    format!("{hash:016x}")
}

/// Later committer timestamp wins; ties break on commit oid.
///
/// Must be symmetric — a "prefer ours" rule would have each machine choose its
/// own side and the two would never converge.
fn wins(ours: &Provenance, theirs: &Provenance) -> bool {
    // Equal provenance (same epoch AND same oid) means the same commit, so
    // there is no actual divergence to resolve — the caller should never
    // reach `wins` with both sides identical. Stated and checked rather than
    // merely believed, since `wins` returns `false` in both directions at
    // this input and is therefore NOT symmetric there.
    debug_assert!(ours != theirs, "wins() called with identical provenance on both sides");
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
    use proptest::prelude::*;

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
        // The deleting user's deletion was undone -- that must be logged,
        // not just correctly resolved. A row resurrecting must never be
        // silent, same as a row disappearing.
        assert!(
            out.decisions.contains(&Decision::EditBeatDelete { id: "a".to_string() }),
            "expected EditBeatDelete to be logged, got {:?}", out.decisions
        );
    }

    #[test]
    fn an_edit_beats_a_delete_the_other_way() {
        // Mirror of `an_edit_beats_a_delete`: this time OURS is the edit and
        // THEIRS is the delete, exercising the `(Some(o), None)` arm instead
        // of `(None, Some(t))`.
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "edited", -100)], 20, 2);
        let theirs = sided(vec![], 10, 1);
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(ids(&out), vec!["a"]);
        assert_eq!(out.rows[0].description, "edited");
        assert!(
            out.decisions.contains(&Decision::EditBeatDelete { id: "a".to_string() }),
            "expected EditBeatDelete to be logged, got {:?}", out.decisions
        );
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

    // --- Critical-1 fix: unchanged-vs-base must beat provenance -----------

    #[test]
    fn edit_survives_against_a_stale_untouched_peer_even_at_higher_epoch() {
        // Regression for the missing three-way check: the merge result R
        // already carries a real edit (a: slime -> book) plus a re-keyed row
        // surviving from an earlier add/add collision. The peer never
        // touched "a" at all -- it only added a new row. The old code never
        // asked "did this side even change it relative to base," so the
        // peer's higher epoch won and resurrected "slime kit" over "book
        // fair," destroying the edit.
        let slime = row("a", "slime kit", -100);
        let suffix = content_suffix(&slime);
        let mut rekeyed = slime.clone();
        rekeyed.id = format!("a-{suffix}");

        let base = vec![slime.clone()];
        let ours = sided(vec![row("a", "book fair", -250), rekeyed], 10, 1);
        // Peer's epoch is far higher, but it never edited "a" -- the edited
        // side must still win.
        let theirs = sided(
            vec![row("a", "slime kit", -100), row("z", "candy", -50)],
            999,
            2,
        );

        let out = merge(Some(&base), &ours, &theirs);
        let total: i64 = out.rows.iter().map(|r| r.amount.cents()).sum();
        assert_eq!(total, -400, "book fair (-250) + re-keyed slime (-100) + candy (-50)");
        assert!(out.rows.iter().any(|r| r.description == "book fair"), "the real edit must survive");
    }

    #[test]
    fn one_sided_edit_wins_regardless_of_epoch_when_ours_is_the_edit() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "edited", -100)], 1, 1); // lower epoch, real edit
        let theirs = sided(vec![row("a", "x", -100)], 999, 2); // higher epoch, untouched
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(out.rows[0].description, "edited");
    }

    #[test]
    fn one_sided_edit_wins_regardless_of_epoch_when_theirs_is_the_edit() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "x", -100)], 999, 1); // higher epoch, untouched
        let theirs = sided(vec![row("a", "edited", -100)], 1, 2); // lower epoch, real edit
        let out = merge(Some(&base), &ours, &theirs);
        assert_eq!(out.rows[0].description, "edited");
    }

    #[test]
    fn merging_a_side_with_itself_is_idempotent() {
        let base = vec![row("a", "x", -100)];
        let x = sided(vec![row("a", "x", -100), row("b", "y", -200)], 10, 1);
        let out = merge(Some(&base), &x, &x);
        let mut expected = x.rows.clone();
        recompute_running_balances(&mut expected);
        assert_eq!(out.rows, expected, "merging a side with itself must reproduce it, canonicalised");
    }

    // --- Critical-2 fix: dedupe must log, never silently drop --------------

    #[test]
    fn dedupe_logs_the_dropped_row_instead_of_discarding_it_silently() {
        let slime = row("a", "slime kit", -100);
        let suffix = content_suffix(&slime);
        let collision_id = format!("a-{suffix}");

        let ours = sided(vec![slime.clone()], 10, 1);
        let mut orphan = row("placeholder", "should not vanish", -999);
        orphan.id = collision_id.clone();
        // theirs wins the add/add (higher epoch), so "ours" (slime) is the
        // one re-keyed to `collision_id` -- exactly the id we planted the
        // orphan row under, forcing a post-loop id collision.
        let theirs = sided(vec![row("a", "book fair", -250), orphan], 20, 2);

        let out = merge(Some(&[]), &ours, &theirs);

        let dropped = out.decisions.iter().find_map(|d| match d {
            Decision::DuplicateDropped { id, discarded_description, discarded_amount, discarded_date }
                if id == &collision_id =>
            {
                Some((discarded_description.clone(), *discarded_amount, discarded_date.clone()))
            }
            _ => None,
        });
        let (desc, amount, date) = dropped.expect(
            "a duplicate id collision must be logged with what it discarded, not silent",
        );
        assert_eq!(desc, "should not vanish");
        // The whole point of logging the drop is that the vanished money is
        // recoverable from the log, not just its description.
        assert_eq!(amount, Money::from_cents(-999));
        assert_eq!(date, "2026-01-01T00:00:00+00:00");
        assert_eq!(
            out.rows.iter().filter(|r| r.id == collision_id).count(),
            1,
            "exactly one survivor at the collided id"
        );
        assert!(!out.rows.iter().any(|r| r.description == "should not vanish"));
    }

    // --- Decisions must be populated, not just correctness of `rows` ------

    #[test]
    fn edit_edit_conflict_is_logged() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![row("a", "ours", -100)], 10, 1);
        let theirs = sided(vec![row("a", "theirs", -100)], 99, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert!(out.decisions.contains(&Decision::TookTheirs { id: "a".to_string() }));
    }

    #[test]
    fn add_add_collision_is_logged() {
        let ours = sided(vec![row("a", "slime kit", -100)], 10, 1);
        let theirs = sided(vec![row("a", "book fair", -250)], 20, 2);
        let out = merge(Some(&[]), &ours, &theirs);
        assert!(out.decisions.iter().any(|d| matches!(
            d,
            Decision::KeptBothReKeyed { original, .. } if original == "a"
        )));
    }

    #[test]
    fn delete_is_logged() {
        let base = vec![row("a", "x", -100)];
        let ours = sided(vec![], 10, 1);
        let theirs = sided(vec![row("a", "x", -100)], 20, 2);
        let out = merge(Some(&base), &ours, &theirs);
        assert!(out.decisions.contains(&Decision::Deleted { id: "a".to_string() }));
    }

    // --- Important-4: wins() states its assumption and checks it ----------
    //
    // debug_assert! compiles out under --release, so this test only exists
    // where the assertion does; without the cfg gate, `cargo test --release`
    // fails with "did not panic as expected" even though nothing is broken.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "identical provenance")]
    fn wins_asserts_against_fully_equal_provenance() {
        let p = Provenance { committer_epoch: 1, commit_oid: [7u8; 20] };
        let _ = wins(&p, &p);
    }

    // --- DuplicateDropped: reachable only by construction ------------------
    //
    // `DuplicateDropped` fires only when a re-keyed row's hash-derived id
    // happens to already be taken by another surviving id -- a 64-bit hash
    // collision that pure random generation will not hit in any realistic
    // test budget. This lives here, as a UNIT test inside `merge`'s own
    // `#[cfg(test)] mod tests`, specifically so it can call the REAL
    // `content_suffix` and `wins` directly. An external integration test
    // (allowance-core/tests/properties.rs) can only see the crate's public
    // API and would have to duplicate both private algorithms to predict
    // the collision -- exactly the drift hazard this project has already
    // been bitten by twice (two id generators, and nearly a second CSV
    // parser). Neither function is made `pub` to solve this; a unit test
    // can already reach them as-is.
    proptest! {
        #[test]
        fn duplicate_dropped_is_reachable_and_logged(
            ours_cents in -100_000i64..100_000,
            theirs_cents in -100_000i64..100_000,
            orphan_cents in -100_000i64..100_000,
            ours_epoch in 0i64..3,
            theirs_epoch in 0i64..3,
            ours_oid in 0u8..80,
            theirs_oid in 100u8..200,
        ) {
            let ours_row = row("p", "slime", ours_cents);
            let theirs_row = row("p", "book", theirs_cents);
            let ours_prov = Provenance { committer_epoch: ours_epoch, commit_oid: [ours_oid; 20] };
            let theirs_prov = Provenance { committer_epoch: theirs_epoch, commit_oid: [theirs_oid; 20] };

            // Whichever side LOSES the add/add tiebreak is the one re-keyed
            // to "p-<hash of the losing row>". Plant an orphan at exactly
            // that id (using the crate's REAL `content_suffix`, not a copy)
            // so the post-loop dedupe must collide the two and log the drop.
            let ours_wins = wins(&ours_prov, &theirs_prov);
            let loser_row = if ours_wins { &theirs_row } else { &ours_row };
            let collision_id = format!("p-{}", content_suffix(loser_row));
            let mut orphan = row("placeholder", "should not vanish", orphan_cents);
            orphan.id = collision_id.clone();

            let mut ours_rows = vec![ours_row.clone()];
            let mut theirs_rows = vec![theirs_row.clone()];
            if ours_wins {
                ours_rows.push(orphan);
            } else {
                theirs_rows.push(orphan);
            }
            let ours = sided(ours_rows, ours_epoch, ours_oid);
            let theirs = sided(theirs_rows, theirs_epoch, theirs_oid);

            let out = merge(Some(&[]), &ours, &theirs);

            let dropped = out.decisions.iter().any(|d| matches!(
                d,
                Decision::DuplicateDropped { id, .. } if id == &collision_id
            ));
            prop_assert!(dropped, "expected a logged DuplicateDropped at {collision_id}, decisions: {:?}", out.decisions);
            prop_assert!(!out.rows.iter().any(|r| r.description == "should not vanish"));
            prop_assert_eq!(out.rows.iter().filter(|r| r.id == collision_id).count(), 1);
        }
    }
}
