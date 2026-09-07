# [2b] Critique — Greg Grubberstone

**Spec reviewed:** `docs/superpowers/specs/2026-08-29-lgs-desktop-sync-design.md`
**Reviewer:** Greg Grubberstone, Senior Engineer
**Date:** 2026-09-06

---

## Overall Verdict

**Approve with changes.**

The shape is right. Pure merge module, three-way against the base, symmetric
resolution rule, "an unpushed commit *is* the retry queue" — that last one
deletes a subsystem instead of adding one, and I wish more specs did that.
My objections are all at the seam between this design and the code that
already exists. Four of them are load-bearing.

## Top Concerns

### Concern 1: The merge cannot run where the spec says it runs

**What I see:** §Ordering constraints — "Sync work runs off the UI thread and
lands through the existing `SyncMessage` channel." But the existing sync thread
touches **zero** repository files. It cannot: the architecture note in
`sync_manager.rs:37-40` says "UI owns all repo I/O," and the thread does
synchronous channel round-trips (`ReadEntityRequest`, `GetChildIdsRequest`,
5-second `recv_timeout`) precisely to avoid it. A git fetch/merge/checkout on
the working repo *is* repository I/O, on the same `transactions.csv` that
`write_transactions_internal` rewrites whole-file.

Constraint 1 says "no write may proceed while a merge is in flight." There is
no mechanism for that today, and the spec doesn't propose one. Constraint 2
says the AWS apply path and the git merge "serialize through the same path."
They don't — `ApplyRemoteEntity` runs on the UI thread
(`app_coordinator.rs:459`), a background checkout doesn't.

**Why it matters:** The obvious fix is a mutex or an "is a merge running"
state machine, and that is a new concurrency surface in an app that currently
has exactly one rule: the UI thread owns the bytes.

**Recommendation:** Keep the rule. Split by what each operation touches:

- Background thread: `fetch` and `push` only. Both touch `.git`
  (objects/refs) and never the working tree; libgit2 locks refs.
- Pure merge: anywhere, it's CPU.
- **All working-tree mutation** — write merged files, recompute, `commit` —
  happens on the UI thread, in the existing `handle_sync_messages` drain, via
  one new `SyncMessage::ApplyMerge { child_id, merged: MergedTree }`.

No mutex, no in-flight flag, no new invariant. Fewer moving parts than what's
proposed, not more.

### Concern 2: The transport is an unverified assumption, and `git2` is built without it

**What I see:** lgs's `clone_url` is `http://localhost:<port>/<name>.git`
(`local-git-sync/src/daemon/ipc.rs:219`). This app declares
`git2 = { version = "0.19", default-features = false }`
(`egui-frontend/Cargo.toml:48`) — no `https`, no `ssh`.

**Why it matters:** "The app has no command-line dependencies at all" is the
sentence the whole install story rests on, and it depends on a
default-features-off libgit2 speaking smart-HTTP to localhost. If it can't,
the answer is either turn on `https` (pull in Security.framework/openssl,
inflate the build) or shell out to `git` — which retracts the claim. Finding
that out during implementation is finding it out too late.

Second problem: that URL contains the daemon's port. The spec freezes it into
`.git/config` at migration time (`git remote add lgs`). Port changes, remote
is stale, push fails forever with a confusing error.

**Recommendation:** (a) One-hour spike before planning: `git2` clone/fetch/push
against a local lgs daemon with the current feature flags. Record the result in
the spec. (b) Re-resolve the remote URL from `lgs status --json` on startup and
reconcile `.git/config` rather than trusting what was written months ago.

### Concern 3: Onboarding step 4 double-clones, and names the remote differently

**What I see:** §Onboarding — "`lgs restore <name> <dir>`, clone via git2."
`lgs restore` already clones: `ensure_working_copy` (`cli.rs:700-733`) runs
`std::process::Command::new("git").arg("clone")` and **refuses a non-empty,
non-repo directory**. So the git2 clone that follows either hits a populated
directory or is dead code.

