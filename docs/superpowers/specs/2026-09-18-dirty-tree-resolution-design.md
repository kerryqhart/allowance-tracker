# Dirty-tree resolution: closing two data-loss paths in child sync

**Date:** 2026-09-18 (revised 2026-09-19 after panel review)
**Status:** Reviewed — ready for implementation planning
**Supersedes nothing.** Extends `2026-08-29-lgs-desktop-sync-design.md`, whose
dirty-tree handling this corrects.

## Problem

Two defects found after the lgs desktop sync branch landed. They look
unrelated — one is a stall, one is a deletion — but they share a root cause.

### Defect 1: a dirty tree with no owned file changed stalls sync silently

`commit_dirty_tree_before_merge` (`egui-frontend/src/ui/app_coordinator.rs:1268`)
stages only `FILES_THIS_APP_OWNS` and calls `commit_if_changed`. When that
returns `Ok(None)` — the tree is dirty, but staging produced no tree change —
it logs a warning, returns `ApplyMergeOutcome::DirtyTreeCommitted`, and
requests a re-poll.

Nothing was committed, so the return value is false. The next cycle re-derives
the same divergence, finds the same dirty tree, and refuses again. After
`STALE_HEAD_REFUSAL_LIMIT` consecutive refusals the re-poll is suppressed as
well, so the child stops syncing with nothing visible in the UI.

The sibling fast-forward path, `commit_dirty_tree_to_unblock_fast_forward`
(`:1650`), handles the identical condition correctly: `SyncFailureNotice`,
`SyncStatus::Error`, `Failed`. The two paths disagree, and the merge path is
the wrong one.

Three reachable routes:

- **`parental_control_attempts.csv`.** Tracked in git, but excluded from
  `FILES_THIS_APP_OWNS` (`backend/sync/paths.rs:19-25`) on the reasoning that
  `ParentalControlRepository` commits it itself. But `commit_file_change`
  (`backend/storage/git/mod.rs:381-390`) swallows commit failures with a `warn!`
  and returns `Ok`. One swallowed failure leaves that file dirty indefinitely,
  and every subsequent merge for that child stalls.

- **A deleted owned file.** Both staging loops skip a file that does not exist
  (`if !child_dir.join(name).exists() { continue; }`), so a deletion is never
  staged. `index.add_path` cannot stage a deletion in any case; that needs
  `index.remove_path`. `git status` reports dirty, staging yields nothing,
  `Ok(None)`, stall.

- **`delete_allowance_config`.** (`allowance_repository.rs:189-190`) calls
  `std::fs::remove_file` on `allowance_config.yaml` — an owned, tracked file —
  with no commit at all. User-triggerable, no crash and no swallowed error
  required. From that moment the tree is permanently dirty and every merge
  stalls. Found in review; this is the route that needs no misfortune to reach.

### Defect 2: `recover_if_dirty` destroys uncommitted rows

`recover_if_dirty` (`backend/sync/child_sync.rs:908`) hard-resets the working
tree when `MERGE_IN_PROGRESS_MARKER` is present. The marker proves a merge
*began and did not finish*. It does not prove the tree's current contents came
from that merge.

After a crash mid-merge the app restarts, and the AWS transport's
non-committing path (`upsert_transaction_no_commit`,
`backend/storage/csv/transaction_repository.rs:225`) can write fresh
MCP-authored rows into that same `transactions.csv` before the next
`apply_merge` runs. `recover_if_dirty` then resets to HEAD and takes those rows
with it. The AWS watermark has already advanced past them, so they cannot be
re-fetched: permanent, silent loss.

This is the same hazard class as the CRITICAL-1 bug the merge path already
fixed. The marker narrowed the window rather than closing it.

The reset also runs *before* the unconditional dirty-tree guard in both call
sites (`:970` and `:1456`), so it pre-empts the code that would have protected
those rows. The existing log message concedes exactly this (`:983-988`):

> "This says nothing about protecting any MCP-authored row that might
> independently be sitting in this dirty tree: that protection is the
> dirty-tree guard below (unconditional, not marker-gated), not this recovery
> step."

### Root cause

