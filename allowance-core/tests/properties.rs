use allowance_core::balance::{recompute_running_balances, validate};
use allowance_core::codec::{parse_transactions, render_transactions};
use allowance_core::merge::{merge, Decision, MergeOutcome};
use allowance_core::money::Money;
use allowance_core::row::{Provenance, Sided, TxRow, TxType};
use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};
use std::collections::HashMap;

// ===========================================================================
// Shared building blocks
// ===========================================================================

fn plain_description_strategy() -> impl Strategy<Value = String> {
    "[a-z ]{0,12}"
}

fn content_strategy() -> impl Strategy<Value = (i64, i64, String)> {
    (0i64..5, -100_000i64..100_000, plain_description_strategy())
}

fn make_row(id: String, day: i64, cents: i64, desc: String) -> TxRow {
    TxRow {
        id,
        child_id: "c".to_string(),
        date: DateTime::from(Utc.timestamp_opt(1_760_000_000 + day * 86_400, 0).unwrap()),
        description: desc,
        // Integer cents only: with f64 the property would fail for reasons
        // unrelated to the merge, and the repair would be to widen an epsilon.
        amount: Money::from_cents(cents),
        balance: Money::from_cents(0),
        tx_type: TxType::Expense,
    }
}

/// Kept for `idempotent`, which merges a side with itself and does not need
/// the base+mutation machinery below — it only needs *some* rows to
/// canonicalize.
fn row_strategy() -> impl Strategy<Value = TxRow> {
    ("[a-z]{1,6}", 0i64..5, -100_000i64..100_000, plain_description_strategy())
        .prop_map(|(id, day, cents, desc)| make_row(id, day, cents, desc))
}

/// Independent rows + provenance, for `idempotent` only. `idempotent` merges
/// a side against itself, so `o.intrinsic_eq(t)` is always true and `wins()`
/// is never invoked — the disjoint-oid-domain guarantee the conflict-bearing
/// properties below need (see IMPORTANT 3 in task-8-report.md) does not
/// apply here.
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

/// Widened description alphabet for the codec round-trip property (MINOR 5):
/// a CSV codec's real byte-stability hazards are commas, quotes, newlines,
/// and non-ASCII text. The plain `[a-z ]` alphabet used elsewhere never
/// exercises any of them.
fn hazard_description_strategy() -> impl Strategy<Value = String> {
    let alphabet: Vec<char> = "abc ,\"\n\u{00e9}\u{00e8}\u{1f600}".chars().collect();
    prop::collection::vec(prop::sample::select(alphabet), 0..12)
        .prop_map(|chars| chars.into_iter().collect())
}

fn round_trip_row_strategy() -> impl Strategy<Value = TxRow> {
    ("[a-z]{1,6}", 0i64..5, -100_000i64..100_000, hazard_description_strategy())
        .prop_map(|(id, day, cents, desc)| make_row(id, day, cents, desc))
}

// ===========================================================================
// Base + mutation scenario generator, for symmetric / fixed_point /
// balances_validate (IMPORTANT 2)
//
// The original design generated `base`, `a`, and `b` fully independently.
// That makes `base.intrinsic_eq(side_row)` — required to detect "this side
// left the row unchanged" for both the delete path and the three-way
// unchanged check — practically never hold, since it needs matching date AND
// description AND exact cents by pure chance. Measured over 200,000 cases,
// this reached the one-sided-edit path (where the money-loss bug this merge
// once had actually lived), the Deleted path, and the equal-epoch oid
// tiebreak path exactly ZERO times.
//
// Fixing this means generating a base, then deriving each side from it by
// mutation, so "unchanged relative to base" is common instead of
// astronomically rare.
// ===========================================================================

const UNIVERSE: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

fn base_strategy() -> impl Strategy<Value = Vec<TxRow>> {
    prop::sample::subsequence(UNIVERSE.to_vec(), 1..=4)
        .prop_flat_map(|ids| {
            let n = ids.len();
            (Just(ids), prop::collection::vec(content_strategy(), n))
        })
        .prop_map(|(ids, contents)| {
            ids.into_iter()
                .zip(contents)
                .map(|(id, (day, cents, desc))| make_row(id.to_string(), day, cents, desc))
                .collect()
        })
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Keep,
    Edit,
    Delete,
}