Two more consequences. `git clone` names the remote `origin`; the migration
path in this spec names it `lgs`. Migrated machines and restored machines will
disagree. And `lgs restore` shelling out to `git` means the app *does* have a
CLI dependency, transitively, on exactly the machine (no Xcode CLT) the
bundling argument was written for.

**Recommendation:** Let lgs's clone be authoritative. After `restore`, open with
git2 and verify/normalize the remote name — one function, `fn ensure_lgs_remote(repo:
&Repository, url: &str)`, used by both migration and onboarding so there is one
remote name in the system. Drop the second clone. Note the transitive `git`
dependency in the spec instead of claiming zero.

### Concern 4: Transaction IDs collide across machines by construction

**What I see:** `Transaction::generate_id` is
`format!("transaction::{}::{}", income|expense, epoch_millis)`
(`shared/src/lib.rs:581`). No device component. The merge keys rows by `id`.

**Why it matters:** Two Macs adding an expense in the same millisecond produce
the *same id* for *two different transactions*. The merge table's last row
(base absent, present on both sides) sends that to "resolve by rule," and
"later commit wins" **discards one of two real transactions**. Silently. Goal 2
of this spec is "no silent data loss."

Note this is a different situation from the row above it. Base-present
edit/edit is a genuine conflict — one row, two edits, pick one. Base-absent
add/add is two rows that collided on a key. The table treats them identically
and that is the bug.

**Recommendation:** Two changes, both small. (1) Split the table: base-absent
add/add with *identical* content → keep one; with *differing* content → keep
both, re-keying one deterministically (append the short commit hash of the side
being re-keyed, so both machines pick the same loser). (2) Fix the generator —
append a short random suffix and relax `parse_id` to `parts.len() >= 3`
(`shared/src/lib.rs:585`). Then the collision case becomes vanishingly rare
instead of merely unlikely, and the merge rule is a backstop rather than the
only defense.

### Concern 5: `recalculate_balances_from_date` is the wrong thing to reuse here

**What I see:** §Then recompute balances — "run `recalculate_balances_from_date`
… reuses them rather than reimplementing the arithmetic." Right instinct,
wrong target. That function goes through `update_transaction_balances`, which
calls `find_child_id_for_transaction` **per row**
(`transaction_repository.rs:384-392`), and each of those reads and parses
*every* child's entire CSV. A merge touching 500 rows is 500 × N full CSV
parses. On a 30-second timer.

It also fires a `SyncEvent` per changed row into the AWS notifier
(`balance_service.rs:87-97`). Each of those is a channel round-trip to the UI
thread with a 5-second timeout plus an HTTP PUT — on both machines, for the
same rows, after every merge.

**Recommendation:** The merge already has the rows in memory. Extract the
arithmetic, not the I/O:

```rust
/// Pure. No repository, no filesystem.
pub fn recompute_running_balances(txs: &mut [Transaction]);
```

Call it from the merge on the merged row set. Then refactor
`recalculate_balances_from_date` to call the same function. One implementation
of the arithmetic, which was the goal, and no quadratic I/O. Separately, build
the merge path's `BalanceService` with `.with_sync_notifier(None)` — the
builder already exists (`balance_service.rs:29`).

### Concern 6: f64 money will make the property test lie

**What I see:** `amount: f64`, `balance: f64`, compared with
`(a - b).abs() > 0.001` (`balance_service.rs:224`). The spec's headline
property is "`validate_all_balances` passes after any merge," under proptest.

**Why it matters:** A merge reorders how a running total accumulates. With
arbitrary `f64` inputs the property fails for reasons that have nothing to do
with the merge, someone widens the epsilon to make it green, and the
"silently wrong balances" failure this spec exists to prevent walks back in
through the test that was supposed to catch it.

**Recommendation:** Generate amounts as integer cents in the proptest
strategy, minimum. Better: a `Money(i64)` newtype at the merge boundary with
`From`/`TryFrom` conversions (Rust API Guidelines C-NEWTYPE, C-CONV). f64 for
money is the textbook anti-pattern and this feature is the first place it
actually bites.

