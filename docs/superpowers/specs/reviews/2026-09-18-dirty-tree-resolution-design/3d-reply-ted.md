# [3D] Reply to Ted Thornberry's Critique

**Spec:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer addressed:** Ted Thornberry
**Reply date:** 2026-09-19

---

## Overall Response

Accepted with one deferral and one partial. Your framing — "the Testing section
is a list of six examples, not a test design" — was the correct diagnosis, and
the question you insisted on ("both defects walked past a suite with 5,000-case
property tests; why?") produced the two most valuable additions in this revision:
the cycle-progress driver and the ownership-completeness contract test.

## Point-by-Point

### Concern 1: The `commit_file_change` fix is inert at every call site

**Verdict:** Accepted.

**Our response:** Found independently by you, Greg and Deiko — three of four
reviewers on one point, which settled it. Under Deiko's Concern 1 the design
widened the guard to all tracked paths, which closes this route directly rather
than by propagating the error: a file left dirty by a swallowed commit is staged
and committed by the guard on the next cycle. Propagation is dropped from scope
with the reason recorded, not quietly retained.

Your "decide, per call site, what a commit failure means" is preserved in the
hygiene half: `#[must_use]` plus an explicit logged disposition at each of the
five sites, so the decision is visible rather than a discard. Your read — a failed
*commit* should not fail the user's *write*, because the data is already on disk —
is the disposition recorded at all five.

### Concern 2: Nothing tests progress across cycles

**Verdict:** Accepted. This is the most valuable change in the revision.

**Our response:** "Defect 1 is a liveness failure" is the sentence the original
test plan needed and did not have. Your observation that the fast-forward path has
the suite's only progress assertion
(`a_dirty_tree_blocking_a_fast_forward_resolves_instead_of_refusing_forever`,
`app_coordinator.rs:3178`) and is also the path the spec says got this right is not
a coincidence, and it is quoted in the spec.

`run_cycles_until_terminal(&mut app, child_id, max)` is adopted as specified,
including the diagnostic you asked for: the outcome sequence on failure, so the
message reads "the same outcome `DirtyTreeCommitted` 5 times with HEAD unmoved"
rather than "assertion failed: false". Every dirty-tree scenario runs through it
instead of asserting one enum.

### Concern 3: The no-silent-stall test should be a table, and the invariant is too weak

**Verdict:** Partial — invariant accepted, table accepted, proptest argument not
re-litigated.

**Our response:** The invariant strengthening is accepted without reservation and
is the part that matters: "tree clean **and HEAD advanced**, or a
`SyncFailureNotice` exists" — because the original postcondition holds vacuously
if the guard commits something unrelated and leaves the real problem for the next
cycle. `assert_resolved_or_explained(&app, &repo, child_id)` is adopted and called
at the end of every dirty-tree test, not only the one that owns the invariant.
That is the durable half of the concern and it is independent of which driver
generates the cases.

The table is adopted too, over the cross product of file states.

Where we did not follow you: the spec does not argue the proptest-versus-table
point at length. Deiko and Greg both praised the invariant as a property, you
argue the domain is enumerable, and all three positions are compatible with the
same implementation — an exhaustive table plus a shared assertion helper. The
shrinker objection is real and is recorded as the reason the table is
deterministic, without turning it into a methodology debate in the spec.

### Concern 4: The atomic-write test claims more than an in-process test can deliver

**Verdict:** Accepted.

**Our response:** "A `write_atomic` with both fsyncs deleted passes every test
listed above, forever" is the decisive observation — a test that cannot fail on a
broken implementation is not evidence.

All three recommendations taken:

1. The concurrent-reader test, with the negative control in the same file: the
   same loop against plain `fs::write` must be *shown* to produce a short read.
2. The `#[cfg(test)]`-injectable sink asserting the recorded syscall sequence.
   Your argument that for a durability primitive the call sequence *is* the
   contract is accepted — this is the one place an implementation test earns its
   keep.
3. Power-loss behaviour stated plainly as unverifiable in CI and added to
   `docs/lgs-sync-acceptance-checklist.md`.

One amendment from the fsync ruling elsewhere in this panel: the asserted sequence
is write → `sync_all` → rename, with no directory fsync, and the durability claim
is scoped to "no reader observes a partial file; a power loss may cost the most
recent write, never the file's integrity." Point 2's mechanism is unchanged; what
it asserts is narrower and now true.

### Concern 5: Existing tests encode the behaviour being removed

**Verdict:** Accepted.

**Our response:** Both named tests are enumerated in the spec with their new
expected outcomes:

- `child_sync.rs:1457` — asserts the hard reset; inverted to assert the dirty
  state is *preserved* and the marker cleared.
- `app_coordinator.rs:2331` — asserts "the crash's garbage must be gone";
  rewritten.

Your reading of the second one is the sharpest single observation in this review:
the suite asserted defect 2 as intended behaviour, with a doc comment explaining
why it was correct. That is the answer to "why did this ship," and it is quoted in
the spec.

On what happens to that fixture's literal `"garbage-from-a-crash"` once the reset
is gone: you and Greg and Deiko all converged on the same answer, and it is
adopted — the guard parse-validates `transactions.csv` with `parse_transactions`
before staging, and unparseable content raises the failure notice rather than
being committed and pushed. The spec states the decision explicitly and tests that
it is deliberate.

The claim that the three suites "run unchanged" is corrected.

### Concern 6: The backstop test's cited fixture will be vacuous

**Verdict:** Accepted.

**Our response:** Correct and specific: `:3140` writes an *untracked* `notes.txt`
and reaches the failure path through a checkout conflict that exists only on the
fast-forward path, while the merge path enters the guard only when
`working_tree_dirty` is true — and that helper sets `include_untracked(false)`
(`child_sync.rs:1019`). Copying that fixture would have produced a test passing
for the wrong reason.

The merge-path test uses a **tracked** non-owned file modified in place, and
`assert!(working_tree_dirty(&repo)?)` is added as a precondition so a fixture that
stops reproducing the condition fails loudly. The precondition-assertion habit is
applied to the other dirty-tree tests too.

Note the scenario's meaning changed under Deiko's Concern 1: a tracked non-owned
file is now *staged and committed* by the guard rather than refused, so this test
asserts resolution rather than failure. The vacuity problem you identified is the
same either way, and so is the fix.

### Concern 7: Nothing tests that `FILES_THIS_APP_OWNS` is complete

**Verdict:** Accepted.

**Our response:** "Worth more than all six proposed regression tests combined" is
a strong claim and we think it is right, because it closes the class rather than
the instance. Adopted as specified: exercise every repository write path against a
fresh child directory, then assert every tracked file present is in
`FILES_THIS_APP_OWNS` or on an explicit, comment-justified exemption list.

Its role shifts slightly under the widening — the guard no longer depends on the
list, so the test now protects `commit_merge` and migration rather than the stall
path. It is still the test that would have caught `.allowance_redirect` or a future
`budgets.csv`, and the rewritten doc comment becomes enforced rather than
aspirational.

### Concern 8: The fixtures are already copy-pasted, and this spec adds to them

**Verdict:** Accepted.

**Our response:** Verified: `app_with_git_backed_child` at `:2233`, `:2846`,
`:3274`; `commit_with_files` at `:2210`, `:2827`, `child_sync.rs:1130`. Your point
that the fixture work is a prerequisite rather than a cleanup is accepted —
Concern 3's cross-product table cannot be written on top of a copy-pasted setup
function. `ChildRepoFixture` with `.with_peer_commit`, `.with_dirty`,
`.with_deleted` and `.with_marker` is in scope and sequenced *before* the new
tests.

### Concern 9: The CI claims do not survive inspection

**Verdict:** Accepted for the corrections; deferred for the new CI job.

**Our response:** Both factual claims verified in the main session.
`.github/workflows/ci.yml` runs `cargo check --workspace` and `cargo test
--workspace` — no `--ignored`, no `LGS_BINARY` — and `two_machine_sync.rs:103`
and `codec_real_data.rs:18` are both `#[ignore]`d. Two of the three suites the
spec promised "run unchanged" have never run in CI.

Corrections taken: the spec's sentence now says which suites actually execute, and
the acceptance checklist's "What CI already covers" entry is fixed. Leaving that
claim standing would have told a reader not to hand-test something nothing tests.

Deferred: the second CI job installing `lgs` and running `--ignored`. The user
ruled to take every other test change and file this one separately, on the
grounds that it is a green-CI question rather than a data-loss one and is likely
to surface pre-existing failures that would otherwise block this work. Your
sentence — "an ignored test that never runs is documentation, not coverage" — is
recorded in the spec's follow-ups so the deferral is visible rather than dropped.

Your related point that the two-machine harness commits `ledger.txt` through raw
`GitManager` calls and never drives `ChildSyncEngine` or `apply_merge` — so no
test anywhere has two app instances converging real financial data through the
real code — is recorded in the spec as the structural reason both defects shipped.

## Questions Answered

### Q: What does the guard do with a dirty `transactions.csv` that does not parse?

A: Refuses. Parse-validated before staging; unparseable content raises the
durable failure notice and commits nothing. Specified, not left implicit.

### Q: `write_atomic` leaves a temp file behind if the process dies mid-write. Startup sweep?

A: No sweep needed, and the reason is a side effect of the widening worth stating:
a leftover temp file is *untracked*, `working_tree_dirty` ignores untracked files,
and `update_all` stages only tracked paths — so it can neither trigger the guard
nor be committed. It is inert litter. Recorded in the spec rather than left as an
open question.

### Q: How does anyone verify the backstop state in production?

A: Partially answered. The child-picker badge from Pierre's Concern 1 makes it
visible on the main surface. Adding a "working tree clean or explained" stage to
the "Check sync" flow is recorded as a follow-up rather than taken here — it is an
operational probe, and this revision is already carrying a UI change.

### Q: Is there a test that both callers' mappings agree on the failure branches?

A: The question dissolves under Greg's Concern 2: there is now one mapping.
`resolve_dirty_tree` returns `Result<git2::Oid, DirtyTreeError>` and both callers
route every error through a single `fail_sync` helper, so the divergence cannot
move up a level. The callers differ only on the success branch.

## Closing

One item deferred by the user's explicit ruling (the `--ignored` CI job), recorded
in the spec's follow-ups. Everything else accepted. Your Concern 2 and Concern 7
changed what this work will actually build, rather than how it is described.