fn mutation_strategy() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        3 => Just(Mutation::Keep),
        3 => Just(Mutation::Edit),
        1 => Just(Mutation::Delete),
    ]
}

/// A positive, nonzero delta so an `Edit` always changes the amount.
fn edit_delta_strategy() -> impl Strategy<Value = i64> {
    1i64..500
}

/// The plain alphabet used elsewhere never contains `-`, so appending this
/// marker guarantees an edited/added row can never collide byte-for-byte
/// with the unedited base row, and that the two sides' independent
/// edits/adds of the *same* id can never accidentally coincide (which would
/// silently turn an edit/edit conflict into a no-op, or an add/add re-key
/// into the identical-content collapse) — both of those independent-luck
/// failures are exactly what starved IMPORTANT 2's paths in the first place.
fn side_marker(side: &'static str) -> String {
    format!("-{side}")
}

fn mutated_side_strategy(
    base: Vec<TxRow>,
    extra_ids: Vec<String>,
    side_tag: &'static str,
) -> impl Strategy<Value = Vec<TxRow>> {
    let base_len = base.len();
    let extra_len = extra_ids.len();
    (
        prop::collection::vec(mutation_strategy(), base_len),
        prop::collection::vec(edit_delta_strategy(), base_len),
        prop::collection::vec(prop::option::of(content_strategy()), extra_len),
    )
        .prop_map(move |(mutations, deltas, adds)| {
            let mut rows = Vec::new();
            for ((base_row, mutation), delta) in base.iter().zip(mutations).zip(deltas) {
                match mutation {
                    Mutation::Keep => rows.push(base_row.clone()),
                    Mutation::Delete => {}
                    Mutation::Edit => {
                        let mut r = base_row.clone();
                        r.description = format!("{}{}", r.description, side_marker(side_tag));
                        r.amount = Money::from_cents(r.amount.cents() + delta);
                        rows.push(r);
                    }
                }
            }
            for (extra_id, maybe_content) in extra_ids.iter().zip(adds) {
                if let Some((day, cents, desc)) = maybe_content {
                    let desc = format!("{desc}{}", side_marker(side_tag));
                    rows.push(make_row(extra_id.clone(), day, cents, desc));
                }
            }
            rows.sort_by(|a, b| a.id.cmp(&b.id));
            rows.dedup_by(|a, b| a.id == b.id);
            rows
        })
}

/// Committer epochs from a tiny range so equal epochs — and therefore the
/// oid tiebreak in `wins()` — are common rather than a 1/1000 fluke.
///
/// The oid ranges passed in by callers below are disjoint between "ours" and
/// "theirs" (IMPORTANT 3): that makes `ours.commit_oid != theirs.commit_oid`
/// a structural guarantee, so the two sides' full `Provenance` can never be
/// bytewise identical even when the epochs coincide — which is exactly what
/// used to be able to trip `wins()`'s
/// `debug_assert!(ours != theirs)`, and which proptest's shrinker tends to
/// shrink INTO (toward epoch 0, oid 0) rather than away from, misreporting
/// its own cause on a genuine future failure.
fn provenance_strategy(oid_range: std::ops::Range<u8>) -> impl Strategy<Value = Provenance> {
    (0i64..3, oid_range).prop_map(|(epoch, oid)| Provenance {
        committer_epoch: epoch,
        commit_oid: [oid; 20],
    })
}

const OURS_OID_DOMAIN: std::ops::Range<u8> = 0..80;
const THEIRS_OID_DOMAIN: std::ops::Range<u8> = 100..200;

fn scenario_strategy() -> impl Strategy<Value = (Vec<TxRow>, Sided, Sided)> {
    base_strategy().prop_flat_map(|base: Vec<TxRow>| {
        let base_ids: Vec<String> = base.iter().map(|r| r.id.clone()).collect();
        let extra_ids: Vec<String> = UNIVERSE
            .iter()
            .map(|s| s.to_string())
            .filter(|id| !base_ids.contains(id))
            .collect();
        let base_for_map = base.clone();
        (
            mutated_side_strategy(base.clone(), extra_ids.clone(), "ours"),
            provenance_strategy(OURS_OID_DOMAIN),
            mutated_side_strategy(base.clone(), extra_ids.clone(), "theirs"),
            provenance_strategy(THEIRS_OID_DOMAIN),
        )
            .prop_map(move |(ours_rows, ours_prov, theirs_rows, theirs_prov)| {
                (
                    base_for_map.clone(),
                    Sided { rows: ours_rows, provenance: ours_prov },
                    Sided { rows: theirs_rows, provenance: theirs_prov },
                )
            })
    })
}