Related, same section: `validate_all_balances` returns `Ok(Vec<String>)` — it
returns `Ok` when balances are **wrong**. A `?` at the call site swallows it
entirely. Give it `Result<(), Vec<BalanceMismatch>>` or a `#[must_use]`
report type before anything relies on it as an assertion.

### Concern 7: CSV codec will fork

**What I see:** The merge reads `(base, ours, theirs)` as **git blobs** — in
memory, no path. Today all CSV parsing lives inside the repositories bound to
file paths (`write_transactions_internal` writes to a `File`; reads go through
`connection`).

**Why it matters:** Whoever implements this writes a second parser for blobs.
Two parsers over one format drift. And the writer serializes amounts with
`f64::to_string()` (`transaction_repository.rs:161`) — if the merge's writer
isn't byte-identical, every merge produces a spurious diff and the repos never
converge, which fails the convergence test in a way that looks like a merge bug.

**Recommendation:** Extract `parse_transactions(&str) -> Result<Vec<Transaction>>`
and `render_transactions(&[Transaction]) -> String` as free functions; have the
repository call them. Add a byte-stability round-trip test:
`render(parse(s)) == s`.

## Smaller notes

- **Don't put a trait behind `LgsClient`.** One implementation, one mock, no
  second caller — that's the trait-with-one-impl anti-pattern. The seam you
  actually want is `fn parse_status(json: &str) -> Result<StatusReport>`, pure,
  and a thin `fn run(args: &[&str]) -> Result<String>` for the spawn. The
  fixture tests hit the parser directly and never spawn anything.
- **Health must be an enum with a catch-all.** lgs's own tests feed
  `"a_variant_from_the_future"` (`cli.rs:1788`). A `#[serde(other)] Unknown`
  variant is required, not optional. Don't model it as `String`.
- **`GitManager` growing five methods** means ~6 `Repository::open` calls per
  merge cycle. Hold one open `Repository` for the cycle, or take `&Repository`
  in the new methods. Also, `P: AsRef<Path>` on every method with one caller
  each buys nothing — `&Path` is fine.
- **Reuse the migration shape you already have.** `plan_migration` /
  `MigrationReport` / `SkippedFolder` (`csv/migration.rs:44-93`) is the same
  plan-report-run discipline this needs. And `tree_checksum`
  (`csv/checksum.rs:17`) is already the exact oracle for "assert identical
  trees" in the convergence test. Free.
- **The cloud-path guard should be one typed function**, not three scattered
  `if`s: `fn reject_if_cloud_synced(&Path) -> Result<(), CloudPathRejected>`,
  with the error carrying *which* rule fired. Table-driven test.

## Questions the spec does not answer

- Which thread owns the working tree during a merge? (Concern 1.)
- Does the merge commit's tree include the recomputed balances, or does
  recompute land as a second commit after the two-parent merge?
- What happens when `push` is rejected because the remote moved between fetch
  and push? Is the fetch→merge→push cycle bounded, or can it spin?
- Do merge-driven row changes re-enter the AWS notifier, and is that intended?
- Is `Goal`'s id generated the same timestamp way? Same collision question.

## What I thought was well-handled

The symmetry argument for "later committer timestamp wins" over "prefer ours"
is correct and the reasoning is stated correctly — permanent divergence is
exactly what "prefer ours" produces, and most people don't see it until
production. The empty-base union is right for the same class of reason.

"An unpushed commit *is* the queue" is the best line in the spec. It removes
`sync_retry_queue.yaml` from this path rather than cloning it.

Testing against a plain bare repo on disk instead of a daemon is the right
seam, correctly identified, and it's what makes the convergence test possible
at all.

## Closing

The architecture is sound and the hard part — the merge semantics — is
reasoned about better than most specs manage. Fix the thread-ownership story
(Concern 1), spike the `git2`-over-http assumption (Concern 2), split the
add/add case from edit/edit (Concern 4), and extract the pure balance
arithmetic (Concern 5) before planning. The rest are cleanups that can ride
along with the tasks.