Both defects decide what a dirty tree means by **inference** rather than
evidence. Defect 1 infers "nothing owned changed ⇒ nothing to do" when the
real cause is a staging bug and an ownership list that was never the right
question. Defect 2 infers "marker present ⇒ these contents are that merge's"
from a marker that carries no such information.

Underneath both: `transactions.csv` is written with a plain `std::fs::write`
(`transaction_repository.rs:188`, `app_coordinator.rs:1168`), so a torn or
truncated file is possible. Because `delete_transaction_no_commit` legitimately
removes rows without committing, a file with fewer rows than HEAD is ambiguous
— an MCP deletion, or a truncation. That ambiguity is unanswerable, and it is
why the original author reached for a hard reset.

### Why the existing test suite did not catch either defect

Named here because the test design in this spec follows from it.

- **Defect 1 is a liveness bug, and every merge-path test is single-shot.** A
  call that returns `DirtyTreeCommitted` having committed nothing is not visibly
  wrong in isolation — only on the second, third and `STALE_HEAD_REFUSAL_LIMIT`th
  cycle. The fast-forward path has the suite's one progress assertion
  (`a_dirty_tree_blocking_a_fast_forward_resolves_instead_of_refusing_forever`,
  `app_coordinator.rs:3178`), and it is not a coincidence that the fast-forward
  path is the one that got this right.
- **Defect 2 was asserted as intended behaviour.**
  `a_dirty_tree_from_a_prior_crash_is_recovered_before_applying_the_next_merge`
  (`app_coordinator.rs:2331`) asserts "the crash's garbage must be gone", with a
  doc comment explaining why that was correct.
- **No test converges two app instances through the real code.** The two-machine
  harness commits `ledger.txt` through raw `GitManager` calls; it never drives
  `ChildSyncEngine` or `apply_merge`. And it never runs in CI (see "CI reality"
  below).

## Design

Five changes, in dependency order.

### 1. Atomic writes

New module `backend/storage/atomic.rs`, built on `tempfile` (promoted from
`[dev-dependencies]`):

```rust
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()>
```

`NamedTempFile::new_in(parent)` → `write_all` → set mode → `sync_all()` →
`persist(path)`. The signature mirrors `std::fs::write` so every conversion is a
mechanical identifier swap. Named `atomic::write`, not `atomic::write_atomic`,
which stutters.

`NamedTempFile` supplies collision-safe naming and cleanup-on-drop, which is why
this is not hand-rolled: "removed on every failure path" is otherwise a promise a
human keeps by hand at every `?`.

**Mode must be set explicitly before persist.** `NamedTempFile` creates at
`0600`, so persisting would *tighten* an existing `0644` file rather than merely
fail to preserve it. Preserve the existing file's mode where there is one;
default to `0644`.

**What this guarantees, precisely:**

- **Atomicity — no reader ever observes a partial file.** From `rename(2)` alone.
  A reader gets the complete old file or the complete new one. This survives
  process death, since the page cache outlives the process. This is the property
  both defects need.
- **Durability — not claimed beyond this:** a power loss or kernel panic may cost
  the *most recent write*, never the file's integrity. `File::sync_all()` is
  `fsync(2)`, which on macOS does not flush the drive's own volatile cache; only
  `fcntl(F_FULLFSYNC)` does.

`F_FULLFSYNC` is **deliberately not used.** It costs tens of milliseconds on
every transaction write, and buys survival of the last write — a property neither
defect concerns, since both are about corrupting or discarding rows that were
already durable. `sync_all()` before rename is kept, but not for APFS, whose
copy-on-write checkpoints already order data before metadata: it is for the odd
volume — an external disk, a network mount, HFS+ — where that ordering is not
guaranteed. The **directory fsync is dropped**; it buys a guarantee macOS does not
honour.

This consolidates as much as it adds. The pattern is already hand-rolled in six
places — `allowance_repository.rs:99`, `child_repository.rs:144`,
`global_config_repository.rs:145`, `child_registry.rs:86`,
`sync_persistence.rs:61` and `:96`, `migration.rs:433` — each a bare `write` +
`rename`. All move to the shared helper. The justification is cleanup-on-drop,
collision-safe naming, and one definition instead of six. (An earlier draft
justified it as "no fsync ⇒ zero-length file on power loss"; that is an ext4
delayed-allocation failure mode, not an APFS one, and the reasoning is withdrawn.)

