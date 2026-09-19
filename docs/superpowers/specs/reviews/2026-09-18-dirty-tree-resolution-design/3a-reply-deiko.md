# [3A] Reply to Deiko Deshimaru's Critique

**Spec:** `docs/superpowers/specs/2026-09-18-dirty-tree-resolution-design.md`
**Reviewer addressed:** Deiko Deshimaru
**Reply date:** 2026-09-19

---

## Overall Response

Accepted in full, including the concern that reverses a decision the author had
already taken with the user. Concern 1 changed the shape of the design: the
guard now stages tracked paths rather than an allowlist, which deletes the stall
class instead of reporting it, and which cascades into three other points across
this panel.

## Point-by-Point

### Concern 1: The backstop contradicts a stated goal of the system it extends

**Verdict:** Accepted — option 1 (widen from "owned" to "tracked").

**Our response:** The reading of what the allowlist is for is correct and
decisive: `FILES_THIS_APP_OWNS` exists to stop `add_all(["*"])` sweeping
*untracked* strays into pushed history (`paths.rs:9-17`), and a file already
tracked in HEAD is already in that history. `index.update_all` stages
modifications and deletions of tracked entries and never adds an untracked path,
so the anti-`add_all` invariant survives intact.

The user was asked to re-decide this, having previously chosen the allowlist plus
a loud backstop, and ruled for your version. Spec §3 is rewritten: the guard
stages tracked paths; `FILES_THIS_APP_OWNS` is retained for `commit_merge` and
migration, exactly as you recommended.

Consequence you predicted and Pierre independently anticipated: with
`working_tree_dirty` already setting `include_untracked(false)`, any tree that
enters the guard now has a tracked change to stage, so `NothingOwnedToCommit` is
unreachable. It survives only as a should-never-happen, worded to the user as
"something unexpected happened" rather than as a routine operating condition. The
backstop's real content is now staging or commit failure — honest errors rather
than a designed terminal state.

Knock-on: `parental_control_attempts.csv` no longer needs to join
`FILES_THIS_APP_OWNS` at all, since the guard reaches it as a tracked path. That
reverses a fold-in the author had made and the user had approved, and it restores
the original doc comment's intent. The comment is rewritten around the two-tier
model — narrow list for `commit_merge` and migration, tracked-wide for the guard
— rather than around the old "who commits it first" reasoning.

### Concern 2: The guard commits bytes it never validated, and pushes them

**Verdict:** Accepted.

**Our response:** This was the sharpest correction in the panel and three
reviewers reached it independently. The spec's claim that atomic writes remove
the ambiguity "at the source" is true only for writes this binary performs after
the upgrade; it says nothing about a file already torn on disk, damaged by Proton
or a partial restore, or written by the old `goal_repository.rs:128` in-place
path.

The escalation you name — one machine's corruption becoming a two-machine outage
once `push_with_retry` propagates unparseable bytes and `read_rows` fails on both
sides — is strictly worse than the defect being fixed, and it would have been
introduced by this change.

Spec §3 now gates the guard: `transactions.csv` is parse-validated with
`allowance_core::codec::parse_transactions` before it is staged. Unparseable
content raises the durable failure notice and commits nothing. The spec states
explicitly that atomic writes bound *this app's* corruption, not all corruption.

### Concern 3: The `commit_file_change` change is inert as specified

**Verdict:** Accepted — your option (a).

**Our response:** Correct, and unanimous with Greg and Ted. Under Concern 1's
widening the point goes further than "inert": it is no longer needed at all. A
`parental_control_attempts.csv` left dirty by a swallowed commit error is now
staged and committed by the guard on the next cycle, so the widening is the whole
fix. The second reason it mattered is also gone — with the hard reset deleted,
nothing discards uncommitted content, so an uncommitted row is no longer at risk.

Propagation is dropped from scope and the spec says so with the reason, rather
than claiming two fixes where one does the work. Retained as hygiene only:
`#[must_use]` on the return, and the five `let _ =` sites replaced with an
explicit logged disposition so the decision is visible rather than discarded.

### Concern 4: The `parental_control_attempts.csv` exception is less bounded than claimed

**Verdict:** Accepted.

