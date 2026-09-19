# [2D] Critique — Ted Thornberry

**Spec reviewed:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer:** Ted Thornberry, Principal Software Test Engineer
**Date:** 2026-09-18

---

## Overall Verdict

Approve with changes.

The design is a testability improvement on net — collapsing two divergent
dirty-tree paths into one helper with a three-variant result enum is exactly
the kind of change that turns "read both call sites and reason" into "assert on
a value." But the Testing section is a list of six examples, not a test design,
and it does not answer the question that matters most: *both of these defects
walked straight past a suite with 5,000-case property tests and eight
hand-built git integration tests.* Until the spec says why, it is proposing the
same kind of tests that already failed to catch this.

## Top Concerns

### Concern 1: the `commit_file_change` fix is inert at every call site

**What I see:** "Error handling" says `commit_file_change` (`git/mod.rs:388`)
"starts returning its commit error instead of swallowing it," and that this is
what strands `parental_control_attempts.csv`. Every one of the five callers
discards the result:

- `transaction_repository.rs:201` — `let _ = self.git_manager.commit_file_change(`
- `allowance_repository.rs:106`, `goal_repository.rs:100`,
  `child_repository.rs:107`, `parental_control_repository.rs:163` — identical.

`parental_control_repository.rs:162` even documents the intent: "This is
non-blocking - git errors won't fail the parental control operation."

**Why it matters:** Change `warn!` to `return Err(...)` and the observable
behaviour of the system is *identical* — the error moves from a log line to a
discarded `Result`. The file is still left dirty. Defect 1's first route is
still open. And no test in the plan detects this, because the plan tests
`stage_owned_files` and the guard, not the commit-failure origin. This is the
single most likely way this work ships and still has the bug.

**Recommendation:** Decide, per call site, what a commit failure means — my
read is that a failed *commit* should not fail the user's *write* (the data is
on disk), but it must become durably visible, not `let _ =`. Route it into the
same `SyncFailureNotice` channel the backstop uses. Then add the regression
test that actually proves the route is closed: fail a commit for
`parental_control_attempts.csv` (inject via `GitManager`, or make the file
unstageable), assert the failure is surfaced, and assert a subsequent
`apply_merge` still progresses rather than stalling.

### Concern 2: nothing in the plan tests progress across cycles — which is the shape of defect 1

**What I see:** Defect 1 is a *liveness* failure. A single `apply_merge` call
that returns `DirtyTreeCommitted` having committed nothing is not visibly wrong
in isolation; it is only wrong on the second, third and `STALE_HEAD_REFUSAL_LIMIT`th
cycle. Every test in `apply_merge_tests` calls `apply_merge` exactly once and
asserts on the returned enum and the resulting tree.

The fast-forward path has the test the merge path lacks:
`a_dirty_tree_blocking_a_fast_forward_resolves_instead_of_refusing_forever`
(`app_coordinator.rs:3178`) re-runs `classify` afterwards and asserts it no
longer returns `FastForward`. That is a progress assertion, and it is not a
coincidence that the fast-forward path is the one the spec says got this right.
The merge path got state tests; the fast-forward path got a progress test.

**Why it matters:** Every regression test the spec proposes is again a
single-shot call. All six can pass while a *different* future "staging produced
nothing" route stalls the same way.

**Recommendation:** Build the capability, not six more cases: a bounded
cycle-driver helper — `run_cycles_until_terminal(&mut app, child_id, max: u8)`
— that drives the real classify/apply loop against a fixed environment and
asserts it reaches a terminal state (Applied, AlreadyUpToDate, or Failed with a
notice) within `max`, returning the outcome sequence on failure so the message
reads "the same outcome DirtyTreeCommitted 5 times with HEAD unmoved" rather
than "assertion failed: false". Then every dirty-tree scenario in the plan runs
through it instead of asserting one enum. That is the test that catches defect
1 without knowing defect 1 exists.

### Concern 3: the no-silent-stall "property test" should be an exhaustive table, and the invariant is too weak

**What I see:** "the property behind all three: after the guard runs on any
dirty tree, either the tree is clean or a durable failure notice exists."

Two problems. First, the state space is finite and small — five owned files
plus a representative unowned tracked file, each in {unchanged, modified,
deleted, created}. That is enumerable. A proptest over this domain costs a
`git init` plus a repo build per case and gives you random coverage of a space
you could cover completely, plus a shrinker that will hand you a misleading
minimal case (see the note `properties.rs:195` already records about shrinking
*into* the failure). Property tests earn their keep over infinite or
high-arity domains — `allowance-core/tests/properties.rs` is a good example of
where they do. This is not one.