Newly protected, unprotected today:

| Site | File | Today |
|---|---|---|
| `transaction_repository.rs:188` | `transactions.csv` | `fs::write` |
| `app_coordinator.rs:1168` | `transactions.csv` (merged) | `fs::write` |
| `goal_repository.rs:128` | `goals.csv` | `File::create`, in place |

For `goal_repository.rs:128`, render to a `Vec<u8>` via
`csv::Writer::from_writer(Vec::new())` then `into_inner()`, and call the same
function. No closure-taking variant for one caller.

**`parental_control_attempts.csv` stays an append** and is *not* converted. It is
written with `OpenOptions::new().create(true).append(true)`
(`parental_control_repository.rs:131-137`). An interrupted append damages at most
the trailing line; an interrupted rewrite can lose the whole audit log.

The original draft justified this by claiming the reader already tolerates a
malformed trailing record. **That was false.** Both readers use a default
non-`flexible` `csv::Reader` with `let record = result?;`
(`parental_control_repository.rs:186-187`, and `get_next_id` at `:112-121`), so a
truncated append fails the entire read *and* permanently blocks future appends —
the log becomes both unreadable and unwritable. The append is kept and the claim
is made true instead: both readers gain `.flexible(true)` and skip malformed
records, with a test that a file with a truncated trailing line still reads every
prior record and still yields a correct next id.

### 2. The guard stages tracked paths, not an allowlist

`FILES_THIS_APP_OWNS` exists to stop `add_all(["*"])` sweeping *untracked*
strays — `.DS_Store`, editor swap files — into a commit that gets pushed into a
child's history permanently (`paths.rs:9-17`). **A file already tracked in HEAD
is already in that pushed history**, so committing its modification is not the
hazard the allowlist guards against. The danger was always the `*`, never
`add_all`.

So the dirty-tree guard stages tracked paths:

```rust
index.update_all(["*"], None)?;   // modifications and deletions of tracked entries only
index.write()?;
```

`update_all` never adds an untracked path, so the anti-`add_all` invariant is
preserved exactly. This replaces the three-branch loop the earlier draft
proposed, along with its `exists()` stat, HEAD peel and tree lookup.

**`FILES_THIS_APP_OWNS` is retained** for `commit_merge` and migration, where the
narrow list is still right. `stage_owned_files` (`git/mod.rs:50`) remains their
single implementation and loses its `repo_path` parameter — `repo.workdir()`
supplies it, and two parameters that must agree is an unenforced invariant. The
two hand-rolled copies of that loop in `app_coordinator.rs` (`:1284`, `:1677`)
are deleted, retiring the "change it in both places" instruction at `:1264`.

`paths.rs`'s doc comment is rewritten around the resulting two-tier model:

> The narrow list governs what `commit_merge` and migration stage. The
> dirty-tree guard is deliberately *not* governed by it — it stages every
> tracked path, because a tracked file is already in pushed history.

**Consequence: `parental_control_attempts.csv` does not join the list.** An
earlier draft folded it in to close defect 1's first route; the guard now reaches
it as a tracked path, so the fold-in is unnecessary and the list keeps its
original membership and intent.

**Consequence: the "nothing to commit" branch becomes unreachable.**
`working_tree_dirty` already sets `include_untracked(false)`
(`child_sync.rs:1019`), so any tree that enters the guard has a tracked change to
stage. The variant survives as a should-never-happen and is worded to the user as
*"something unexpected happened"*, not as a routine operating condition.

**Consequence: a leftover temp file is inert.** If the process dies mid-write,
`.tmp` residue is untracked — so it cannot make the tree dirty, cannot trigger the
guard, and cannot be staged. No startup sweep is needed.

### 3. Parse-validate before committing

The guard must not commit bytes it has not validated. Atomic writes bound *this
app's* corruption after the upgrade; they say nothing about a `transactions.csv`
already torn on disk at upgrade time, damaged by Proton or a partial restore, or
written by the old in-place `goal_repository.rs:128` path.

This matters more than it appears. With the hard reset gone, nothing else
validates, so the guard would commit corrupt bytes and `push_with_retry` would
propagate them to the peer — `read_rows` (`child_sync.rs:978-990`) then fails to
parse HEAD's `transactions.csv` on *both* machines and `apply_merge` can compute
nothing. A one-machine corruption becomes a two-machine outage: strictly worse
than the defect being fixed.

