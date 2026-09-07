# [2D] Critique — Ted Thornberry

**Spec reviewed:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md`
**Reviewer:** Ted Thornberry, Principal Software Test Engineer
**Date:** 2026-08-29

---

## Overall Verdict

**Approve with changes.** The Testing section is better than most specs get: you
named the offline seam, you made `merge` I/O-free on purpose, and you identified
the symmetry property as the load-bearing invariant. Two of the changes below are
blocking for the merge module — not because the design is wrong, but because as
written **I cannot write the merge tests**, and neither can whoever gets the plan.

## Top Concerns

### Concern 1 (blocking): the resolution rule's input is undefined, and one reading makes `merge` impure

**What I see:** §The resolution rule must be symmetric — "Later committer
timestamp wins; ties broken by commit hash." Later than *what*? There are two
readings and the spec picks neither:

- **Tip-of-side:** compare `ours` HEAD committer time vs `theirs` HEAD.
- **Per-row provenance:** for each conflicting row id, find the last commit on
  each side that touched that row, and compare those.

**Why it matters:** They give different answers and one of them loses money.
Tip-of-side means an unrelated later commit on machine B — B edited a *goal* —
wins a `transactions.csv` row conflict that B's tip never touched. Silent
overwrite, resolved "by rule", no conflict UI, logged and gone.

Per-row provenance is the correct semantics, but it means the merge needs commit
history per row, which **breaks the `(base, ours, theirs) → merged` signature and
the "no I/O" boundary you called deliberate.** §Component boundaries says `merge`
depends on "nothing". It cannot depend on nothing and also key off commit
metadata.

**Recommendation:** Resolve this before planning. My preference: keep `merge`
pure and change its signature to take provenance explicitly —
`merge(base, ours: Sided<Row>, theirs: Sided<Row>)` where `Sided` pairs each row
with `(committer_epoch, commit_oid)` resolved by the caller. The caller does the
git walk; the merge stays a total function over data and stays exhaustively
testable. Then write the test that distinguishes the two readings: B commits an
unrelated goal edit *after* A's conflicting transaction edit; assert A's row wins.

### Concern 2 (blocking): symmetry is necessary and not sufficient — you converge on values, then diverge on bytes

**What I see:** The stated invariants are `merge(A,B) == merge(B,A)` and
"`validate_all_balances` passes after any merge". §Testing's convergence test
asserts "identical trees".

**Why it matters:** Those two properties can both hold while the trees differ,
which is the re-merge-forever bug reviewers (b) already flagged — but the cause
runs deeper than row order.

1. `validate_all_balances` (`backend/domain/balance_service.rs:216`) compares with
   `0.001` epsilon and returns `Vec<String>`, not an error. Balances that differ
   in the last ULP pass it.
2. Balances are accumulated as `f64` (`running_balance += transaction.amount`) and
   written via `f64::to_string()` (`transaction_repository.rs:163`). Float addition
   is not associative. Two machines that apply the same row *set* in different
   orders can produce byte-different balance strings that validate clean.
3. `recalculate_balances_from_date` sorts `by(|a,b| a.date.cmp(&b.date))` with **no
   id tiebreak** (`balance_service.rs:57`). Rust's sort is stable, so same-timestamp
   rows get their balances assigned in *file read order*. Store appends
   (`store_transaction` pushes to the end), so file order is per-machine insertion
   order. Canonicalising the merge output is not enough — the *recompute* must be a
   pure function of the row set.
4. `parse_date_string` falls back to `Utc::now()` on an unparseable date
   (`transaction_repository.rs:129`) and to `chrono::Local` for date-only values.
   One malformed row makes read-modify-write non-idempotent: the file changes on
   every cycle, both machines re-merge forever, and the cloud drive churns. A
   date-only row parses differently in two timezones.

**Recommendation:** Replace the single symmetry property with a four-property
suite over **bytes**, and state the total order explicitly (`date`, then `id`) in
the spec:

- `merge(A,B) == merge(B,A)` — symmetry (keep it)
- `merge(A,A) == canonicalize(A)` — idempotence
- `merge(merge(A,B), B) == merge(A,B)` — **fixed point.** This is the one that
  proves the re-merge loop cannot happen.
- `write(read(x)) == x` for canonical `x`, over a corpus that includes the real
  production `transactions.csv` — round-trip stability.

Add a determinism test that runs the same merge under `TZ=UTC` and
`TZ=America/Los_Angeles` and asserts identical bytes. And kill the `Utc::now()`
fallback — a parse failure must be a hard error before it reaches a merge, not a
fresh timestamp.

### Concern 3: the money recompute is repository-coupled, so the pure boundary breaks where the risk lives

**What I see:** §Then recompute balances reuses `recalculate_balances_from_date`
and `validate_all_balances`. Both are `BalanceService` methods that read and write
through `TransactionRepository`, which reads and writes CSV files on disk, and
`update_transaction_balances` even does an O(children) scan to find each row's
owner (`transaction_repository.rs:385`).

**Why it matters:** Your stated reason for the pure merge — "it is where the
correctness risk lives, and it must be exhaustively testable without a filesystem"
— applies at least as much to the balance arithmetic. As specified, the property
"validate_all_balances passes after any merge" requires a `CsvConnection`, a temp
dir, and a registry per generated case. Under `proptest` that is 256 filesystem
round trips per property, per run, inside a crate that links wgpu.

**Recommendation:** Extract `fn recompute(rows: &[Transaction]) -> Vec<Transaction>`
and `fn validate(rows: &[Transaction]) -> Vec<BalanceError>` as pure functions, and
have the existing `BalanceService` methods become thin read/apply/write wrappers
over them. Then the merge pipeline is `merge → recompute → validate`, all pure,
all property-testable in microseconds. Reuse without I/O beats reuse with it.

### Concern 4: the two-machine test does not need two machines, and the manual checklist should shrink accordingly

**What I see:** §Testing says a manual acceptance checklist "covers what needs
real hardware: two Macs, a real Proton mount, a real login," and the `LgsClient`
gets "recorded `--json` fixtures".

**Why it matters:** More of this is automatable than you have credited, and lgs
already proves it. `lgs::paths` resolves everything from `$HOME`
(`local-git-sync/src/paths.rs:6`), and `local-git-sync/tests/common/mod.rs`'s
`spawn_test_daemon` explicitly supports "two daemons can share one cloud" against
a `TempDir` cloud root. A full A→bare→cloud-dir→bare→B loop runs offline, in CI,
with the real bundled binary, by spawning two daemons under two `HOME`s. That is
the difference between "we believe it converges" and "CI knows it converges".

Recorded fixtures also rot in the direction that hurts: the fixture keeps passing
after lgs changes `ProjectJson`, and the real binary breaks in the field. lgs's
own `tests/fixtures/README.md` names this exact defect — "a fixture someone
retyped by hand is a second representation of a contract".

**Recommendation:**
- Build a `TwoMachineHarness` fixture (`machine_a()`, `machine_b()`, `cloud()`,
  `sync_both_ways()`), one place, not per-test setup. Every convergence and
  onboarding case is then three lines.
- Keep the recorded fixture only for *parse* tests, and source it from lgs's
  blessed `current-status-json.json` rather than recording our own second copy.
  Pin the lgs commit the bundle is built from and fail CI when the vendored
  fixture drifts from that commit's blessed bytes.
- Move these off the manual checklist and into CI: `install-service` idempotence,
  `restore` refusal fall-through, archived-project 403, adoptable filtering.
  What's genuinely left for the manual list is Proton File Provider
  materialization, launchd-at-login, and real clock skew. That is a checklist
  someone will actually run.

### Concern 5: no injection seams for the new roots — the safety guard is untestable

**What I see:** The spec adds three new machine-scoped resolutions with no seam:
`~/Library/Application Support/Allowance Tracker/children/`, the bundled binary
path in `Contents/Resources/`, and the guard paths (`~/Documents` symlink check,
`~/Library/Mobile Documents/`, the lgs cloud root). `Backend::with_data_dir`
(`backend/mod.rs:81`) is the existing seam and it covers only the first root.

**Why it matters:** The guard in §Repo layout is the single most important safety
rule in this document — it is the thing standing between the user and the `.git`
corruption the spec exists to end. If it reads `dirs::home_dir()` internally, it
cannot be unit tested, so it will be verified once by hand and then silently
regress. Same for `Signature::now()` in `GitManager::commit`
(`backend/storage/git/mod.rs:111`): with the clock hardcoded, no test can
construct a committer-timestamp tie, and the tiebreak-by-hash branch of your
resolution rule ships uncovered.

**Recommendation:** One `SyncPaths { data_dir, children_root, lgs_binary,
cloud_root }` value threaded from startup, and make the guard
`fn is_cloud_synced(candidate: &Path, env: &SyncPaths) -> Option<Reason>` — a pure
predicate over injected paths, with a table test covering each rejection reason
plus the accept case. Give `GitManager` an injectable time source.

### Concern 6: `backend/` is `#[path]`-included into the GUI crate; proptest does not belong there