// ===========================================================================
// Correctness properties
// ===========================================================================

proptest! {
    // Raised from the default 256 (IMPORTANT 2's closing note): at the old
    // generator's ~1.29% add/add hit rate, roughly 1 run in 27 never
    // exercised that path at all. 5,000 cases makes starving any of the
    // now-common paths in a single CI run implausible.
    #![proptest_config(ProptestConfig::with_cases(5000))]

    /// Symmetry — "prefer ours" would make each machine choose its own side and
    /// the two would diverge permanently.
    #[test]
    fn symmetric((base, ours, theirs) in scenario_strategy()) {
        let ab = merge(Some(&base), &ours, &theirs);
        let ba = merge(Some(&base), &theirs, &ours);
        prop_assert_eq!(render_transactions(&ab.rows), render_transactions(&ba.rows));
    }

    /// FIXED POINT — re-merging a merged result against one of its inputs
    /// changes nothing. This is what proves the machines stop re-merging.
    ///
    /// CRITICAL 1 fix: the merged side used to be re-wrapped with
    /// `provenance: a.provenance` (the ORIGINAL "ours" provenance), so both
    /// the first and second merge saw the identical (ours, theirs)
    /// provenance pair. Any PURE FUNCTION OF PROVENANCE — e.g. a
    /// provenance-derived re-key suffix, which is precisely the defect this
    /// property exists to catch — produces the same suffix both times under
    /// that setup, and the property passed anyway (256 and 200,000 cases).
    /// A real second sync round is a NEW commit, with a fresh oid and a
    /// later epoch than anything already seen, so `merged_side` must get a
    /// FRESH provenance instead of reusing either side's.
    #[test]
    fn fixed_point((base, ours, theirs) in scenario_strategy()) {
        let first = merge(Some(&base), &ours, &theirs);
        let fresh_epoch = ours.provenance.committer_epoch.max(theirs.provenance.committer_epoch) + 1;
        // oid 220 is outside both OURS_OID_DOMAIN (0..80) and
        // THEIRS_OID_DOMAIN (100..200), so it can never coincide with either
        // side's oid.
        let fresh_provenance = Provenance { committer_epoch: fresh_epoch, commit_oid: [220u8; 20] };
        let merged_side = Sided { rows: first.rows.clone(), provenance: fresh_provenance };
        let second = merge(Some(&base), &merged_side, &theirs);
        prop_assert_eq!(render_transactions(&first.rows), render_transactions(&second.rows));
    }
}

proptest! {
    /// Idempotence — merging a side with itself is just canonicalisation.
    ///
    /// MINOR 6: `ours == theirs == a` here, so `o.intrinsic_eq(t)` is always
    /// true inside `merge` and NO conflict branch (edit/edit, one-sided-edit,
    /// delete, add/add) is ever entered. This property only guards
    /// canonicalisation (sort + running-balance recompute); conflict-path
    /// coverage lives in `symmetric` / `fixed_point` / `balances_validate`
    /// and in `generator_path_coverage` / `duplicate_dropped_is_reachable_and_logged`
    /// below.
    #[test]
    fn idempotent(a in sided_strategy()) {
        let out = merge(Some(&a.rows), &a, &a);
        prop_assert_eq!(render_transactions(&out.rows), render_transactions(&{
            let mut rows = a.rows.clone();
            recompute_running_balances(&mut rows);
            rows
        }));
    }

    /// Byte round-trip — render(parse(x)) == x for canonical x.
    ///
    /// MINOR 5: description now includes commas, double quotes, newlines,
    /// and non-ASCII text (see `hazard_description_strategy`) — the actual
    /// byte-stability hazards for a CSV codec. The old `[a-z ]` alphabet
    /// never exercised any of them.
    #[test]
    fn round_trip_is_byte_stable(rows in prop::collection::vec(round_trip_row_strategy(), 0..10)) {
        let once = render_transactions(&rows);
        let twice = render_transactions(&parse_transactions(&once).unwrap().rows);
        prop_assert_eq!(once, twice);
    }

    /// Money is always self-consistent after a merge.
    #[test]
    fn balances_validate((base, ours, theirs) in scenario_strategy()) {
        let out = merge(Some(&base), &ours, &theirs);
        prop_assert!(validate(&out.rows).is_empty());
    }
}