So before staging, parse `transactions.csv` with
`allowance_core::codec::parse_transactions` — the function `read_rows` already
calls. If it does not parse, commit nothing and raise the durable failure notice.

### 4. One dirty-tree resolution, returning a `Result`

`commit_dirty_tree_before_merge` and `commit_dirty_tree_to_unblock_fast_forward`
converge on:

```rust
fn resolve_dirty_tree(repo: &Repository, message: &str)
    -> Result<git2::Oid, DirtyTreeError>

#[derive(Debug, thiserror::Error)]
enum DirtyTreeError {
    #[error("could not stage local changes")]
    Stage(#[source] git2::Error),
    #[error("could not commit local changes")]
    Commit(#[source] anyhow::Error),
    #[error("{file} could not be read as transaction data")]
    Unparseable { file: &'static str },
    #[error("nothing tracked was changed")]   // should never happen — see §2
    NothingToCommit,
}
```

A `Result`, not an enum wearing one: the earlier draft's three variants were
mapped identically by both callers, and its `Failed(String)` flattened the
`git2::Error` cause chain into a display string exactly where the cause is needed.
`Committed(String)` becomes `git2::Oid`, which is `Copy` and already the right
type.

**The variants carry structure, not prose.** `Display` exists for logs and the
error chain, where a developer is the reader; the *user-facing* sentence is
composed by the UI. Prose produced deep in merge code is the mechanism by which
"stage", "dirty tree" and "oid" reached the screen in the first place, and every
later improvement to the wording would otherwise be an edit to
`app_coordinator.rs`.

Both callers collapse to one shape, with the success branch the only place they
differ (the fast-forward path records `FastForwardBlockedNotice`; the merge path
does not):

```rust
match resolve_dirty_tree(&repo, &message) {
    Ok(oid) => { /* path-specific */ }
    Err(e)  => { self.fail_sync(child_id, &e); Outcome::Failed }
}
```

`fn fail_sync(&mut self, child_id: &str, err: &DirtyTreeError)` absorbs the eight
copies of "format a message, set `sync.status`, call `record_sync_failure`, return
`Failed`" currently spread across the two functions.

The merge path stops returning `DirtyTreeCommitted` when it committed nothing.
That false return is what made the stall silent.

### 5. `recover_if_dirty` loses its reset

```rust
pub fn clear_interrupted_merge_marker(repo: &Repository) -> Result<bool>
```

Clears a stale marker if present; returns whether there was one. Callers log "a
previous merge for this child was interrupted" and continue. No
`git2::ResetType::Hard` remains anywhere in the sync paths. `Recovered` and
`Recovered::DiscardedAndReMerged` are deleted. `write_merge_marker`,
`clear_merge_marker` and `MERGE_IN_PROGRESS_MARKER` stay, with doc comments
rewritten to state that the marker is a diagnostic breadcrumb, not an
authorization to discard.

Content handling for an interrupted merge is then the same code path that handles
an ordinary MCP write, which is what stops defect 2 from being a special case
rather than merely patching it.

**Convergence after an interrupted merge.** The guard commits the partially
merged content as an ordinary *single-parent* commit, so `theirs` never becomes
an ancestor through that commit. This converges, and the argument is worth
stating rather than implying: `theirs` is still in the object database, the next
cycle re-classifies against it and recomputes the merge, and
`allowance_core::merge` de-duplicates rows that are `intrinsic_eq` on both sides,
so rows already committed are not doubled. The result reaches the same row set by
a different commit topology. This is the price of "same code path as an ordinary
MCP write."

Retiring the marker entirely remains a **non-goal**: worth doing only once atomic
writes have real runtime behind them.

## What the user sees

The backstop is the product's first "I have stopped, you must fix something"
state, and it needs more than an error-table row.

**Where it appears.** `SyncStatus` is written in roughly thirty places in
`app_coordinator.rs` and **read by no UI component at all** — it is a write-only
field today, and this spec records that so no future author believes the status
write is doing work it is not. `SyncFailureNotice`'s only reader is
`render_sync_notices` (`lgs_sync_modal.rs:614`), inside a 120px scroll area,
inside Settings → "Sync with another Mac…".

