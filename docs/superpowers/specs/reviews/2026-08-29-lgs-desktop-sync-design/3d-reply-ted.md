# [3d] Reply to Ted Thornberry's Critique

**Spec:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md`
**Reviewer addressed:** Ted Thornberry
**Reply date:** 2026-09-06

---

## Overall Response

Both blocking concerns accepted. Concern 2 is the one that mattered most: the
spec had chosen the right invariant and then stopped one property short of the
one that actually proves the loop terminates. Concern 6 also caught a factual
error in the spec's own testing section, which is now corrected.

## Point-by-Point

### Concern 1 (blocking): the resolution rule's input is undefined, and one reading makes `merge` impure

**Verdict:** Accepted, adopting your preferred resolution.

**Our response:** You are right that the spec picked neither reading, and right
that they give different answers. Deiko reached the same ambiguity from the
architecture side and added the sharper version of why tip-of-side is bad:
because every write rewrites the whole file, `git blame` attributes every row to
the newest commit, so per-row provenance derived from history would degenerate to
tip-of-side anyway.

Specified as you proposed: `merge` takes provenance explicitly —
`merge(base, ours: Sided<Row>, theirs: Sided<Row>)` where `Sided` pairs each row
with `(committer_epoch, commit_oid)` resolved by the caller. The caller does the
git walk; the merge stays a total function over data and stays exhaustively
testable, and §Component boundaries no longer claims `merge` depends on
"nothing" while secretly needing commit metadata.

Your discriminating test is in the spec's test table: B commits an unrelated goal
edit *after* A's conflicting transaction edit; assert A's row wins.

### Concern 2 (blocking): symmetry is necessary and not sufficient

**Verdict:** Accepted in full, including all four properties.

**Our response:** All four causes verified. The `Utc::now()` fallback
(`transaction_repository.rs:128-130`) is the worst of them and is not merely a
test problem — one malformed row makes read-modify-write non-idempotent, so both
machines re-merge forever and the cloud drive churns. It is now a hard error
before a merge, never a fresh timestamp.

The single symmetry property is replaced by the four-property suite over bytes:
symmetry, idempotence, **fixed point** (`merge(merge(A,B), B) == merge(A,B)`), and
round-trip stability over a corpus including the real production
`transactions.csv`. The fixed-point property is the one that proves the re-merge
loop cannot happen, and the spec says so rather than leaving the reader to infer
it.

The total order `(date, id)` is stated explicitly and applies on every write, not
only post-merge, so the recompute is a pure function of the row set rather than
of file read order. The `TZ=UTC` vs `TZ=America/Los_Angeles` determinism test is
in the table.

On floats: the author took the larger fix rather than the epsilon workaround —
`Money(i64)` at the domain boundary with a canonical 2-decimal renderer replacing
`f64::to_string()`. That removes the non-associativity and the ULP-drift path
outright rather than making the property tolerate them.

### Concern 3: the money recompute is repository-coupled

**Verdict:** Accepted.

**Our response:** Your point that the stated reason for a pure merge applies at
least as much to the balance arithmetic is correct, and the spec's boundary was
inconsistent with its own rationale. 256 filesystem round trips per property run
inside a crate that links wgpu is not a test suite anyone will keep.

Specified: pure `recompute(rows)` and `validate(rows)`, with the existing
`BalanceService` methods becoming thin read/apply/write wrappers. The merge
pipeline is `merge → recompute → validate`, all pure. Greg reached the same
conclusion from the performance side (`find_child_id_for_transaction` per row is
quadratic I/O), so the change is doubly motivated.

### Concern 4: the two-machine test does not need two machines

**Verdict:** Accepted.

**Our response:** This materially improves the spec's testing story and shrinks
the manual checklist to something someone will actually run.

Specified: a `TwoMachineHarness` fixture (`machine_a()`, `machine_b()`,
`cloud()`, `sync_both_ways()`) in one place, built on the pattern lgs already
proves — `lgs::paths` resolves from `$HOME` and `spawn_test_daemon` supports two
daemons over one `TempDir` cloud root. Moved into CI: `install-service`
idempotence, `restore` refusal fall-through, archived-project 403, adoptable
filtering.

Your fixture-rot argument is accepted and specified: recorded fixtures are kept
for *parse* tests only, sourced from lgs's blessed `current-status-json.json`
rather than a hand-retyped second copy, with the lgs commit pinned and CI failing
on drift.

What remains manual: Proton File Provider materialization, launchd-at-login, and
real clock skew.

### Concern 5: no injection seams for the new roots

**Verdict:** Accepted.

**Our response:** Correct that the cloud-path guard is the most important safety
rule in the document and that, as written, it would have been verified once by
hand and then silently regressed.

Specified: one `SyncPaths { data_dir, children_root, lgs_binary, cloud_root }`
threaded from startup, and the guard as a pure predicate
`is_cloud_synced(candidate: &Path, env: &SyncPaths) -> Option<Reason>` with a
table test per rejection reason plus the accept case. Greg asked for the same
function shape from the code-structure side.

`GitManager` gets an injectable time source. Your reason for it is the one
recorded in the spec: with `Signature::now()` hardcoded, no test can construct a
committer-timestamp tie, so the tiebreak-by-hash branch of the resolution rule
would ship uncovered.

### Concern 6: `backend/` is `#[path]`-included into the GUI crate