Second, "tree clean or notice exists" is a single-shot postcondition. It holds
vacuously if the guard commits *something unrelated* and leaves the real
problem for the next cycle. The invariant that failed is "the system makes
progress or says why."

**Recommendation:** Write it as a table-driven test over the cross product,
deterministic, one assertion helper:
`assert_resolved_or_explained(&app, &repo, child_id)` — and call that helper at
the end of *every* dirty-tree test in the suite, not only in the one that owns
the invariant. Strengthen the invariant to: the tree is clean **and HEAD
advanced**, or a `SyncFailureNotice` for this child exists. Keep proptest for
the merge algebra where it belongs.

### Concern 4: the atomic-write test claims more than an in-process test can deliver

**What I see:** "**Atomic write.** Content correct after success; no temp
residue after either success or failure; a failed write leaves the prior file
byte-identical."

Those three are all worth having and all testable. None of them test
atomicity. The durability half of `write_atomic` — `File::sync_all`, the order
of rename relative to it, and the containing-directory fsync — has *no
in-process observable at all*. A `write_atomic` with both fsyncs deleted passes
every test listed above, forever, and reintroduces exactly the zero-length-file
failure the spec cites against the six existing hand-rolled copies. Killing a
child process with SIGKILL proves nothing either: the page cache survives the
process, so a torn file cannot be produced that way.

**Recommendation:** Three things.

1. **Test the atomicity you can actually observe:** a reader thread reading and
   parsing `transactions.csv` in a tight loop while a writer performs N
   `write_atomic` calls with distinct contents. Assert every observed read is
   byte-equal to one of the known versions — never a prefix, never empty. Add
   the negative control in the same test file: the same loop against plain
   `fs::write` must be *shown* to produce a short read. A test that cannot fail
   on a broken implementation is not evidence.
2. **Make the syscall order observable.** For a durability primitive the call
   sequence *is* the contract; there is no other behaviour. I will accept an
   implementation test here — thread a `#[cfg(test)]`-injectable sink through
   `write_atomic` and assert the recorded sequence is
   write → sync_all → rename → dir-fsync. Do not leave a durability guarantee
   resting on a code comment.
3. **Say plainly what CI cannot prove** — power-loss behaviour — and add it to
   `docs/lgs-sync-acceptance-checklist.md` alongside item 1, or state that it is
   accepted unverified. That file is exactly where this belongs, and it already
   has the discipline for it.

### Concern 5: existing tests encode the behaviour being removed, and the spec says the suites run unchanged

**What I see:** "Existing `two_machine_sync`, `wire_compat` and
`codec_real_data` suites run unchanged." True, and beside the point. These will
break, and the spec does not name them:

- `child_sync.rs:1457` `a_dirty_tree_with_the_marker_present_is_discarded_and_the_marker_cleared`
  — asserts the hard reset this spec deletes.
- `app_coordinator.rs:2331` `a_dirty_tree_from_a_prior_crash_is_recovered_before_applying_the_next_merge`
  — asserts `Applied` and asserts "the crash's garbage must be gone."

That second one is the answer to "why did defect 2 ship": the suite asserted the
defect as intended behaviour, with a doc comment explaining why it was correct.

**Why it matters:** Its new expected outcome is uncomfortable and the spec
never states it. With the reset gone, the fixture's literal
`"garbage-from-a-crash"` is now *staged, committed, and pushed to the peer*.
The design's answer is "atomic writes make torn files impossible" — but that is
only true for files written by the new code, on this machine, after the
upgrade. A pre-upgrade torn file, or a half-materialized Proton read (checklist
item 1, still unrun), reaches the same guard.

**Recommendation:** Enumerate the tests to invert or delete in the spec, with
their new expected outcomes. And decide explicitly whether the guard commits
content it cannot parse: I would gate the `transactions.csv` stage on
`parse_transactions` succeeding, raising the backstop notice rather than
pushing unparseable bytes into the family's shared history. If you decide not
to, say so in the spec and test that the decision is deliberate.

### Concern 6: the backstop test's cited fixture will be vacuous on the merge path

**What I see:** "Backstop. Dirty a tracked file outside the owned list
(`notes.txt` — the shape `app_coordinator.rs:3140` already sets up)."

`:3140` writes an **untracked** `notes.txt`; it reaches the failure path via a
*checkout conflict*, which only exists on the fast-forward path. The merge path
enters the guard only when `working_tree_dirty` is true, and that helper sets
`include_untracked(false)` (`child_sync.rs:1019`). Copy that fixture and the
guard never runs: the test passes for the wrong reason, or the author
"corrects" the assertion until it does.