**What I see:** `egui-frontend/src/lib.rs:16` pulls `backend/mod.rs` into the
crate that depends on `eframe`, `wgpu`, `image`, `lettre`, and `reqwest`. Every
backend test compiles all of it.

Also, §Testing's "`proptest` must be added — the workspace has no dev-dependencies
today" is **factually wrong**: `egui-frontend/Cargo.toml:58` has
`tempfile = "3.0"`, and there is a real fixture harness at
`backend/storage/csv/test_utils.rs` (`TestHelper`, `TestEnvironment`) whose
`create_test_child_with_distinct_id` guard is genuinely good work. Don't start a
new island next to it.

**Recommendation:** Extract the merge module — ideally the whole of `backend/` —
into its own workspace crate with no GUI dependencies. That makes the "merge has
no I/O" boundary a *compile error* to violate rather than a paragraph in a spec,
and it makes a 256-case property run cheap. Correct the dev-dependency claim and
say explicitly that new fixtures extend `TestHelper`.

### Concern 7: the test table omits several claims the spec makes

- **The cloud-synced-path guard.** Not in the table at all. See Concern 5.
- **"No retry queue is needed."** That is a testable claim: kill the remote, do
  five writes, restore the remote, assert all five land and in order. Untested, it
  is an assertion.
- **AWS/git write serialization** (§Ordering constraints 2). Two writers on one
  CSV is the highest-consequence race here and it has no test. It needs a seam —
  a lock you can hold from a test — or it will be verified by hoping.