// ===========================================================================
// The timezone hazard, tested directly (IMPORTANT 4)
//
// The old test manipulated `TZ` around a SINGLE `+00:00` literal, so
// differing offsets were never exercised; `parse_from_rfc3339`/`to_rfc3339`
// never consult `chrono::Local` in the first place; and `set_var("TZ")`
// without a `tzset()` call would not even move the libc-cached offset. It
// applied the same pure function to the same string twice and asserted the
// tautology. It also left `TZ=America/Los_Angeles` set process-wide for
// every test that ran after it.
//
// The real hazard `intrinsic_eq` was fixed for: two machines in different
// UTC offsets can each write "the same instant" with different rendered
// bytes (`DateTime`'s `PartialEq` compares instants; `to_rfc3339` renders the
// stored offset). Test that directly: same instant, two different offsets,
// and check the merge treats them as a genuine, deterministically-resolved
// conflict rather than silently colliding — regardless of which side is
// "ours".
// ===========================================================================

#[test]
fn merge_treats_same_instant_different_offset_as_distinct_and_converges() {
    let make = |offset: &str, desc: &str| TxRow {
        id: "a".to_string(),
        child_id: "c".to_string(),
        date: DateTime::parse_from_rfc3339(offset).unwrap(),
        description: desc.to_string(),
        amount: Money::from_cents(100),
        balance: Money::from_cents(0),
        tx_type: TxType::Expense,
    };
    let base_row = make("2025-12-01T00:00:00+00:00", "orig");
    // Same instant, two different machine-local offsets -- the hazard.
    let utc_write = make("2026-01-01T00:00:00+00:00", "edited");
    let la_write = make("2025-12-31T19:00:00-05:00", "edited");
    assert_eq!(
        utc_write.date, la_write.date,
        "must be the same instant for this test to mean anything"
    );
    assert_ne!(
        utc_write.date.to_rfc3339(),
        la_write.date.to_rfc3339(),
        "must render to different bytes despite being the same instant"
    );
    assert!(
        !utc_write.intrinsic_eq(&la_write),
        "same instant, different offset must be treated as a genuine difference"
    );

    let base = vec![base_row];
    let ours = Sided {
        rows: vec![utc_write.clone()],
        provenance: Provenance { committer_epoch: 1, commit_oid: [5u8; 20] },
    };
    let theirs = Sided {
        // Higher epoch: theirs must win the conflict.
        rows: vec![la_write.clone()],
        provenance: Provenance { committer_epoch: 9, commit_oid: [200u8; 20] },
    };

    let ab = merge(Some(&base), &ours, &theirs);
    let ba = merge(Some(&base), &theirs, &ours);
    assert_eq!(
        render_transactions(&ab.rows),
        render_transactions(&ba.rows),
        "must converge to identical bytes regardless of side order"
    );
    assert_eq!(ab.rows.len(), 1, "one id 'a' must yield exactly one surviving row, not a silent duplicate");
    assert_eq!(
        ab.rows[0].date.to_rfc3339(),
        la_write.date.to_rfc3339(),
        "the higher-epoch write must win"
    );
}

// ===========================================================================
// IMPORTANT 3: the strategy must be structurally incapable of producing
// identical provenance on both sides.
// ===========================================================================

#[test]
fn provenance_strategy_domains_never_overlap() {
    assert!(OURS_OID_DOMAIN.end <= THEIRS_OID_DOMAIN.start);
    for oid in OURS_OID_DOMAIN {
        assert!(!THEIRS_OID_DOMAIN.contains(&oid));
    }
}

// ===========================================================================
// IMPORTANT 2: instrument and report the actual hit rate of each decision
// path the generator is supposed to reach.
// ===========================================================================

#[derive(Default, Debug)]
struct PathHits {
    add_add_rekey: u32,
    edit_edit_both_changed: u32,
    one_sided_edit_wins: u32,
    deleted: u32,
    equal_epoch_conflict: u32,
    duplicate_dropped: u32,
    edit_beat_delete: u32,
}