That is not good enough for a blocking state: a parent recording Tuesday's
allowance on the kitchen Mac, while the study Mac stopped syncing eleven days
ago, would see nothing. So a **badge appears near the child picker** whenever
`sync_failures` is non-empty, and opens the sync modal. The child picker is the
surface where the affected child may be the *selected* one, showing a balance the
other Mac does not share — the moment the user is most likely to be misled.

**Severity.** `FastForwardBlockedNotice` means "handled, nothing for you to do";
the backstop means "this child is not syncing until you act". They currently
render identically. Notices gain a severity and sort blocking-first.

**Wording.** The notice names the child by `label` and the file by its human path,
both already on `RegistryEntry` (`child_registry.rs:22`), plus a **"Show the
folder"** button built on `path`. Without it the remediation instruction is "open
Terminal and run git", which is not an instruction this product can give. A
parent knows *Amélie*; they do not know `child_7f3a…`, an oid, or what "stage"
means.

**Blast radius, stated in the notice.** The app keeps working, transactions save,
nothing is lost — this Mac and the other stop agreeing until it is resolved, and
the other Mac shows no error because nothing is wrong there. **Recovery is
automatic** on the next cycle once the file is resolved (`clear_sync_failure`
already runs on `Applied`, `app_coordinator.rs:1249`, `:1617`), so the notice says
so rather than leaving the user hunting for a retry button that does not exist.

## What a cycle looks like afterwards

```
open repo
  → clear + log any stale interrupted-merge marker
  → stale-HEAD check
  → if working tree dirty:
        parse-validate transactions.csv        (§3)
        index.update_all(["*"])                (§2 — tracked paths only)
        tree differs from HEAD ? commit it : should-never-happen notice
        discard this merge computation, re-poll (debounced)
  → goals-divergence notice
  → write marker
  → atomic write of merged CSV                 (§1)
  → merge commit
  → clear marker
  → push
```

The dirty-tree guard becomes the only code that touches uncommitted content, and
it only ever commits content it has validated.

## Error handling

| Condition | Result |
|---|---|
| Staging failure | `fail_sync` → `SyncFailureNotice` + `SyncStatus::Error` + `Failed` |
| Commit failure | `fail_sync` → same |
| `transactions.csv` unparseable | `fail_sync` → same; nothing committed, file untouched |
| `NothingToCommit` | `fail_sync` → same, worded "something unexpected happened" |
| Atomic write failure | Previous file intact; operation fails; never a partial file |
| Marker clear failure | Logged, non-fatal — the marker is diagnostic only |

`SyncFailureNotice` rather than status alone, because a status write is erased by
the next unrelated sync event; a stall that needs a human must survive that.

**`commit_file_change`'s swallowed error is deliberately left in place.** An
earlier draft proposed propagating it. Under §2 that is unnecessary: a file left
dirty by a swallowed commit is staged and committed by the guard on the next
cycle, so the widening is the whole fix — and with the hard reset gone, an
uncommitted row is no longer at risk from anything. Claiming two fixes where one
does the work is the "looks-fixed" pattern this spec exists to avoid. What is
kept is hygiene: `#[must_use]` on the return, and the five `let _ =` sites
(`transaction_repository.rs:201`, `allowance_repository.rs:106`,
`goal_repository.rs:100`, `child_repository.rs:107`,
`parental_control_repository.rs:163`) replaced with an explicit logged disposition
— a failed *commit* does not fail the user's *write*, because the data is already
on disk.

Known inconsistency, recorded rather than fixed: `commit_file_change` gates on
`has_uncommitted_changes`, which uses `statuses(None)` and therefore counts
untracked files, while its staging is a narrow allowlist.

## Testing

The plan is a set of capabilities, not a list of examples — because both defects
walked past a suite with 5,000-case property tests and eight git integration
tests.

### Infrastructure first

- **`ChildRepoFixture`** — `.with_peer_commit(files)`, `.with_dirty(file,
  content)`, `.with_deleted(file)`, `.with_marker()`. A prerequisite, not a
  cleanup: `app_with_git_backed_child` already exists three times
  (`app_coordinator.rs:2233`, `:2846`, `:3274`) and `commit_with_files` three
  times (`:2210`, `:2827`, `child_sync.rs:1130`), and the cross-product table
  below cannot be written on top of a copy-pasted setup function.