**Verdict:** Compromise.

**Our response:** The diagnosis is accepted and the factual correction is
accepted without qualification — the spec's claim that "the workspace has no
dev-dependencies today" was wrong (`egui-frontend/Cargo.toml:58` has `tempfile`,
and `backend/storage/csv/test_utils.rs` has a real `TestHelper` fixture). The
spec is corrected and now says explicitly that new fixtures extend `TestHelper`
rather than starting an island beside it.

The compromise is on scope. Extracting all of `backend/` into its own crate is
the right end state and is a large mechanical refactor orthogonal to sync. The
author's call: extract **only** the merge and the pure balance arithmetic into a
small crate now. That puts the compile-error boundary exactly where the
correctness risk lives — a merge module that cannot import `std::fs` because
nothing in its dependency graph offers it — and leaves the wider extraction for
its own change.

What that leaves on the table: backend tests other than the merge still compile
wgpu, and the property suite's speed advantage applies only to the extracted
module. Both are acceptable at current suite size and neither blocks the
invariant you were protecting.

### Concern 7: the test table omits several claims the spec makes

**Verdict:** Accepted, with one narrowed.

**Our response:** Added to the test table: the cloud-synced-path guard; "no retry
queue is needed" as an actual test (kill the remote, five writes, restore, assert
all five land in order); AWS/git write serialization, which is now testable
because all working-tree mutation is on the UI thread behind one `ApplyMerge`
message; crash mid-merge; and schema-skew fidelity — a version marker that
refuses a merge from a newer schema, since `parse_transaction_type` falling
through to *derivation* would otherwise let an older app silently downgrade a row
type and push it.

Crash mid-merge is specified: on startup, a dirty tree on a diverged branch
discards the working changes and re-runs the merge, which is safe precisely
because the merge is deterministic.

**Narrowed:** three-machine convergence. Rather than test it or leave the claim
unsupported, the author narrowed the goal — the spec now says two machines, not
"two or more". Your observation that "edit beats delete" is not obviously
confluent across three peers is recorded in the spec as the reason, so the
constraint is documented rather than forgotten.

### Concern 8: operational testability stops at reporting

**Verdict:** Accepted.

**Our response:** A "Check sync" action is specified: write a sentinel to a
scratch file in the child repo, commit, push, fetch, read back, reporting each
stage pass/fail by name. Your framing — the manual checklist's happy path,
automated, usable by a non-technical person over the phone — is the reason it
earns its place, given the no-terminal goal.

The spec now also says where merge decisions are logged and how long they are
kept, replacing "discarded versions are logged".

## Questions Answered

### Q: What does the app do when `validate_all_balances` returns errors after a merge?

A: It becomes `Result<(), Vec<BalanceMismatch>>` (Greg's Concern 6). A validation
failure aborts the merge before commit and surfaces; it does not commit and warn.
Money that fails its own check must not reach the remote.

### Q: Where does CI get the lgs source to bundle?

A: A pinned git dependency on a specific lgs commit, recorded in the spec. You
are right that compiling the binary couples the build to that source anyway — the
reason for preferring the CLI over the library was interface stability, not
build decoupling, and the spec no longer implies otherwise.

### Q: What is the merge's failure mode when a CSV is malformed on one side?

A: Refuse and surface. With the `Utc::now()` fallback removed, a parse failure is
an error, and a merge that cannot read one side does not proceed.

### Q: Does the convergence test assert identical trees or identical parsed content?

A: Trees, via the existing `tree_checksum` (`csv/checksum.rs:17`) that Greg
identified. Stated explicitly, with your reason: the float and timezone hazards
only fail under the strong assertion.

### Q: How does a test reach the "already registered, `lgs restore` refuses" branch?

A: Via the `TwoMachineHarness` from Concern 4. Said so in the spec.

## Closing

Concerns 1 and 2 were named as blocking for planning; both are resolved. Concern
6 is a documented compromise with its cost stated. Nothing else is left open.