fn classify(base: &[TxRow], ours: &Sided, theirs: &Sided, out: &MergeOutcome, hits: &mut PathHits) {
    let base_map: HashMap<&str, &TxRow> = base.iter().map(|r| (r.id.as_str(), r)).collect();
    let ours_map: HashMap<&str, &TxRow> = ours.rows.iter().map(|r| (r.id.as_str(), r)).collect();
    let theirs_map: HashMap<&str, &TxRow> = theirs.rows.iter().map(|r| (r.id.as_str(), r)).collect();
    for d in &out.decisions {
        match d {
            Decision::KeptBothReKeyed { .. } => hits.add_add_rekey += 1,
            Decision::Deleted { .. } => hits.deleted += 1,
            Decision::DuplicateDropped { .. } => hits.duplicate_dropped += 1,
            Decision::EditBeatDelete { .. } => hits.edit_beat_delete += 1,
            Decision::TookOurs { id } | Decision::TookTheirs { id } => {
                if let (Some(b), Some(o), Some(t)) =
                    (base_map.get(id.as_str()), ours_map.get(id.as_str()), theirs_map.get(id.as_str()))
                {
                    let o_unchanged = b.intrinsic_eq(o);
                    let t_unchanged = b.intrinsic_eq(t);
                    if o_unchanged != t_unchanged {
                        hits.one_sided_edit_wins += 1;
                    } else {
                        hits.edit_edit_both_changed += 1;
                        if ours.provenance.committer_epoch == theirs.provenance.committer_epoch {
                            hits.equal_epoch_conflict += 1;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn generator_path_coverage() {
    const CASES: u32 = 20_000;
    let mut runner = TestRunner::new(Config::with_cases(CASES));
    let strategy = scenario_strategy();
    let mut hits = PathHits::default();
    for _ in 0..CASES {
        let tree = strategy.new_tree(&mut runner).expect("strategy generation must not fail");
        let (base, ours, theirs) = tree.current();
        let out = merge(Some(&base), &ours, &theirs);
        classify(&base, &ours, &theirs, &out, &mut hits);
    }

    // NOTE: these are raw DECISION COUNTS across all {CASES} cases, not
    // per-case hit probabilities. A single case's scenario has 1-4 base
    // rows, and each row can independently produce its own decision, so a
    // count can (and does) reflect more than one decision per case on
    // average or fewer, depending on the path -- do not divide by CASES and
    // read the result as "the probability a case exercises this path."
    println!(
        "generator_path_coverage over {CASES} cases (decision counts, not per-case rates): \
         add_add_rekey={} edit_edit_both_changed={} one_sided_edit_wins={} \
         deleted={} edit_beat_delete={} equal_epoch_conflict={} duplicate_dropped_organic={}",
        hits.add_add_rekey,
        hits.edit_edit_both_changed,
        hits.one_sided_edit_wins,
        hits.deleted,
        hits.edit_beat_delete,
        hits.equal_epoch_conflict,
        hits.duplicate_dropped,
    );

    assert!(hits.add_add_rekey > 0, "add/add re-key path never hit: {hits:?}");
    assert!(hits.edit_edit_both_changed > 0, "edit/edit both-changed path never hit: {hits:?}");
    assert!(hits.one_sided_edit_wins > 0, "one-sided-edit path never hit: {hits:?}");
    assert!(hits.deleted > 0, "Deleted path never hit: {hits:?}");
    assert!(hits.edit_beat_delete > 0, "EditBeatDelete path never hit: {hits:?}");
    assert!(hits.equal_epoch_conflict > 0, "equal-epoch oid-tiebreak path never hit: {hits:?}");
    // `duplicate_dropped` is deliberately NOT asserted here: it only fires on
    // a 64-bit hash collision between a re-keyed id and another surviving
    // id, which is not organically reachable from this generator at any
    // realistic case count. `merge::tests::duplicate_dropped_is_reachable_and_logged`
    // (a UNIT test inside allowance-core/src/merge.rs, which can call the
    // real private `content_suffix`/`wins` directly instead of a duplicated
    // copy) reaches it by construction instead. This test still reports its
    // organic count (expected: 0) for visibility.
}