- **`run_cycles_until_terminal(&mut app, child_id, max)`** — drives the real
  classify/apply loop against a fixed environment and asserts it reaches a
  terminal state within `max`, returning the outcome sequence on failure so the
  message reads "the same outcome `DirtyTreeCommitted` 5 times with HEAD unmoved"
  rather than "assertion failed: false". **This is the test that catches defect 1
  without knowing defect 1 exists**, because defect 1 is a liveness failure and
  every existing merge-path test is single-shot.

- **`assert_resolved_or_explained(&app, &repo, child_id)`** — the invariant:
  *the tree is clean **and HEAD advanced**, or a `SyncFailureNotice` for this
  child exists.* Never neither. Called at the end of every dirty-tree test, not
  only the one that owns the invariant. The "HEAD advanced" half matters: the
  weaker "tree clean or notice exists" holds vacuously if the guard commits
  something unrelated and leaves the real problem for the next cycle.

### Coverage

- **Cross-product table**, deterministic rather than generated: the owned files
  plus a representative tracked-unowned file, each in {unchanged, modified,
  deleted, created}. The space is finite and enumerable, and a shrinker would
  hand back a misleading minimal case.
- **Three stall routes**, each through `run_cycles_until_terminal`: deleted
  tracked file; dirty `parental_control_attempts.csv`; `delete_allowance_config`.
- **Tracked non-owned file** modified in place — with
  `assert!(working_tree_dirty(&repo)?)` as a precondition so a fixture that stops
  reproducing the condition fails loudly. (The earlier draft cited
  `app_coordinator.rs:3140`, which writes an *untracked* `notes.txt` and reaches
  its failure path through a checkout conflict that exists only on the
  fast-forward path. Since `working_tree_dirty` sets `include_untracked(false)`,
  copying it would have produced a test that passes for the wrong reason.)
- **Dirty and unparseable** ⇒ failure notice, no commit, file untouched.
- **Crash plus MCP row** — marker present *and* an uncommitted MCP-authored row:
  the row survives. Defect 2's regression test.
- **Truncated `parental_control_attempts.csv`** still reads every prior record and
  still yields a correct next id.
- **Ownership-completeness contract test** — exercise every repository write path
  against a fresh child directory, then assert every tracked file present is in
  `FILES_THIS_APP_OWNS` or on an explicit, comment-justified exemption list. This
  closes the *class*: `.allowance_redirect` (`migration.rs:250`) already lives in
  a child directory, and a future `budgets.csv` would walk into the same place.
  It makes the rewritten doc comment enforced rather than aspirational.

### Atomic-write tests

The three obvious assertions — content correct, no temp residue, prior file
intact on failure — are worth having and **none of them test atomicity**: a
`write` with the `sync_all` deleted passes all three forever. So:

- **Concurrent reader**, with a negative control in the same file: a reader
  parsing `transactions.csv` in a tight loop while a writer performs N
  `atomic::write` calls with distinct contents; every observed read is byte-equal
  to one of the known versions, never a prefix, never empty. The same loop against
  plain `fs::write` must be *shown* to produce a short read — a test that cannot
  fail on a broken implementation is not evidence.
- **Syscall order**, via a `#[cfg(test)]`-injectable sink asserting
  write → `sync_all` → rename. For a durability primitive the call sequence *is*
  the contract; it must not rest on a code comment.
- **Failure induced deterministically** by `chmod 0555` on the parent directory,
  which fails `NamedTempFile::new_in` for a non-root user on macOS while leaving
  the existing file intact. Named here so the test does not get written as
  `#[ignore]` and never run.
- **Mode preservation**: persisting over an existing `0644` file leaves it `0644`,
  not `0600`.
- **Power-loss behaviour is not verifiable in CI.** Stated plainly, and added to
  `docs/lgs-sync-acceptance-checklist.md`.

### Existing tests that must be inverted

The earlier draft claimed the suites run unchanged. Untrue:

| Test | Change |
|---|---|
| `child_sync.rs:1457` `a_dirty_tree_with_the_marker_present_is_discarded_and_the_marker_cleared` | Inverted: the dirty state is **preserved**, the marker cleared |
| `app_coordinator.rs:2331` `a_dirty_tree_from_a_prior_crash_is_recovered_before_applying_the_next_merge` | Rewritten: asserts "the crash's garbage must be gone" — the defect, asserted as intended behaviour |

### CI reality

`.github/workflows/ci.yml` runs `cargo check --workspace` and `cargo test
--workspace` — no `--ignored`, no `LGS_BINARY`. `two_machine_sync.rs:103` and
`codec_real_data.rs:18` are both `#[ignore]`d, so **neither has ever run in CI**,
and `docs/lgs-sync-acceptance-checklist.md`'s "What CI already covers" entry
wrongly tells a reader not to hand-test the two-machine harness. That entry is
corrected as part of this work.

Adding a CI job that installs `lgs` and runs `--ignored` is deliberately **not**
in scope — see follow-ups.

## Scope boundaries

Out of scope, and unchanged by this work:

- `goals.csv` semantic merge (a known, recorded gap in the lgs design).
- `.git/index.lock` and index-corruption recovery.
- The AWS transport's non-committing write strategy (Task 16's decision stands).

### Named follow-ups

Deliberately deferred, recorded so they are visible rather than dropped:

1. **Retire `MERGE_IN_PROGRESS_MARKER` entirely**, once atomic writes have real
   runtime behind them.
2. **A second CI job** installing `lgs` and running `cargo test --workspace --
   --ignored`, even if non-blocking: "an ignored test that never runs is
   documentation, not coverage." Deferred because it is a green-CI question
   rather than a data-loss one, and likely to surface pre-existing failures.
3. **Collapse the three notice `Vec`s** (`sync_failures`, `goals_diverged`,
   `fast_forward_blocked`) into one `SyncNotice` with severity. Severity is added
   now; the structural collapse is a UI change beyond a data-loss fix. Until then
   three `record_*` helpers survive and the next condition still has to pick a
   `Vec`.
4. **A "working tree clean or explained" stage in the "Check sync" flow**, so the
   backstop state has an operational probe and not only a UI notice.
5. **A merge rule for `parental_control_attempts.csv`.** A peer's appends are
   dropped on merge the same way `goals.csv`'s are, except `goals.csv` at least
   raises `GoalsDivergedNotice`. Pre-existing, and a merge-semantics gap rather
   than a dirty-tree one.
6. **Two app instances converging real financial data through the real code.**
   The two-machine harness drives `GitManager` directly and never reaches
   `ChildSyncEngine` or `apply_merge`. This is the structural reason both defects
   shipped.

## Files touched

| File | Change |
|---|---|
| `backend/storage/atomic.rs` | New — `atomic::write` on `tempfile` |
| `backend/storage/mod.rs` | Register the module (`:41-46`) |
| `egui-frontend/Cargo.toml` | `tempfile` dev-dependency → dependency |
| `backend/storage/csv/transaction_repository.rs` | Atomic ledger write; `let _` disposition |
| `backend/storage/csv/goal_repository.rs` | Atomic `goals.csv` write; `let _` disposition |
| `backend/storage/csv/allowance_repository.rs` | Shared helper; `let _` disposition |
| `backend/storage/csv/child_repository.rs` | Shared helper; `let _` disposition |
| `backend/storage/csv/global_config_repository.rs` | Shared helper |
| `backend/storage/csv/child_registry.rs` | Shared helper |
| `backend/storage/csv/migration.rs` | Shared helper |
| `backend/storage/csv/parental_control_repository.rs` | `.flexible(true)` readers; `let _` disposition |
| `backend/domain/sync_persistence.rs` | Shared helper |
| `backend/storage/git/mod.rs` | `stage_owned_files` loses `repo_path`; `#[must_use]`; delete six dead `*_sync` forwarders (`:410-446`, zero callers) |
| `backend/sync/paths.rs` | Rewrite doc comment for the two-tier model |
| `backend/sync/child_sync.rs` | `recover_if_dirty` → `clear_interrupted_merge_marker` |
| `egui-frontend/src/ui/app_coordinator.rs` | `resolve_dirty_tree` + `fail_sync`; `update_all`; parse gate; atomic merged write |
| `egui-frontend/src/ui/state/sync_state.rs` | Notice severity; blocking-first ordering |
| `egui-frontend/src/ui/components/settings/lgs_sync_modal.rs` | Composed wording; "Show the folder" |
| child picker component | Blocking-notice badge |
| `docs/lgs-sync-acceptance-checklist.md` | Correct "What CI already covers"; add power-loss item |