- **Crash mid-merge.** App dies after writing merged CSVs, before the merge commit.
  Next startup finds a dirty tree on a diverged branch. Spec doesn't say what
  happens; no test.
- **Schema skew between the two Macs.** `parse_transaction_type` falls through to
  *derivation* for an unrecognised type string (`transaction_repository.rs:97`), and
  the merge is a whole-file read-modify-write. An older app on machine B will
  silently downgrade a row type it doesn't understand and push it. Needs either a
  fidelity test (unknown values survive a merge byte-identical) or a version marker
  that refuses the merge.
- **Three machines.** §Goals says "two or more Macs". Every listed property is
  2-way. "Edit beats delete" is not obviously confluent across three peers with
  repeated merges. Either test 3-way convergence or narrow the goal to two.

### Concern 8: operational testability stops at reporting

**What I see:** §Health must be surfaced is good — relaying `daemon.message`
verbatim and separating `durability` from `failed_sync_attempts` is exactly right.
But there's no way for the user, or for you at a distance, to *verify* the loop
works on a given machine.

**Recommendation:** Add a "Check sync" action: write a sentinel to a scratch file
in the child repo, commit, push, fetch, read back, report each stage pass/fail
with the stage name. It's the manual checklist's happy path, automated, one click,
usable by a non-technical person over the phone. Also say where merge decisions are
logged and how long they're kept — "discarded versions are logged" is not an
observability plan.

## Questions the spec does not answer

- What does the app do when `validate_all_balances` returns errors after a merge?
  It returns `Vec<String>`, not a `Result`. Abort the merge? Commit anyway and
  warn? The test cannot be written until this is decided.
- Where does CI get the lgs source to bundle? §lgs integration rejects the Rust
  library partly to avoid "coupling this app's build to a sibling checkout" — but
  compiling the binary into `Contents/Resources/` couples the build to that
  checkout anyway. Submodule, vendored tarball, pinned git dep?
- What is the merge's failure mode when a CSV is malformed on one side? Merge with
  what it can parse, or refuse and surface?
- Does the convergence test assert identical *trees* (git hashes) or identical
  *parsed content*? Trees is the stronger and correct assertion — say so, because
  Concerns 2's float and timezone hazards only fail under the strong one.
- How does a test reach the "already registered, `lgs restore` refuses" branch
  without a second real machine? (With the harness in Concern 4 it's trivial; say
  so.)

## What I thought was well-handled

Naming `merge` as pure and I/O-free, and giving the reason. Identifying that the
sync logic doesn't care the remote is lgs, so tests point at a bare repo on disk —
that is the seam, and you found it without being told. Calling out that the
symmetry property is "exactly the class of bug that passes every example test and
fails in the field" is the right instinct; my Concern 2 is that you stopped one
property short, not that you were looking in the wrong place. Migration ordering
(copy-then-repoint, registry last, old folder never deleted) is testable by
construction and the "fails at each step in turn" test is the right test. And
carrying lgs's unproven File Provider assumption forward into your own checklist
rather than quietly inheriting it is honest work.

## Closing

Fix Concerns 1 and 2 before this goes to planning — the merge tests cannot be
written against the current text, and the invariant as stated will not catch the
bug it was chosen to catch. Concerns 3 through 6 are shape changes I would want in
the plan rather than blockers on the spec. The design is testable in the ways that
matter; it just isn't yet specified precisely enough to test.