**Our response:** The premise was false and the memo proves it: both readers use a
default non-`flexible` `csv::Reader` with `let record = result?;`
(`parental_control_repository.rs:186-187` and `:112-121`), so a truncated append
fails the entire read *and* permanently blocks `get_next_id`, making the log
unwritable from then on. That is close to the opposite of the "bounded corruption
of the last record" the spec recorded as a conscious trade.

The append stays — the reasoning for it is sound and you agreed. The claim is made
true instead: both readers gain `.flexible(true)` and skip malformed records, with
a test that a file with a truncated trailing line still reads every prior record
and still yields a correct next id.

### Concern 5: A third reachable route, and a `commit_merge` behaviour change

**Verdict:** Accepted.

**Our response:** `delete_allowance_config` (`allowance_repository.rs:189-190`)
was verified in the main session: `std::fs::remove_file` on a tracked owned file
with no commit, user-triggerable, no crash or swallowed error required. Added to
the spec as a third route with its own regression test.

On the `commit_merge` interaction: under Concern 1's widening, `stage_owned_files`
keeps the narrow list and `commit_merge`'s tree semantics are unchanged, which
dissolves the concern rather than answering it. The deletion branch now lives in
the guard's `update_all`, not in `stage_owned_files`. The test you asked for — a
merge commit does not record a `goals.csv` deletion in any normal flow — is added
anyway, because the property is worth pinning regardless of which code could
violate it.

Your point about keying on the *index* rather than HEAD is moot for the same
reason: `update_all` handles index-vs-worktree state itself.

## Questions Answered

### Q: Second-parent loss on crash recovery — `theirs` never becomes an ancestor.

A: Added to the spec as an explicit convergence argument rather than an implied
one. After a crash between the atomic write and `commit_merge`, the guard commits
the merged content as an ordinary single-parent commit. The next cycle
re-classifies against `theirs`, which is still in the object database, and
recomputes the merge; `allowance_core::merge` de-duplicates rows that are
`intrinsic_eq` on both sides, so the rows already committed are not doubled. The
result converges to the same row set by a different commit topology. You are right
that this is the price of "same code path as an ordinary MCP write" and that it
deserved naming.

### Q: macOS durability — `fsync` vs `F_FULLFSYNC`. Which claim is being made?

A: Resolved against you and Greg jointly, with the user ruling. The spec now
separates the two properties: atomicity (no reader observes a partial file) comes
from `rename(2)` alone and is what both defects need; durability across power loss
is a different property that `fsync` does not deliver on macOS. `F_FULLFSYNC` is
rejected — tens of milliseconds on every transaction write, to buy survival of the
*most recent* write, when rename already guarantees the file is never corrupt.
`sync_all()` before rename is kept for non-APFS volumes (external disks, network
mounts, HFS+) where data-before-metadata ordering is not guaranteed. The directory
fsync is dropped per Greg. The claim is scoped honestly: no reader ever observes a
partial file; a power loss may cost the most recent write, never the file's
integrity.

A correction the spec also carries: its "no fsync, so power loss can leave a
zero-length file" justification for consolidating the six hand-rolled writers
imported an ext4 delayed-allocation failure mode into an APFS context. The
consolidation stands on cleanup-on-drop, collision-safe naming, and one definition
instead of six.

### Q: Does `Failed` every cycle need a circuit breaker?

A: Much less pressing under Concern 1. `Failed` is no longer a designed terminal
state reachable by an ordinary tracked file; it now means staging or commit
genuinely failed. `record_sync_failure` already dedupes by `child_id`
(`sync_state.rs:202-204`), so the UI does not accumulate. The per-tick log and
work remain unbounded; noted in the spec as a known, accepted cost rather than
silently left.

### Q: Permissions on `rename` — the temp file's mode, not the replaced file's.

A: Accepted, and sharpened by Greg's `tempfile` recommendation: `NamedTempFile`
creates at `0600`, so `persist` would *tighten* an existing `0644` file rather
than merely fail to preserve it. The spec now requires the mode to be set
explicitly before persist — preserving the existing file's mode where one exists,
defaulting to `0644` otherwise — with a test.

## Closing

Nothing open. Concerns 1 and 2 — the two you would not proceed without — are both
accepted as specified, and Concern 1 restructured the design rather than adjusting
it. The spec's Review Change Appendix records each change against your name.