## Review Change Appendix

Changes prompted by the reviewer panel (see
`reviews/2026-09-18-dirty-tree-resolution-design/` for full memos):

- **Guard stages tracked paths instead of the owned allowlist** (prompted by
  Deiko): accepted, and it restructured the design — the stall class is now
  unreachable rather than merely reported. Reverses the earlier decision to fold
  `parental_control_attempts.csv` into `FILES_THIS_APP_OWNS`, which is no longer
  needed.
- **Parse-validate `transactions.csv` before committing** (prompted by Deiko,
  Greg and Ted independently): accepted. Removing the hard reset would otherwise
  have let corruption propagate to the peer — a one-machine fault becoming a
  two-machine outage.
- **`commit_file_change` propagation dropped from scope** (prompted by Deiko,
  Greg and Ted independently): accepted. All five callers are `let _ =`, so the
  change was inert; under the widening it is also unnecessary. `#[must_use]` and
  explicit dispositions retained.
- **`parental_control_attempts.csv` readers become `.flexible(true)`** (prompted
  by Deiko): accepted. The spec's "bounded corruption" premise was false — both
  readers use `result?` on a non-flexible reader, so one torn line made the log
  unreadable *and* unwritable.
- **Third stall route documented and tested** (prompted by Deiko):
  `delete_allowance_config` removes a tracked owned file with no commit —
  user-triggerable, no misfortune required.
- **`DirtyTreeResolution` → `Result<git2::Oid, DirtyTreeError>`; `fail_sync`
  helper** (prompted by Greg): accepted; collapses eight copies of the same
  failure block.
- **Variants carry structure, not prose** (prompted by Pierre): accepted, and
  amends Greg's proposal that `Display` be the user-facing message.
- **`tempfile::NamedTempFile`; `atomic::write` naming; `fs::write` signature**
  (prompted by Greg): accepted. Mode must be set before persist, since
  `NamedTempFile` creates at `0600` (prompted by Deiko's permissions question).
- **Directory fsync dropped; `F_FULLFSYNC` rejected; durability claim scoped**
  (prompted by Greg and Deiko): accepted. The "zero-length file on power loss"
  justification was an ext4 failure mode imported into an APFS context, and is
  withdrawn.
- **Six dead `*_sync` forwarders deleted** (prompted by Greg): accepted — zero
  callers, in the file this work rewrites.
- **Badge on the child picker; severity on notices; child named by label; "Show
  the folder"; blast radius and automatic recovery stated** (prompted by Pierre):
  accepted at the "medium" level. `SyncStatus` having no UI reader is now recorded
  in the spec. The single-`SyncNotice` collapse is deferred as follow-up 3.
- **`run_cycles_until_terminal`, `assert_resolved_or_explained`,
  `ChildRepoFixture`, ownership-completeness contract test** (prompted by Ted):
  accepted. Defect 1 is a liveness bug and every merge-path test was single-shot.
- **Invariant strengthened to require HEAD advanced** (prompted by Ted): accepted
  — the weaker form holds vacuously.
- **Atomic-write tests rebuilt around a concurrent reader with a negative control
  and an injectable syscall-order sink** (prompted by Ted): accepted; the original
  three assertions pass with the fsync deleted.
- **Existing tests enumerated for inversion; "suites run unchanged" corrected;
  CI reality documented** (prompted by Ted): accepted. `app_coordinator.rs:2331`
  asserted defect 2 as intended behaviour, which is why it shipped. The
  `--ignored` CI job is deferred as follow-up 2.
- **Backstop fixture corrected** (prompted by Ted): the cited fixture is untracked
  and `working_tree_dirty` ignores untracked files, so the test would have been
  vacuous.
- **"Why the existing test suite did not catch either defect" section added**
  (prompted by Ted): the question the original test plan did not answer.