**Recommendation:** The merge-path backstop needs a **tracked** non-owned file
modified in place. Add a precondition assertion to the test —
`assert!(working_tree_dirty(&repo)?)` — so a fixture that stops reproducing the
condition fails loudly instead of passing vacuously.

### Concern 7: nothing tests that `FILES_THIS_APP_OWNS` is complete

**What I see:** Defect 1's first route is, stripped down, "this app writes a
file into a child directory that is not in the ownership list." The fix adds
one name to the list. Nothing prevents the next one — `.allowance_redirect`
(`migration.rs:250`) already lives in a child directory, and any future
`budgets.csv` walks into the same stall.

**Recommendation:** One contract test closes the whole class: exercise every
repository write path against a fresh child directory, then assert every
tracked file present is in `FILES_THIS_APP_OWNS` or on an explicit,
comment-justified exemption list. The spec's rewritten doc comment ("this list
is everything the dirty-tree guard may commit") becomes enforced rather than
aspirational. This is worth more than all six proposed regression tests
combined.

### Concern 8: the fixtures are already copy-pasted, and this spec adds to them

**What I see:** `app_with_git_backed_child` exists three times in
`app_coordinator.rs` (`:2233`, `:2846`, `:3274`); `commit_with_files` three
times (`:2210`, `:2827`, `child_sync.rs:1130`). The spec adds at least six more
tests into those same modules.

**Recommendation:** Extract a `ChildRepoFixture` builder into shared test
support before writing the new tests — `.with_peer_commit(files)`,
`.with_dirty(file, content)`, `.with_deleted(file)`, `.with_marker()`. You
cannot write Concern 3's exhaustive table on top of a copy-pasted setup
function; the fixture work is a prerequisite, not a cleanup.

### Concern 9: the CI claims do not survive inspection

**What I see:** `.github/workflows/ci.yml` runs `cargo test --workspace` — no
`--ignored`, no `LGS_BINARY`. `two_machine_sync.rs:103` and
`codec_real_data.rs:18` are both `#[ignore]`d. So two of the three suites the
spec promises "run unchanged" have never run in CI at all. Worse, the
acceptance checklist's "What CI already covers" lists the two-machine harness
as covered and tells the reader not to re-test it by hand. It is not covered.

And even when run, that harness commits `ledger.txt` through raw `GitManager`
calls — it never drives `ChildSyncEngine` or `apply_merge`. There is no test
anywhere in which two instances of this app converge real financial data
through the real code. That is the structural reason both defects shipped.

**Recommendation:** Two corrections and one addition. Correct the spec's
sentence to say which suites actually execute in CI. Correct the checklist's
"What CI already covers" entry. Then add a second job to `ci.yml` that installs
`lgs` and runs `cargo test --workspace -- --ignored`, even if it starts as
non-blocking — an ignored test that never runs is documentation, not coverage.

## Questions the spec does not answer

- What does the guard do with a dirty `transactions.csv` that does not parse?
  Commit and push it, or refuse and raise the backstop? The spec's own
  "torn or truncated file is possible" framing makes this reachable today.
- `write_atomic` leaves `.<name>.tmp-<pid>-<nonce>` behind if the process dies
  mid-write. Is there a startup sweep? What tests it?
- When the backstop fires and sync stops for a child, how does anyone verify
  that state in production? Does "Check sync" gain a stage for "working tree
  clean or explained"? Right now the only evidence is a UI notice, with no
  operational probe.
- `DirtyTreeResolution` is returned to two callers that map it to two different
  outcome enums. Is there a test that both mappings agree on the failure
  branches, or does the divergence this spec is fixing just move up a level?

## What I thought was well-handled

The existing property suite in `allowance-core/tests/properties.rs` is the best
test code in this repository — `generator_path_coverage` asserting that each
decision path is actually reached is the discipline most property suites skip,
and the comments explaining *why* the old generator starved those paths are
worth more than the tests. `two_machine.rs`'s isolation argument ("structurally
guaranteed by `Machine::lgs_command`") and its v1/v2/v3 history of the
convergence loop are how a harness should be documented. The spec's own
`DirtyTreeResolution` enum is a genuine testability win, and the deliberate
append-only exception for `parental_control_attempts.csv` — with its weaker
guarantee written down rather than assumed — is exactly the right way to record
a trade.

## Closing

The design is right; the test plan is a list of examples where it needs to be a
set of capabilities. Fix Concern 1 (the inert commit fix — that is a design
bug, not a test gap), add the cycle-progress harness and the ownership-completeness
contract test, downgrade the atomic-write and property-test claims to what they
can actually prove, and name the existing tests that must be inverted. Then
this is ready.
